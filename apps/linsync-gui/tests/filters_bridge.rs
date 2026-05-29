// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

use linsync::test_support::{
    delete_filter, list_filters, load_walk_options, migrate_filter, save_filter, set_walk_option,
    temp_app_paths, validate_filter_err,
};
use linsync_core::FilterParseErrorKind;

// ─── Walk-option round-trips ──────────────────────────────────────────────────

#[test]
fn walk_option_respect_gitignore_round_trip() {
    let paths = temp_app_paths("walk-gitignore");
    // Default is true; flip it to false and confirm persistence.
    let saved = set_walk_option(&paths, "respect_gitignore", "false")
        .expect("respect_gitignore set should succeed");
    assert!(!saved.respect_gitignore, "in-memory value should be false");
    let loaded = load_walk_options(&paths);
    assert!(
        !loaded.respect_gitignore,
        "respect_gitignore should be false after reload"
    );
}

#[test]
fn walk_option_follow_symlinks_round_trip() {
    let paths = temp_app_paths("walk-symlinks");
    let saved = set_walk_option(&paths, "follow_symlinks", "true")
        .expect("follow_symlinks set should succeed");
    assert!(saved.follow_symlinks, "in-memory value should be true");
    let loaded = load_walk_options(&paths);
    assert!(
        loaded.follow_symlinks,
        "follow_symlinks should be true after reload"
    );
}

#[test]
fn walk_option_max_depth_round_trip() {
    let paths = temp_app_paths("walk-maxdepth");
    let saved =
        set_walk_option(&paths, "max_walk_depth", "7").expect("max_walk_depth set should succeed");
    assert_eq!(saved.max_walk_depth, 7, "in-memory value should be 7");
    let loaded = load_walk_options(&paths);
    assert_eq!(
        loaded.max_walk_depth, 7,
        "max_walk_depth should be 7 after reload"
    );
}

#[test]
fn walk_option_includes_and_excludes_round_trip() {
    let paths = temp_app_paths("walk-lists");
    set_walk_option(&paths, "includes", "*.rs,*.toml").expect("includes set should succeed");
    set_walk_option(&paths, "excludes", "target/**,node_modules/**")
        .expect("excludes set should succeed");
    let loaded = load_walk_options(&paths);
    assert!(
        loaded.session_includes.contains(&"*.rs".to_owned()),
        "includes should contain *.rs"
    );
    assert!(
        loaded.session_includes.contains(&"*.toml".to_owned()),
        "includes should contain *.toml"
    );
    assert!(
        loaded.session_excludes.contains(&"target/**".to_owned()),
        "excludes should contain target/**"
    );
    assert!(
        loaded
            .session_excludes
            .contains(&"node_modules/**".to_owned()),
        "excludes should contain node_modules/**"
    );
}

// ─── Filter save / list round-trip ───────────────────────────────────────────

#[test]
fn save_named_filter_round_trip() {
    let paths = temp_app_paths("filter-save");
    let body = "name: Rust Sources\nwf:*.rs\nwf:*.toml";
    let saved = save_filter(&paths, body).expect("save should succeed");
    assert!(
        saved
            .filters
            .iter()
            .any(|f| f.name.as_deref() == Some("Rust Sources")),
        "saved list should contain 'Rust Sources', got: {saved:?}"
    );

    // Reload from disk and verify persistence.
    let listed = list_filters(&paths);
    assert!(
        listed
            .filters
            .iter()
            .any(|f| f.name.as_deref() == Some("Rust Sources")),
        "listed filters should contain 'Rust Sources', got: {listed:?}"
    );
}

#[test]
fn save_multiple_named_filters_round_trip() {
    let paths = temp_app_paths("filter-save-multi");
    save_filter(&paths, "name: Rust\nwf:*.rs").expect("save Rust should succeed");
    save_filter(&paths, "name: Config\nwf:*.toml\nwf:*.yaml").expect("save Config should succeed");

    let listed = list_filters(&paths);
    assert!(
        listed
            .filters
            .iter()
            .any(|f| f.name.as_deref() == Some("Rust")),
        "should contain 'Rust' filter"
    );
    assert!(
        listed
            .filters
            .iter()
            .any(|f| f.name.as_deref() == Some("Config")),
        "should contain 'Config' filter"
    );
}

// ─── Filter delete ────────────────────────────────────────────────────────────

#[test]
fn delete_named_filter() {
    let paths = temp_app_paths("filter-delete");
    save_filter(&paths, "name: TempFilter\nwf:*.tmp").expect("save should succeed");
    save_filter(&paths, "name: Keeper\nwf:*.log").expect("save keeper should succeed");

    let after_delete = delete_filter(&paths, "TempFilter").expect("delete should succeed");
    assert!(
        !after_delete
            .filters
            .iter()
            .any(|f| f.name.as_deref() == Some("TempFilter")),
        "'TempFilter' should be gone after delete"
    );
    assert!(
        after_delete
            .filters
            .iter()
            .any(|f| f.name.as_deref() == Some("Keeper")),
        "'Keeper' should survive the delete"
    );

    // Confirm persistence via re-load.
    let listed = list_filters(&paths);
    assert!(
        !listed
            .filters
            .iter()
            .any(|f| f.name.as_deref() == Some("TempFilter")),
        "'TempFilter' should be gone after reload"
    );
}

// ─── Filter validation ────────────────────────────────────────────────────────

#[test]
fn validate_valid_filter_expression_succeeds() {
    // A well-formed filter (no name header needed for validate-only path).
    let result = validate_filter_err("wf:*.rs");
    assert!(
        result.is_ok(),
        "valid expression should parse, got: {result:?}"
    );
}

#[test]
fn validate_returns_parse_error_for_bad_expression() {
    // An expression with an unknown attribute → InvalidExpression.
    let err = validate_filter_err("fe:chocolate > 1")
        .expect_err("invalid expression should fail to parse");
    assert_eq!(
        err.kind,
        FilterParseErrorKind::InvalidExpression,
        "expected InvalidExpression, got: {:?}",
        err.kind
    );
}

#[test]
fn validate_returns_windows_metadata_hint_for_legacy_prefix() {
    // Windows-only metadata prefix → UnsupportedWindowsMetadata (migration hint).
    let err =
        validate_filter_err("attr: archive").expect_err("Windows-only prefix should fail to parse");
    assert_eq!(
        err.kind,
        FilterParseErrorKind::UnsupportedWindowsMetadata,
        "expected UnsupportedWindowsMetadata, got: {:?}",
        err.kind
    );
    assert!(
        err.is_migration_hint(),
        "error should be flagged as a migration hint"
    );
}

#[test]
fn validate_returns_error_for_unknown_prefix() {
    let err = validate_filter_err("zzz:pattern").expect_err("unknown prefix should fail to parse");
    assert_eq!(
        err.kind,
        FilterParseErrorKind::UnknownPrefix,
        "expected UnknownPrefix, got: {:?}",
        err.kind
    );
}

#[test]
fn save_filter_without_name_header_returns_error() {
    let paths = temp_app_paths("filter-no-name");
    // No `name:` header — save_filter should reject it.
    let result = save_filter(&paths, "wf:*.rs");
    assert!(result.is_err(), "missing name header should be rejected");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("name"),
        "error should mention 'name', got: {msg}"
    );
}

// ─── Legacy .flt migration ────────────────────────────────────────────────────

#[test]
fn migrate_filter_preserves_supported_prefixes() {
    let input = "name: Rust\nwf:*.rs\nwd!:target\n";
    let result = migrate_filter(input);
    assert!(
        result.migrated.contains("wf:*.rs"),
        "wf: line should be preserved; got: {}",
        result.migrated
    );
    assert!(
        result.migrated.contains("wd!:target"),
        "wd!: line should be preserved; got: {}",
        result.migrated
    );
    assert!(
        result.warnings.is_empty(),
        "no warnings expected for supported prefixes; got: {:?}",
        result.warnings
    );
}

#[test]
fn migrate_filter_comments_out_unsupported_windows_prefixes() {
    let input = "attr: archive\ndos: hidden\nshell: preview\nversion: 1.0\n";
    let result = migrate_filter(input);
    assert!(
        result.migrated.contains("# UNSUPPORTED: attr: archive"),
        "attr: should be commented out; got: {}",
        result.migrated
    );
    assert!(
        result.migrated.contains("# UNSUPPORTED: dos: hidden"),
        "dos: should be commented out; got: {}",
        result.migrated
    );
    assert_eq!(
        result.warnings.len(),
        4,
        "expected 4 warnings (one per unsupported line); got: {:?}",
        result.warnings
    );
}

#[test]
fn migrate_filter_rewrites_ctime_to_mtime() {
    let input = "ctime: > '2020-01-01'\n";
    let result = migrate_filter(input);
    assert!(
        result.migrated.contains("e: mtime"),
        "ctime: should be migrated to e: mtime; got: {}",
        result.migrated
    );
    assert!(
        result.migrated.contains("# migrated from ctime"),
        "migration comment should be present; got: {}",
        result.migrated
    );
    // ctime → mtime migration is not lossy-enough to warrant a warning.
    assert!(
        result.warnings.is_empty(),
        "no warnings expected for ctime→mtime migration; got: {:?}",
        result.warnings
    );
}

#[test]
fn migrate_filter_comments_and_blank_lines_are_preserved() {
    let input = "# This is a comment\n\nwf:*.toml\n";
    let result = migrate_filter(input);
    assert!(
        result.migrated.contains("# This is a comment"),
        "comment should be preserved; got: {}",
        result.migrated
    );
    assert!(
        result.migrated.contains("wf:*.toml"),
        "rule should be preserved; got: {}",
        result.migrated
    );
}

#[test]
fn migrate_filter_round_trip_is_stable() {
    // A fully-supported filter should be idempotent across two passes.
    let input = "name: Stable\nwf:*.rs\nd!:target\n";
    let first = migrate_filter(input);
    let second = migrate_filter(&first.migrated);
    assert_eq!(
        first.migrated, second.migrated,
        "migration should be idempotent"
    );
}
