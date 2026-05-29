// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Response-shape tests for `/compare/webpage` and
// `/compare/webpage/clear-cache`.
//
// These do not exercise real network traffic. They verify that the
// bridge handler enforces its query-parameter contract and that the
// rendered / screenshot modes (which require the web-engine feature)
// surface as a structured JSON error rather than a silent success or
// panic. The QML disables those modes' controls and relies on the
// error shape to drive its disabled state.

use linsync::test_support::temp_app_paths;
use linsync::{webpage_clear_cache_bridge_response, webpage_compare_bridge_response};

#[test]
fn missing_left_param_returns_error_json() {
    let paths = temp_app_paths("webpage-missing-left");
    let body = webpage_compare_bridge_response("right=http://example.com/", &paths);
    let v: serde_json::Value = serde_json::from_str(&body).expect("body is JSON");
    assert!(
        v.get("error").is_some(),
        "missing left → error JSON, got: {body}"
    );
}

#[test]
fn missing_right_param_returns_error_json() {
    let paths = temp_app_paths("webpage-missing-right");
    let body = webpage_compare_bridge_response("left=http://example.com/", &paths);
    let v: serde_json::Value = serde_json::from_str(&body).expect("body is JSON");
    assert!(
        v.get("error").is_some(),
        "missing right → error JSON, got: {body}"
    );
}

#[test]
fn rendered_mode_returns_unsupported_error() {
    // The Webpage Compare page exposes a "Rendered (not implemented
    // yet)" entry whose value is `rendered`. The bridge must surface
    // this as {"error": "unsupported mode: rendered"} so the QML can
    // display the failure rather than appearing to succeed silently.
    let paths = temp_app_paths("webpage-rendered");
    let body = webpage_compare_bridge_response(
        "left=http://example.com/a&right=http://example.com/b&mode=rendered",
        &paths,
    );
    let v: serde_json::Value = serde_json::from_str(&body).expect("body is JSON");
    let err = v
        .get("error")
        .and_then(|e| e.as_str())
        .expect("rendered mode should return an error string");
    assert!(
        err.contains("unsupported") && err.contains("rendered"),
        "expected 'unsupported … rendered' error, got: {err}"
    );
}

#[test]
fn screenshot_mode_returns_unsupported_error() {
    let paths = temp_app_paths("webpage-screenshot");
    let body = webpage_compare_bridge_response(
        "left=http://example.com/a&right=http://example.com/b&mode=screenshot",
        &paths,
    );
    let v: serde_json::Value = serde_json::from_str(&body).expect("body is JSON");
    let err = v
        .get("error")
        .and_then(|e| e.as_str())
        .expect("screenshot mode should return an error string");
    assert!(
        err.contains("unsupported") && err.contains("screenshot"),
        "expected 'unsupported … screenshot' error, got: {err}"
    );
}

#[test]
fn clear_cache_does_not_panic_on_empty_cache() {
    // The /compare/webpage/clear-cache endpoint must be idempotent —
    // calling it when no cache exists must succeed silently.
    let paths = temp_app_paths("webpage-clear-cache-empty");
    let body = webpage_clear_cache_bridge_response(&paths);
    let v: serde_json::Value = serde_json::from_str(&body).expect("body is JSON");
    // The response should be either {"ok": true} or {"cleared": ...}
    // but in all cases must be JSON and must not be an error.
    assert!(
        v.get("error").is_none() || v["error"].is_null(),
        "clear-cache on empty cache should not error, got: {body}"
    );
}
