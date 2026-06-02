//! Basic end-to-end test: run_plugin_helper works with the sandbox enabled.

use linsync_core::plugin::{
    PluginClass, PluginExecutionOptions, PluginManifest, PluginSandbox, run_plugin_helper,
};
use tempfile::TempDir;

fn make_manifest(dir: &std::path::Path) -> PluginManifest {
    let path = dir.join("helper.sh");
    std::fs::write(&path, b"#!/bin/sh\nread REQ\necho '{\"ok\":true}'").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    PluginManifest {
        schema_version: 1,
        id: "test.basic".into(),
        name: "Test".into(),
        version: "1.0".into(),
        license: "MIT".into(),
        entry: vec!["./helper.sh".into()],
        classes: vec![PluginClass::Unpacker],
        mime_types: vec![],
        extensions: vec![],
        capabilities: vec![],
        deterministic: true,
        sandbox: PluginSandbox {
            network: false,
            writes_input: false,
            requires_home_access: false,
        },
        streaming: false,
        options_schema: vec![],
        normalization_categories: vec![],
    }
}

#[test]
fn sandboxed_helper_runs_successfully() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.txt");
    std::fs::write(&source, b"content").unwrap();

    let req = serde_json::json!({
        "op": "probe",
        "source": source.to_str().unwrap(),
    });

    let manifest = make_manifest(tmp.path());
    let opts = PluginExecutionOptions {
        timeout: std::time::Duration::from_secs(5),
        ..Default::default()
    };

    let result = run_plugin_helper(tmp.path(), &manifest, &req.to_string(), &opts);
    assert!(
        result.is_ok(),
        "sandboxed helper should succeed; err={:?}",
        result.err()
    );
    assert!(result.unwrap().stdout.contains("\"ok\":true"));
}
