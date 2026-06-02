// All tests in this file require the `image-compare` feature.

#[cfg(feature = "image-compare")]
mod image_compare_tests {
    use ::image::{ImageBuffer, Rgba, RgbaImage};
    use linsync_core::image::{
        FrameCompareMode, ImageCompareError, ImageCompareMode, ImageCompareOptions,
        ImageCompareResult, compare_images, compare_images_streaming,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn save_png(dir: &TempDir, name: &str, img: &RgbaImage) -> PathBuf {
        let path = dir.path().join(name);
        img.save(&path).expect("save PNG");
        path
    }

    fn solid(r: u8, g: u8, b: u8, a: u8) -> RgbaImage {
        ImageBuffer::from_fn(8, 8, |_, _| Rgba([r, g, b, a]))
    }

    fn one_pixel_different() -> (RgbaImage, RgbaImage) {
        let base: RgbaImage = ImageBuffer::from_fn(16, 16, |_, _| Rgba([200, 200, 200, 255]));
        let mut modified = base.clone();
        modified.put_pixel(7, 7, Rgba([0, 0, 0, 255]));
        (base, modified)
    }

    fn gradient_pair(shift: u8) -> (RgbaImage, RgbaImage) {
        let width = 64u32;
        let height = 4u32;
        let left: RgbaImage = ImageBuffer::from_fn(width, height, |x, _| {
            Rgba([(x * 4).min(255) as u8, 128, 128, 255])
        });
        let right: RgbaImage = ImageBuffer::from_fn(width, height, |x, _| {
            let r = ((x * 4) as i32 + shift as i32).clamp(0, 255) as u8;
            Rgba([r, 128, 128, 255])
        });
        (left, right)
    }

    #[test]
    fn exact_mode_is_default() {
        let opts = ImageCompareOptions::default();
        assert!(matches!(opts.mode, ImageCompareMode::Exact));
        assert_eq!(opts.tolerance, 0);
        assert!((opts.delta_e_threshold - 2.3_f32).abs() < 0.001);
        assert!(!opts.trust_extension_fallback);
        assert_eq!(opts.stream_stripe_rows, 64);
    }

    #[test]
    fn tolerance_mode_builder_round_trip() {
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Tolerance { tolerance: 10 },
            tolerance: 10,
            ..ImageCompareOptions::default()
        };
        assert!(matches!(
            opts.mode,
            ImageCompareMode::Tolerance { tolerance: 10 }
        ));
    }

    #[test]
    fn perceptual_mode_builder_round_trip() {
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Perceptual,
            delta_e_threshold: 5.0,
            ..ImageCompareOptions::default()
        };
        assert!(matches!(opts.mode, ImageCompareMode::Perceptual));
        assert!((opts.delta_e_threshold - 5.0_f32).abs() < 0.001);
    }

    #[test]
    fn result_equal_when_zero_mismatched() {
        let result = ImageCompareResult {
            equal: true,
            left_dims: (4, 4),
            right_dims: (4, 4),
            total_pixels: 16,
            differing_pixels: 0,
            diff_ratio: 0.0,
            mode_used: ImageCompareMode::Exact,
            diff_bbox: None,
            overlay: Vec::new(),
            padded: false,
            diff_regions: Vec::new(),
            color_type_left: None,
            color_type_right: None,
            frame_count: None,
            per_frame_summaries: Vec::new(),
        };
        assert!(result.equal);
        assert_eq!(result.differing_pixels, 0);
        assert!(result.diff_bbox.is_none());
    }

    #[test]
    fn error_variants_are_distinct() {
        let dim = ImageCompareError::DimensionMismatch {
            left: (1, 2),
            right: (3, 4),
        };
        let fmt = ImageCompareError::UnsupportedFormat("bmp".into());
        let io = ImageCompareError::IoError("not found".into());
        let dec = ImageCompareError::DecodeError("bad header".into());
        assert!(matches!(dim, ImageCompareError::DimensionMismatch { .. }));
        assert!(matches!(fmt, ImageCompareError::UnsupportedFormat(_)));
        assert!(matches!(io, ImageCompareError::IoError(_)));
        assert!(matches!(dec, ImageCompareError::DecodeError(_)));
    }

    #[test]
    fn exact_identical_images_equal() {
        let dir = TempDir::new().unwrap();
        let img = solid(100, 150, 200, 255);
        let left = save_png(&dir, "left.png", &img);
        let right = save_png(&dir, "right.png", &img);
        let opts = ImageCompareOptions::default();
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(result.equal);
        assert_eq!(result.differing_pixels, 0);
        assert!(result.diff_bbox.is_none());
    }

    #[test]
    fn exact_different_images_all_pixels_differ() {
        let dir = TempDir::new().unwrap();
        let red = solid(255, 0, 0, 255);
        let blue = solid(0, 0, 255, 255);
        let left = save_png(&dir, "left.png", &red);
        let right = save_png(&dir, "right.png", &blue);
        let opts = ImageCompareOptions::default();
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(!result.equal);
        assert_eq!(result.differing_pixels, 8 * 8);
        assert_eq!(result.total_pixels, 8 * 8);
        assert!(result.diff_bbox.is_some());
    }

    #[test]
    fn exact_one_pixel_different_bbox_matches() {
        let dir = TempDir::new().unwrap();
        let (base, modified) = one_pixel_different();
        let left = save_png(&dir, "left.png", &base);
        let right = save_png(&dir, "right.png", &modified);
        let opts = ImageCompareOptions::default();
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(!result.equal);
        assert_eq!(result.differing_pixels, 1);
        assert_eq!(result.diff_bbox, Some((7, 7, 7, 7)));
    }

    #[test]
    fn exact_dimension_mismatch_pads_and_compares() {
        let dir = TempDir::new().unwrap();
        let small: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([0u8, 0, 0, 255]));
        let large: RgbaImage = ImageBuffer::from_fn(16, 16, |_, _| Rgba([0u8, 0, 0, 255]));
        let left = save_png(&dir, "left.png", &small);
        let right = save_png(&dir, "right.png", &large);
        let opts = ImageCompareOptions::default();
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(
            !result.equal,
            "padded images with different dims should not be equal"
        );
        assert!(result.padded);
        assert_eq!(result.left_dims, (8, 8));
        assert_eq!(result.right_dims, (16, 16));
        assert!(
            result.differing_pixels > 0,
            "padded region should count as differing pixels"
        );
    }

    #[test]
    fn magic_byte_detection_png_with_jpg_extension() {
        let dir = TempDir::new().unwrap();
        let img = solid(10, 20, 30, 255);
        let png_path = dir.path().join("actual.png");
        img.save(&png_path).unwrap();
        let png_bytes = std::fs::read(&png_path).unwrap();
        let disguised = dir.path().join("disguised.jpg");
        std::fs::write(&disguised, &png_bytes).unwrap();
        let opts = ImageCompareOptions::default();
        let result = compare_images(&png_path, &disguised, &opts).unwrap();
        assert!(result.equal);
    }

    #[test]
    fn tolerance_zero_behaves_like_exact() {
        let dir = TempDir::new().unwrap();
        let red = solid(255, 0, 0, 255);
        let slightly_off: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([254, 0, 0, 255]));
        let left = save_png(&dir, "left.png", &red);
        let right = save_png(&dir, "right.png", &slightly_off);
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Tolerance { tolerance: 0 },
            ..ImageCompareOptions::default()
        };
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(!result.equal);
        assert_eq!(result.differing_pixels, 8 * 8);
    }

    #[test]
    fn tolerance_one_accepts_off_by_one() {
        let dir = TempDir::new().unwrap();
        let red = solid(255, 0, 0, 255);
        let slightly_off: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([254, 0, 0, 255]));
        let left = save_png(&dir, "left.png", &red);
        let right = save_png(&dir, "right.png", &slightly_off);
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Tolerance { tolerance: 1 },
            tolerance: 1,
            ..ImageCompareOptions::default()
        };
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(result.equal, "off-by-one should be equal at tolerance=1");
        assert_eq!(result.differing_pixels, 0);
    }

    #[test]
    fn tolerance_channel_independent_any_channel_must_exceed() {
        let dir = TempDir::new().unwrap();
        let base: RgbaImage = ImageBuffer::from_fn(4, 4, |_, _| Rgba([100u8, 100, 100, 255]));
        let shifted: RgbaImage = ImageBuffer::from_fn(4, 4, |_, _| Rgba([100u8, 100, 105, 255]));
        let left = save_png(&dir, "left.png", &base);
        let right = save_png(&dir, "right.png", &shifted);

        let opts_strict = ImageCompareOptions {
            mode: ImageCompareMode::Tolerance { tolerance: 4 },
            tolerance: 4,
            ..ImageCompareOptions::default()
        };
        let strict = compare_images(&left, &right, &opts_strict).unwrap();
        assert!(!strict.equal, "delta=5 exceeds tolerance=4");

        let opts_loose = ImageCompareOptions {
            mode: ImageCompareMode::Tolerance { tolerance: 5 },
            tolerance: 5,
            ..ImageCompareOptions::default()
        };
        let loose = compare_images(&left, &right, &opts_loose).unwrap();
        assert!(loose.equal, "delta=5 is within tolerance=5");
    }

    #[test]
    fn tolerance_255_accepts_any_difference() {
        let dir = TempDir::new().unwrap();
        let black = solid(0, 0, 0, 255);
        let white = solid(255, 255, 255, 255);
        let left = save_png(&dir, "left.png", &black);
        let right = save_png(&dir, "right.png", &white);
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Tolerance { tolerance: 255 },
            tolerance: 255,
            ..ImageCompareOptions::default()
        };
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(result.equal, "tolerance=255 accepts any pixel value");
    }

    #[test]
    fn perceptual_identical_images_equal() {
        let dir = TempDir::new().unwrap();
        let img = solid(80, 120, 160, 255);
        let left = save_png(&dir, "left.png", &img);
        let right = save_png(&dir, "right.png", &img);
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Perceptual,
            delta_e_threshold: 2.3,
            ..ImageCompareOptions::default()
        };
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(result.equal);
        assert_eq!(result.differing_pixels, 0);
    }

    #[test]
    fn perceptual_large_shift_detected_above_threshold() {
        let dir = TempDir::new().unwrap();
        let (left_img, right_img) = gradient_pair(40);
        let left = save_png(&dir, "left.png", &left_img);
        let right = save_png(&dir, "right.png", &right_img);
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Perceptual,
            delta_e_threshold: 2.3,
            ..ImageCompareOptions::default()
        };
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(!result.equal);
        assert!(
            result.differing_pixels > 0,
            "large shift must produce differing pixels"
        );
    }

    #[test]
    fn perceptual_tiny_shift_within_jnd_threshold() {
        let dir = TempDir::new().unwrap();
        let base: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([128u8, 128, 128, 255]));
        let nudged: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([129u8, 128, 128, 255]));
        let left = save_png(&dir, "left.png", &base);
        let right = save_png(&dir, "right.png", &nudged);
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Perceptual,
            delta_e_threshold: 2.3,
            ..ImageCompareOptions::default()
        };
        let result = compare_images(&left, &right, &opts).unwrap();
        assert!(
            result.equal,
            "1-unit shift in mid-grey should be below JND; differing={}",
            result.differing_pixels
        );
    }

    #[test]
    fn streaming_large_synthetic_image_exact_equal() {
        let dir = TempDir::new().unwrap();
        let img: RgbaImage = ImageBuffer::from_fn(512, 512, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        });
        let left = save_png(&dir, "left.png", &img);
        let right = save_png(&dir, "right.png", &img);

        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Exact,
            stream_stripe_rows: 16,
            ..ImageCompareOptions::default()
        };
        let result = compare_images_streaming(&left, &right, &opts).unwrap();
        assert!(result.equal);
        assert_eq!(result.differing_pixels, 0);
    }

    #[test]
    fn streaming_detects_single_pixel_diff() {
        let dir = TempDir::new().unwrap();
        let base: RgbaImage = ImageBuffer::from_fn(128, 128, |_, _| Rgba([50u8, 50, 50, 255]));
        let mut modified = base.clone();
        modified.put_pixel(63, 63, Rgba([200, 200, 200, 255]));
        let left = save_png(&dir, "left.png", &base);
        let right = save_png(&dir, "right.png", &modified);
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Exact,
            stream_stripe_rows: 32,
            ..ImageCompareOptions::default()
        };
        let result = compare_images_streaming(&left, &right, &opts).unwrap();
        assert!(!result.equal);
        assert_eq!(result.differing_pixels, 1);
        assert_eq!(result.diff_bbox, Some((63, 63, 63, 63)));
    }

    #[test]
    fn frame_mode_defaults_to_first_frame() {
        // Backward compatibility: existing callers/profiles see FirstFrame.
        assert_eq!(
            ImageCompareOptions::default().frame_mode,
            FrameCompareMode::FirstFrame
        );
    }

    #[test]
    fn color_type_metadata_is_reported() {
        let dir = TempDir::new().unwrap();
        let img: RgbaImage = ImageBuffer::from_pixel(4, 4, Rgba([1, 2, 3, 255]));
        let left = save_png(&dir, "l.png", &img);
        let right = save_png(&dir, "r.png", &img);
        let result = compare_images(&left, &right, &ImageCompareOptions::default()).unwrap();
        assert_eq!(result.color_type_left.as_deref(), Some("Rgba8"));
        assert_eq!(result.color_type_right.as_deref(), Some("Rgba8"));
    }

    fn write_two_frame_gif(dir: &TempDir, name: &str, f0: &RgbaImage, f1: &RgbaImage) -> PathBuf {
        use ::image::codecs::gif::GifEncoder;
        use ::image::{Delay, Frame};
        let path = dir.path().join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = GifEncoder::new(file);
        let delay = Delay::from_numer_denom_ms(100, 1);
        encoder
            .encode_frame(Frame::from_parts(f0.clone(), 0, 0, delay))
            .unwrap();
        encoder
            .encode_frame(Frame::from_parts(f1.clone(), 0, 0, delay))
            .unwrap();
        drop(encoder);
        path
    }

    #[test]
    fn all_frames_mode_compares_every_frame() {
        let dir = TempDir::new().unwrap();
        let red: RgbaImage = ImageBuffer::from_pixel(8, 8, Rgba([255, 0, 0, 255]));
        let green: RgbaImage = ImageBuffer::from_pixel(8, 8, Rgba([0, 255, 0, 255]));
        let blue: RgbaImage = ImageBuffer::from_pixel(8, 8, Rgba([0, 0, 255, 255]));
        // Both share frame 0 (red); frame 1 differs (green vs blue).
        let left = write_two_frame_gif(&dir, "left.gif", &red, &green);
        let right = write_two_frame_gif(&dir, "right.gif", &red, &blue);

        // FirstFrame (default) only looks at frame 0 → equal, no per-frame data.
        let first = compare_images(&left, &right, &ImageCompareOptions::default()).unwrap();
        assert!(first.equal, "the first frames are identical");
        assert!(first.per_frame_summaries.is_empty());
        assert_eq!(first.frame_count, None);

        // AllFrames compares both frames → differs, with a per-frame breakdown.
        let opts = ImageCompareOptions {
            frame_mode: FrameCompareMode::AllFrames,
            ..ImageCompareOptions::default()
        };
        let all = compare_images(&left, &right, &opts).unwrap();
        assert_eq!(all.frame_count, Some(2), "two frames per side");
        assert_eq!(all.per_frame_summaries.len(), 2);
        assert!(all.per_frame_summaries[0].equal, "frame 0 (red) matches");
        assert!(
            !all.per_frame_summaries[1].equal,
            "frame 1 (green vs blue) differs"
        );
        assert!(!all.equal, "overall different because a frame differs");
    }

    #[test]
    fn all_frames_flags_frame_count_mismatch() {
        let dir = TempDir::new().unwrap();
        let red: RgbaImage = ImageBuffer::from_pixel(8, 8, Rgba([255, 0, 0, 255]));
        let green: RgbaImage = ImageBuffer::from_pixel(8, 8, Rgba([0, 255, 0, 255]));
        let two_frame = write_two_frame_gif(&dir, "two.gif", &red, &green);
        // A single-frame PNG vs a two-frame GIF: frame 1 is one-sided.
        let single = save_png(&dir, "single.png", &red);
        let opts = ImageCompareOptions {
            frame_mode: FrameCompareMode::AllFrames,
            ..ImageCompareOptions::default()
        };
        let result = compare_images(&single, &two_frame, &opts).unwrap();
        assert_eq!(result.frame_count, Some(2));
        assert!(
            result.per_frame_summaries[1].one_sided,
            "extra frame is one-sided"
        );
        // The one-sided frame's full pixel count is counted as differing (8x8),
        // and folded into the aggregate rather than silently dropped.
        assert_eq!(
            result.per_frame_summaries[1].differing_pixels, 64,
            "a one-sided 8x8 frame counts all 64 pixels as differing"
        );
        assert!(
            result.differing_pixels >= 64,
            "the one-sided frame's pixels are included in the aggregate"
        );
        assert!(!result.equal);
    }

    #[test]
    fn hdr_files_decode_and_report_float_color_type() {
        use ::image::{DynamicImage, ImageBuffer, Rgb};
        let dir = TempDir::new().unwrap();
        // Two identical Radiance-HDR images (32-bit float) — exercises the newly
        // enabled `hdr` decoder and the color-type metadata for an HDR source.
        let make = |name: &str| {
            let buf: ImageBuffer<Rgb<f32>, Vec<f32>> =
                ImageBuffer::from_pixel(4, 4, Rgb([0.5f32, 0.25, 0.125]));
            let path = dir.path().join(name);
            DynamicImage::ImageRgb32F(buf)
                .save(&path)
                .expect("encode .hdr");
            path
        };
        let left = make("a.hdr");
        let right = make("b.hdr");
        let result = compare_images(&left, &right, &ImageCompareOptions::default()).unwrap();
        assert!(result.equal, "identical HDR images compare equal");
        let ct = result.color_type_left.as_deref().unwrap_or("");
        assert!(
            ct.contains("32F"),
            "HDR decodes to a 32-bit float color type, got {ct:?}"
        );
    }

    #[test]
    fn all_frames_dimension_mismatch_marks_padded() {
        // Two 2-frame animations of different sizes (GIF coalesces every frame
        // to the logical-screen size, so per-frame dims are uniform within a
        // file): AllFrames must report padded=true and not-equal. This also
        // covers the per-frame-pair padding path feeding the overall flag.
        let dir = TempDir::new().unwrap();
        let small_a: RgbaImage = ImageBuffer::from_pixel(8, 8, Rgba([10, 20, 30, 255]));
        let small_b: RgbaImage = ImageBuffer::from_pixel(8, 8, Rgba([40, 50, 60, 255]));
        let big_a: RgbaImage = ImageBuffer::from_pixel(12, 12, Rgba([10, 20, 30, 255]));
        let big_b: RgbaImage = ImageBuffer::from_pixel(12, 12, Rgba([40, 50, 60, 255]));
        let left = write_two_frame_gif(&dir, "left.gif", &small_a, &small_b);
        let right = write_two_frame_gif(&dir, "right.gif", &big_a, &big_b);

        let opts = ImageCompareOptions {
            frame_mode: FrameCompareMode::AllFrames,
            ..ImageCompareOptions::default()
        };
        let result = compare_images(&left, &right, &opts).unwrap();
        assert_eq!(result.frame_count, Some(2));
        assert!(
            result.padded,
            "different animation dimensions must set padded=true"
        );
        assert!(!result.equal, "size-mismatched frames make the pair differ");
    }
}
