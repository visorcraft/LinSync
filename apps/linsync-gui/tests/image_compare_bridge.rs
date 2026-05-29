use image::{ImageBuffer, Rgba, RgbaImage};
use linsync::test_support::image_compare_test;
use std::path::PathBuf;
use tempfile::TempDir;

fn save_png(dir: &TempDir, name: &str, img: &RgbaImage) -> PathBuf {
    let path = dir.path().join(name);
    img.save(&path).unwrap();
    path
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

// ── Phase 0 drift regression ─────────────────────────────────────────────────
// The image overlay PNG returned by the bridge is currently a transparent
// placeholder (apps/linsync-gui/src/lib.rs::build_overlay_png allocates a
// fully-zero RGBA buffer). When the contract is fixed (PLAN.md Phase 5
// "Image"), the overlay must mark differing pixels with a non-transparent
// channel so the GUI can render a meaningful diff layer.
#[test]
#[ignore = "drift: build_overlay_png returns a transparent placeholder (PLAN.md Phase 5 Image)"]
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
