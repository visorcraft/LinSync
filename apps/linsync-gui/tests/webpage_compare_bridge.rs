// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Response-shape tests for `/compare/webpage` and
// `/compare/webpage/clear-cache`.
//
// These do not exercise real network traffic. They verify that the
// bridge handler enforces its query-parameter contract and that direct
// rendered / screenshot requests (which require a real web-engine path)
// surface as a structured JSON error rather than a silent success or
// panic. The default-build QML does not offer those modes.

use linsync::test_support::temp_app_paths;
use linsync::{
    resolve_webpage_options, webpage_clear_cache_bridge_response, webpage_compare_bridge_response,
};
use linsync_core::WebpageCompareOptions;

/// Build a `WebpageCompareOptions` standing in for a resolved profile.
/// `confirmed_by_user` is set to `false` on purpose so the tests can prove
/// the bridge always re-asserts consent regardless of the profile value.
fn profile_options(
    depth: u8,
    timeout: u32,
    max_requests: u32,
    user_agent: Option<&str>,
) -> WebpageCompareOptions {
    WebpageCompareOptions {
        resource_tree_depth: depth,
        timeout_secs: timeout,
        max_requests,
        user_agent: user_agent.map(str::to_owned),
        confirmed_by_user: false,
    }
}

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
    // The bridge still guards direct or stale callers even though the
    // default-build QML no longer offers this mode.
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

// ── Phase 1: profile resolution + per-request query overrides ────────────────
// `resolve_webpage_options` proves the /compare/webpage handler honours the
// resolved profile's fetch controls and lets per-request query params override
// them field-by-field, mirroring the CLI flags and the image/document routes.

#[test]
fn resolve_webpage_options_inherits_profile_when_query_omits_overrides() {
    // Only left/right/mode in the query — depth/timeout/max_requests/user_agent
    // must all come from the resolved profile.
    let profile = profile_options(2, 45, 99, Some("Profile-UA/1.0"));
    let got = resolve_webpage_options("left=http://a/&right=http://b/&mode=tree", &profile);
    assert_eq!(got.resource_tree_depth, 2);
    assert_eq!(got.timeout_secs, 45);
    assert_eq!(got.max_requests, 99);
    assert_eq!(got.user_agent.as_deref(), Some("Profile-UA/1.0"));
    // The consent gate is always re-asserted by the bridge, never sourced from
    // the profile (which here ships `confirmed_by_user: false`).
    assert!(
        got.confirmed_by_user,
        "bridge must force confirmed_by_user = true regardless of the profile"
    );
}

#[test]
fn resolve_webpage_options_query_overrides_win_over_profile() {
    let profile = profile_options(1, 30, 50, None);
    let got = resolve_webpage_options(
        "left=http://a/&right=http://b/&mode=tree\
         &depth=3&timeout=10&max_requests=7&user_agent=QA%2FBot%209",
        &profile,
    );
    assert_eq!(got.resource_tree_depth, 3, "?depth overrides profile");
    assert_eq!(got.timeout_secs, 10, "?timeout overrides profile");
    assert_eq!(got.max_requests, 7, "?max_requests overrides profile");
    assert_eq!(
        got.user_agent.as_deref(),
        Some("QA/Bot 9"),
        "?user_agent overrides profile and is percent-decoded"
    );
}

#[test]
fn resolve_webpage_options_clamps_depth_to_one_through_three() {
    let profile = profile_options(1, 30, 50, None);
    let over = resolve_webpage_options("depth=9", &profile);
    assert_eq!(over.resource_tree_depth, 3, "?depth clamps to max 3");
    let under = resolve_webpage_options("depth=0", &profile);
    assert_eq!(under.resource_tree_depth, 1, "?depth clamps to min 1");
}
