use image::{ImageBuffer, Rgba, RgbaImage};
use linsync::test_support::image_compare_test;
use linsync::{image_compare_bridge_response_with_profile, image_formats_bridge_response};
use linsync_core::{ImageCompareMode, ImageCompareOptions};
use std::path::PathBuf;
use tempfile::TempDir;

fn save_png(dir: &TempDir, name: &str, img: &RgbaImage) -> PathBuf {
    let path = dir.path().join(name);
    img.save(&path).unwrap();
    path
}

// ── Phase 1: profile-aware route honours the ImageCompareOptions argument ─────
// When the query selects tolerance mode but omits an explicit `?tolerance`, the
// `_with_profile` variant must take the threshold from the resolved profile.
// Two near-identical images (one channel off by 30) straddle a high vs. low
// profile tolerance, proving the profile value flows through (a high tolerance
// → equal, a low tolerance → different).
#[test]
fn with_profile_honours_profile_tolerance_when_query_omits_it() {
    let dir = TempDir::new().unwrap();
    let base: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([100u8, 100, 100, 255]));
    let off: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([130u8, 100, 100, 255])); // red +30
    let left = save_png(&dir, "left.png", &base);
    let right = save_png(&dir, "right.png", &off);

    // No `&tolerance=` in the query → the threshold must come from the profile.
    let query = format!(
        "left={}&right={}&mode=tolerance",
        left.to_str().unwrap(),
        right.to_str().unwrap()
    );

    let high = ImageCompareOptions {
        tolerance: 64,
        ..Default::default()
    };
    let (body, _) = image_compare_bridge_response_with_profile(&query, &high);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        v["equal"],
        serde_json::json!(true),
        "profile tolerance 64 must treat a +30 delta as equal: {body}"
    );

    let low = ImageCompareOptions {
        tolerance: 4,
        ..Default::default()
    };
    let (body_low, _) = image_compare_bridge_response_with_profile(&query, &low);
    let v_low: serde_json::Value = serde_json::from_str(&body_low).unwrap();
    assert_eq!(
        v_low["equal"],
        serde_json::json!(false),
        "profile tolerance 4 must treat a +30 delta as different: {body_low}"
    );
}

#[test]
fn response_mode_reports_effective_profile_mode_when_query_omits_mode() {
    let dir = TempDir::new().unwrap();
    let img: RgbaImage = ImageBuffer::from_fn(4, 4, |_, _| Rgba([12u8, 34, 56, 255]));
    let left = save_png(&dir, "left.png", &img);
    let right = save_png(&dir, "right.png", &img);
    let query = format!(
        "left={}&right={}",
        left.to_str().unwrap(),
        right.to_str().unwrap()
    );
    let profile = ImageCompareOptions {
        mode: ImageCompareMode::Perceptual,
        ..Default::default()
    };

    let (body, _) = image_compare_bridge_response_with_profile(&query, &profile);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["mode"], serde_json::json!("perceptual"));
}

#[test]
fn image_formats_response_matches_compiled_decoder_features() {
    let body = image_formats_bridge_response();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["schema_version"], serde_json::json!(1));

    let labels: Vec<&str> = v["formats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|format| format["name"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"PNG"));
    assert!(labels.contains(&"JPEG"));
    assert!(labels.contains(&"WebP"));
    assert!(labels.contains(&"TIFF"));

    let globs: Vec<&str> = v["extension_globs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|glob| glob.as_str().unwrap())
        .collect();
    assert!(globs.contains(&"*.png"));
    assert!(globs.contains(&"*.jpg"));
    assert!(globs.contains(&"*.jpeg"));
    assert!(globs.contains(&"*.webp"));
    assert!(globs.contains(&"*.tif"));
    assert!(globs.contains(&"*.tiff"));
    assert!(
        !globs.contains(&"*.bmp") && !globs.contains(&"*.gif"),
        "the default build does not enable BMP/GIF decoders, so the UI must not advertise them"
    );
}

#[test]
fn bridge_identical_images_returns_equal_true() {
    let dir = TempDir::new().unwrap();
    let img: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([200u8, 100, 50, 255]));
    let left = save_png(&dir, "left.png", &img);
    let right = save_png(&dir, "right.png", &img);

    let json_resp = image_compare_test(
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "exact",
        0,
        2.3,
        false, // no overlay
    )
    .unwrap();

    let v: serde_json::Value = serde_json::from_str(&json_resp).unwrap();
    assert_eq!(v["equal"], serde_json::json!(true));
    assert_eq!(v["differing_pixels"], serde_json::json!(0));
}

#[test]
fn bridge_different_images_returns_equal_false() {
    let dir = TempDir::new().unwrap();
    let red: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([255u8, 0, 0, 255]));
    let blue: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([0u8, 0, 255, 255]));
    let left = save_png(&dir, "left.png", &red);
    let right = save_png(&dir, "right.png", &blue);

    let json_resp = image_compare_test(
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "exact",
        0,
        2.3,
        false,
    )
    .unwrap();

    let v: serde_json::Value = serde_json::from_str(&json_resp).unwrap();
    assert_eq!(v["equal"], serde_json::json!(false));
    assert!(v["differing_pixels"].as_u64().unwrap() > 0);
}

#[test]
fn bridge_overlay_populated_when_requested() {
    let dir = TempDir::new().unwrap();
    let red: RgbaImage = ImageBuffer::from_fn(4, 4, |_, _| Rgba([255u8, 0, 0, 255]));
    let blue: RgbaImage = ImageBuffer::from_fn(4, 4, |_, _| Rgba([0u8, 0, 255, 255]));
    let left = save_png(&dir, "left.png", &red);
    let right = save_png(&dir, "right.png", &blue);

    let json_resp = image_compare_test(
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "exact",
        0,
        2.3,
        true, // request overlay
    )
    .unwrap();

    let v: serde_json::Value = serde_json::from_str(&json_resp).unwrap();
    // overlay_path is a file:// URI to the temp PNG on disk
    assert!(
        v["overlay_path"]
            .as_str()
            .map(|s| s.starts_with("file://"))
            .unwrap_or(false),
        "overlay_path should be a file:// URI when overlay=true"
    );
}

// The image overlay PNG returned by the bridge must mark differing pixels with
// a non-transparent channel so the GUI can render a meaningful diff layer.
#[test]
fn overlay_png_has_visible_pixels_for_diffs() {
    let dir = TempDir::new().unwrap();
    let red: RgbaImage = ImageBuffer::from_fn(4, 4, |_, _| Rgba([255u8, 0, 0, 255]));
    let blue: RgbaImage = ImageBuffer::from_fn(4, 4, |_, _| Rgba([0u8, 0, 255, 255]));
    let left = save_png(&dir, "left.png", &red);
    let right = save_png(&dir, "right.png", &blue);

    let json_resp = image_compare_test(
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "exact",
        0,
        2.3,
        true,
    )
    .unwrap();

    let v: serde_json::Value = serde_json::from_str(&json_resp).unwrap();
    let uri = v["overlay_path"]
        .as_str()
        .expect("overlay_path should be present");
    let path = uri
        .strip_prefix("file://")
        .expect("overlay_path should be a file:// URI");

    let overlay = image::open(path)
        .expect("overlay PNG should open")
        .to_rgba8();
    let has_visible_pixel = overlay
        .pixels()
        .any(|px| px.0[3] != 0 || px.0[0] != 0 || px.0[1] != 0 || px.0[2] != 0);
    assert!(
        has_visible_pixel,
        "overlay PNG at {path} should mark differing pixels (all-zero buffer is the placeholder)"
    );
}

// The overlay PNG must not be written to a predictable, world-readable path in
// the shared temp dir. It must live in a per-process directory locked to the
// owner (0700), the file itself must be owner-only (0600), and repeated
// overlays in the same process must get distinct, unpredictable names.
#[cfg(unix)]
#[test]
fn overlay_png_is_owner_only_and_unpredictable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let red: RgbaImage = ImageBuffer::from_fn(4, 4, |_, _| Rgba([255u8, 0, 0, 255]));
    let blue: RgbaImage = ImageBuffer::from_fn(4, 4, |_, _| Rgba([0u8, 0, 255, 255]));
    let left = save_png(&dir, "left.png", &red);
    let right = save_png(&dir, "right.png", &blue);

    let overlay_path = |_label: &str| -> PathBuf {
        let json = image_compare_test(
            left.to_str().unwrap(),
            right.to_str().unwrap(),
            "exact",
            0,
            2.3,
            true,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let uri = v["overlay_path"].as_str().expect("overlay_path present");
        PathBuf::from(uri.strip_prefix("file://").expect("file:// URI"))
    };

    let first = overlay_path("first");
    let second = overlay_path("second");

    // Distinct (process-wide counter) so concurrent compares cannot collide and
    // an attacker cannot guess the next name from the previous one.
    assert_ne!(
        first, second,
        "successive overlays must use distinct, unpredictable paths"
    );

    // The containing directory must be a per-process dir locked to 0700.
    let parent = first.parent().expect("overlay has a parent dir");
    assert!(
        parent
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("linsync-overlays-"))
            .unwrap_or(false),
        "overlay must live in a per-process linsync-overlays-<pid> dir, got {}",
        parent.display()
    );
    let dir_mode = std::fs::metadata(parent).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        dir_mode, 0o700,
        "overlay directory must be owner-only (0700), got {dir_mode:o}"
    );

    // The PNG itself must be owner-only (0600) — no group/other read.
    let file_mode = std::fs::metadata(&first).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        file_mode, 0o600,
        "overlay PNG must be owner-only (0600), got {file_mode:o}"
    );
}
