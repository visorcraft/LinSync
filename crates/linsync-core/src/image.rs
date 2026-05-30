// Image compare engine. Requires feature `image-compare`.

use std::path::Path;

use ::image::{DynamicImage, GenericImageView, ImageReader, RgbaImage};
use lab::Lab;
use serde::{Deserialize, Serialize};

const STREAM_SIZE_THRESHOLD: u64 = 100 * 1024 * 1024;
const STREAM_DIM_THRESHOLD: u32 = 16_384;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImageCompareMode {
    /// Byte-exact RGBA8 match on every channel.
    Exact,
    /// Per-channel absolute difference must not exceed the tolerance.
    Tolerance(u8),
    /// CIEDE2000 delta-E per pixel must not exceed `delta_e_threshold`.
    Perceptual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageCompareOptions {
    pub mode: ImageCompareMode,
    /// Per-channel tolerance for `Tolerance` mode. Ignored by other modes.
    pub tolerance: u8,
    /// Delta-E threshold for `Perceptual` mode.
    pub delta_e_threshold: f32,
    /// When true, use the file extension to choose decoder if magic-byte detection fails.
    pub trust_extension_fallback: bool,
    /// Number of image rows decoded per stripe when streaming large images.
    pub stream_stripe_rows: u32,
}

impl Default for ImageCompareOptions {
    fn default() -> Self {
        Self {
            mode: ImageCompareMode::Exact,
            tolerance: 0,
            delta_e_threshold: 2.3,
            trust_extension_fallback: false,
            stream_stripe_rows: 64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageCompareResult {
    pub equal: bool,
    pub left_dims: (u32, u32),
    pub right_dims: (u32, u32),
    pub total_pixels: u64,
    pub differing_pixels: u64,
    pub diff_ratio: f64,
    pub mode_used: ImageCompareMode,
    /// Bounding box of the diff region as (x_min, y_min, x_max, y_max).
    pub diff_bbox: Option<(u32, u32, u32, u32)>,
    /// RGBA8 overlay buffer. Empty in CLI mode; populated by the GUI bridge.
    #[serde(skip)]
    pub overlay: Vec<u8>,
    /// True when images had different dimensions and were padded to a common canvas.
    pub padded: bool,
}

#[derive(Debug, Clone)]
pub enum ImageCompareError {
    DimensionMismatch { left: (u32, u32), right: (u32, u32) },
    UnsupportedFormat(String),
    DecodeError(String),
    IoError(String),
}

impl std::fmt::Display for ImageCompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DimensionMismatch { left, right } => write!(
                f,
                "dimension mismatch: left {}x{} vs right {}x{}",
                left.0, left.1, right.0, right.1,
            ),
            Self::UnsupportedFormat(fmt) => write!(f, "unsupported image format: {fmt}"),
            Self::DecodeError(msg) => write!(f, "decode error: {msg}"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl std::error::Error for ImageCompareError {}

pub fn compare_images(
    left: &Path,
    right: &Path,
    options: &ImageCompareOptions,
) -> Result<ImageCompareResult, ImageCompareError> {
    if should_stream(left, right) {
        return compare_images_streaming(left, right, options);
    }

    let left_img = open_image(left)?;
    let right_img = open_image(right)?;

    let orig_left_dims = left_img.dimensions();
    let orig_right_dims = right_img.dimensions();

    let (left_img, right_img) = if orig_left_dims != orig_right_dims {
        pad_to_common_canvas(left_img, right_img)
    } else {
        (left_img, right_img)
    };

    let mut result = match &options.mode {
        ImageCompareMode::Exact => compare_exact(&left_img, &right_img),
        ImageCompareMode::Tolerance(tol) => compare_tolerance(&left_img, &right_img, *tol),
        ImageCompareMode::Perceptual => {
            compare_perceptual(&left_img, &right_img, options.delta_e_threshold)
        }
    }?;

    let padded = orig_left_dims != orig_right_dims;
    if padded {
        result.left_dims = orig_left_dims;
        result.right_dims = orig_right_dims;
        result.padded = true;
        result.equal = false;
    }

    Ok(result)
}

pub fn compare_images_streaming(
    left: &Path,
    right: &Path,
    options: &ImageCompareOptions,
) -> Result<ImageCompareResult, ImageCompareError> {
    let left_img = open_image(left)?;
    let right_img = open_image(right)?;

    let orig_left_dims = left_img.dimensions();
    let orig_right_dims = right_img.dimensions();

    let (left_img, right_img) = if orig_left_dims != orig_right_dims {
        pad_to_common_canvas(left_img, right_img)
    } else {
        (left_img, right_img)
    };

    let (width, height) = left_img.dimensions();
    let stripe = options.stream_stripe_rows.max(1);
    let total = width as u64 * height as u64;

    let left_rgba = left_img.to_rgba8();
    let right_rgba = right_img.to_rgba8();

    let mut differing = 0;
    let mut bbox = None;

    let mut y_start = 0;
    while y_start < height {
        let y_end = y_start.saturating_add(stripe).min(height);
        for y in y_start..y_end {
            for x in 0..width {
                let lp = left_rgba.get_pixel(x, y).0;
                let rp = right_rgba.get_pixel(x, y).0;
                if pixels_differ(&lp, &rp, options) {
                    differing += 1;
                    expand_bbox(&mut bbox, x, y);
                }
            }
        }
        y_start = y_end;
    }

    let mut result = build_result(
        orig_left_dims,
        orig_right_dims,
        total,
        differing,
        bbox,
        options.mode.clone(),
    );

    let padded = orig_left_dims != orig_right_dims;
    if padded {
        result.padded = true;
        result.equal = false;
    }

    Ok(result)
}

fn should_stream(left: &Path, right: &Path) -> bool {
    path_stream_trigger(left) || path_stream_trigger(right)
}

fn path_stream_trigger(path: &Path) -> bool {
    let size_trigger = std::fs::metadata(path)
        .map(|metadata| metadata.len() > STREAM_SIZE_THRESHOLD)
        .unwrap_or(false);

    size_trigger || dimension_stream_trigger(path)
}

fn dimension_stream_trigger(path: &Path) -> bool {
    ImageReader::open(path)
        .ok()
        .and_then(|reader| reader.with_guessed_format().ok())
        .and_then(|reader| reader.into_dimensions().ok())
        .map(|(width, height)| width > STREAM_DIM_THRESHOLD || height > STREAM_DIM_THRESHOLD)
        .unwrap_or(false)
}

fn open_image(path: &Path) -> Result<DynamicImage, ImageCompareError> {
    ImageReader::open(path)
        .map_err(|e| ImageCompareError::IoError(e.to_string()))?
        .with_guessed_format()
        .map_err(|e| ImageCompareError::UnsupportedFormat(e.to_string()))?
        .decode()
        .map_err(|e| ImageCompareError::DecodeError(e.to_string()))
}

fn pad_to_common_canvas(left: DynamicImage, right: DynamicImage) -> (DynamicImage, DynamicImage) {
    let lw = left.width();
    let lh = left.height();
    let rw = right.width();
    let rh = right.height();
    let cw = lw.max(rw);
    let ch = lh.max(rh);

    let left_padded = if lw < cw || lh < ch {
        let mut buf = RgbaImage::from_pixel(cw, ch, ::image::Rgba([0, 0, 0, 0]));
        ::image::imageops::overlay(&mut buf, &left.to_rgba8(), 0, 0);
        DynamicImage::ImageRgba8(buf)
    } else {
        left
    };

    let right_padded = if rw < cw || rh < ch {
        let mut buf = RgbaImage::from_pixel(cw, ch, ::image::Rgba([0, 0, 0, 0]));
        ::image::imageops::overlay(&mut buf, &right.to_rgba8(), 0, 0);
        DynamicImage::ImageRgba8(buf)
    } else {
        right
    };

    (left_padded, right_padded)
}

fn compare_exact(
    left: &DynamicImage,
    right: &DynamicImage,
) -> Result<ImageCompareResult, ImageCompareError> {
    let left_dims = left.dimensions();
    let right_dims = right.dimensions();
    let (width, height) = left_dims;
    let total = width as u64 * height as u64;

    let left_rgba = left.to_rgba8();
    let right_rgba = right.to_rgba8();

    let mut differing = 0;
    let mut bbox = None;

    for y in 0..height {
        for x in 0..width {
            let lp = left_rgba.get_pixel(x, y);
            let rp = right_rgba.get_pixel(x, y);
            if lp != rp {
                differing += 1;
                expand_bbox(&mut bbox, x, y);
            }
        }
    }

    Ok(build_result(
        left_dims,
        right_dims,
        total,
        differing,
        bbox,
        ImageCompareMode::Exact,
    ))
}

fn compare_tolerance(
    left: &DynamicImage,
    right: &DynamicImage,
    tolerance: u8,
) -> Result<ImageCompareResult, ImageCompareError> {
    let left_dims = left.dimensions();
    let right_dims = right.dimensions();
    let (width, height) = left_dims;
    let total = width as u64 * height as u64;

    let left_rgba = left.to_rgba8();
    let right_rgba = right.to_rgba8();

    let mut differing = 0;
    let mut bbox = None;

    for y in 0..height {
        for x in 0..width {
            let lp = left_rgba.get_pixel(x, y).0;
            let rp = right_rgba.get_pixel(x, y).0;
            let is_diff = lp
                .iter()
                .zip(rp.iter())
                .any(|(&l, &r)| l.abs_diff(r) > tolerance);
            if is_diff {
                differing += 1;
                expand_bbox(&mut bbox, x, y);
            }
        }
    }

    Ok(build_result(
        left_dims,
        right_dims,
        total,
        differing,
        bbox,
        ImageCompareMode::Tolerance(tolerance),
    ))
}

fn compare_perceptual(
    left: &DynamicImage,
    right: &DynamicImage,
    delta_e_threshold: f32,
) -> Result<ImageCompareResult, ImageCompareError> {
    let left_dims = left.dimensions();
    let right_dims = right.dimensions();
    let (width, height) = left_dims;
    let total = width as u64 * height as u64;

    let left_rgba = left.to_rgba8();
    let right_rgba = right.to_rgba8();

    let mut differing = 0;
    let mut bbox = None;

    for y in 0..height {
        for x in 0..width {
            let lp = left_rgba.get_pixel(x, y).0;
            let rp = right_rgba.get_pixel(x, y).0;

            let left_lab = Lab::from_rgb(&[lp[0], lp[1], lp[2]]);
            let right_lab = Lab::from_rgb(&[rp[0], rp[1], rp[2]]);
            let delta_e = ciede2000(&left_lab, &right_lab);

            if delta_e > delta_e_threshold {
                differing += 1;
                expand_bbox(&mut bbox, x, y);
            }
        }
    }

    Ok(build_result(
        left_dims,
        right_dims,
        total,
        differing,
        bbox,
        ImageCompareMode::Perceptual,
    ))
}

pub(crate) fn build_result(
    left_dims: (u32, u32),
    right_dims: (u32, u32),
    total: u64,
    differing: u64,
    bbox: Option<(u32, u32, u32, u32)>,
    mode_used: ImageCompareMode,
) -> ImageCompareResult {
    let padded = left_dims != right_dims;
    ImageCompareResult {
        equal: differing == 0 && !padded,
        left_dims,
        right_dims,
        total_pixels: total,
        differing_pixels: differing,
        diff_ratio: if total == 0 {
            0.0
        } else {
            differing as f64 / total as f64
        },
        mode_used,
        diff_bbox: bbox,
        overlay: Vec::new(),
        padded,
    }
}

pub(crate) fn expand_bbox(bbox: &mut Option<(u32, u32, u32, u32)>, x: u32, y: u32) {
    match bbox {
        None => *bbox = Some((x, y, x, y)),
        Some((x0, y0, x1, y1)) => {
            *x0 = (*x0).min(x);
            *y0 = (*y0).min(y);
            *x1 = (*x1).max(x);
            *y1 = (*y1).max(y);
        }
    }
}

pub fn generate_overlay(
    left: &Path,
    right: &Path,
    options: &ImageCompareOptions,
) -> Result<ImageCompareResult, ImageCompareError> {
    let left_img = open_image(left)?;
    let right_img = open_image(right)?;
    let orig_left_dims = left_img.dimensions();
    let orig_right_dims = right_img.dimensions();

    let (left_img, right_img) = if orig_left_dims != orig_right_dims {
        pad_to_common_canvas(left_img, right_img)
    } else {
        (left_img, right_img)
    };

    let (width, height) = left_img.dimensions();
    let left_rgba = left_img.to_rgba8();
    let right_rgba = right_img.to_rgba8();
    let total = width as u64 * height as u64;
    let mut differing: u64 = 0;
    let mut bbox = None;
    let mut overlay_rgba = vec![0u8; (width * height * 4) as usize];

    for y in 0..height {
        for x in 0..width {
            let lp = left_rgba.get_pixel(x, y).0;
            let rp = right_rgba.get_pixel(x, y).0;
            let idx = ((y * width + x) * 4) as usize;
            if pixels_differ(&lp, &rp, options) {
                differing += 1;
                expand_bbox(&mut bbox, x, y);
                overlay_rgba[idx] = 255;
                overlay_rgba[idx + 1] = 0;
                overlay_rgba[idx + 2] = 0;
                overlay_rgba[idx + 3] = 160;
            } else {
                overlay_rgba[idx] = 0;
                overlay_rgba[idx + 1] = 0;
                overlay_rgba[idx + 2] = 0;
                overlay_rgba[idx + 3] = 0;
            }
        }
    }

    let padded = orig_left_dims != orig_right_dims;
    Ok(ImageCompareResult {
        equal: differing == 0 && !padded,
        left_dims: orig_left_dims,
        right_dims: orig_right_dims,
        total_pixels: total,
        differing_pixels: differing,
        diff_ratio: if total == 0 {
            0.0
        } else {
            differing as f64 / total as f64
        },
        mode_used: options.mode.clone(),
        diff_bbox: bbox,
        overlay: overlay_rgba,
        padded,
    })
}

fn pixels_differ(left: &[u8; 4], right: &[u8; 4], options: &ImageCompareOptions) -> bool {
    match &options.mode {
        ImageCompareMode::Exact => left != right,
        ImageCompareMode::Tolerance(tolerance) => left
            .iter()
            .zip(right.iter())
            .any(|(&l, &r)| l.abs_diff(r) > *tolerance),
        ImageCompareMode::Perceptual => {
            let left_lab = Lab::from_rgb(&[left[0], left[1], left[2]]);
            let right_lab = Lab::from_rgb(&[right[0], right[1], right[2]]);
            ciede2000(&left_lab, &right_lab) > options.delta_e_threshold
        }
    }
}

fn ciede2000(left: &Lab, right: &Lab) -> f32 {
    const K_L: f32 = 1.0;
    const K_C: f32 = 1.0;
    const K_H: f32 = 1.0;
    const POW_25_7: f32 = 6_103_515_625.0;

    let c1 = (left.a.mul_add(left.a, left.b * left.b)).sqrt();
    let c2 = (right.a.mul_add(right.a, right.b * right.b)).sqrt();
    let c_bar = (c1 + c2) * 0.5;
    let c_bar_7 = c_bar.powi(7);
    let g = 0.5 * (1.0 - (c_bar_7 / (c_bar_7 + POW_25_7)).sqrt());

    let a1_prime = (1.0 + g) * left.a;
    let a2_prime = (1.0 + g) * right.a;
    let c1_prime = (a1_prime.mul_add(a1_prime, left.b * left.b)).sqrt();
    let c2_prime = (a2_prime.mul_add(a2_prime, right.b * right.b)).sqrt();

    let h1_prime = hue_degrees(left.b, a1_prime);
    let h2_prime = hue_degrees(right.b, a2_prime);

    let delta_l_prime = right.l - left.l;
    let delta_c_prime = c2_prime - c1_prime;
    let delta_h_prime = if c1_prime * c2_prime == 0.0 {
        0.0
    } else {
        let h_diff = h2_prime - h1_prime;
        if h_diff.abs() <= 180.0 {
            h_diff
        } else if h_diff > 180.0 {
            h_diff - 360.0
        } else {
            h_diff + 360.0
        }
    };
    let delta_h_prime =
        2.0 * (c1_prime * c2_prime).sqrt() * (0.5 * delta_h_prime).to_radians().sin();

    let l_bar_prime = (left.l + right.l) * 0.5;
    let c_bar_prime = (c1_prime + c2_prime) * 0.5;
    let h_bar_prime = hue_average(c1_prime, c2_prime, h1_prime, h2_prime);

    let t = 1.0 - 0.17 * (h_bar_prime - 30.0).to_radians().cos()
        + 0.24 * (2.0 * h_bar_prime).to_radians().cos()
        + 0.32 * (3.0 * h_bar_prime + 6.0).to_radians().cos()
        - 0.20 * (4.0 * h_bar_prime - 63.0).to_radians().cos();

    let delta_theta = 30.0 * (-(((h_bar_prime - 275.0) / 25.0).powi(2))).exp();
    let c_bar_prime_7 = c_bar_prime.powi(7);
    let r_c = 2.0 * (c_bar_prime_7 / (c_bar_prime_7 + POW_25_7)).sqrt();
    let l_bar_offset = l_bar_prime - 50.0;
    let s_l =
        1.0 + (0.015 * l_bar_offset * l_bar_offset) / (20.0 + l_bar_offset * l_bar_offset).sqrt();
    let s_c = 1.0 + 0.045 * c_bar_prime;
    let s_h = 1.0 + 0.015 * c_bar_prime * t;
    let r_t = -r_c * (2.0 * delta_theta).to_radians().sin();

    let l_term = delta_l_prime / (K_L * s_l);
    let c_term = delta_c_prime / (K_C * s_c);
    let h_term = delta_h_prime / (K_H * s_h);

    (l_term * l_term + c_term * c_term + h_term * h_term + r_t * c_term * h_term).sqrt()
}

fn hue_degrees(b: f32, a_prime: f32) -> f32 {
    if a_prime == 0.0 && b == 0.0 {
        0.0
    } else {
        b.atan2(a_prime).to_degrees().rem_euclid(360.0)
    }
}

fn hue_average(c1_prime: f32, c2_prime: f32, h1_prime: f32, h2_prime: f32) -> f32 {
    if c1_prime * c2_prime == 0.0 {
        h1_prime + h2_prime
    } else {
        let h_sum = h1_prime + h2_prime;
        let h_diff = (h1_prime - h2_prime).abs();
        if h_diff <= 180.0 {
            h_sum * 0.5
        } else if h_sum < 360.0 {
            (h_sum + 360.0) * 0.5
        } else {
            (h_sum - 360.0) * 0.5
        }
    }
}
