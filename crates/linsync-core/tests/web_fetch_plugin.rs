#[test]
fn web_fetch_plugin_manifest_is_valid() {
    let manifest_path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packaging/plugins/web-fetch/linsync-plugin.json"
    ));
    if !manifest_path.exists() {
        return; // skip in environments where packaging/ is absent
    }
    let manifest = linsync_core::PluginManifest::from_manifest_file(manifest_path).unwrap();
    let plugin_dir = manifest_path.parent().unwrap();
    manifest.validate(plugin_dir).unwrap();
    assert_eq!(manifest.id, "com.visorcraft.web-fetch");
    assert!(manifest.sandbox.network);
    assert!(!manifest.sandbox.writes_input);
}
