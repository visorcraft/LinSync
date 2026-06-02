use image::{ImageBuffer, Rgba, RgbaImage};
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn cli_bin() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("linsync-cli")
}

fn save_png(dir: &TempDir, name: &str, img: &RgbaImage) -> PathBuf {
    let path = dir.path().join(name);
    img.save(&path).expect("save PNG");
    path
}

#[test]
fn identical_images_exit_0_and_json_equal_true() {
    let dir = TempDir::new().unwrap();
    let img: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([100u8, 150, 200, 255]));
    let left = save_png(&dir, "left.png", &img);
    let right = save_png(&dir, "right.png", &img);

    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "image",
            "--json",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");

    assert_eq!(
        out.status.code(),
        Some(0),
        "exit code must be 0 for equal images"
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["equal"], serde_json::json!(true));
    assert_eq!(json["differing_pixels"], serde_json::json!(0));
}

#[test]
fn different_images_exit_1_and_json_equal_false() {
    let dir = TempDir::new().unwrap();
    let red: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([255u8, 0, 0, 255]));
    let blue: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([0u8, 0, 255, 255]));
    let left = save_png(&dir, "left.png", &red);
    let right = save_png(&dir, "right.png", &blue);

    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "image",
            "--json",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");

    assert_eq!(
        out.status.code(),
        Some(1),
        "exit code must be 1 for different images"
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["equal"], serde_json::json!(false));
    assert!(json["differing_pixels"].as_u64().unwrap() > 0);
}

#[test]
fn tolerance_mode_via_cli_flag() {
    let dir = TempDir::new().unwrap();
    let base: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([100u8, 100, 100, 255]));
    let nudged: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([101u8, 100, 100, 255]));
    let left = save_png(&dir, "left.png", &base);
    let right = save_png(&dir, "right.png", &nudged);

    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "image",
            "--image-mode",
            "tolerance",
            "--image-tolerance",
            "2",
            "--json",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");

    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["equal"], serde_json::json!(true));
}

#[test]
fn save_result_then_report_from_json_round_trips() {
    let dir = TempDir::new().unwrap();
    let red: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([255u8, 0, 0, 255]));
    let blue: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([0u8, 0, 255, 255]));
    let left = save_png(&dir, "left.png", &red);
    let right = save_png(&dir, "right.png", &blue);
    let saved = dir.path().join("result.json");
    let report = dir.path().join("report.html");

    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "image",
            "--quiet",
            "--save-result",
            saved.to_str().unwrap(),
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");
    assert_eq!(out.status.code(), Some(1), "different images exit 1");
    let envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&saved).unwrap()).expect("valid envelope JSON");
    assert_eq!(envelope["kind"], serde_json::json!("image"));
    assert_eq!(envelope["schema_version"], serde_json::json!(1));
    assert_eq!(envelope["result"]["equal"], serde_json::json!(false));

    let report_out = Command::new(cli_bin())
        .args([
            "report",
            "--from-json",
            saved.to_str().unwrap(),
            "--output",
            report.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli report");
    assert_eq!(
        report_out.status.code(),
        Some(1),
        "report exit mirrors result equality (different => 1)"
    );
    let html = std::fs::read_to_string(&report).expect("report written");
    assert!(html.contains("LinSync image report"));
    assert!(html.contains("different"));
}

#[test]
fn image_frames_flag_is_accepted() {
    // --image-frames must parse; on a still PNG, AllFrames reports a single
    // frame and equal images still exit 0.
    let dir = TempDir::new().unwrap();
    let img: RgbaImage = ImageBuffer::from_fn(8, 8, |_, _| Rgba([10u8, 20, 30, 255]));
    let left = save_png(&dir, "left.png", &img);
    let right = save_png(&dir, "right.png", &img);
    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "image",
            "--image-frames",
            "all",
            "--json",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");
    assert_eq!(out.status.code(), Some(0), "equal still images exit 0");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["equal"], serde_json::json!(true));
    assert_eq!(json["frame_count"], serde_json::json!(1));

    // An unknown value is a usage error.
    let bad = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "image",
            "--image-frames",
            "nope",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn missing_file_exits_2() {
    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "image",
            "nonexistent_left.png",
            "nonexistent_right.png",
        ])
        .output()
        .expect("run linsync-cli");
    assert_eq!(out.status.code(), Some(2));
}
