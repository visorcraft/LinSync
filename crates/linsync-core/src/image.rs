// Image compare engine. Requires feature `image-compare`.

use std::collections::VecDeque;
use std::path::Path;

use ::image::{DynamicImage, GenericImageView, ImageReader, RgbaImage};
use lab::Lab;
use serde::{Deserialize, Serialize};

const STREAM_SIZE_THRESHOLD: u64 = 100 * 1024 * 1024;
const STREAM_DIM_THRESHOLD: u32 = 16_384;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffRegion {
    pub id: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixel_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageFormatSupport {
    pub name: String,
    pub extensions: Vec<String>,
}

impl ImageFormatSupport {
    fn new(name: &str, extensions: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            extensions: extensions.iter().map(|ext| (*ext).to_owned()).collect(),
        }
    }
}

pub fn supported_image_formats() -> Vec<ImageFormatSupport> {
    vec![
        ImageFormatSupport::new("PNG", &["png"]),
        ImageFormatSupport::new("JPEG", &["jpg", "jpeg", "jfif"]),
        ImageFormatSupport::new("WebP", &["webp"]),
        ImageFormatSupport::new("TIFF", &["tif", "tiff"]),
        ImageFormatSupport::new("GIF", &["gif"]),
        ImageFormatSupport::new("Radiance HDR", &["hdr"]),
        ImageFormatSupport::new("OpenEXR", &["exr"]),
        #[cfg(feature = "image-avif")]
        ImageFormatSupport::new("AVIF", &["avif", "avifs"]),
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImageCompareMode {
    /// Byte-exact RGBA8 match on every channel.
    Exact,
    /// Per-channel absolute difference must not exceed the tolerance.
    ///
    /// A struct variant (rather than a newtype) so the internally-tagged
    /// (`{"kind": "tolerance", "tolerance": N}`) representation serializes —
    /// serde cannot serialize an internally-tagged newtype variant wrapping a
    /// bare integer. The unit variants keep their `{"kind": "…"}` form, so
    /// existing on-disk profile JSON is unaffected.
    Tolerance { tolerance: u8 },
    /// CIEDE2000 delta-E per pixel must not exceed `delta_e_threshold`.
    Perceptual,
}

/// How animated images (GIF, APNG, animated WebP) are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FrameCompareMode {
    /// Compare only the first frame (today's behavior, the default). A
    /// still image is always a single "first frame".
    #[default]
    FirstFrame,
    /// Decode every frame and compare corresponding frames pairwise, reporting a
    /// per-frame summary. Frame-count mismatches mark the extra frames different.
    AllFrames,
}

/// Per-frame outcome of an `AllFrames` animated-image comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameSummary {
    /// Zero-based frame index.
    pub frame: usize,
    /// Whether the two frames compared equal.
    pub equal: bool,
    /// Fraction of differing pixels (0.0–1.0); 1.0 for a one-sided frame.
    pub diff_ratio: f64,
    /// Number of differing pixels.
    pub differing_pixels: u64,
    /// True when the frame exists on only one side (frame-count mismatch).
    pub one_sided: bool,
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
    /// How animated images are compared. Defaults to `FirstFrame` so existing
    /// behavior (and existing profiles) are unchanged.
    #[serde(default)]
    pub frame_mode: FrameCompareMode,
}

impl Default for ImageCompareOptions {
    fn default() -> Self {
        Self {
            mode: ImageCompareMode::Exact,
            tolerance: 0,
            delta_e_threshold: 2.3,
            trust_extension_fallback: false,
            stream_stripe_rows: 64,
            frame_mode: FrameCompareMode::FirstFrame,
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
    pub diff_regions: Vec<DiffRegion>,
    /// Decoded color type of each side (e.g. `"Rgba8"`, `"Rgb32F"` for HDR),
    /// surfaced so a client can flag a color-type/bit-depth mismatch. `None` for
    /// results that predate metadata capture (e.g. a JSON round-trip).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_type_left: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_type_right: Option<String>,
    /// Total frame count for an `AllFrames` animated compare (`None` for a
    /// single-frame/first-frame compare).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_count: Option<usize>,
    /// Per-frame summaries for an `AllFrames` animated compare; empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_frame_summaries: Vec<FrameSummary>,
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
    // Animated comparison is its own path (it decodes every frame and cannot use
    // the large-image streaming shortcut).
    if matches!(options.frame_mode, FrameCompareMode::AllFrames) {
        return compare_images_all_frames(left, right, options);
    }
    if should_stream(left, right) {
        return compare_images_streaming(left, right, options);
    }

    let left_img = open_image(left)?;
    let right_img = open_image(right)?;

    let color_left = format!("{:?}", left_img.color());
    let color_right = format!("{:?}", right_img.color());

    let orig_left_dims = left_img.dimensions();
    let orig_right_dims = right_img.dimensions();

    let (left_img, right_img) = if orig_left_dims != orig_right_dims {
        pad_to_common_canvas(left_img, right_img)
    } else {
        (left_img, right_img)
    };

    let mut result = match &options.mode {
        ImageCompareMode::Exact => compare_exact(&left_img, &right_img),
        ImageCompareMode::Tolerance { tolerance } => {
            compare_tolerance(&left_img, &right_img, *tolerance)
        }
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

    result.color_type_left = Some(color_left);
    result.color_type_right = Some(color_right);

    Ok(result)
}

pub fn compare_images_streaming(
    left: &Path,
    right: &Path,
    options: &ImageCompareOptions,
) -> Result<ImageCompareResult, ImageCompareError> {
    let left_img = open_image(left)?;
    let right_img = open_image(right)?;

    let color_left = format!("{:?}", left_img.color());
    let color_right = format!("{:?}", right_img.color());

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
    let mut diff_mask = vec![vec![false; width as usize]; height as usize];

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
                    diff_mask[y as usize][x as usize] = true;
                }
            }
        }
        y_start = y_end;
    }

    let diff_regions = find_diff_regions(&diff_mask, width, height);

    let mut result = build_result(
        orig_left_dims,
        orig_right_dims,
        total,
        differing,
        bbox,
        options.mode.clone(),
        diff_regions,
    );

    let padded = orig_left_dims != orig_right_dims;
    if padded {
        result.padded = true;
        result.equal = false;
    }

    result.color_type_left = Some(color_left);
    result.color_type_right = Some(color_right);

    Ok(result)
}

/// Decode every frame of an animated image (GIF, APNG, animated WebP) to RGBA8.
/// Still images (or formats/files without animation) yield a single frame, so a
/// caller in `AllFrames` mode degrades gracefully to a one-frame comparison.
fn decode_image_frames(path: &Path) -> Result<Vec<DynamicImage>, ImageCompareError> {
    use ::image::{AnimationDecoder, ImageFormat};

    let format = ImageReader::open(path)
        .ok()
        .and_then(|reader| reader.with_guessed_format().ok())
        .and_then(|reader| reader.format());

    let collected: Option<Vec<::image::Frame>> = match format {
        Some(ImageFormat::Gif) => std::fs::File::open(path).ok().and_then(|file| {
            ::image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file))
                .ok()
                .and_then(|decoder| decoder.into_frames().collect_frames().ok())
        }),
        Some(ImageFormat::WebP) => std::fs::File::open(path).ok().and_then(|file| {
            ::image::codecs::webp::WebPDecoder::new(std::io::BufReader::new(file))
                .ok()
                .filter(|decoder| decoder.has_animation())
                .and_then(|decoder| decoder.into_frames().collect_frames().ok())
        }),
        Some(ImageFormat::Png) => std::fs::File::open(path).ok().and_then(|file| {
            ::image::codecs::png::PngDecoder::new(std::io::BufReader::new(file))
                .ok()
                .filter(|decoder| decoder.is_apng().unwrap_or(false))
                .and_then(|decoder| decoder.apng().ok())
                .and_then(|decoder| decoder.into_frames().collect_frames().ok())
        }),
        _ => None,
    };

    if let Some(frames) = collected
        && !frames.is_empty()
    {
        return Ok(frames
            .into_iter()
            .map(|frame| DynamicImage::ImageRgba8(frame.into_buffer()))
            .collect());
    }
    // Not animated (or a single-frame source): decode the one image.
    Ok(vec![open_image(path)?])
}

/// Compare two already-decoded frames, padding to a common canvas when their
/// dimensions differ (mirrors [`compare_images`] for in-memory images).
fn compare_decoded(
    left: &DynamicImage,
    right: &DynamicImage,
    options: &ImageCompareOptions,
) -> Result<ImageCompareResult, ImageCompareError> {
    let orig_left = left.dimensions();
    let orig_right = right.dimensions();
    if orig_left != orig_right {
        let (l, r) = pad_to_common_canvas(left.clone(), right.clone());
        let mut result = compare_decoded_same_size(&l, &r, options)?;
        result.left_dims = orig_left;
        result.right_dims = orig_right;
        result.padded = true;
        result.equal = false;
        Ok(result)
    } else {
        compare_decoded_same_size(left, right, options)
    }
}

fn compare_decoded_same_size(
    left: &DynamicImage,
    right: &DynamicImage,
    options: &ImageCompareOptions,
) -> Result<ImageCompareResult, ImageCompareError> {
    match &options.mode {
        ImageCompareMode::Exact => compare_exact(left, right),
        ImageCompareMode::Tolerance { tolerance } => compare_tolerance(left, right, *tolerance),
        ImageCompareMode::Perceptual => compare_perceptual(left, right, options.delta_e_threshold),
    }
}

/// Compare every frame of two animated (or still) images pairwise. Reports a
/// per-frame summary plus aggregate pixel counts; a frame-count mismatch marks
/// the extra frames as one-sided (their full pixel count counted as differing)
/// and the overall result as different. Aggregate `differing_pixels`/`diff_ratio`
/// are pixel-count-weighted across frames (a larger frame contributes more); use
/// `per_frame_summaries` for un-weighted per-frame ratios.
pub fn compare_images_all_frames(
    left: &Path,
    right: &Path,
    options: &ImageCompareOptions,
) -> Result<ImageCompareResult, ImageCompareError> {
    let left_frames = decode_image_frames(left)?;
    let right_frames = decode_image_frames(right)?;
    let frame_count = left_frames.len().max(right_frames.len());

    // Inner comparisons run as a single still frame (avoid recursing here).
    let inner = ImageCompareOptions {
        frame_mode: FrameCompareMode::FirstFrame,
        ..options.clone()
    };

    let color_left = left_frames.first().map(|f| format!("{:?}", f.color()));
    let color_right = right_frames.first().map(|f| format!("{:?}", f.color()));
    let left_dims = left_frames
        .first()
        .map(|f| f.dimensions())
        .unwrap_or((0, 0));
    let right_dims = right_frames
        .first()
        .map(|f| f.dimensions())
        .unwrap_or((0, 0));

    let mut summaries = Vec::with_capacity(frame_count);
    // Aggregate metrics are pixel-count-weighted across frames: each frame
    // contributes its own (possibly padded) pixel count, so a high-resolution
    // frame weighs more than a low-resolution one. The per-frame summaries carry
    // the un-weighted per-frame ratios.
    let mut total_pixels: u64 = 0;
    let mut differing_pixels: u64 = 0;
    let mut any_diff = left_frames.len() != right_frames.len();
    let mut any_padded = left_dims != right_dims;

    for index in 0..frame_count {
        match (left_frames.get(index), right_frames.get(index)) {
            (Some(l), Some(r)) => {
                let cmp = compare_decoded(l, r, &inner)?;
                total_pixels += cmp.total_pixels;
                differing_pixels += cmp.differing_pixels;
                if !cmp.equal {
                    any_diff = true;
                }
                if cmp.padded {
                    any_padded = true;
                }
                summaries.push(FrameSummary {
                    frame: index,
                    equal: cmp.equal,
                    diff_ratio: cmp.diff_ratio,
                    differing_pixels: cmp.differing_pixels,
                    one_sided: false,
                });
            }
            other => {
                // The frame exists on only one side: count all of its pixels as
                // differing so the aggregate reflects the missing content rather
                // than silently dropping it.
                let present = match other {
                    (Some(frame), None) | (None, Some(frame)) => Some(frame),
                    _ => None,
                };
                let pixels = present
                    .map(|f| {
                        let (w, h) = f.dimensions();
                        w as u64 * h as u64
                    })
                    .unwrap_or(0);
                total_pixels += pixels;
                differing_pixels += pixels;
                any_diff = true;
                summaries.push(FrameSummary {
                    frame: index,
                    equal: false,
                    diff_ratio: 1.0,
                    differing_pixels: pixels,
                    one_sided: true,
                });
            }
        }
    }

    Ok(ImageCompareResult {
        equal: !any_diff,
        left_dims,
        right_dims,
        total_pixels,
        differing_pixels,
        diff_ratio: if total_pixels == 0 {
            0.0
        } else {
            differing_pixels as f64 / total_pixels as f64
        },
        mode_used: options.mode.clone(),
        diff_bbox: None,
        overlay: Vec::new(),
        // Padded if any frame pair needed a common canvas (not just the first).
        padded: any_padded,
        diff_regions: Vec::new(),
        color_type_left: color_left,
        color_type_right: color_right,
        frame_count: Some(frame_count),
        per_frame_summaries: summaries,
    })
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
    // Strict dimension cap for untrusted input; combined with the default
    // 512 MiB allocation cap below it bounds the decoded image (and therefore
    // the RGBA + overlay buffers built from it).
    const MAX_IMAGE_DIMENSION: u32 = 30_000;

    let mut reader = ImageReader::open(path)
        .map_err(|e| ImageCompareError::IoError(e.to_string()))?
        .with_guessed_format()
        .map_err(|e| ImageCompareError::UnsupportedFormat(e.to_string()))?;
    // Without calling `limits`, the decoder is completely unbounded, so a
    // crafted decompression bomb can force unbounded allocation. `default()`
    // caps total decoder allocation at 512 MiB; we add a strict pixel cap.
    let mut limits = ::image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    reader.limits(limits);
    reader
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
    let mut diff_mask = vec![vec![false; width as usize]; height as usize];

    for y in 0..height {
        for x in 0..width {
            let lp = left_rgba.get_pixel(x, y);
            let rp = right_rgba.get_pixel(x, y);
            if lp != rp {
                differing += 1;
                expand_bbox(&mut bbox, x, y);
                diff_mask[y as usize][x as usize] = true;
            }
        }
    }

    let diff_regions = find_diff_regions(&diff_mask, width, height);

    Ok(build_result(
        left_dims,
        right_dims,
        total,
        differing,
        bbox,
        ImageCompareMode::Exact,
        diff_regions,
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
    let mut diff_mask = vec![vec![false; width as usize]; height as usize];

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
                diff_mask[y as usize][x as usize] = true;
            }
        }
    }

    let diff_regions = find_diff_regions(&diff_mask, width, height);

    Ok(build_result(
        left_dims,
        right_dims,
        total,
        differing,
        bbox,
        ImageCompareMode::Tolerance { tolerance },
        diff_regions,
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
    let mut diff_mask = vec![vec![false; width as usize]; height as usize];

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
                diff_mask[y as usize][x as usize] = true;
            }
        }
    }

    let diff_regions = find_diff_regions(&diff_mask, width, height);

    Ok(build_result(
        left_dims,
        right_dims,
        total,
        differing,
        bbox,
        ImageCompareMode::Perceptual,
        diff_regions,
    ))
}

pub(crate) fn build_result(
    left_dims: (u32, u32),
    right_dims: (u32, u32),
    total: u64,
    differing: u64,
    bbox: Option<(u32, u32, u32, u32)>,
    mode_used: ImageCompareMode,
    diff_regions: Vec<DiffRegion>,
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
        diff_regions,
        color_type_left: None,
        color_type_right: None,
        frame_count: None,
        per_frame_summaries: Vec::new(),
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
    // usize arithmetic: `width * height * 4` in u32 overflows and wraps for
    // large images, yielding an undersized buffer and out-of-bounds writes
    // below. Decoder limits in `open_image` bound the dimensions up front.
    let mut overlay_rgba = vec![0u8; width as usize * height as usize * 4];
    let mut diff_mask = vec![vec![false; width as usize]; height as usize];

    for y in 0..height {
        for x in 0..width {
            let lp = left_rgba.get_pixel(x, y).0;
            let rp = right_rgba.get_pixel(x, y).0;
            let idx = (y as usize * width as usize + x as usize) * 4;
            if pixels_differ(&lp, &rp, options) {
                differing += 1;
                expand_bbox(&mut bbox, x, y);
                overlay_rgba[idx] = 255;
                overlay_rgba[idx + 1] = 0;
                overlay_rgba[idx + 2] = 0;
                overlay_rgba[idx + 3] = 160;
                diff_mask[y as usize][x as usize] = true;
            } else {
                overlay_rgba[idx] = 0;
                overlay_rgba[idx + 1] = 0;
                overlay_rgba[idx + 2] = 0;
                overlay_rgba[idx + 3] = 0;
            }
        }
    }

    let diff_regions = find_diff_regions(&diff_mask, width, height);

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
        diff_regions,
        color_type_left: None,
        color_type_right: None,
        frame_count: None,
        per_frame_summaries: Vec::new(),
    })
}

fn pixels_differ(left: &[u8; 4], right: &[u8; 4], options: &ImageCompareOptions) -> bool {
    match &options.mode {
        ImageCompareMode::Exact => left != right,
        ImageCompareMode::Tolerance { tolerance } => left
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

fn find_diff_regions(diff_mask: &[Vec<bool>], width: u32, height: u32) -> Vec<DiffRegion> {
    let w = width as usize;
    let h = height as usize;
    let mut visited = vec![vec![false; w]; h];
    let mut regions = Vec::new();
    let mut next_id = 0usize;

    for y in 0..h {
        for x in 0..w {
            if diff_mask[y][x] && !visited[y][x] {
                let mut queue = VecDeque::new();
                queue.push_back((x, y));
                visited[y][x] = true;
                let mut pixel_count = 0usize;
                let mut min_x = x;
                let mut min_y = y;
                let mut max_x = x;
                let mut max_y = y;

                while let Some((cx, cy)) = queue.pop_front() {
                    pixel_count += 1;
                    min_x = min_x.min(cx);
                    min_y = min_y.min(cy);
                    max_x = max_x.max(cx);
                    max_y = max_y.max(cy);

                    for &(dx, dy) in &[(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            let nx = nx as usize;
                            let ny = ny as usize;
                            if diff_mask[ny][nx] && !visited[ny][nx] {
                                visited[ny][nx] = true;
                                queue.push_back((nx, ny));
                            }
                        }
                    }
                }

                regions.push(DiffRegion {
                    id: next_id,
                    x: min_x as u32,
                    y: min_y as u32,
                    width: (max_x - min_x + 1) as u32,
                    height: (max_y - min_y + 1) as u32,
                    pixel_count,
                });
                next_id += 1;
            }
        }
    }

    regions
}

impl ImageCompareResult {
    /// Human-readable label for the compare mode that produced this result.
    pub fn mode_label(&self) -> String {
        match &self.mode_used {
            ImageCompareMode::Exact => "exact".to_owned(),
            ImageCompareMode::Tolerance { tolerance } => format!("tolerance ({tolerance})"),
            ImageCompareMode::Perceptual => "perceptual".to_owned(),
        }
    }

    /// Render a standalone HTML report of this image comparison. The raster
    /// overlay is intentionally excluded (it is `#[serde(skip)]` and not carried
    /// across a JSON round-trip); the report summarizes dimensions, the compare
    /// mode, pixel difference counts, and the detected diff regions so a saved
    /// result can be re-rendered by `report --from-json`.
    pub fn to_html_report(&self) -> String {
        let mut html = String::new();
        html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">\n");
        html.push_str("<title>LinSync image report</title>\n");
        html.push_str(
            "<style>\n\
             body{font-family:system-ui,sans-serif;margin:1.5rem;}\n\
             table{border-collapse:collapse;margin-top:0.5rem;}\n\
             td,th{border:1px solid #ccc;padding:2px 6px;font-family:monospace;text-align:left;}\n\
             .equal{color:#1a7f37;}\n\
             .diff{color:#b00020;}\n\
             </style>\n</head><body>\n",
        );
        html.push_str("<h1>LinSync image report</h1>\n");
        let status = if self.equal {
            "<span class=\"equal\">equal</span>"
        } else {
            "<span class=\"diff\">different</span>"
        };
        html.push_str(&format!("<p>Result: {status}</p>\n"));
        html.push_str("<table>\n<tbody>\n");
        html.push_str(&format!(
            "<tr><th>Left dimensions</th><td>{}&times;{}</td></tr>\n",
            self.left_dims.0, self.left_dims.1
        ));
        html.push_str(&format!(
            "<tr><th>Right dimensions</th><td>{}&times;{}</td></tr>\n",
            self.right_dims.0, self.right_dims.1
        ));
        html.push_str(&format!(
            "<tr><th>Compare mode</th><td>{}</td></tr>\n",
            self.mode_label()
        ));
        html.push_str(&format!(
            "<tr><th>Differing pixels</th><td>{} of {} ({:.4}%)</td></tr>\n",
            self.differing_pixels,
            self.total_pixels,
            self.diff_ratio * 100.0
        ));
        if self.padded {
            html.push_str("<tr><th>Canvas</th><td>padded (dimensions differed)</td></tr>\n");
        }
        if let Some((x0, y0, x1, y1)) = self.diff_bbox {
            html.push_str(&format!(
                "<tr><th>Diff bounding box</th><td>({x0}, {y0}) – ({x1}, {y1})</td></tr>\n"
            ));
        }
        html.push_str(&format!(
            "<tr><th>Diff regions</th><td>{}</td></tr>\n",
            self.diff_regions.len()
        ));
        html.push_str("</tbody></table>\n");
        if !self.diff_regions.is_empty() {
            html.push_str("<h2>Diff regions</h2>\n<table>\n");
            html.push_str(
                "<thead><tr><th>#</th><th>x</th><th>y</th><th>width</th><th>height</th><th>pixels</th></tr></thead>\n<tbody>\n",
            );
            for region in &self.diff_regions {
                html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                    region.id, region.x, region.y, region.width, region.height, region.pixel_count
                ));
            }
            html.push_str("</tbody></table>\n");
        }
        html.push_str("</body></html>\n");
        html
    }

    pub fn diff_region_count(&self) -> usize {
        self.diff_regions.len()
    }

    pub fn first_diff_region(&self) -> Option<&DiffRegion> {
        self.diff_regions.first()
    }

    pub fn next_diff_region_after(&self, id: usize) -> Option<&DiffRegion> {
        let idx = self.diff_regions.iter().position(|r| r.id == id)?;
        self.diff_regions.get(idx + 1)
    }

    pub fn diff_region_at(&self, x: u32, y: u32) -> Option<&DiffRegion> {
        self.diff_regions
            .iter()
            .find(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_formats_match_enabled_image_features() {
        let formats = supported_image_formats();
        let names: Vec<&str> = formats.iter().map(|format| format.name.as_str()).collect();
        assert_eq!(names[..4], ["PNG", "JPEG", "WebP", "TIFF"]);

        let extensions: Vec<&str> = formats
            .iter()
            .flat_map(|format| format.extensions.iter().map(String::as_str))
            .collect();
        assert!(extensions.contains(&"png"));
        assert!(extensions.contains(&"jpg"));
        assert!(extensions.contains(&"jpeg"));
        assert!(extensions.contains(&"webp"));
        assert!(extensions.contains(&"tif"));
        assert!(extensions.contains(&"tiff"));
        assert!(!extensions.contains(&"bmp"));
        // GIF/HDR/EXR decoders are now enabled for animation + HDR support.
        assert!(extensions.contains(&"gif"));
        assert!(extensions.contains(&"hdr"));
        assert!(extensions.contains(&"exr"));

        #[cfg(feature = "image-avif")]
        {
            assert!(names.contains(&"AVIF"));
            assert!(extensions.contains(&"avif"));
        }
    }

    #[test]
    fn find_diff_regions_single_block() {
        let mut mask = vec![vec![false; 10]; 10];
        for row in mask.iter_mut().take(7).skip(2) {
            for cell in row.iter_mut().take(8).skip(3) {
                *cell = true;
            }
        }
        let regions = find_diff_regions(&mask, 10, 10);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].x, 3);
        assert_eq!(regions[0].y, 2);
        assert_eq!(regions[0].width, 5);
        assert_eq!(regions[0].height, 5);
        assert_eq!(regions[0].pixel_count, 25);
    }

    #[test]
    fn find_diff_regions_two_separate_blocks() {
        let mut mask = vec![vec![false; 20]; 10];
        for row in mask.iter_mut().take(4) {
            for cell in row.iter_mut().take(4) {
                *cell = true;
            }
        }
        for row in mask.iter_mut().take(10).skip(6) {
            for cell in row.iter_mut().take(20).skip(14) {
                *cell = true;
            }
        }
        let regions = find_diff_regions(&mask, 20, 10);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].x, 0);
        assert_eq!(regions[0].y, 0);
        assert_eq!(regions[1].x, 14);
        assert_eq!(regions[1].y, 6);
    }

    #[test]
    fn find_diff_regions_no_differences() {
        let mask = vec![vec![false; 10]; 10];
        let regions = find_diff_regions(&mask, 10, 10);
        assert!(regions.is_empty());
    }

    #[test]
    fn find_diff_regions_keeps_single_pixel_differences() {
        let mut mask = vec![vec![false; 10]; 10];
        mask[0][0] = true;
        let regions = find_diff_regions(&mask, 10, 10);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].x, 0);
        assert_eq!(regions[0].y, 0);
        assert_eq!(regions[0].width, 1);
        assert_eq!(regions[0].height, 1);
        assert_eq!(regions[0].pixel_count, 1);
    }

    #[test]
    fn diff_navigation_methods_work() {
        let result = ImageCompareResult {
            equal: false,
            left_dims: (10, 10),
            right_dims: (10, 10),
            total_pixels: 100,
            differing_pixels: 20,
            diff_ratio: 0.2,
            mode_used: ImageCompareMode::Exact,
            diff_bbox: Some((0, 0, 9, 9)),
            overlay: Vec::new(),
            padded: false,
            color_type_left: None,
            color_type_right: None,
            frame_count: None,
            per_frame_summaries: Vec::new(),
            diff_regions: vec![
                DiffRegion {
                    id: 0,
                    x: 0,
                    y: 0,
                    width: 5,
                    height: 4,
                    pixel_count: 16,
                },
                DiffRegion {
                    id: 1,
                    x: 6,
                    y: 6,
                    width: 4,
                    height: 4,
                    pixel_count: 4,
                },
            ],
        };

        assert_eq!(result.diff_region_count(), 2);
        let first = result.first_diff_region().unwrap();
        assert_eq!(first.id, 0);
        let next = result.next_diff_region_after(0).unwrap();
        assert_eq!(next.id, 1);
        assert!(result.next_diff_region_after(1).is_none());
        assert!(result.diff_region_at(2, 2).is_some());
        assert!(result.diff_region_at(7, 7).is_some());
        assert!(result.diff_region_at(5, 5).is_none());
        assert!(result.next_diff_region_after(99).is_none());
    }

    #[test]
    fn result_json_round_trips_excluding_overlay() {
        let result = ImageCompareResult {
            equal: false,
            left_dims: (12, 8),
            right_dims: (12, 8),
            total_pixels: 96,
            differing_pixels: 7,
            diff_ratio: 7.0 / 96.0,
            mode_used: ImageCompareMode::Tolerance { tolerance: 5 },
            diff_bbox: Some((1, 1, 4, 4)),
            overlay: vec![1, 2, 3, 4], // skipped on serialize
            padded: false,
            color_type_left: None,
            color_type_right: None,
            frame_count: None,
            per_frame_summaries: Vec::new(),
            diff_regions: vec![DiffRegion {
                id: 0,
                x: 1,
                y: 1,
                width: 3,
                height: 3,
                pixel_count: 7,
            }],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("overlay"), "overlay is #[serde(skip)]");
        let back: ImageCompareResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.equal, result.equal);
        assert_eq!(back.left_dims, result.left_dims);
        assert_eq!(back.right_dims, result.right_dims);
        assert_eq!(back.differing_pixels, result.differing_pixels);
        assert_eq!(back.mode_used, result.mode_used);
        assert_eq!(back.diff_bbox, result.diff_bbox);
        assert_eq!(back.diff_regions, result.diff_regions);
        assert!(back.overlay.is_empty(), "overlay defaults to empty on load");
    }

    #[test]
    fn to_html_report_contains_summary_and_regions() {
        let result = ImageCompareResult {
            equal: false,
            left_dims: (12, 8),
            right_dims: (10, 8),
            total_pixels: 96,
            differing_pixels: 7,
            diff_ratio: 7.0 / 96.0,
            mode_used: ImageCompareMode::Tolerance { tolerance: 5 },
            diff_bbox: Some((1, 1, 4, 4)),
            overlay: Vec::new(),
            padded: true,
            color_type_left: None,
            color_type_right: None,
            frame_count: None,
            per_frame_summaries: Vec::new(),
            diff_regions: vec![DiffRegion {
                id: 0,
                x: 1,
                y: 1,
                width: 3,
                height: 3,
                pixel_count: 7,
            }],
        };
        let html = result.to_html_report();
        assert!(html.contains("<html"));
        assert!(html.contains("tolerance (5)"), "mode label rendered");
        assert!(html.contains("different"), "status rendered");
        assert!(html.contains("padded"), "padding noted");
        assert!(html.contains("Diff regions"));
        assert!(
            html.contains("12") && html.contains("8"),
            "dimensions rendered"
        );
    }

    #[test]
    fn diff_regions_serialized_in_result() {
        let result = ImageCompareResult {
            equal: false,
            left_dims: (10, 10),
            right_dims: (10, 10),
            total_pixels: 100,
            differing_pixels: 5,
            diff_ratio: 0.05,
            mode_used: ImageCompareMode::Exact,
            diff_bbox: None,
            overlay: Vec::new(),
            padded: false,
            color_type_left: None,
            color_type_right: None,
            frame_count: None,
            per_frame_summaries: Vec::new(),
            diff_regions: vec![DiffRegion {
                id: 0,
                x: 1,
                y: 2,
                width: 3,
                height: 4,
                pixel_count: 12,
            }],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("diff_regions"));
        assert!(json.contains("\"id\":0"));
        assert!(json.contains("\"x\":1"));
        assert!(json.contains("\"y\":2"));
    }
}
