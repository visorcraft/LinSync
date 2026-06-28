// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Shared helpers for integration tests in this crate.
// Each test file that needs these helpers declares `mod common;` at the top.

#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

/// Returns the workspace root (the directory that contains both `crates/` and `apps/`).
/// CARGO_MANIFEST_DIR for crates/linsync-core is two levels up.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Returns `true` if every tool in `tools` is found on PATH via `which`.
pub fn tools_available(tools: &[&str]) -> bool {
    tools.iter().all(|tool| {
        Command::new("which")
            .arg(tool)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}
