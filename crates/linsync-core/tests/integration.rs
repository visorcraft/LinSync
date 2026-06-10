use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use linsync_core::{
    BinaryCompareOptions, CompareOptions, CompareSession, DiffBlockKind, FolderCompareOptions,
    FolderEntryState, PluginClass, PluginExecutionOptions, PluginInputDescriptor, PluginManifest,
    PluginSandbox, ProjectFile, ProjectFileStore, RecentPathStore, SessionFile, SessionFileStore,
    Settings, SettingsStore, TableCompareOptions, TextCompareOptions, TextDocument,
    ThemePreference, compare_binary_files, compare_documents, compare_folders, compare_table_files,
    compare_text_files, merge_three_way, run_prediffer_plugin,
};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(path)
}

#[test]
fn shared_fixtures_exercise_file_engines_and_patch_output() -> Result<(), Box<dyn std::error::Error>>
{
    let text = compare_text_files(
        &fixture("text/left.txt"),
        &fixture("text/right.txt"),
        &TextCompareOptions::default(),
    )?;
    assert_eq!(text.difference_count(), 1);
    let patch = text.to_unified_diff(1);
    assert!(patch.contains("-beta"));
    assert!(patch.contains("+gamma"));

    let binary = compare_binary_files(
        &fixture("binary/left.bin"),
        &fixture("binary/right.bin"),
        &BinaryCompareOptions {
            bytes_per_row: 4,
            ..BinaryCompareOptions::default()
        },
    )?;
    assert_eq!(binary.differences.len(), 4);
    assert!(binary.rows.iter().any(|row| row.has_difference));

    let table = compare_table_files(
        &fixture("table/left.csv"),
        &fixture("table/right.csv"),
        &TableCompareOptions {
            delimiter: ',',
            has_header: true,
            ..TableCompareOptions::default()
        },
    )?;
    assert_eq!(table.changed_cells, 1);

    Ok(())
}

#[test]
fn shared_folder_fixture_reports_all_entry_states() -> Result<(), Box<dyn std::error::Error>> {
    let result = compare_folders(
        &fixture("folders/left"),
        &fixture("folders/right"),
        &FolderCompareOptions::default(),
    )?;
    let states: BTreeMap<_, _> = result
        .entries
        .iter()
        .map(|entry| (entry.relative_path.as_path(), entry.state))
        .collect();

    assert_eq!(states[Path::new("same.txt")], FolderEntryState::Identical);
    assert_eq!(
        states[Path::new("different.txt")],
        FolderEntryState::Different
    );
    assert_eq!(
        states[Path::new("left-only.txt")],
        FolderEntryState::LeftOnly
    );
    assert_eq!(
        states[Path::new("right-only.txt")],
        FolderEntryState::RightOnly
    );
    assert_eq!(
        states[Path::new("nested/shared.txt")],
        FolderEntryState::Identical
    );

    Ok(())
}

#[test]
fn merge_fixture_produces_conflict() {
    let result = merge_three_way(
        &fs::read_to_string(fixture("merge/base.txt")).unwrap(),
        &fs::read_to_string(fixture("merge/left.txt")).unwrap(),
        &fs::read_to_string(fixture("merge/right.txt")).unwrap(),
    );

    assert_eq!(result.conflicts.len(), 1);
    assert!(result.text().contains("<<<<<<< LEFT"));
}

#[test]
fn settings_sessions_projects_and_recent_paths_round_trip() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = TempFixture::new();
    let settings_store = SettingsStore::new(temp.path.join("settings.json"));
    let settings = Settings {
        theme_preference: ThemePreference::Dark,
        recent_limit: 3,
        default_recursive_folder_compare: false,
        ..Settings::default()
    };
    settings_store.save(&settings)?;
    assert_eq!(settings_store.load_or_default()?, settings);

    let recent_store = RecentPathStore::new(temp.path.join("recent.json"), 2);
    recent_store.add(fixture("text/left.txt"))?;
    let recent = recent_store.add(fixture("text/right.txt"))?;
    assert_eq!(recent.paths.len(), 2);

    let session = CompareSession {
        title: "fixture compare".to_owned(),
        left: fixture("text/left.txt"),
        base: Some(fixture("text/base.txt")),
        right: fixture("text/right.txt"),
        options: CompareOptions::default(),
    };
    let session_file = SessionFile::new(session);
    let session_store = SessionFileStore::new(temp.path.join("session.linsync.json"));
    session_store.save(&session_file)?;
    assert_eq!(session_store.load()?, session_file);

    let mut project = ProjectFile::new("fixture project");
    project.sessions.push(session_file);
    project.active_session_index = Some(0);
    let project_store = ProjectFileStore::new(temp.path.join("project.linsync-project.json"));
    project_store.save(&project)?;
    assert_eq!(project_store.load()?, project);

    Ok(())
}

#[test]
fn plugin_integration_prediff_uses_inline_text_protocol() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = TempFixture::new();
    let plugin_dir = temp.path.join("plugin");
    fs::create_dir_all(&plugin_dir)?;
    write_helper(
        &plugin_dir,
        "prediff.sh",
        r#"#!/bin/sh
request=$(cat)
request_id=$(printf '%s' "$request" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
cat <<JSON
{"protocol_version":1,"request_id":"$request_id","status":"ok","outputs":[{"role":"left","kind":"text","inline_text":"alpha\nbeta","encoding":"utf-8","line_ending":"lf"}],"diagnostics":[]}
JSON
"#,
    )?;
    let manifest = PluginManifest {
        schema_version: linsync_core::CURRENT_PLUGIN_SCHEMA_VERSION,
        id: "example.integration-prediff".to_owned(),
        name: "Integration Prediff".to_owned(),
        version: "1.0.0".to_owned(),
        license: "MIT".to_owned(),
        entry: vec!["prediff.sh".to_owned()],
        classes: vec![PluginClass::Prediffer],
        mime_types: vec!["text/plain".to_owned()],
        extensions: vec!["txt".to_owned()],
        capabilities: vec!["deterministic-output".to_owned()],
        deterministic: true,
        sandbox: PluginSandbox::default(),
        streaming: false,
        options_schema: vec![],
        normalization_categories: vec![],
    };

    let result = run_prediffer_plugin(
        &plugin_dir,
        &manifest,
        PluginInputDescriptor::for_file("left", fixture("text/left.txt")),
        &PluginExecutionOptions::default(),
    )?;

    assert_eq!(result.role, "left");
    assert_eq!(result.text, "alpha\nbeta");
    assert_eq!(result.encoding.as_deref(), Some("utf-8"));

    Ok(())
}

fn write_helper(plugin_dir: &Path, name: &str, script: &str) -> std::io::Result<()> {
    let path = plugin_dir.join(name);
    fs::write(&path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)?;
    }
    Ok(())
}

struct TempFixture {
    path: PathBuf,
}

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

impl TempFixture {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "linsync-core-integration-{}-{suffix}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Smoke test for moved-block detection: verifies that a compare with
/// `detect_moves: true` surfaces `Moved` blocks in both the result struct
/// and a JSON-serialized representation of the block count.
#[test]
fn moved_block_detection_smoke() {
    let left = "section A\nline 1\nline 2\nsection B\nline 3\nline 4\n";
    let right = "section B\nline 3\nline 4\nsection A\nline 1\nline 2\n";
    let opts = TextCompareOptions {
        detect_moves: true,
        ..TextCompareOptions::default()
    };
    let result = compare_documents(
        TextDocument::from_text("left", left),
        TextDocument::from_text("right", right),
        &opts,
    );

    let moved_count = result
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, DiffBlockKind::Moved { .. }))
        .count();
    assert_eq!(moved_count, 2, "expected two Moved blocks");

    // Confirm the result serialises cleanly and contains the moved count.
    let json = serde_json::json!({
        "equal": result.is_equal(),
        "differences": result.difference_count(),
        "moved_blocks": moved_count,
    });
    assert_eq!(json["moved_blocks"], 2);
    assert!(!json["equal"].as_bool().unwrap());
}

/// Auto syntax detection end-to-end: comparing `.py` files with the syntax
/// mode left at `Auto` must resolve to Python via the file extension and
/// attach keyword spans on the view rows (the path the GUI renders from).
#[cfg(feature = "syntax-rich")]
#[test]
fn auto_syntax_mode_detects_python_and_attaches_keyword_spans()
-> Result<(), Box<dyn std::error::Error>> {
    use linsync_core::TextSyntaxMode;

    let dir = tempfile::tempdir()?;
    let left = dir.path().join("left.py");
    let right = dir.path().join("right.py");
    fs::write(&left, "def f():\n    return 1\n")?;
    fs::write(&right, "def f():\n    return 2\n")?;

    let options = TextCompareOptions {
        syntax_mode: TextSyntaxMode::Auto,
        ..TextCompareOptions::default()
    };
    let result = compare_text_files(&left, &right, &options)?;

    let rows = result.view_rows(&options);
    assert!(
        rows.iter().any(|row| row
            .left_syntax
            .iter()
            .chain(row.right_syntax.iter())
            .any(|span| span.class == "keyword")),
        "expected keyword spans from auto-detected Python on view rows"
    );

    // Windowed path must resolve Auto identically.
    let page = result.view_rows_window(&options, 0, 10);
    assert!(
        page.rows.iter().any(|row| row
            .left_syntax
            .iter()
            .chain(row.right_syntax.iter())
            .any(|span| span.class == "keyword")),
        "expected keyword spans from auto-detected Python on windowed rows"
    );
    Ok(())
}
