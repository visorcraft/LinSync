use super::*;
use linsync_core::backup_path;
use std::io::{Read, Write};

#[test]
fn plugin_id_guard_blocks_path_traversal() {
    // The bridge guards /plugins/options/{get,set} ids with the core's
    // stable-id rule.
    use linsync_core::is_stable_plugin_id as safe;
    assert!(safe("tesseract-ocr"));
    assert!(safe("com.example.plugin_v2"));
    // Anything that could escape `<options-dir>/{id}.json` is rejected.
    assert!(!safe(""));
    assert!(!safe("."));
    assert!(!safe(".."));
    assert!(!safe("../../etc/cron.d/evil"));
    assert!(!safe("a/b"));
    assert!(!safe("a\\b"));
    assert!(!safe("with space"));
}

fn test_app_paths(name: &str) -> AppPaths {
    let root = env::temp_dir().join(format!("linsync-gui-test-{name}-{}", process::id()));
    AppPaths::from_base_dirs(
        root.join("config"),
        root.join("data"),
        root.join("cache"),
        root.join("state"),
    )
}

fn test_bridge_state(initial_context: Option<GuiLaunchContext>) -> Arc<Mutex<GuiBridgeState>> {
    Arc::new(Mutex::new(GuiBridgeState::new(initial_context)))
}

fn test_file_root(name: &str) -> PathBuf {
    let root = env::temp_dir().join(format!("linsync-gui-files-{name}-{}", process::id()));
    fs::create_dir_all(&root).expect("test file root should be created");
    root
}

fn command_available(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

// ── Phase 3: Table / Hex modes emit real navigable rows ──────────────────
#[test]
fn table_mode_emits_real_aligned_rows() {
    let root = test_file_root("table-real-rows");
    let left = root.join("left.csv");
    let right = root.join("right.csv");
    fs::write(&left, "a,b,c\n1,2,3\n").unwrap();
    fs::write(&right, "a,b,c\n1,9,3\n").unwrap();
    let tab =
        build_tab_for_paths_with_mode(&left, &right, Some("Table"), &GuiCompareOptions::default());
    assert_eq!(
        tab.left_rows.len(),
        tab.right_rows.len(),
        "left/right table rows must align 1:1"
    );
    assert!(
        tab.left_rows.iter().any(|r| r.state == "changed"),
        "the differing row must be marked changed"
    );
    // The differing cell values appear on their respective sides.
    assert!(tab.left_rows.iter().any(|r| r.text.contains('2')));
    assert!(tab.right_rows.iter().any(|r| r.text.contains('9')));
}

#[test]
fn table_tab_exposes_cells_and_headers() {
    let root = test_file_root("table-cells-headers");
    let left = root.join("left.csv");
    let right = root.join("right.csv");
    fs::write(&left, "name,count\nalpha,1\nbeta,2\n").unwrap();
    fs::write(&right, "name,count\nalpha,1\nbeta,3\n").unwrap();
    let mut options = GuiCompareOptions::default();
    options.table.has_header = true;
    let tab = build_tab_for_paths_with_mode(&left, &right, Some("Table"), &options);
    assert!(
        tab.table_cells.as_ref().is_some_and(|r| !r.is_empty()),
        "table_cells must carry the structured row data"
    );
    assert_eq!(
        tab.table_headers.as_deref(),
        Some(["name".to_owned(), "count".to_owned()].as_slice()),
        "table_headers must carry the parsed header row"
    );
}

#[test]
fn large_table_response_is_windowed_on_the_wire() {
    // A table over TABLE_WINDOW_THRESHOLD rows is windowed for the wire:
    // total_rows carries the full count and only the first window of
    // table_cells is embedded (the GUI pages the rest via
    // /compare/table/window). The canonical tab stays full — this windows
    // a clone at serialization.
    let total = TABLE_WINDOW_THRESHOLD + 7;
    let rows: Vec<linsync_core::TableRowDiff> = (0..total)
        .map(|i| linsync_core::TableRowDiff {
            row_index: i,
            cells: vec![linsync_core::TableCellDiff {
                column_index: 0,
                left: Some(format!("v{i}")),
                right: Some(format!("v{i}")),
                state: linsync_core::TableCellState::Equal,
                column_name: None,
                diff_type: linsync_core::table::CellDiffType::ValueChanged,
                inline_diff: None,
            }],
            has_difference: false,
        })
        .collect();
    let tab = {
        let mut t = compare_tab(
            "Table",
            ("/l.csv".to_owned(), "/r.csv".to_owned()),
            "ok".to_owned(),
            0,
            GuiOpenValidation {
                compatible: true,
                path_kind: "Files".to_owned(),
                message: String::new(),
            },
            vec![],
            (vec![], vec![]),
            vec![],
            None,
            Some(rows),
            Vec::new(),
            None,
        );
        t.table_headers = Some(vec!["col".to_owned()]);
        t
    };
    let ctx = GuiLaunchContext::single_tab(tab);
    let value = context_to_value(&ctx).expect("serialize");
    let wire = &value["session"]["tabs"][0];
    assert_eq!(
        wire["total_rows"].as_u64(),
        Some(total as u64),
        "the full row count is reported"
    );
    assert_eq!(
        wire["table_cells"].as_array().unwrap().len(),
        TABLE_WINDOW_SIZE,
        "only the first window of cells is embedded"
    );

    // A small table is NOT windowed (no total_rows; every cell embedded).
    let small_rows: Vec<linsync_core::TableRowDiff> = (0..3)
        .map(|i| linsync_core::TableRowDiff {
            row_index: i,
            cells: vec![linsync_core::TableCellDiff {
                column_index: 0,
                left: Some(format!("v{i}")),
                right: Some(format!("v{i}")),
                state: linsync_core::TableCellState::Equal,
                column_name: None,
                diff_type: linsync_core::table::CellDiffType::ValueChanged,
                inline_diff: None,
            }],
            has_difference: false,
        })
        .collect();
    let small_tab = {
        let mut t = compare_tab(
            "Table",
            ("/l.csv".to_owned(), "/r.csv".to_owned()),
            "ok".to_owned(),
            0,
            GuiOpenValidation {
                compatible: true,
                path_kind: "Files".to_owned(),
                message: String::new(),
            },
            vec![],
            (vec![], vec![]),
            vec![],
            None,
            Some(small_rows),
            Vec::new(),
            None,
        );
        t.table_headers = Some(vec!["col".to_owned()]);
        t
    };
    let small = GuiLaunchContext::single_tab(small_tab);
    let small_value = context_to_value(&small).expect("serialize");
    let small_wire = &small_value["session"]["tabs"][0];
    assert!(
        small_wire["total_rows"].is_null(),
        "small tables are not windowed"
    );
    assert_eq!(small_wire["table_cells"].as_array().unwrap().len(), 3);
}

#[test]
fn table_window_returns_paged_rows() {
    let root = test_file_root("table-window");
    let left = root.join("left.csv");
    let right = root.join("right.csv");
    let mut left_text = "id\n".to_owned();
    let mut right_text = "id\n".to_owned();
    for i in 0..9 {
        left_text.push_str(&format!("{i}\n"));
        right_text.push_str(&format!("{i}\n"));
    }
    fs::write(&left, left_text).unwrap();
    fs::write(&right, right_text).unwrap();
    let paths = test_app_paths("table-window");
    let state = test_bridge_state(None);

    // Prime the session with a table compare.
    let _ = bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Table HTTP/1.1\r\n",
            urlencoding::encode(left.to_str().unwrap()),
            urlencoding::encode(right.to_str().unwrap())
        ),
        &paths,
        &state,
    );

    let page = |offset: usize, limit: usize| -> serde_json::Value {
        json_response_body(
            &String::from_utf8(bridge_response(
                &format!("GET /compare/table/window?offset={offset}&limit={limit} HTTP/1.1\r\n"),
                &paths,
                &state,
            ))
            .expect("utf-8"),
        )
    };

    let first = page(0, 3);
    assert_eq!(first["total"].as_u64().unwrap(), 10);
    assert_eq!(first["rows"].as_array().unwrap().len(), 3);
    assert_eq!(first["offset"].as_u64().unwrap(), 0);
    assert_eq!(first["hasMore"], serde_json::json!(true));

    let second = page(3, 3);
    assert_eq!(second["rows"].as_array().unwrap().len(), 3);
    assert_eq!(second["offset"].as_u64().unwrap(), 3);

    let tail = page(6, 10);
    assert_eq!(tail["rows"].as_array().unwrap().len(), 4);
    assert_eq!(tail["hasMore"], serde_json::json!(false));
}

#[test]
fn hex_mode_emits_real_aligned_rows() {
    let root = test_file_root("hex-real-rows");
    let left = root.join("left.bin");
    let right = root.join("right.bin");
    fs::write(&left, b"hello world").unwrap();
    fs::write(&right, b"hello WORLD").unwrap();
    let tab =
        build_tab_for_paths_with_mode(&left, &right, Some("Hex"), &GuiCompareOptions::default());
    assert!(
        !tab.left_rows.is_empty(),
        "Hex mode must emit real rows, not summary-only"
    );
    assert_eq!(tab.left_rows.len(), tab.right_rows.len());
    assert!(
        tab.left_rows.iter().any(|r| r.state == "changed"),
        "rows containing differing bytes must be marked changed"
    );
    // 'h' = 0x68 appears in the hex text of the first row.
    assert!(
        tab.left_rows[0].text.contains("68"),
        "hex row text must contain the hex byte dump, got: {}",
        tab.left_rows[0].text
    );
}

// ── Phase 3: request-id cancellation ─────────────────────────────────────
#[test]
fn build_tab_cancellable_aborts_when_flagged() {
    let root = test_file_root("cancel-build");
    let left = root.join("l.txt");
    let right = root.join("r.txt");
    fs::write(&left, "a\nb\nc\n").unwrap();
    fs::write(&right, "a\nx\nc\n").unwrap();
    // An already-cancelled flag aborts the compare → None.
    let cancelled = build_tab_for_paths_with_mode_cancellable(
        &left,
        &right,
        Some("Text"),
        &GuiCompareOptions::default(),
        &|| true,
        None,
    );
    assert!(cancelled.is_none(), "always-cancel must abort the compare");
    // A live flag completes normally.
    let ok = build_tab_for_paths_with_mode_cancellable(
        &left,
        &right,
        Some("Text"),
        &GuiCompareOptions::default(),
        &|| false,
        None,
    );
    assert!(ok.is_some(), "never-cancel must complete the compare");
}

#[test]
fn cancel_endpoint_sets_registered_flag() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let paths = test_app_paths("cancel-endpoint");
    let state = test_bridge_state(None);
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut s = state.lock().unwrap();
        s.compare_cancels.insert("req-123".to_owned(), flag.clone());
    }
    let resp = String::from_utf8(bridge_response(
        "GET /cancel?id=req-123 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 200"),
        "cancel should return 200: {resp}"
    );
    assert!(
        flag.load(Ordering::Relaxed),
        "/cancel must set the registered cancel flag"
    );
    // Unknown id is harmless and reports cancelled:false.
    let resp2 = String::from_utf8(bridge_response(
        "GET /cancel?id=does-not-exist HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp2.contains("\"cancelled\":false"),
        "unknown id → cancelled:false, got: {resp2}"
    );
}

#[test]
fn progress_endpoint_returns_registered_progress() {
    let paths = test_app_paths("progress-endpoint");
    let state = test_bridge_state(None);
    let progress = Arc::new(Mutex::new(CompareProgress {
        phase: "comparing".to_owned(),
        current: 5,
        total: 10,
        message: "file.txt".to_owned(),
    }));
    {
        let mut s = state.lock().unwrap();
        s.compare_progress.insert("req-456".to_owned(), progress);
    }
    let resp = String::from_utf8(bridge_response(
        "GET /progress?id=req-456 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 200"),
        "progress should return 200: {resp}"
    );
    let body = json_response_body(&resp);
    assert_eq!(body["phase"].as_str().unwrap(), "comparing");
    assert_eq!(body["current"].as_u64().unwrap(), 5);
    assert_eq!(body["total"].as_u64().unwrap(), 10);
    assert_eq!(body["message"].as_str().unwrap(), "file.txt");
    // Unknown id returns phase=none.
    let resp2 = String::from_utf8(bridge_response(
        "GET /progress?id=does-not-exist HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body2 = json_response_body(&resp2);
    assert_eq!(body2["phase"].as_str().unwrap(), "none");
}

// ── Phase 3: host parity ─────────────────────────────────────────────────
// Both hosts (external qml6 and in-process cxx-qt) drive the QML over the
// same HTTP bridge, and both the HTTP `/compare` route and the cxx-qt
// `compare_paths` qinvokable build their tab from
// `build_tab_for_paths_with_mode`. This asserts the shared builder and the
// HTTP route agree, so the two hosts produce identical comparisons.
#[test]
fn http_route_and_shared_builder_agree_on_compare() {
    let root = test_file_root("host-parity");
    let left = root.join("l.txt");
    let right = root.join("r.txt");
    fs::write(&left, "a\nb\nc\n").unwrap();
    fs::write(&right, "a\nX\nc\n").unwrap();

    // What the cxx-qt host's `compare_paths` qinvokable calls.
    let shared =
        build_tab_for_paths_with_mode(&left, &right, Some("Text"), &GuiCompareOptions::default());

    // What the QML uses over HTTP on both hosts.
    let paths = test_app_paths("host-parity");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Text HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let http_tab = &body["session"]["tabs"][0];

    assert_eq!(
        http_tab["difference_count"].as_u64().unwrap() as usize,
        shared.difference_count,
        "HTTP and cxx-qt-shared builder must report the same difference count"
    );
    assert_eq!(
        http_tab["left_rows"].as_array().unwrap().len(),
        shared.left_rows.len(),
        "HTTP and cxx-qt-shared builder must produce the same row count"
    );
}

fn json_response_body(response: &str) -> serde_json::Value {
    let (_, body) = response
        .split_once("\r\n\r\n")
        .expect("HTTP response should include body separator");
    serde_json::from_str(body).expect("response body should be JSON")
}

fn bridge_address_and_token_path(base_url: &str) -> (String, String) {
    let rest = base_url
        .strip_prefix("http://")
        .expect("bridge URL should use http");
    let (address, token) = rest
        .split_once('/')
        .expect("bridge URL should include a token path");
    (address.to_owned(), format!("/{token}"))
}

#[test]
fn qml_candidates_include_source_tree() {
    let candidates = qml_file_candidates();
    assert!(
        candidates
            .iter()
            .any(|path| path.ends_with("apps/linsync-gui/qml/Main.qml")),
        "expected source-tree QML candidate in {candidates:?}"
    );
}

#[test]
fn source_tree_qml_file_exists() {
    let source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml");
    assert!(source_file.is_file(), "missing {}", source_file.display());
}

#[test]
fn source_tree_qml_wires_text_compare_controls() {
    let source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml");
    let qml = fs::read_to_string(&source_file).expect("Main.qml should be readable");
    for needle in [
        "syntaxRichTextForRow",
        "Text.RichText",
        "regex_rule_set",
        "textEncoding",
        "appendBookmarkParams",
        "\"html\"",
    ] {
        assert!(
            qml.contains(needle),
            "Main.qml should include text compare control/rendering hook {needle}"
        );
    }
}

#[test]
fn source_tree_qml_titles_include_all_compare_sections() {
    let source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml");
    let qml = fs::read_to_string(&source_file).expect("Main.qml should be readable");
    for title in ["Image Compare", "Webpage Compare", "Document Compare"] {
        assert!(
            qml.contains(title),
            "Main.qml section titles should include {title}"
        );
    }
}

#[test]
fn source_tree_qml_exposes_webpage_mode_without_bypassing_consent() {
    let source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml");
    let qml = fs::read_to_string(&source_file).expect("Main.qml should be readable");
    assert!(qml.contains("\"Webpage\""));
    assert!(
        qml.contains("webpageComparePage.startFromMain"),
        "Main Compare Webpage mode should hand off to the consent-gated Webpage page"
    );
}

#[test]
fn webpage_qml_gates_rendered_modes_on_web_engine() {
    let source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/WebpageComparePage.qml");
    let qml = fs::read_to_string(&source_file).expect("WebpageComparePage.qml should be readable");
    // Always-available modes.
    for mode in ["html", "text", "tree"] {
        assert!(
            qml.contains(&format!("value: \"{mode}\"")),
            "WebpageComparePage should expose implemented mode {mode}"
        );
    }
    // Rendered/screenshot are offered, but only behind the web-engine
    // capability flag (set from /capabilities), so a non-web-engine build
    // never shows them.
    for mode in ["rendered", "screenshot"] {
        assert!(
            qml.contains(&format!("value: \"{mode}\"")),
            "WebpageComparePage should offer web-engine mode {mode}"
        );
    }
    assert!(
        qml.contains("webEngineAvailable"),
        "rendered/screenshot modes must be gated on webEngineAvailable"
    );
}

#[test]
fn webpage_qml_surfaces_renderer_backend() {
    let page_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/WebpageComparePage.qml");
    let page = fs::read_to_string(&page_file).expect("WebpageComparePage.qml should be readable");
    // Rendered/screenshot are additionally gated on a usable runtime
    // renderer ("none" hides them and shows the unavailable hint).
    assert!(
        page.contains("root.webRenderer !== \"none\""),
        "mode combo must hide rendered/screenshot when web_renderer is none"
    );
    assert!(
        page.contains("root.webRenderer === \"none\""),
        "the renderer-unavailable hint must show only when web_renderer is none"
    );
    // The Chromium fallback is disclosed with a small tag.
    assert!(
        page.contains("root.webRenderer === \"chromium\""),
        "the via-Chromium tag must show only for the chromium backend"
    );
    assert!(
        page.contains("qsTr(\"via Chromium\")"),
        "the chromium backend tag text must be present"
    );
    // Main.qml feeds the property from /capabilities.
    let main_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml");
    let main = fs::read_to_string(&main_file).expect("Main.qml should be readable");
    assert!(
        main.contains("payload.web_renderer"),
        "Main.qml must wire web_renderer from /capabilities into the page"
    );
}

#[test]
fn source_tree_qml_keeps_folder_table_on_entry_model() {
    let source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml");
    let qml = fs::read_to_string(&source_file).expect("Main.qml should be readable");
    assert!(qml.contains("model: root.visibleFolderEntries"));
    assert!(qml.contains("root.visibleFolderEntries.length : root.leftRows.length"));
    assert!(
        !qml.contains("function folderRowForEntry"),
        "folder rows should not be duplicated into text-row objects"
    );
    assert!(
        !qml.contains("folderRowForEntry("),
        "folder table should use the entry model directly"
    );
}

#[test]
fn source_tree_qml_wires_reduced_motion_setting() {
    let qml_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml");
    let main = fs::read_to_string(qml_root.join("Main.qml")).expect("Main.qml should read");
    assert!(main.contains(r#""reduceMotion": false"#));
    assert!(main.contains("reduceMotion: root.reduceMotion"));
    assert!(main.contains("duration: root.reduceMotion ? 0 : 160"));

    let settings =
        fs::read_to_string(qml_root.join("SettingsPage.qml")).expect("SettingsPage should read");
    assert!(settings.contains("page.emit(\"reduceMotion\", checked)"));

    let nav =
        fs::read_to_string(qml_root.join("LinSyncNavItem.qml")).expect("nav item should read");
    assert!(nav.contains("duration: nav.reduceMotion ? 0 : 110"));

    let plugins =
        fs::read_to_string(qml_root.join("PluginsPage.qml")).expect("PluginsPage should read");
    assert!(plugins.contains("duration: page.reduceMotion ? 0 : 120"));
}

#[test]
fn source_tree_qml_wires_live_compare_setting() {
    let qml_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml");
    let main = fs::read_to_string(qml_root.join("Main.qml")).expect("Main.qml should read");
    assert!(main.contains(r#""liveCompare": false"#));
    assert!(main.contains("liveCompare: root.liveCompareEnabled"));
    assert!(main.contains(r#"key === "liveCompare""#));

    let settings =
        fs::read_to_string(qml_root.join("SettingsPage.qml")).expect("SettingsPage should read");
    assert!(settings.contains("property bool liveCompare: false"));
    assert!(settings.contains(r#"page.emit("liveCompare", checked)"#));
}

#[test]
fn source_tree_qml_wires_live_compare_timer_and_toggle() {
    let qml_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml");
    let main = fs::read_to_string(qml_root.join("Main.qml")).expect("Main.qml should read");
    assert!(main.contains("id: liveCompareTimer"));
    assert!(main.contains("liveCompareTimer.restart()"));
    assert!(main.contains("root.scheduleRawTextPreview()"));
    assert!(main.contains("liveCompareTimer.stop()"));
    assert!(main.contains("Toggle live raw compare"));
}

#[test]
fn source_tree_qml_resets_raw_preview_on_mode_boundary() {
    let qml_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml");
    let main = fs::read_to_string(qml_root.join("Main.qml")).expect("Main.qml should read");

    assert!(main.contains("readonly property bool rawTextInputMode"));
    assert!(main.contains("return root.rawTextInputMode"));
    assert!(
        main.contains("const shouldResetRows = root.rawPreviewActive || root.rawTextInputMode")
    );
    assert!(main.contains("property bool rawInputMode: root.rawTextInputMode"));
    assert!(main.contains("onRawInputModeChanged: resetRowsModel()"));
    assert!(main.contains(
        "const rawText = pane.sideKey === \"left\" ? root.leftPaneText : root.rightPaneText"
    ));
    assert!(main.contains("if (contentArea.text !== rawText)"));
}

#[test]
fn source_tree_window_icon_file_exists() {
    let source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml");
    let icon_file = resolve_window_icon_file(&source_file).expect("missing window icon file");
    assert!(icon_file.is_file(), "missing {}", icon_file.display());
}

#[test]
fn positional_paths_accept_two_paths_after_separator() {
    let paths = positional_paths(&[
        OsString::from("--"),
        OsString::from("left.txt"),
        OsString::from("right.txt"),
    ])
    .expect("paths should parse");

    assert_eq!(paths[0], PathBuf::from("left.txt"));
    assert_eq!(paths[1], PathBuf::from("right.txt"));
}

#[test]
fn positional_paths_reject_extra_arguments() {
    assert!(
        positional_paths(&[
            OsString::from("left.txt"),
            OsString::from("right.txt"),
            OsString::from("extra.txt"),
        ])
        .is_none()
    );
}

#[test]
fn launch_context_includes_text_rows() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/text/left.txt");
    let right = fixture_root.join("tests/fixtures/text/right.txt");
    let context = build_launch_context(&[left.into_os_string(), right.into_os_string()])
        .expect("context should build");
    let tab = context.active_tab().expect("active tab");

    assert_eq!(tab.mode, "Text");
    assert_eq!(tab.difference_count, 1);
    assert!(tab.validation.compatible);
    assert_eq!(tab.validation.path_kind, "Files");
    assert_eq!(context.session.active_tab_id, tab.id);
    assert_eq!(context.session.tabs.len(), 1);
    assert_eq!(context.session.recent_paths.len(), 2);
    assert!(!tab.left_rows.is_empty());
    assert!(tab.left_rows.iter().all(|row| !row.row_id.is_empty()));
    assert!(tab.left_rows.iter().any(|row| row.state == "changed"));
}

#[test]
fn launch_context_uses_folder_entries_for_virtual_table() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/folders/left");
    let right = fixture_root.join("tests/fixtures/folders/right");
    let context = build_launch_context(&[left.into_os_string(), right.into_os_string()])
        .expect("context should build");
    let tab = context.active_tab().expect("active tab");

    assert_eq!(tab.mode, "Folder");
    assert!(tab.validation.compatible);
    assert_eq!(tab.validation.path_kind, "Folders");
    assert!(
        tab.left_rows.is_empty() && tab.right_rows.is_empty(),
        "folder tabs should not duplicate the virtualized table model into text rows"
    );
    assert!(!tab.folder_entries.is_empty());
    assert!(
        tab.folder_entries
            .iter()
            .any(|row| row.state == "left_only")
    );
    assert!(
        tab.folder_entries
            .iter()
            .any(|row| row.state == "right_only")
    );
}

#[test]
fn folder_compare_response_omits_text_rows_for_virtual_table() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/folders/left");
    let right = fixture_root.join("tests/fixtures/folders/right");
    let paths = test_app_paths("folder-virtual-table");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Folder HTTP/1.1\r\n",
            urlencoding::encode(left.to_str().unwrap()),
            urlencoding::encode(right.to_str().unwrap())
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let tab = &body["session"]["tabs"][0];
    let entries = tab["folder_entries"].as_array();
    assert!(entries.is_some_and(|v| !v.is_empty()));
    // The QML type-filter reads `entry.entryType`; every entry must carry it
    // so the client-side filter can categorize file/directory/symlink.
    for entry in entries.unwrap() {
        let ty = entry["entryType"].as_str();
        assert!(
            matches!(ty, Some("file" | "directory" | "symlink" | "special")),
            "folder entry should expose a recognized entryType, got {ty:?}: {entry}"
        );
    }
    assert!(
        tab.get("left_rows").is_none() && tab.get("right_rows").is_none(),
        "folder response should not duplicate virtual table data into text rows: {body}"
    );
}

#[test]
fn compare_auto_routes_archive_pair_to_folder_view() {
    let paths = test_app_paths("archive-route");
    // Install an enabled folder_virtualizer for the ".myarc" extension whose
    // helper emits a one-file tree keyed by the source file's content.
    let plugin_dir = paths.user_plugins_dir().join("test.myarc");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let helper = plugin_dir.join("v.sh");
    std::fs::write(
            &helper,
            "#!/bin/sh\nrequest=$(cat)\n\
             source=$(printf '%s' \"$request\" | sed -n 's/.*\"source\":\"\\([^\"]*\\)\".*/\\1/p')\n\
             content=$(cat \"$source\")\n\
             printf '{\"ok\":true,\"tree\":[{\"path\":\"entry.txt\",\"kind\":\"file\",\"sha256\":\"%s\"}]}\\n' \"$content\"\n",
        )
        .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&helper).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&helper, perms).unwrap();
    }
    std::fs::write(
        plugin_dir.join("linsync-plugin.json"),
        r#"{
              "schema_version": 1, "id": "test.myarc", "name": "MyArc",
              "version": "1.0.0", "license": "GPL-3.0-only", "entry": ["./v.sh"],
              "classes": ["folder_virtualizer"], "mime_types": ["application/x-myarc"],
              "extensions": ["myarc"], "capabilities": [], "deterministic": true,
              "sandbox": { "network": false, "writes_input": false, "requires_home_access": false },
              "options_schema": []
            }"#,
    )
    .unwrap();

    let files = test_file_root("archive-route-files");
    let left = files.join("a.myarc");
    let right = files.join("b.myarc");
    std::fs::write(&left, "SAME").unwrap();
    std::fs::write(&right, "DIFFERENT").unwrap();
    let state = test_bridge_state(None);

    // No explicit mode → auto-routed to archive-as-folder.
    let body = json_response_body(
        &String::from_utf8(bridge_response(
            &format!(
                "GET /compare?left={}&right={} HTTP/1.1\r\n",
                urlencoding::encode(left.to_str().unwrap()),
                urlencoding::encode(right.to_str().unwrap())
            ),
            &paths,
            &state,
        ))
        .unwrap(),
    );
    let tab = &body["session"]["tabs"][0];
    // Rendered through the folder view, titled as an archive compare.
    assert_eq!(
        tab["mode"], "Folder",
        "archive routes to the folder view: {tab}"
    );
    let entries = tab["folder_entries"].as_array().expect("folder entries");
    assert!(
        entries.iter().any(|e| e["path"] == "entry.txt"),
        "the unpacked member should appear: {tab}"
    );
    // Differing content → the member is reported different.
    assert!(tab["difference_count"].as_u64().unwrap() >= 1);
}

#[test]
fn compare_builtin_archive_routes_to_folder_view() {
    if !command_available("zip") || !command_available("unzip") {
        return;
    }
    let paths = test_app_paths("builtin-archive-route");
    let files = test_file_root("builtin-archive-route-files");
    let left = files.join("left.zip");
    let right = files.join("right.zip");
    let a = files.join("a.txt");
    let b = files.join("b.txt");
    fs::write(&a, "hello").unwrap();
    fs::write(&b, "world").unwrap();
    Command::new("zip")
        .args(["-q", "-j", left.to_str().unwrap(), a.to_str().unwrap()])
        .status()
        .unwrap();
    Command::new("zip")
        .args(["-q", "-j", right.to_str().unwrap(), b.to_str().unwrap()])
        .status()
        .unwrap();

    let state = test_bridge_state(None);
    let body = json_response_body(
        &String::from_utf8(bridge_response(
            &format!(
                "GET /compare?mode=Archive&left={}&right={} HTTP/1.1\r\n",
                urlencoding::encode(left.to_str().unwrap()),
                urlencoding::encode(right.to_str().unwrap())
            ),
            &paths,
            &state,
        ))
        .unwrap(),
    );
    let tab = &body["session"]["tabs"][0];
    assert_eq!(
        tab["mode"], "Folder",
        "builtin archive routes to the folder view: {tab}"
    );
    let entries = tab["folder_entries"].as_array().expect("folder entries");
    assert!(
        entries
            .iter()
            .any(|e| e["path"] == "a.txt" || e["path"] == "b.txt"),
        "unpacked members should appear: {tab}"
    );
    assert!(tab["difference_count"].as_u64().unwrap() >= 1);
}

#[test]
fn active_profile_prediffer_toggle_round_trips() {
    let paths = test_app_paths("profile-prediffer");
    let store = linsync_core::ProfileStore::with_builtins(
        paths.profiles_dir(),
        paths.active_profile_pointer_file(),
    );
    let id = linsync_core::ProfileId::new("my-user-profile".to_owned()).unwrap();
    store
        .save(&linsync_core::CompareProfile::new(id.clone(), "My Profile"))
        .unwrap();
    store.save_active_pointer(&id).unwrap();
    let state = test_bridge_state(None);

    // Add a prediffer to the active user profile.
    let add = json_response_body(
        &String::from_utf8(bridge_response(
            "GET /profiles/active/prediffer?id=org.example.norm&enabled=true HTTP/1.1\r\n",
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert_eq!(add["ok"], serde_json::json!(true), "add: {add}");
    assert!(
        add["prediffers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "org.example.norm")
    );

    // /plugins/list reflects the editable active profile + its prediffers.
    let list = json_response_body(
        &String::from_utf8(bridge_response(
            "GET /plugins/list HTTP/1.1\r\n",
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert_eq!(list["active_profile"]["editable"], serde_json::json!(true));
    assert!(
        list["active_profile"]["prediffers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "org.example.norm")
    );

    // Removing it persists too.
    let rm = json_response_body(
        &String::from_utf8(bridge_response(
            "GET /profiles/active/prediffer?id=org.example.norm&enabled=false HTTP/1.1\r\n",
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert!(
        !rm["prediffers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "org.example.norm")
    );

    // A built-in active profile is read-only (409).
    store
        .save_active_pointer(&linsync_core::ProfileId::new("default".to_owned()).unwrap())
        .unwrap();
    let rejected = String::from_utf8(bridge_response(
        "GET /profiles/active/prediffer?id=org.example.norm&enabled=true HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .unwrap();
    assert!(
        rejected.starts_with("HTTP/1.1 409"),
        "editing a built-in profile should 409: {rejected}"
    );
}

#[test]
fn active_profile_plugin_enabled_toggle_round_trips() {
    let paths = test_app_paths("profile-plugin-enabled");
    let store = linsync_core::ProfileStore::with_builtins(
        paths.profiles_dir(),
        paths.active_profile_pointer_file(),
    );
    let id = linsync_core::ProfileId::new("my-user-profile".to_owned()).unwrap();
    store
        .save(&linsync_core::CompareProfile::new(id.clone(), "My Profile"))
        .unwrap();
    store.save_active_pointer(&id).unwrap();
    let state = test_bridge_state(None);

    // Disable a plugin for the active user profile.
    let set = json_response_body(
        &String::from_utf8(bridge_response(
            "GET /profiles/active/plugin-enabled?id=org.example.unzip&enabled=false HTTP/1.1\r\n",
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert_eq!(set["ok"], serde_json::json!(true), "set: {set}");
    assert_eq!(set["enabled"], serde_json::json!(false));
    assert_eq!(
        set["plugin_enablement"]["org.example.unzip"],
        serde_json::json!(false)
    );

    // It persisted to disk.
    let loaded = store.load(&id).unwrap();
    assert_eq!(
        loaded.plugin_enablement.get("org.example.unzip"),
        Some(&false)
    );

    // /plugins/list surfaces the active profile's override map.
    let list = json_response_body(
        &String::from_utf8(bridge_response(
            "GET /plugins/list HTTP/1.1\r\n",
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert_eq!(
        list["active_profile"]["plugin_enablement"]["org.example.unzip"],
        serde_json::json!(false)
    );

    // Re-enabling overwrites the override.
    let reenable = json_response_body(
        &String::from_utf8(bridge_response(
            "GET /profiles/active/plugin-enabled?id=org.example.unzip&enabled=true HTTP/1.1\r\n",
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert_eq!(reenable["enabled"], serde_json::json!(true));

    // Missing 'enabled' parameter is a 400.
    let bad = String::from_utf8(bridge_response(
        "GET /profiles/active/plugin-enabled?id=org.example.unzip HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .unwrap();
    assert!(
        bad.starts_with("HTTP/1.1 400"),
        "missing enabled => 400: {bad}"
    );

    // A built-in active profile is read-only (409).
    store
        .save_active_pointer(&linsync_core::ProfileId::new("default".to_owned()).unwrap())
        .unwrap();
    let rejected = String::from_utf8(bridge_response(
        "GET /profiles/active/plugin-enabled?id=org.example.unzip&enabled=false HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .unwrap();
    assert!(
        rejected.starts_with("HTTP/1.1 409"),
        "editing a built-in profile should 409: {rejected}"
    );
}

#[test]
fn capabilities_reports_web_engine_flag() {
    let paths = test_app_paths("capabilities");
    let state = test_bridge_state(None);
    let body = json_response_body(
        &String::from_utf8(bridge_response(
            "GET /capabilities HTTP/1.1\r\n",
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert!(
        body["web_engine"].is_boolean(),
        "capabilities reports the web_engine flag: {body}"
    );
    // The default test build has no web-engine feature.
    assert_eq!(
        body["web_engine"],
        serde_json::json!(cfg!(feature = "web-engine"))
    );
}

#[test]
fn capabilities_reports_web_renderer_kind() {
    let paths = test_app_paths("capabilities-renderer");
    let state = test_bridge_state(None);
    let body = json_response_body(
        &String::from_utf8(bridge_response(
            "GET /capabilities HTTP/1.1\r\n",
            &paths,
            &state,
        ))
        .unwrap(),
    );
    let kind = body["web_renderer"]
        .as_str()
        .expect("capabilities must report a web_renderer string");
    // The value is host-dependent (which renderer binaries exist on PATH),
    // so assert membership rather than a fixed value.
    assert!(
        ["qml", "chromium", "none"].contains(&kind),
        "web_renderer must be qml | chromium | none, got: {kind}"
    );
    // A build without the web-engine feature can never render.
    if !cfg!(feature = "web-engine") {
        assert_eq!(kind, "none", "non-web-engine build must report none");
    }
}

#[test]
fn text_window_returns_paged_rows() {
    let files = test_file_root("text-window");
    let left = files.join("left.txt");
    let right = files.join("right.txt");
    // 10 lines each, line 5 differs.
    let mk = |marker: &str| {
        (1..=10)
            .map(|n| {
                if n == 5 {
                    format!("line{n}-{marker}\n")
                } else {
                    format!("line{n}\n")
                }
            })
            .collect::<String>()
    };
    std::fs::write(&left, mk("L")).unwrap();
    std::fs::write(&right, mk("R")).unwrap();
    let paths = test_app_paths("text-window");
    let state = test_bridge_state(None);

    // Full window: total rows reported, hasMore false.
    let all = json_response_body(
        &String::from_utf8(bridge_response(
            &format!(
                "GET /compare/text/window?left={}&right={} HTTP/1.1\r\n",
                urlencoding::encode(left.to_str().unwrap()),
                urlencoding::encode(right.to_str().unwrap())
            ),
            &paths,
            &state,
        ))
        .unwrap(),
    );
    let total = all["totalRows"].as_u64().expect("totalRows present");
    assert!(total >= 10, "all view rows should be reported, got {total}");
    assert_eq!(all["hasMore"], serde_json::json!(false));

    // A bounded window returns only `limit` rows and reports more remain.
    let page = json_response_body(
        &String::from_utf8(bridge_response(
            &format!(
                "GET /compare/text/window?left={}&right={}&offset=0&limit=3 HTTP/1.1\r\n",
                urlencoding::encode(left.to_str().unwrap()),
                urlencoding::encode(right.to_str().unwrap())
            ),
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert_eq!(
        page["totalRows"].as_u64(),
        Some(total),
        "total stays stable"
    );
    // The window returns the same left_rows/right_rows split the /compare
    // response embeds, so a fetched window appends seamlessly.
    assert_eq!(
        page["left_rows"].as_array().unwrap().len(),
        3,
        "window honors limit on the left side"
    );
    assert_eq!(
        page["right_rows"].as_array().unwrap().len(),
        3,
        "window honors limit on the right side"
    );
    assert_eq!(page["returned"], serde_json::json!(3));
    assert_eq!(page["offset"], serde_json::json!(0));
    assert_eq!(page["hasMore"], serde_json::json!(true));

    // A second window picks up exactly where the first left off — its first
    // row is the row after the previous window's last (rows are split so
    // each side carries the same per-row text the /compare path produced).
    let next = json_response_body(
        &String::from_utf8(bridge_response(
            &format!(
                "GET /compare/text/window?left={}&right={}&offset=3&limit=3 HTTP/1.1\r\n",
                urlencoding::encode(left.to_str().unwrap()),
                urlencoding::encode(right.to_str().unwrap())
            ),
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert_eq!(next["offset"], serde_json::json!(3));
    assert_eq!(
        next["left_rows"][0]["text"], all["left_rows"][3]["text"],
        "the next window continues from where the first ended"
    );
}

#[test]
fn large_text_compare_response_is_windowed() {
    // A diff larger than TEXT_WINDOW_THRESHOLD must come back windowed: only
    // the first window of rows embedded, the full row count in total_rows,
    // and the full change-row index list (covering changes BEYOND the
    // window) so next/prev-change navigation still reaches them.
    let files = test_file_root("text-windowed-compare");
    let left = files.join("left.txt");
    let right = files.join("right.txt");
    let total_lines = TEXT_WINDOW_THRESHOLD + 500;
    // Differ on the last line — well past the first window — so a correct
    // diff_row_indexes must contain an index >= TEXT_WINDOW_THRESHOLD.
    let mk = |last: &str| {
        (1..=total_lines)
            .map(|n| {
                if n == total_lines {
                    format!("line{n}-{last}\n")
                } else {
                    format!("line{n}\n")
                }
            })
            .collect::<String>()
    };
    std::fs::write(&left, mk("L")).unwrap();
    std::fs::write(&right, mk("R")).unwrap();
    let paths = test_app_paths("text-windowed-compare");
    let state = test_bridge_state(None);

    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Text HTTP/1.1\r\n",
            urlencoding::encode(left.to_str().unwrap()),
            urlencoding::encode(right.to_str().unwrap())
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let tab = &body["session"]["tabs"][0];

    let total = tab["total_rows"].as_u64().expect("total_rows present");
    assert!(
        total as usize >= total_lines,
        "total_rows reports the full diff length, got {total}"
    );
    assert_eq!(
        tab["left_rows"].as_array().unwrap().len(),
        TEXT_WINDOW_THRESHOLD,
        "only the first window of rows is embedded"
    );
    let indexes = tab["diff_row_indexes"]
        .as_array()
        .expect("diff_row_indexes present");
    assert!(
        indexes
            .iter()
            .filter_map(|v| v.as_u64())
            .any(|i| i as usize >= TEXT_WINDOW_THRESHOLD),
        "the change beyond the first window is still in the navigation index"
    );

    // A small diff must NOT be windowed (no total_rows / diff_row_indexes,
    // every row embedded) so the common path is byte-for-byte unchanged.
    let small_left = files.join("small-left.txt");
    let small_right = files.join("small-right.txt");
    std::fs::write(&small_left, "a\nb\nc\n").unwrap();
    std::fs::write(&small_right, "a\nB\nc\n").unwrap();
    let small = json_response_body(
        &String::from_utf8(bridge_response(
            &format!(
                "GET /compare?left={}&right={}&mode=Text HTTP/1.1\r\n",
                urlencoding::encode(small_left.to_str().unwrap()),
                urlencoding::encode(small_right.to_str().unwrap())
            ),
            &paths,
            &state,
        ))
        .unwrap(),
    );
    let small_tab = &small["session"]["tabs"][0];
    assert!(
        small_tab["total_rows"].is_null(),
        "small diffs are not windowed (total_rows omitted)"
    );
    assert!(
        small_tab["diff_row_indexes"]
            .as_array()
            .is_none_or(|v| v.is_empty()),
        "small diffs carry no server-side navigation index"
    );
}

#[test]
fn plugins_list_response_includes_sandbox_status() {
    let paths = test_app_paths("plugins-sandbox-status");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /plugins/list HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    assert!(
        body["sandbox"]["label"].is_string(),
        "plugins/list should report the sandbox confinement label: {body}"
    );
    assert!(
        body["sandbox"]["confined"].is_boolean(),
        "plugins/list should report whether plugin helpers run confined: {body}"
    );
}

#[test]
fn folder_query_filters_and_paginates() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/folders/left");
    let right = fixture_root.join("tests/fixtures/folders/right");
    let paths = test_app_paths("folder-query");
    let state = test_bridge_state(None);

    // Unpaged: capture the full match count.
    let all = json_response_body(
        &String::from_utf8(bridge_response(
            &format!(
                "GET /folder/query?left={}&right={} HTTP/1.1\r\n",
                urlencoding::encode(left.to_str().unwrap()),
                urlencoding::encode(right.to_str().unwrap())
            ),
            &paths,
            &state,
        ))
        .expect("utf-8"),
    );
    let total = all["totalMatched"].as_u64().expect("totalMatched present");
    assert!(total > 1, "fixture folders should yield several entries");
    assert_eq!(all["hasMore"], serde_json::json!(false));

    // limit=1 returns a single entry and reports more remain.
    let page = json_response_body(
        &String::from_utf8(bridge_response(
            &format!(
                "GET /folder/query?left={}&right={}&limit=1 HTTP/1.1\r\n",
                urlencoding::encode(left.to_str().unwrap()),
                urlencoding::encode(right.to_str().unwrap())
            ),
            &paths,
            &state,
        ))
        .expect("utf-8"),
    );
    assert_eq!(page["totalMatched"].as_u64(), Some(total));
    assert_eq!(page["entries"].as_array().unwrap().len(), 1);
    assert_eq!(page["hasMore"], serde_json::json!(true));
}

fn dummy_folder_entry(i: usize) -> GuiFolderEntry {
    GuiFolderEntry {
        path: format!("file_{i:06}.txt"),
        is_dir: false,
        entry_type: "file".to_owned(),
        state: if i.is_multiple_of(2) {
            "changed"
        } else {
            "equal"
        }
        .to_owned(),
        left_size: Some(1),
        right_size: Some(2),
        left_modified: None,
        right_modified: None,
        method: "Content".to_owned(),
    }
}

fn folder_tab_with(entries: Vec<GuiFolderEntry>) -> GuiCompareTab {
    compare_tab(
        "Folder",
        ("/l".to_owned(), "/r".to_owned()),
        "ok".to_owned(),
        0,
        GuiOpenValidation {
            compatible: true,
            path_kind: "Folders".to_owned(),
            message: String::new(),
        },
        vec![],
        (vec![], vec![]),
        entries,
        None,
        None,
        Vec::new(),
        None,
    )
}

#[test]
fn large_folder_response_is_windowed_on_the_wire() {
    // A folder over FOLDER_WINDOW_THRESHOLD entries is windowed for the wire:
    // folder_total carries the full count and only the first page is embedded
    // (the GUI pages the rest via /folder/query). The canonical tab stays
    // full — this windows a clone at serialization.
    let total = FOLDER_WINDOW_THRESHOLD + 7;
    let entries: Vec<GuiFolderEntry> = (0..total).map(dummy_folder_entry).collect();
    let ctx = GuiLaunchContext::single_tab(folder_tab_with(entries));
    let value = context_to_value(&ctx).expect("serialize");
    let wire = &value["session"]["tabs"][0];
    assert_eq!(
        wire["folder_total"].as_u64(),
        Some(total as u64),
        "the full entry count is reported"
    );
    assert_eq!(
        wire["folder_entries"].as_array().unwrap().len(),
        FOLDER_WINDOW_THRESHOLD,
        "only the first page is embedded"
    );

    // A small folder is NOT windowed (no folder_total; every entry embedded).
    let small =
        GuiLaunchContext::single_tab(folder_tab_with((0..3).map(dummy_folder_entry).collect()));
    let small_value = context_to_value(&small).expect("serialize");
    let small_wire = &small_value["session"]["tabs"][0];
    assert!(
        small_wire["folder_total"].is_null(),
        "small folders are not windowed"
    );
    assert_eq!(small_wire["folder_entries"].as_array().unwrap().len(), 3);
}

#[test]
fn folder_query_honors_state_filter_and_sort_direction() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/folders/left");
    let right = fixture_root.join("tests/fixtures/folders/right");
    let paths = test_app_paths("folder-query-sort");
    let state = test_bridge_state(None);

    let query = |q: &str| -> serde_json::Value {
        json_response_body(
            &String::from_utf8(bridge_response(
                &format!(
                    "GET /folder/query?left={}&right={}&{q} HTTP/1.1\r\n",
                    urlencoding::encode(left.to_str().unwrap()),
                    urlencoding::encode(right.to_str().unwrap())
                ),
                &paths,
                &state,
            ))
            .expect("utf-8"),
        )
    };

    // `state=changed` returns only differing entries (fewer than the total).
    let all = query("");
    let changed = query("state=changed");
    let total = all["totalMatched"].as_u64().unwrap();
    let changed_total = changed["totalMatched"].as_u64().unwrap();
    assert!(
        changed_total > 0 && changed_total <= total,
        "state filter narrows to differing entries ({changed_total} of {total})"
    );
    for entry in changed["entries"].as_array().unwrap() {
        let s = entry["state"].as_str().unwrap_or("");
        assert!(
            matches!(s, "left_only" | "right_only" | "changed"),
            "state=changed must exclude equal entries, saw '{s}'"
        );
    }

    // `sort=path&descending=1` reverses the path order of the first page.
    let asc = query("sort=path");
    let desc = query("sort=path&descending=1");
    let first_asc = asc["entries"][0]["path"].as_str().unwrap_or("");
    let first_desc = desc["entries"][0]["path"].as_str().unwrap_or("");
    assert_ne!(
        first_asc, first_desc,
        "descending sort should change which entry is first"
    );
}

#[test]
fn folder_query_paginates_a_large_windowed_folder() {
    // The windowed-folder path is served via /folder/query a page at a time.
    // Generate a >FOLDER_WINDOW_THRESHOLD entry pair and verify the GUI's
    // paging contract: a bounded page, an accurate total, and offset paging
    // that walks the whole set. (Visual ListView rendering can't be
    // confirmed under the no-WM Xvfb review harness — this asserts the model
    // the view consumes, which is the part that was previously only
    // serialization-tested.)
    let root = test_file_root("folder-window-page");
    let left = root.join("left");
    let right = root.join("right");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let count = FOLDER_WINDOW_THRESHOLD + 50;
    for i in 0..count {
        // Same content on both sides → equal entries; cheap to create.
        let name = format!("file-{i:05}.txt");
        std::fs::write(left.join(&name), b"x").unwrap();
        std::fs::write(right.join(&name), b"x").unwrap();
    }

    let paths = test_app_paths("folder-window-page");
    let state = test_bridge_state(None);
    let page = |offset: usize, limit: usize| -> serde_json::Value {
        json_response_body(
            &String::from_utf8(bridge_response(
                &format!(
                    "GET /folder/query?left={}&right={}&offset={offset}&limit={limit} HTTP/1.1\r\n",
                    urlencoding::encode(left.to_str().unwrap()),
                    urlencoding::encode(right.to_str().unwrap())
                ),
                &paths,
                &state,
            ))
            .expect("utf-8"),
        )
    };

    let first = page(0, 5000);
    assert_eq!(
        first["totalMatched"].as_u64().unwrap(),
        count as u64,
        "the full entry count is reported"
    );
    assert_eq!(
        first["entries"].as_array().unwrap().len(),
        5000,
        "the page is bounded by the requested limit"
    );
    assert_eq!(first["hasMore"], serde_json::json!(true));

    // The next page returns the remainder and reports no more pages.
    let second = page(5000, 5000);
    assert_eq!(
        second["entries"].as_array().unwrap().len(),
        count - 5000,
        "the second page carries the remaining entries"
    );
    assert_eq!(second["offset"].as_u64().unwrap(), 5000);
    assert_eq!(second["hasMore"], serde_json::json!(false));
}

#[test]
fn folder_query_handles_empty_folders() {
    // Boundary case: two empty folders must page cleanly — zero total, no
    // entries, no further pages.
    let root = test_file_root("folder-query-empty");
    let left = root.join("left");
    let right = root.join("right");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let paths = test_app_paths("folder-query-empty");
    let state = test_bridge_state(None);
    let body = json_response_body(
        &String::from_utf8(bridge_response(
            &format!(
                "GET /folder/query?left={}&right={}&offset=0&limit=5000 HTTP/1.1\r\n",
                urlencoding::encode(left.to_str().unwrap()),
                urlencoding::encode(right.to_str().unwrap())
            ),
            &paths,
            &state,
        ))
        .expect("utf-8"),
    );
    assert_eq!(body["totalMatched"].as_u64().unwrap(), 0);
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);
    assert_eq!(body["hasMore"], serde_json::json!(false));
}

#[test]
fn plugins_diagnostic_returns_structured_verdict() {
    let paths = test_app_paths("plugins-diagnostic");
    // Install a fixture plugin with a helper that answers a probe.
    let plugin_dir = paths.user_plugins_dir().join("test.diag");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let helper = plugin_dir.join("run.sh");
    std::fs::write(
            &helper,
            "#!/bin/sh\nread request\nprintf '%s\\n' '{\"protocol_version\":1,\"request_id\":\"x\",\"status\":\"ok\",\"outputs\":[],\"diagnostics\":[]}'\n",
        )
        .unwrap();
    std::fs::write(
        plugin_dir.join("linsync-plugin.json"),
        r#"{
              "schema_version": 1,
              "id": "test.diag",
              "name": "Diagnostic Fixture",
              "version": "1.0.0",
              "license": "GPL-3.0-only",
              "entry": ["./run.sh"],
              "classes": ["prediffer"],
              "mime_types": ["text/plain"],
              "extensions": ["txt"],
              "capabilities": [],
              "deterministic": true,
              "sandbox": { "network": false, "writes_input": false, "requires_home_access": false },
              "options_schema": []
            }"#,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&helper).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&helper, perms).unwrap();
    }
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /plugins/diagnostic?id=test.diag HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    // The endpoint discovered the plugin, ran a probe, and returned a
    // structured verdict including the sandbox confinement. (The probe's
    // healthy/exit outcome is exercised by linsync-core's probe_plugin
    // tests; here we verify the bridge wraps it.)
    assert_eq!(body["id"], "test.diag", "diagnostic body: {body}");
    assert!(
        body["healthy"].is_boolean(),
        "diagnostic reports a health verdict: {body}"
    );
    assert!(
        body["sandbox"]["label"].is_string(),
        "diagnostic reports the sandbox confinement: {body}"
    );

    // An unknown plugin id is a 404.
    let missing = String::from_utf8(bridge_response(
        "GET /plugins/diagnostic?id=does.not.exist HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        missing.starts_with("HTTP/1.1 404"),
        "unknown plugin id should 404: {missing}"
    );
}

#[test]
fn plugins_install_and_remove_round_trip() {
    let paths = test_app_paths("plugins-install");
    let state = test_bridge_state(None);

    // Stage a valid plugin OUTSIDE the user plugins dir.
    let source = test_file_root("plugins-install-src").join("staged");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("run.sh"), "#!/bin/sh\n").unwrap();
    std::fs::write(
        source.join("linsync-plugin.json"),
        r#"{
              "schema_version": 1,
              "id": "test.installable",
              "name": "Installable Fixture",
              "version": "1.0.0",
              "license": "GPL-3.0-only",
              "entry": ["./run.sh"],
              "classes": ["prediffer"],
              "mime_types": ["text/plain"],
              "extensions": ["txt"],
              "capabilities": [],
              "deterministic": true,
              "sandbox": { "network": false, "writes_input": false, "requires_home_access": false },
              "options_schema": []
            }"#,
    )
    .unwrap();

    // Install via the bridge.
    let install = String::from_utf8(bridge_response(
        &format!(
            "GET /plugins/install?path={} HTTP/1.1\r\n",
            urlencoding::encode(source.to_str().unwrap())
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&install);
    assert_eq!(body["ok"], serde_json::json!(true), "install body: {body}");
    assert_eq!(body["id"], "test.installable");
    assert!(
        paths
            .user_plugins_dir()
            .join("test.installable/linsync-plugin.json")
            .exists()
    );

    // Re-installing the same id is a 409 Conflict.
    let dup = String::from_utf8(bridge_response(
        &format!(
            "GET /plugins/install?path={} HTTP/1.1\r\n",
            urlencoding::encode(source.to_str().unwrap())
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        dup.starts_with("HTTP/1.1 409"),
        "duplicate install should 409: {dup}"
    );

    // Remove via the bridge.
    let remove = String::from_utf8(bridge_response(
        "GET /plugins/remove?id=test.installable HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert_eq!(
        json_response_body(&remove)["ok"],
        serde_json::json!(true),
        "remove body: {remove}"
    );
    assert!(!paths.user_plugins_dir().join("test.installable").exists());

    // Removing again is a 404.
    let gone = String::from_utf8(bridge_response(
        "GET /plugins/remove?id=test.installable HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        gone.starts_with("HTTP/1.1 404"),
        "removing absent plugin should 404: {gone}"
    );
}

#[test]
fn plugins_trust_endpoint_and_list_field() {
    let paths = test_app_paths("plugins-trust");
    // Install a fixture so it appears in /plugins/list.
    let plugin_dir = paths.user_plugins_dir().join("test.trustable");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("run.sh"), "#!/bin/sh\n").unwrap();
    std::fs::write(
        plugin_dir.join("linsync-plugin.json"),
        r#"{
              "schema_version": 1,
              "id": "test.trustable",
              "name": "Trustable Fixture",
              "version": "1.0.0",
              "license": "GPL-3.0-only",
              "entry": ["./run.sh"],
              "classes": ["prediffer"],
              "mime_types": ["text/plain"],
              "extensions": ["txt"],
              "capabilities": [],
              "deterministic": true,
              "sandbox": { "network": false, "writes_input": false, "requires_home_access": false },
              "options_schema": []
            }"#,
    )
    .unwrap();
    let state = test_bridge_state(None);

    let trusted_in_list = || {
        let body = json_response_body(
            &String::from_utf8(bridge_response(
                "GET /plugins/list HTTP/1.1\r\n",
                &paths,
                &state,
            ))
            .unwrap(),
        );
        body["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "test.trustable")
            .unwrap()["trusted"]
            .clone()
    };

    // Discovered plugins start untrusted in the list payload.
    assert_eq!(trusted_in_list(), serde_json::json!(false));

    // Trust via the bridge, then the list reflects it.
    let resp = String::from_utf8(bridge_response(
        "GET /plugins/trust?id=test.trustable&trusted=true HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .unwrap();
    assert_eq!(json_response_body(&resp)["ok"], serde_json::json!(true));
    assert_eq!(trusted_in_list(), serde_json::json!(true));

    // Revoke trust.
    let _ = bridge_response(
        "GET /plugins/trust?id=test.trustable&trusted=false HTTP/1.1\r\n",
        &paths,
        &state,
    );
    assert_eq!(trusted_in_list(), serde_json::json!(false));
}

#[test]
fn launch_context_records_invalid_open_validation() {
    let context = build_context_for_paths(
        Path::new("/missing-left-for-linsync-test"),
        Path::new("/missing-right-for-linsync-test"),
    );
    let tab = context.active_tab().expect("active tab");

    assert_eq!(tab.mode, "Text");
    assert!(!tab.validation.compatible);
    assert_eq!(tab.validation.path_kind, "Invalid");
    assert!(tab.validation.message.contains("Cannot access left path"));
}

#[test]
fn invalid_launch_context_does_not_record_recent_stores() {
    let paths = test_app_paths("invalid-recent");
    let context = build_context_for_paths(
        Path::new("/missing-left-for-linsync-test"),
        Path::new("/missing-right-for-linsync-test"),
    );
    let context_path =
        write_launch_context(&paths, &context).expect("context write should succeed");

    assert!(context_path.is_file());
    let recent = RecentPathStore::new(paths.recent_paths_file(), 20)
        .load_or_default()
        .expect("recent paths should load");
    assert!(recent.paths.is_empty());
    let recent_sessions = RecentSessionStore::new(paths.recent_sessions_file(), 20)
        .load_or_default()
        .expect("recent sessions should load");
    assert!(recent_sessions.sessions.is_empty());
}

#[test]
fn persist_recent_paths_setting_gates_history() {
    let paths = test_app_paths("recent-privacy");
    let _ = fs::remove_dir_all(
        env::temp_dir().join(format!("linsync-gui-test-recent-privacy-{}", process::id())),
    );
    let files = test_file_root("recent-privacy-files");
    let left = files.join("a.txt");
    let right = files.join("b.txt");
    fs::write(&left, "one\n").unwrap();
    fs::write(&right, "two\n").unwrap();
    let context = build_context_for_paths(&left, &right);
    assert!(
        context.active_tab().unwrap().validation.compatible,
        "two real text files should validate as comparable"
    );
    let store = SettingsStore::new(paths.settings_file());

    // Privacy ON (persist disabled): recording is a no-op.
    let mut settings = Settings {
        persist_recent_paths: false,
        ..Settings::default()
    };
    store.save(&settings).unwrap();
    record_recent_context(&paths, &context);
    assert!(
        RecentPathStore::new(paths.recent_paths_file(), 20)
            .load_or_default()
            .unwrap()
            .paths
            .is_empty(),
        "no recent paths should be stored when persistence is off"
    );
    assert!(
        RecentSessionStore::new(paths.recent_sessions_file(), 20)
            .load_or_default()
            .unwrap()
            .sessions
            .is_empty(),
        "no recent session should be stored when persistence is off"
    );

    // Privacy OFF (persist enabled): the comparison is remembered.
    settings.persist_recent_paths = true;
    store.save(&settings).unwrap();
    record_recent_context(&paths, &context);
    assert!(
        !RecentPathStore::new(paths.recent_paths_file(), 20)
            .load_or_default()
            .unwrap()
            .paths
            .is_empty(),
        "recent paths should be stored once persistence is on"
    );
    assert!(
        !RecentSessionStore::new(paths.recent_sessions_file(), 20)
            .load_or_default()
            .unwrap()
            .sessions
            .is_empty(),
        "a recent session should be stored once persistence is on"
    );
}

#[test]
fn multi_tab_session_persists_and_restores_all_tabs() {
    let paths = test_app_paths("multitab");
    let _ = fs::remove_dir_all(
        env::temp_dir().join(format!("linsync-gui-test-multitab-{}", process::id())),
    );
    let files = test_file_root("multitab-files");
    // Two independent comparable pairs → two tabs.
    let (l1, r1) = (files.join("a1.txt"), files.join("b1.txt"));
    let (l2, r2) = (files.join("a2.txt"), files.join("b2.txt"));
    fs::write(&l1, "one\n").unwrap();
    fs::write(&r1, "ONE\n").unwrap();
    fs::write(&l2, "two\n").unwrap();
    fs::write(&r2, "TWO\n").unwrap();

    let mut tab1 = build_tab_for_paths(&l1, &r1);
    let mut tab2 = build_tab_for_paths(&l2, &r2);
    tab1.id = 1;
    tab2.id = 2;
    assert!(tab1.validation.compatible && tab2.validation.compatible);
    let context = GuiLaunchContext::from_tabs(vec![tab1, tab2], 2);

    record_recent_session(&paths, &context);

    // Load the recent session and restore the full multi-tab workspace.
    let recent = RecentSessionStore::new(paths.recent_sessions_file(), 20)
        .load_or_default()
        .expect("recent sessions load");
    let session = recent
        .sessions
        .first()
        .expect("a recent session was recorded");
    let restored =
        restore_multi_tab_context(session).expect("a multi-tab snapshot should be restored");
    assert_eq!(restored.session.tabs.len(), 2, "both tabs should restore");
    assert_eq!(
        restored.session.active_tab_id, 2,
        "the active tab id should round-trip"
    );
    let restored_paths: Vec<&str> = restored
        .session
        .tabs
        .iter()
        .map(|t| t.left_path.as_str())
        .collect();
    assert!(restored_paths.contains(&l1.to_str().unwrap()));
    assert!(restored_paths.contains(&l2.to_str().unwrap()));
}

#[test]
fn sessions_reopen_restores_multi_tab_workspace() {
    let paths = test_app_paths("multitab-reopen");
    let files = test_file_root("multitab-reopen-files");
    let (l1, r1) = (files.join("a1.txt"), files.join("b1.txt"));
    let (l2, r2) = (files.join("a2.txt"), files.join("b2.txt"));
    fs::write(&l1, "one\n").unwrap();
    fs::write(&r1, "ONE\n").unwrap();
    fs::write(&l2, "two\n").unwrap();
    fs::write(&r2, "TWO\n").unwrap();
    let mut tab1 = build_tab_for_paths(&l1, &r1);
    let mut tab2 = build_tab_for_paths(&l2, &r2);
    tab1.id = 1;
    tab2.id = 2;
    record_recent_session(&paths, &GuiLaunchContext::from_tabs(vec![tab1, tab2], 2));

    // Reopen into a fresh bridge session: both tabs come back, and the
    // tab that was active when the workspace was recorded is active.
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /sessions/reopen?index=0 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .unwrap();
    let body = json_response_body(&resp);
    let tabs = body["session"]["tabs"].as_array().expect("tabs array");
    assert_eq!(tabs.len(), 2, "both workspace tabs should reopen: {body}");
    let active_id = body["session"]["active_tab_id"].as_u64().unwrap();
    let active = tabs
        .iter()
        .find(|t| t["id"].as_u64() == Some(active_id))
        .expect("active tab present");
    assert_eq!(
        active["left_path"],
        serde_json::json!(l2.to_str().unwrap()),
        "the recorded active tab should be active again: {body}"
    );
}

#[test]
fn fixture_heuristic_only_matches_linsync_fixture_paths() {
    // Use the real workspace fixture path (resolved from CARGO_MANIFEST_DIR)
    // rather than a hardcoded absolute path, so this test is portable.
    let real_fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/text/left.txt");
    assert!(path_looks_like_internal_test_fixture(&real_fixture));
    assert!(path_looks_like_internal_test_fixture(Path::new(
        "/home/dev/linsync/tests/fixtures"
    )));
    // Another project's golden files must stay persistable.
    assert!(!path_looks_like_internal_test_fixture(Path::new(
        "/home/dev/myapp/tests/fixtures/expected.txt"
    )));
    // "linsync" alone (no fixtures dir) is not a fixture either.
    assert!(!path_looks_like_internal_test_fixture(Path::new(
        "/home/dev/linsync-notes/readme.md"
    )));
}

#[test]
fn project_save_and_open_round_trips_tabs() {
    let paths = test_app_paths("project-save");
    let files = test_file_root("project-save-files");
    let (l1, r1) = (files.join("p1a.txt"), files.join("p1b.txt"));
    let (l2, r2) = (files.join("p2a.txt"), files.join("p2b.txt"));
    fs::write(&l1, "one\n").unwrap();
    fs::write(&r1, "ONE\n").unwrap();
    fs::write(&l2, "two\n").unwrap();
    fs::write(&r2, "TWO\n").unwrap();
    let mut tab1 = build_tab_for_paths(&l1, &r1);
    let mut tab2 = build_tab_for_paths(&l2, &r2);
    tab1.id = 1;
    tab2.id = 2;
    let context = GuiLaunchContext::from_tabs(vec![tab1, tab2], 2);
    let state = test_bridge_state(Some(context));

    let project_path = files.join("workspace.linsync-project");
    // Save the open tabs as a project.
    let save = String::from_utf8(bridge_response(
        &format!(
            "GET /project/save?path={}&name=Demo HTTP/1.1\r\n",
            urlencoding::encode(project_path.to_str().unwrap())
        ),
        &paths,
        &state,
    ))
    .unwrap();
    let save_body = json_response_body(&save);
    assert_eq!(
        save_body["ok"],
        serde_json::json!(true),
        "save: {save_body}"
    );
    assert_eq!(save_body["sessions"], serde_json::json!(2));
    assert!(project_path.exists(), "project file should be written");

    // The saved project now appears in the recent-workspaces list.
    let recent = json_response_body(
        &String::from_utf8(bridge_response(
            "GET /project/recent HTTP/1.1\r\n",
            &paths,
            &state,
        ))
        .unwrap(),
    );
    assert!(
        recent["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["path"] == serde_json::json!(project_path.to_str().unwrap())),
        "recent workspaces should include the saved project: {recent}"
    );

    // Open it back: the response is a launch context with both tabs.
    let open = String::from_utf8(bridge_response(
        &format!(
            "GET /project/open?path={} HTTP/1.1\r\n",
            urlencoding::encode(project_path.to_str().unwrap())
        ),
        &paths,
        &state,
    ))
    .unwrap();
    let open_body = json_response_body(&open);
    assert_eq!(open_body["name"], "Demo");
    let tabs = open_body["session"]["tabs"].as_array().expect("tabs array");
    assert_eq!(tabs.len(), 2, "both comparisons should reopen: {open_body}");

    // Opening a missing project is a 400.
    let missing = String::from_utf8(bridge_response(
        "GET /project/open?path=/no/such/workspace.linsync-project HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .unwrap();
    assert!(
        missing.starts_with("HTTP/1.1 400"),
        "missing project: {missing}"
    );
}

#[test]
fn single_tab_session_has_no_multi_tab_snapshot() {
    let paths = test_app_paths("single-tab-nomulti");
    let _ = fs::remove_dir_all(env::temp_dir().join(format!(
        "linsync-gui-test-single-tab-nomulti-{}",
        process::id()
    )));
    let files = test_file_root("single-tab-files");
    let (l, r) = (files.join("a.txt"), files.join("b.txt"));
    fs::write(&l, "x\n").unwrap();
    fs::write(&r, "y\n").unwrap();
    let context = build_context_for_paths(&l, &r);

    record_recent_session(&paths, &context);
    let recent = RecentSessionStore::new(paths.recent_sessions_file(), 20)
        .load_or_default()
        .unwrap();
    let session = recent.sessions.first().unwrap();
    // A single open tab keeps the snapshot out of the file (it round-trips
    // through the normal session fields), so multi-tab restore declines.
    assert!(restore_multi_tab_context(session).is_none());
}

#[cfg(unix)]
#[test]
fn owner_only_write_tightens_existing_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = test_file_root("owner-only");
    let path = root.join("context.json");
    fs::write(&path, b"old").expect("seed file should write");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
        .expect("seed permissions should update");

    write_owner_only(&path, b"new").expect("owner-only write should succeed");

    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(fs::read_to_string(path).unwrap(), "new");
}

#[test]
fn launch_context_records_recent_paths_and_sessions_in_xdg_store() {
    // Use files that are *not* under tests/fixtures so the "don't record
    // internal test fixtures as recent" guard does not suppress them.
    let root = test_file_root("recent-record");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "hello\nworld\n").expect("write left");
    fs::write(&right, "hello\nthere\n").expect("write right");

    let paths = test_app_paths("recent");
    let context = build_context_for_paths(&left, &right);
    let context_path =
        write_launch_context(&paths, &context).expect("context write should succeed");

    assert!(context_path.is_file());
    let recent = RecentPathStore::new(paths.recent_paths_file(), 20)
        .load_or_default()
        .expect("recent paths should load");
    assert!(recent.paths.contains(&left));
    assert!(recent.paths.contains(&right));
    let recent_sessions = RecentSessionStore::new(paths.recent_sessions_file(), 20)
        .load_or_default()
        .expect("recent sessions should load");
    assert_eq!(recent_sessions.sessions.len(), 1);
    assert_eq!(recent_sessions.sessions[0].session.left, left);
    assert_eq!(recent_sessions.sessions[0].session.right, right);
    assert_eq!(
        recent_sessions.sessions[0].selected_view,
        CompareViewMode::Text
    );
}

#[test]
fn bridge_decodes_percent_encoded_query_values() {
    assert_eq!(
        percent_decode("left%20path%2Ffile%2Etxt"),
        "left path/file.txt"
    );
}

#[test]
fn bridge_response_serves_compare_context() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/text/left.txt");
    let right = fixture_root.join("tests/fixtures/text/right.txt");
    let request = format!(
        "GET /compare?left={}&right={} HTTP/1.1\r\n",
        left.display(),
        right.display()
    );
    let paths = test_app_paths("bridge-context");
    let state = test_bridge_state(None);
    let response =
        String::from_utf8(bridge_response(&request, &paths, &state)).expect("utf-8 response");

    assert!(response.contains("HTTP/1.1 200 OK"));
    assert!(response.contains(r#""mode":"Text""#));
    assert!(response.contains(r#""left_rows""#));
}

#[test]
fn bridge_settings_round_trip_through_core_store() {
    let paths = test_app_paths("bridge-settings");
    let state = test_bridge_state(None);
    let response = String::from_utf8(bridge_response(
        "GET /settings HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");

    assert!(response.contains("HTTP/1.1 200 OK"));
    let body = json_response_body(&response);
    assert_eq!(body["themePreference"], 0);
    assert_eq!(body["maxRecentPaths"], 20);
    assert_eq!(body["reduceMotion"], false);
    assert_eq!(body["keepArchiveBackup"], false);

    let response = String::from_utf8(bridge_response(
        "GET /settings/set?key=keepArchiveBackup&value=true HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");

    assert!(response.contains("HTTP/1.1 200 OK"));
    let body = json_response_body(&response);
    assert_eq!(body["keepArchiveBackup"], true);
    let settings = SettingsStore::new(paths.settings_file())
        .load_or_default()
        .expect("settings should load");
    assert!(settings.keep_archive_backup);

    let response = String::from_utf8(bridge_response(
        "GET /settings/set?key=liveCompare&value=true HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");

    assert!(response.contains("HTTP/1.1 200 OK"));
    let body = json_response_body(&response);
    assert_eq!(body["liveCompare"], serde_json::json!(true));
    let settings = SettingsStore::new(paths.settings_file())
        .load_or_default()
        .expect("settings should load");
    assert!(settings.live_compare);

    let response = String::from_utf8(bridge_response(
        "GET /settings/set?key=reduceMotion&value=true HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");

    assert!(response.contains("HTTP/1.1 200 OK"));
    let body = json_response_body(&response);
    assert_eq!(body["reduceMotion"], true);
    let settings = SettingsStore::new(paths.settings_file())
        .load_or_default()
        .expect("settings should load");
    assert!(settings.reduce_motion);

    let response = String::from_utf8(bridge_response(
        "GET /settings/set?key=themePreference&value=12 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");

    assert!(response.contains("HTTP/1.1 200 OK"));
    let body = json_response_body(&response);
    assert_eq!(body["themePreference"], 12);
    let settings = SettingsStore::new(paths.settings_file())
        .load_or_default()
        .expect("settings should load");
    assert_eq!(settings.theme_preference, ThemePreference::OledBlack);

    let response = String::from_utf8(bridge_response(
        "GET /settings/reset HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");

    assert!(response.contains("HTTP/1.1 200 OK"));
    let body = json_response_body(&response);
    assert_eq!(body["themePreference"], 0);
    assert_eq!(body["reduceMotion"], false);
}

#[test]
fn bridge_compare_endpoint_honors_text_mode_override_for_table_files() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/table/left.csv");
    let right = fixture_root.join("tests/fixtures/table/right.csv");
    let request = format!(
        "GET /compare?left={}&right={}&mode=Text HTTP/1.1\r\n",
        left.display(),
        right.display()
    );
    let paths = test_app_paths("bridge-text-override");
    let state = test_bridge_state(None);
    let response =
        String::from_utf8(bridge_response(&request, &paths, &state)).expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["tabs"][0]["mode"], "Text");
    assert_eq!(
        body["session"]["tabs"][0]["validation"]["path_kind"],
        "Files"
    );
}

#[test]
fn bridge_bookmark_set_updates_active_tab_rows() {
    let root = test_file_root("bridge-bookmark-set");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "same\nleft\n").unwrap();
    fs::write(&right, "same\nright\n").unwrap();
    let paths = test_app_paths("bridge-bookmark-set");
    let state = test_bridge_state(Some(build_context_for_paths(&left, &right)));

    let response = String::from_utf8(bridge_response(
        "GET /bookmark/set?row=1&bookmarked=1 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&response);
    assert_eq!(
        body["session"]["tabs"][0]["left_rows"][1]["bookmarked"],
        serde_json::json!(true)
    );
    assert_eq!(
        body["session"]["tabs"][0]["right_rows"][1]["bookmarked"],
        serde_json::json!(true)
    );
}

#[test]
fn bridge_compare_endpoint_honors_hex_mode_override_for_text_files() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/text/left.txt");
    let right = fixture_root.join("tests/fixtures/text/right.txt");
    let request = format!(
        "GET /compare?left={}&right={}&mode=Hex HTTP/1.1\r\n",
        left.display(),
        right.display()
    );
    let paths = test_app_paths("bridge-hex-override");
    let state = test_bridge_state(None);
    let response =
        String::from_utf8(bridge_response(&request, &paths, &state)).expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["tabs"][0]["mode"], "Hex");
    assert_eq!(
        body["session"]["tabs"][0]["validation"]["path_kind"],
        "Files"
    );
}

#[test]
fn bridge_compare_endpoint_rejects_folder_mode_for_files() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/text/left.txt");
    let right = fixture_root.join("tests/fixtures/text/right.txt");
    let request = format!(
        "GET /compare?left={}&right={}&mode=Folder HTTP/1.1\r\n",
        left.display(),
        right.display()
    );
    let paths = test_app_paths("bridge-folder-override-invalid");
    let state = test_bridge_state(None);
    let response =
        String::from_utf8(bridge_response(&request, &paths, &state)).expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["tabs"][0]["mode"], "Folder");
    assert_eq!(
        body["session"]["tabs"][0]["validation"]["compatible"],
        false
    );
    assert!(
        body["session"]["tabs"][0]["status"]
            .as_str()
            .unwrap()
            .contains("requires two folders")
    );
}

#[test]
fn bridge_session_preserves_active_tab_and_can_open_new_tab() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let first_left = fixture_root.join("tests/fixtures/text/left.txt");
    let first_right = fixture_root.join("tests/fixtures/text/right.txt");
    let second_left = fixture_root.join("tests/fixtures/folders/left");
    let second_right = fixture_root.join("tests/fixtures/folders/right");
    let paths = test_app_paths("bridge-session");
    let initial_context = build_context_for_paths(&first_left, &first_right);
    let initial_row_id = initial_context.active_tab().expect("active tab").left_rows[0]
        .row_id
        .clone();
    let state = test_bridge_state(Some(initial_context));

    let same_request = format!(
        "GET /compare?left={}&right={} HTTP/1.1\r\n",
        first_left.display(),
        first_right.display()
    );
    let same_response =
        String::from_utf8(bridge_response(&same_request, &paths, &state)).expect("utf-8 response");
    let same_body = json_response_body(&same_response);

    assert_eq!(same_body["session"]["active_tab_id"], 1);
    assert_eq!(same_body["session"]["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(
        same_body["session"]["tabs"][0]["left_rows"][0]["row_id"],
        initial_row_id
    );

    let new_tab_request = format!(
        "GET /compare?left={}&right={}&new_tab=1 HTTP/1.1\r\n",
        second_left.display(),
        second_right.display()
    );
    let new_tab_response = String::from_utf8(bridge_response(&new_tab_request, &paths, &state))
        .expect("utf-8 response");
    let new_tab_body = json_response_body(&new_tab_response);

    assert_eq!(new_tab_body["session"]["active_tab_id"], 2);
    assert_eq!(new_tab_body["session"]["tabs"].as_array().unwrap().len(), 2);
    assert!(
        new_tab_body["session"]["recent_paths"]
            .as_array()
            .unwrap()
            .len()
            >= 4
    );
}

#[test]
fn bridge_can_activate_tab_before_merge_actions() {
    let root = test_file_root("bridge-activate-tab");
    let first_left = root.join("first-left.txt");
    let first_right = root.join("first-right.txt");
    let second_left = root.join("second-left.txt");
    let second_right = root.join("second-right.txt");
    fs::write(&first_left, "alpha\nfirst left\nomega\n").unwrap();
    fs::write(&first_right, "alpha\nfirst right\nomega\n").unwrap();
    fs::write(&second_left, "alpha\nsecond left\nomega\n").unwrap();
    fs::write(&second_right, "alpha\nsecond right\nomega\n").unwrap();

    let paths = test_app_paths("bridge-activate-tab");
    let state = test_bridge_state(Some(build_context_for_paths(&first_left, &first_right)));
    let new_tab_request = format!(
        "GET /compare?left={}&right={}&new_tab=1 HTTP/1.1\r\n",
        second_left.display(),
        second_right.display()
    );
    let _ = bridge_response(&new_tab_request, &paths, &state);

    let activate_response = String::from_utf8(bridge_response(
        "GET /tab/activate?id=1 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let activate_body = json_response_body(&activate_response);
    assert_eq!(activate_body["session"]["active_tab_id"], 1);

    let copy_response = String::from_utf8(bridge_response(
        "GET /copy-all?direction=left_to_right HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let copy_body = json_response_body(&copy_response);

    assert_eq!(copy_body["session"]["active_tab_id"], 1);
    assert_eq!(copy_body["session"]["tabs"][0]["right_dirty"], true);
    assert_eq!(copy_body["session"]["tabs"][0]["difference_count"], 0);
    assert_eq!(copy_body["session"]["tabs"][1]["right_dirty"], false);
    assert_eq!(copy_body["session"]["tabs"][1]["difference_count"], 1);
}

#[test]
fn bridge_can_close_active_tab() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let first_left = fixture_root.join("tests/fixtures/text/left.txt");
    let first_right = fixture_root.join("tests/fixtures/text/right.txt");
    let second_left = fixture_root.join("tests/fixtures/folders/left");
    let second_right = fixture_root.join("tests/fixtures/folders/right");
    let paths = test_app_paths("bridge-close-tab");
    let state = test_bridge_state(Some(build_context_for_paths(&first_left, &first_right)));
    let new_tab_request = format!(
        "GET /compare?left={}&right={}&new_tab=1 HTTP/1.1\r\n",
        second_left.display(),
        second_right.display()
    );
    let _ = bridge_response(&new_tab_request, &paths, &state);

    let response = String::from_utf8(bridge_response(
        "GET /tab/close?id=2 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["active_tab_id"], 1);
    assert_eq!(body["session"]["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(body["session"]["tabs"][0]["mode"], "Text");
}

#[test]
fn bridge_state_copy_row_updates_rows_and_dirty_side() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/text/left.txt");
    let right = fixture_root.join("tests/fixtures/text/right.txt");
    let mut state = GuiBridgeState::new(Some(build_context_for_paths(&left, &right)));
    let row = state
        .context()
        .active_tab()
        .expect("active tab")
        .left_rows
        .iter()
        .position(|row| row.state == "changed")
        .expect("changed row");

    let context = state
        .copy_row(row, "left_to_right")
        .expect("copy should succeed");
    let tab = context.active_tab().expect("active tab");

    assert!(tab.right_dirty);
    assert!(!tab.left_dirty);
    assert_eq!(tab.left_rows[row].text, tab.right_rows[row].text);
    assert_eq!(tab.left_rows[row].state, "equal");
    assert_eq!(tab.right_rows[row].state, "equal");
    assert_eq!(tab.difference_count, 0);
    assert_eq!(tab.status, "Copied left to right");
}

#[test]
fn bridge_state_copy_row_applies_whole_text_diff_block() {
    let root = test_file_root("copy-block");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nleft one\nleft two\nomega\n").unwrap();
    fs::write(&right, "alpha\nright one\nright two\nomega\n").unwrap();
    let mut state = GuiBridgeState::new(Some(build_context_for_paths(&left, &right)));
    let row = state
        .context()
        .active_tab()
        .expect("active tab")
        .left_rows
        .iter()
        .position(|row| row.state == "changed")
        .expect("changed row");

    let context = state
        .copy_row(row, "left_to_right")
        .expect("copy should succeed");
    let tab = context.active_tab().expect("active tab");

    assert!(tab.right_dirty);
    assert_eq!(tab.difference_count, 0);
    assert!(
        tab.right_rows
            .iter()
            .any(|row| row.text == "left one" && row.state == "equal")
    );
    assert!(
        tab.right_rows
            .iter()
            .any(|row| row.text == "left two" && row.state == "equal")
    );
}

#[test]
fn bridge_copy_endpoint_updates_session_state() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/text/left.txt");
    let right = fixture_root.join("tests/fixtures/text/right.txt");
    let paths = test_app_paths("bridge-copy");
    let initial_context = build_context_for_paths(&left, &right);
    let row = initial_context
        .active_tab()
        .expect("active tab")
        .left_rows
        .iter()
        .position(|row| row.state == "changed")
        .expect("changed row");
    let state = test_bridge_state(Some(initial_context));
    let request = format!("GET /copy?row={row}&direction=right_to_left HTTP/1.1\r\n");
    let response =
        String::from_utf8(bridge_response(&request, &paths, &state)).expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["tabs"][0]["left_dirty"], true);
    assert_eq!(body["session"]["tabs"][0]["right_dirty"], false);
    assert_eq!(body["session"]["tabs"][0]["difference_count"], 0);
    assert_eq!(body["session"]["tabs"][0]["status"], "Copied right to left");
}

#[test]
fn bridge_state_copy_all_applies_all_text_diff_blocks() {
    let root = test_file_root("copy-all-state");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nleft one\nshared\nleft two\nomega\n").unwrap();
    fs::write(&right, "alpha\nright one\nshared\nright two\nomega\n").unwrap();
    let mut state = GuiBridgeState::new(Some(build_context_for_paths(&left, &right)));

    let context = state
        .copy_all("left_to_right")
        .expect("copy all should succeed");
    let tab = context.active_tab().expect("active tab");

    assert!(tab.right_dirty);
    assert_eq!(tab.difference_count, 0);
    assert_eq!(tab.status, "Copied all left to right");
    assert!(tab.right_rows.iter().any(|row| row.text == "left one"));
    assert!(tab.right_rows.iter().any(|row| row.text == "left two"));
}

#[test]
fn bridge_state_undo_restores_previous_text_tab_snapshot() {
    let root = test_file_root("undo-state");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nbeta\n").unwrap();
    fs::write(&right, "alpha\ngamma\n").unwrap();
    let mut state = GuiBridgeState::new(Some(build_context_for_paths(&left, &right)));
    let row = state
        .context()
        .active_tab()
        .expect("active tab")
        .left_rows
        .iter()
        .position(|row| row.state == "changed")
        .expect("changed row");
    let changed = state
        .copy_row(row, "left_to_right")
        .expect("copy should succeed");
    assert!(changed.active_tab().expect("active tab").can_undo);

    let context = state.undo().expect("undo should succeed");
    let tab = context.active_tab().expect("active tab");

    assert!(!tab.can_undo);
    assert!(tab.can_redo);
    assert!(!tab.right_dirty);
    assert_eq!(tab.difference_count, 1);
    assert_eq!(tab.right_rows[row].text, "gamma");
    assert_eq!(tab.status, "Undid last merge action");
}

#[test]
fn bridge_state_redo_restores_undone_text_tab_snapshot() {
    let root = test_file_root("redo-state");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nbeta\n").unwrap();
    fs::write(&right, "alpha\ngamma\n").unwrap();
    let mut state = GuiBridgeState::new(Some(build_context_for_paths(&left, &right)));

    state
        .copy_all("left_to_right")
        .expect("copy all should succeed");
    state.undo().expect("undo should succeed");
    let context = state.redo().expect("redo should succeed");
    let tab = context.active_tab().expect("active tab");

    assert!(tab.can_undo);
    assert!(!tab.can_redo);
    assert!(tab.right_dirty);
    assert_eq!(tab.difference_count, 0);
    assert_eq!(tab.right_rows[1].text, "beta");
    assert_eq!(tab.status, "Redid last merge action");
}

#[test]
fn bridge_copy_all_endpoint_updates_session_state() {
    let root = test_file_root("copy-all-endpoint");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nleft one\nshared\nleft two\nomega\n").unwrap();
    fs::write(&right, "alpha\nright one\nshared\nright two\nomega\n").unwrap();
    let paths = test_app_paths("bridge-copy-all");
    let state = test_bridge_state(Some(build_context_for_paths(&left, &right)));
    let response = String::from_utf8(bridge_response(
        "GET /copy-all?direction=left_to_right HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["tabs"][0]["right_dirty"], true);
    assert_eq!(body["session"]["tabs"][0]["difference_count"], 0);
    assert_eq!(
        body["session"]["tabs"][0]["status"],
        "Copied all left to right"
    );
}

#[test]
fn bridge_undo_endpoint_restores_previous_session_state() {
    let root = test_file_root("undo-endpoint");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nbeta\n").unwrap();
    fs::write(&right, "alpha\ngamma\n").unwrap();
    let paths = test_app_paths("bridge-undo");
    let state = test_bridge_state(Some(build_context_for_paths(&left, &right)));
    let _ = bridge_response(
        "GET /copy-all?direction=left_to_right HTTP/1.1\r\n",
        &paths,
        &state,
    );
    let response = String::from_utf8(bridge_response("GET /undo HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["tabs"][0]["can_undo"], false);
    assert_eq!(body["session"]["tabs"][0]["can_redo"], true);
    assert_eq!(body["session"]["tabs"][0]["right_dirty"], false);
    assert_eq!(body["session"]["tabs"][0]["difference_count"], 1);
    assert_eq!(
        body["session"]["tabs"][0]["status"],
        "Undid last merge action"
    );
}

#[test]
fn bridge_redo_endpoint_restores_undone_session_state() {
    let root = test_file_root("redo-endpoint");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nbeta\n").unwrap();
    fs::write(&right, "alpha\ngamma\n").unwrap();
    let paths = test_app_paths("bridge-redo");
    let state = test_bridge_state(Some(build_context_for_paths(&left, &right)));
    let _ = bridge_response(
        "GET /copy-all?direction=left_to_right HTTP/1.1\r\n",
        &paths,
        &state,
    );
    let _ = bridge_response("GET /undo HTTP/1.1\r\n", &paths, &state);
    let response = String::from_utf8(bridge_response("GET /redo HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["tabs"][0]["can_undo"], true);
    assert_eq!(body["session"]["tabs"][0]["can_redo"], false);
    assert_eq!(body["session"]["tabs"][0]["right_dirty"], true);
    assert_eq!(body["session"]["tabs"][0]["difference_count"], 0);
    assert_eq!(
        body["session"]["tabs"][0]["status"],
        "Redid last merge action"
    );
}

#[test]
fn bridge_state_save_dirty_text_side_writes_backup_safe_file() {
    let root = test_file_root("save-state");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nbeta\n").unwrap();
    fs::write(&right, "alpha\ngamma\n").unwrap();
    let mut state = GuiBridgeState::new(Some(build_context_for_paths(&left, &right)));
    let row = state
        .context()
        .active_tab()
        .expect("active tab")
        .left_rows
        .iter()
        .position(|row| row.state == "changed")
        .expect("changed row");
    state
        .copy_row(row, "left_to_right")
        .expect("copy should succeed");

    let context = state.save_side("dirty").expect("save should succeed");
    let tab = context.active_tab().expect("active tab");

    assert_eq!(fs::read_to_string(&right).unwrap(), "alpha\nbeta\n");
    assert_eq!(
        fs::read_to_string(backup_path(&right)).unwrap(),
        "alpha\ngamma\n"
    );
    assert!(!tab.right_dirty);
    assert_eq!(tab.status, "Saved right");
}

#[test]
fn bridge_save_endpoint_writes_dirty_side() {
    let root = test_file_root("save-endpoint");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nbeta\n").unwrap();
    fs::write(&right, "alpha\ngamma\n").unwrap();
    let paths = test_app_paths("bridge-save");
    let mut initial_state = GuiBridgeState::new(Some(build_context_for_paths(&left, &right)));
    let row = initial_state
        .context()
        .active_tab()
        .expect("active tab")
        .left_rows
        .iter()
        .position(|row| row.state == "changed")
        .expect("changed row");
    initial_state
        .copy_row(row, "left_to_right")
        .expect("copy should succeed");
    let state = Arc::new(Mutex::new(initial_state));
    let response = String::from_utf8(bridge_response(
        "GET /save?side=dirty HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["tabs"][0]["right_dirty"], false);
    assert_eq!(fs::read_to_string(&right).unwrap(), "alpha\nbeta\n");
}

#[test]
fn bridge_session_endpoint_returns_current_state() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/text/left.txt");
    let right = fixture_root.join("tests/fixtures/text/right.txt");
    let paths = test_app_paths("bridge-session-endpoint");
    let state = test_bridge_state(Some(build_context_for_paths(&left, &right)));
    let response = String::from_utf8(bridge_response("GET /session HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    let body = json_response_body(&response);

    assert_eq!(body["session"]["active_tab_id"], 1);
    assert_eq!(body["session"]["tabs"][0]["mode"], "Text");
}

#[test]
fn context_json_includes_schema_version_for_all_transports() {
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/text/left.txt");
    let right = fixture_root.join("tests/fixtures/text/right.txt");
    let context = build_context_for_paths(&left, &right);
    let body: serde_json::Value =
        serde_json::from_str(&context_to_json(&context).expect("context JSON")).unwrap();
    assert_eq!(
        body["schema_version"],
        serde_json::json!(RESPONSE_SCHEMA_VERSION)
    );
    assert!(body["session"]["tabs"].is_array());
}

#[test]
fn bridge_server_answers_health_requests() {
    let bridge =
        start_bridge_server(test_app_paths("bridge-health"), None).expect("bridge should start");
    let (address, token_path) = bridge_address_and_token_path(&bridge.base_url);
    let mut stream = TcpStream::connect(address).expect("bridge should accept connections");
    let request = format!("GET {token_path}/health HTTP/1.1\r\nHost: localhost\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .expect("request should write");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("response should read");

    assert!(response.contains("HTTP/1.1 200 OK"));
    assert!(response.contains(r#""ok":true"#));
    assert!(response.contains(&format!(r#""bridge_version":{BRIDGE_VERSION}"#)));
    assert!(response.contains(&format!(r#""schema_version":{RESPONSE_SCHEMA_VERSION}"#)));
}

#[test]
fn bridge_responses_do_not_advertise_wildcard_cors() {
    let paths = test_app_paths("cors");
    let state = test_bridge_state(None);
    let response = String::from_utf8(bridge_response("GET /health HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    assert!(
        !response.contains("Access-Control-Allow-Origin"),
        "bridge must not advertise CORS to browser-origin pages: {response}"
    );
}

#[test]
fn bridge_rejects_cross_origin_requests() {
    let bridge = start_bridge_server(test_app_paths("origin"), None).expect("bridge should start");
    let (address, token_path) = bridge_address_and_token_path(&bridge.base_url);
    let mut stream = TcpStream::connect(address).expect("bridge should accept connections");
    let request = format!(
        "GET {token_path}/health HTTP/1.1\r\nHost: localhost\r\nOrigin: https://evil.example\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .expect("request should write");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("response should read");

    assert!(
        response.contains("HTTP/1.1 403 Forbidden"),
        "expected 403 for cross-origin request, got: {response}"
    );
}

#[test]
fn bridge_accepts_loopback_origin() {
    let bridge =
        start_bridge_server(test_app_paths("origin-ok"), None).expect("bridge should start");
    let (address, token_path) = bridge_address_and_token_path(&bridge.base_url);
    let mut stream = TcpStream::connect(address).expect("bridge should accept connections");
    let request = format!(
        "GET {token_path}/health HTTP/1.1\r\nHost: localhost\r\nOrigin: http://127.0.0.1:1234\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .expect("request should write");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("response should read");

    assert!(response.contains("HTTP/1.1 200 OK"));
}

#[test]
fn bridge_token_is_required_when_configured() {
    let paths = test_app_paths("bridge-token");
    let state = test_bridge_state(None);
    let missing = String::from_utf8(bridge_response_with_token(
        "GET /health HTTP/1.1\r\n",
        &[],
        &paths,
        &state,
        Some("secret-token"),
    ))
    .expect("utf-8 response");
    let present = String::from_utf8(bridge_response_with_token(
        "GET /secret-token/health HTTP/1.1\r\n",
        &[],
        &paths,
        &state,
        Some("secret-token"),
    ))
    .expect("utf-8 response");

    assert!(missing.contains("HTTP/1.1 403 Forbidden"));
    assert!(present.contains("HTTP/1.1 200 OK"));
}

#[test]
fn copy_row_rejects_out_of_range_index() {
    let root = test_file_root("oob-row");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nbeta\n").unwrap();
    fs::write(&right, "alpha\ngamma\n").unwrap();
    let mut state = GuiBridgeState::new(Some(build_context_for_paths(&left, &right)));

    let err = state
        .copy_row(usize::MAX, "left_to_right")
        .expect_err("usize::MAX must be rejected");
    assert!(err.contains("exceeds"), "unexpected error: {err}");
}

#[test]
fn bridge_filters_validate_returns_migration_hint_for_legacy_prefix() {
    let paths = test_app_paths("bridge-filter-validate");
    let state = test_bridge_state(None);
    // Windows-only metadata prefix `attr:` — still unsupported, still a migration hint.
    // URL-encoded: "name: Demo\nattr: archive"
    let response = String::from_utf8(bridge_response(
        "GET /filters/validate?body=name%3A%20Demo%0Aattr%3A%20archive HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(response.contains("HTTP/1.1 200 OK"));
    let body = json_response_body(&response);
    assert_eq!(body["ok"], false);
    assert_eq!(body["migration_hint"], true);
    assert_eq!(body["kind"], "UnsupportedWindowsMetadata");
}

#[test]
fn bridge_filters_migrate_round_trips() {
    // A fixture with supported, unsupported, and ctime-migration lines.
    let fixture = "name: LegacyFilter\n\
                       wf:*.rs\n\
                       attr: archive\n\
                       ctime: > '2020-01-01'\n";
    // URL-encode the fixture for the query string.
    let encoded = fixture
        .chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                vec![c as u8]
            }
            ' ' => vec![b'+'],
            c => {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                s.bytes()
                    .flat_map(|b| format!("%{b:02X}").into_bytes())
                    .collect()
            }
        })
        .collect::<Vec<u8>>();
    let encoded = String::from_utf8(encoded).unwrap();

    let paths = test_app_paths("bridge-filter-migrate");
    let state = test_bridge_state(None);
    let request = format!("GET /filters/migrate?body={encoded} HTTP/1.1\r\n");
    let response =
        String::from_utf8(bridge_response(&request, &paths, &state)).expect("utf-8 response");
    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "expected 200 OK in {response}"
    );
    let body = json_response_body(&response);
    assert_eq!(body["ok"], true);
    let migrated = body["migrated"]
        .as_str()
        .expect("migrated should be a string");
    // Supported line is preserved.
    assert!(
        migrated.contains("wf:*.rs"),
        "migrated should contain wf:*.rs"
    );
    // attr: line is commented out.
    assert!(
        migrated.contains("# UNSUPPORTED:"),
        "migrated should contain UNSUPPORTED comment"
    );
    // ctime: is rewritten to e: mtime.
    assert!(
        migrated.contains("e: mtime"),
        "ctime should be migrated to e: mtime; got: {migrated}"
    );
    // Warnings are returned for the unsupported attr: line.
    let warnings = body["warnings"]
        .as_array()
        .expect("warnings should be an array");
    assert!(
        !warnings.is_empty(),
        "warnings should not be empty for attr: line"
    );
}

#[test]
fn bridge_filters_migrate_requires_body_param() {
    let paths = test_app_paths("bridge-filter-migrate-no-body");
    let state = test_bridge_state(None);
    let response = String::from_utf8(bridge_response(
        "GET /filters/migrate HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    // Missing body → 400
    assert!(
        response.contains("HTTP/1.1 400"),
        "expected 400 in {response}"
    );
}

#[test]
fn bridge_walk_options_round_trip_through_settings_store() {
    let paths = test_app_paths("bridge-walk");
    let state = test_bridge_state(None);
    let initial = String::from_utf8(bridge_response("GET /walk HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    assert!(initial.contains("HTTP/1.1 200 OK"));
    let initial_body = json_response_body(&initial);
    assert_eq!(initial_body["respect_gitignore"], true);
    assert_eq!(initial_body["max_walk_depth"], 0);

    let _ = bridge_response(
        "GET /walk/set?key=max_walk_depth&value=12 HTTP/1.1\r\n",
        &paths,
        &state,
    );
    let _ = bridge_response(
        "GET /walk/set?key=excludes&value=target%2F**%2Cnode_modules%2F** HTTP/1.1\r\n",
        &paths,
        &state,
    );
    let after = String::from_utf8(bridge_response("GET /walk HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    let body = json_response_body(&after);
    assert_eq!(body["max_walk_depth"], 12);
    let excludes = body["excludes"].as_array().expect("array");
    assert!(excludes.iter().any(|v| v == "target/**"));
    assert!(excludes.iter().any(|v| v == "node_modules/**"));
}

#[test]
fn walk_set_rejects_invalid_bool_and_leaves_disk_unchanged() {
    let paths = test_app_paths("walk-set-invalid-bool");
    let state = test_bridge_state(None);

    // Confirm initial default (respect_gitignore = true from CoreSettings::default).
    let initial = String::from_utf8(bridge_response("GET /walk HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    let initial_body = json_response_body(&initial);
    let initial_value = initial_body["respect_gitignore"].as_bool().unwrap();

    // Submit an invalid bool value for a bool key — should get 400.
    let resp = String::from_utf8(bridge_response(
        "GET /walk/set?key=respect_gitignore&value=maybe HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 400 Bad Request"),
        "invalid bool should return 400; got: {resp}"
    );

    // Disk state must be unchanged.
    let after = String::from_utf8(bridge_response("GET /walk HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    let after_body = json_response_body(&after);
    assert_eq!(
        after_body["respect_gitignore"].as_bool().unwrap(),
        initial_value,
        "disk state should be unchanged after invalid-bool rejection"
    );
}

#[test]
fn bridge_filters_save_and_list_persist_filters() {
    let paths = test_app_paths("bridge-filters-save");
    let state = test_bridge_state(None);
    let _ = bridge_response(
        "GET /filters/save?body=name%3A%20Rust%0Awf%3A*.rs HTTP/1.1\r\n",
        &paths,
        &state,
    );
    let response = String::from_utf8(bridge_response(
        "GET /filters/list HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&response);
    let filters = body["filters"].as_array().expect("array");
    assert!(
        filters.iter().any(|f| f["name"] == "Rust"),
        "expected Rust filter in {filters:?}"
    );
}

#[test]
fn bridge_sessions_recent_returns_persisted_pairs() {
    // Non-fixture paths so the internal-test guard does not suppress recording.
    let root = test_file_root("sessions-recent");
    let left = root.join("l.txt");
    let right = root.join("r.txt");
    fs::write(&left, "a\nb\n").unwrap();
    fs::write(&right, "a\nc\n").unwrap();

    let paths = test_app_paths("bridge-sessions-recent");
    let context = build_context_for_paths(&left, &right);
    let _ = write_launch_context(&paths, &context).expect("context write should succeed");

    let state = test_bridge_state(None);
    let response = String::from_utf8(bridge_response(
        "GET /sessions/recent HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&response);
    let sessions = body["sessions"].as_array().expect("array");
    assert!(!sessions.is_empty());
    assert_eq!(sessions[0]["mode"], "Text");
}

#[test]
fn bridge_folder_open_rejects_unknown_key() {
    let paths = test_app_paths("bridge-folder-open-bad");
    let state = test_bridge_state(None);
    let response = String::from_utf8(bridge_response(
        "GET /folder/open?key=evilkey HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(response.contains("HTTP/1.1 400 Bad Request"));
}

#[test]
fn bridge_merge_conflicts_lists_difference_blocks() {
    let root = test_file_root("bridge-merge-conflicts");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "alpha\nleft one\nshared\nleft two\nomega\n").unwrap();
    fs::write(&right, "alpha\nright one\nshared\nright two\nomega\n").unwrap();
    let paths = test_app_paths("bridge-merge-conflicts");
    let state = test_bridge_state(Some(build_context_for_paths(&left, &right)));
    let response = String::from_utf8(bridge_response(
        "GET /merge/conflicts HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&response);
    let conflicts = body["conflicts"].as_array().expect("array");
    assert!(!conflicts.is_empty());
}

#[test]
fn bridge_plugins_list_returns_discovered_plugin_with_enabled_true() {
    let paths = test_app_paths("bridge-plugins-list");
    let state = test_bridge_state(None);
    // Write a fixture plugin manifest into the user plugins dir.
    let plugin_dir = paths.user_plugins_dir().join("example.smoke");
    fs::create_dir_all(&plugin_dir).expect("plugin dir should be created");
    let manifest = linsync_core::PluginManifest {
        schema_version: linsync_core::CURRENT_PLUGIN_SCHEMA_VERSION,
        id: "example.smoke".to_owned(),
        name: "Smoke Plugin".to_owned(),
        version: "1.0.0".to_owned(),
        license: "MIT".to_owned(),
        entry: vec!["run.sh".to_owned()],
        classes: vec![linsync_core::PluginClass::Prediffer],
        mime_types: vec![],
        extensions: vec![],
        capabilities: vec![],
        deterministic: false,
        sandbox: linsync_core::PluginSandbox::default(),
        streaming: false,
        options_schema: vec![],
        normalization_categories: vec![],
    };
    let manifest_text = serde_json::to_string_pretty(&manifest).unwrap();
    fs::write(
        plugin_dir.join(linsync_core::plugin::PLUGIN_MANIFEST_FILE),
        &manifest_text,
    )
    .unwrap();

    // /plugins/list should return it with enabled:true (no persisted state).
    let response = String::from_utf8(bridge_response(
        "GET /plugins/list HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&response);
    let plugins = body["plugins"].as_array().expect("plugins array");
    let plugin = plugins
        .iter()
        .find(|p| p["id"] == "example.smoke")
        .expect("example.smoke should appear in /plugins/list");
    assert_eq!(plugin["enabled"], true, "plugin should default to enabled");

    // Toggle it off via /plugins/toggle.
    let _ = bridge_response(
        "GET /plugins/toggle?id=example.smoke&enabled=false HTTP/1.1\r\n",
        &paths,
        &state,
    );

    // /plugins/list again should return enabled:false.
    let response2 = String::from_utf8(bridge_response(
        "GET /plugins/list HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body2 = json_response_body(&response2);
    let plugins2 = body2["plugins"].as_array().expect("plugins array");
    let plugin2 = plugins2
        .iter()
        .find(|p| p["id"] == "example.smoke")
        .expect("example.smoke should still appear after toggle");
    assert_eq!(
        plugin2["enabled"], false,
        "plugin should be disabled after toggle"
    );
}

#[test]
fn bridge_plugins_toggle_returns_ok() {
    let paths = test_app_paths("bridge-plugins-toggle-ok");
    let state = test_bridge_state(None);
    let response = String::from_utf8(bridge_response(
        "GET /plugins/toggle?id=any.plugin&enabled=false HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(response.contains("HTTP/1.1 200 OK"));
    let body = json_response_body(&response);
    assert_eq!(body["ok"], true);
}

#[test]
fn bridge_plugins_toggle_requires_id_param() {
    let paths = test_app_paths("bridge-plugins-toggle-noid");
    let state = test_bridge_state(None);
    let response = String::from_utf8(bridge_response(
        "GET /plugins/toggle HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(response.contains("HTTP/1.1 400 Bad Request"));
}

#[test]
fn plugin_enabled_concurrent_toggles_persist_all_entries() {
    // Spawn two threads each toggling 50 unique plugin IDs concurrently
    // through the same shared state (and therefore the same plugin_enabled
    // mutex). Assert that all 100 entries are present in the persisted file.
    let paths = Arc::new(test_app_paths("plugin-concurrent-toggles"));
    // Both threads share the same GuiBridgeState (and therefore the same
    // plugin_enabled Mutex) to exercise concurrent access.
    let state = test_bridge_state(None);

    let paths_a = Arc::clone(&paths);
    let state_a = Arc::clone(&state);
    let handle_a = thread::spawn(move || {
        for i in 0..50_u32 {
            let req = format!("GET /plugins/toggle?id=plugin.a.{i}&enabled=true HTTP/1.1\r\n");
            bridge_response(&req, &paths_a, &state_a);
        }
    });

    let paths_b = Arc::clone(&paths);
    let state_b = Arc::clone(&state);
    let handle_b = thread::spawn(move || {
        for i in 0..50_u32 {
            let req = format!("GET /plugins/toggle?id=plugin.b.{i}&enabled=false HTTP/1.1\r\n");
            bridge_response(&req, &paths_b, &state_b);
        }
    });

    handle_a.join().expect("thread A should finish");
    handle_b.join().expect("thread B should finish");

    // Load the persisted file and verify all 100 entries are present.
    let file = paths.plugins_enabled_file();
    let text = fs::read_to_string(&file).expect("plugins_enabled_file should exist");
    let map: HashMap<String, bool> =
        serde_json::from_str(&text).expect("plugins_enabled_file should be valid JSON");

    for i in 0..50_u32 {
        assert!(
            map.contains_key(&format!("plugin.a.{i}")),
            "plugin.a.{i} should be in persisted map"
        );
        assert!(
            map.contains_key(&format!("plugin.b.{i}")),
            "plugin.b.{i} should be in persisted map"
        );
    }
    assert_eq!(map.len(), 100, "expected exactly 100 entries");
}

#[test]
fn origin_is_loopback_recognises_common_loopback_hosts() {
    assert!(origin_is_loopback("http://localhost"));
    assert!(origin_is_loopback("http://localhost:5173"));
    assert!(origin_is_loopback("http://127.0.0.1:1234"));
    assert!(origin_is_loopback("http://[::1]"));
    assert!(origin_is_loopback("http://[::1]:80"));
    assert!(!origin_is_loopback("https://evil.example"));
    assert!(!origin_is_loopback(
        "http://attacker.localhost.evil.example"
    ));
    assert!(!origin_is_loopback("http://[::1].evil.example"));
    assert!(!origin_is_loopback("null"));
}

// ── Helpers shared by merge3 tests ────────────────────────────────────────

fn url_encode(path: &std::path::Path) -> String {
    let s = path.display().to_string();
    s.bytes()
        .flat_map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                vec![b]
            }
            b => format!("%{b:02X}").into_bytes(),
        })
        .collect::<Vec<u8>>()
        .into_iter()
        .map(|b| b as char)
        .collect()
}

// ── Three-way merge bridge tests ──────────────────────────────────────────

#[test]
fn bridge_merge3_start_returns_conflicts() {
    let root = test_file_root("merge3-start");
    let base = root.join("base.txt");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&base, "a\nb\nc\n").unwrap();
    fs::write(&left, "a\nB\nc\n").unwrap();
    fs::write(&right, "a\nC\nc\n").unwrap();

    let paths = test_app_paths("merge3-start");
    let state = test_bridge_state(None);
    let query = format!(
        "base={}&left={}&right={}",
        url_encode(&base),
        url_encode(&left),
        url_encode(&right)
    );
    let request = format!("GET /merge3/start?{query} HTTP/1.1\r\n");
    let response =
        String::from_utf8(bridge_response(&request, &paths, &state)).expect("utf-8 response");
    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "expected 200 OK, got: {response}"
    );
    let body = json_response_body(&response);
    assert_eq!(body["ok"], true);
    let conflicts = body["conflicts"].as_array().expect("conflicts array");
    assert!(
        !conflicts.is_empty(),
        "expected at least one conflict for diverging changes"
    );
    // Each conflict should carry line arrays.
    let first = &conflicts[0];
    assert!(first["left_lines"].is_array());
    assert!(first["right_lines"].is_array());
    assert!(first["base_lines"].is_array());
    // output_text should be non-empty.
    assert!(!body["output_text"].as_str().unwrap_or("").is_empty());
}

#[test]
fn bridge_merge3_midfile_conflict_yields_in_range_scroll_indices() {
    // Regression coverage for the conflict scroll-to-line fix: a conflict in
    // the middle of a 40-line file must produce per-side line arrays whose
    // QML scroll formula (currentConflictStart=0, End=len-1) yields indices
    // that are always within each side's own line array — never an
    // out-of-range positionViewAtIndex call.
    let root = test_file_root("merge3-midfile");
    let base = root.join("base.txt");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    // 40-line files; line 20 diverges between left and right.
    let make = |marker: &str| {
        (1..=40)
            .map(|n| {
                if n == 20 {
                    format!("line-20-{marker}")
                } else {
                    format!("line-{n}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    };
    fs::write(&base, make("base")).unwrap();
    fs::write(&left, make("left")).unwrap();
    fs::write(&right, make("right")).unwrap();

    let paths = test_app_paths("merge3-midfile");
    let state = test_bridge_state(None);
    let query = format!(
        "base={}&left={}&right={}",
        url_encode(&base),
        url_encode(&left),
        url_encode(&right)
    );
    let response = String::from_utf8(bridge_response(
        &format!("GET /merge3/start?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(response.contains("HTTP/1.1 200 OK"), "got: {response}");
    let body = json_response_body(&response);
    let conflicts = body["conflicts"].as_array().expect("conflicts array");
    assert!(!conflicts.is_empty(), "a divergent line must conflict");

    for conflict in conflicts {
        for side in ["base_lines", "left_lines", "right_lines"] {
            let lines = conflict[side].as_array().expect("per-side line array");
            assert!(
                !lines.is_empty(),
                "{side} must be non-empty so the scroll index is valid"
            );
            // The QML formula: start = 0, end = len - 1 (both in [0, len-1]).
            let start = 0usize;
            let end = lines.len() - 1;
            assert!(start < lines.len(), "{side} start index in range");
            assert!(
                end < lines.len() && end >= start,
                "{side} end index in range and >= start"
            );
        }
    }
}

#[test]
fn bridge_merge3_resolve_then_save_roundtrip() {
    let root = test_file_root("merge3-roundtrip");
    let base = root.join("base.txt");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    let out = root.join("merged.txt");
    fs::write(&base, "shared\nbase\n").unwrap();
    fs::write(&left, "shared\nleft\n").unwrap();
    fs::write(&right, "shared\nright\n").unwrap();

    let paths = test_app_paths("merge3-roundtrip");
    let state = test_bridge_state(None);

    // Start the merge session.
    let start_query = format!(
        "base={}&left={}&right={}",
        url_encode(&base),
        url_encode(&left),
        url_encode(&right)
    );
    let start_resp = String::from_utf8(bridge_response(
        &format!("GET /merge3/start?{start_query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let start_body = json_response_body(&start_resp);
    assert_eq!(start_body["ok"], true, "start should succeed");
    let conflicts = start_body["conflicts"].as_array().expect("conflicts array");
    assert!(!conflicts.is_empty(), "expected at least one conflict");

    // Resolve all conflicts choosing Left.
    for conflict in conflicts {
        let id = conflict["id"].as_u64().expect("conflict id");
        let resolve_query = format!("id={id}&choice=left");
        let resolve_resp = String::from_utf8(bridge_response(
            &format!("GET /merge3/resolve?{resolve_query} HTTP/1.1\r\n"),
            &paths,
            &state,
        ))
        .expect("utf-8 response");
        let resolve_body = json_response_body(&resolve_resp);
        assert_eq!(
            resolve_body["ok"], true,
            "resolve should succeed for id={id}"
        );
    }

    // Save to the output path.
    let save_query = format!("path={}", url_encode(&out));
    let save_resp = String::from_utf8(bridge_response(
        &format!("GET /merge3/save?{save_query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let save_body = json_response_body(&save_resp);
    assert_eq!(save_body["ok"], true, "save should succeed");

    // The saved file should contain the left-side content, not the right-side.
    let saved = fs::read_to_string(&out).expect("merged output should be readable");
    assert!(
        saved.contains("left"),
        "saved output should contain 'left'; got: {saved:?}"
    );
    assert!(
        !saved.contains("right"),
        "saved output must not contain 'right' after left-choice; got: {saved:?}"
    );
}

#[test]
fn bridge_merge3_start_rejects_missing_params() {
    let paths = test_app_paths("merge3-missing");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /merge3/start?left=/a&right=/b HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 400"),
        "expected 400 for missing base param"
    );
}

#[test]
fn bridge_merge3_resolve_rejects_unknown_conflict_id() {
    let root = test_file_root("merge3-unknown-id");
    let base = root.join("base.txt");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&base, "x\n").unwrap();
    fs::write(&left, "y\n").unwrap();
    fs::write(&right, "z\n").unwrap();

    let paths = test_app_paths("merge3-unknown-id");
    let state = test_bridge_state(None);

    // Start a session first.
    let start_query = format!(
        "base={}&left={}&right={}",
        url_encode(&base),
        url_encode(&left),
        url_encode(&right)
    );
    bridge_response(
        &format!("GET /merge3/start?{start_query} HTTP/1.1\r\n"),
        &paths,
        &state,
    );

    // Resolve with an invalid id.
    let resp = String::from_utf8(bridge_response(
        "GET /merge3/resolve?id=9999&choice=left HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 400"),
        "expected 400 for unknown conflict id; got: {resp}"
    );
}

#[test]
fn bridge_merge3_save_without_session_returns_error() {
    let paths = test_app_paths("merge3-no-session");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /merge3/save?path=/tmp/linsync-merge3-test-nosession.txt HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    // Returns 200 with ok:false (consistent with folder-op error pattern).
    let body = json_response_body(&resp);
    assert!(
        body.get("error").is_some(),
        "save without session should return an error"
    );
}

#[test]
fn merge_save_rejects_unresolved_conflicts() {
    let root = test_file_root("merge3-validation");
    let base = root.join("base.txt");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    let out = root.join("merged.txt");
    fs::write(&base, "shared\nbase line\n").unwrap();
    fs::write(&left, "shared\nleft line\n").unwrap();
    fs::write(&right, "shared\nright line\n").unwrap();

    let paths = test_app_paths("merge3-validation");
    let state = test_bridge_state(None);

    let start_query = format!(
        "base={}&left={}&right={}",
        url_encode(&base),
        url_encode(&left),
        url_encode(&right)
    );
    let start_resp = String::from_utf8(bridge_response(
        &format!("GET /merge3/start?{start_query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let start_body = json_response_body(&start_resp);
    assert_eq!(start_body["ok"], true);
    let conflicts = start_body["conflicts"].as_array().expect("conflicts array");
    assert!(!conflicts.is_empty());

    let save_query = format!("path={}", url_encode(&out));
    let save_resp = String::from_utf8(bridge_response(
        &format!("GET /merge3/save?{save_query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        save_resp.contains("HTTP/1.1 409 Conflict"),
        "expected 409 for unresolved conflicts; got: {save_resp}"
    );
    let save_body = json_response_body(&save_resp);
    assert_eq!(save_body["ok"], false);
    assert!(
        save_body["unresolved_count"].as_u64().unwrap_or(0) > 0,
        "unresolved_count should be > 0"
    );

    for conflict in conflicts {
        let id = conflict["id"].as_u64().expect("conflict id");
        let resolve_query = format!("id={id}&choice=left");
        let resolve_resp = String::from_utf8(bridge_response(
            &format!("GET /merge3/resolve?{resolve_query} HTTP/1.1\r\n"),
            &paths,
            &state,
        ))
        .expect("utf-8 response");
        let resolve_body = json_response_body(&resolve_resp);
        assert_eq!(resolve_body["ok"], true);
    }

    let save_resp2 = String::from_utf8(bridge_response(
        &format!("GET /merge3/save?{save_query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        save_resp2.contains("HTTP/1.1 200 OK"),
        "expected 200 after resolving all conflicts; got: {save_resp2}"
    );
    let save_body2 = json_response_body(&save_resp2);
    assert_eq!(save_body2["ok"], true);
}

// ── Plugin options bridge tests ───────────────────────────────────────────

/// Write a minimal plugin manifest with an `options_schema` under the user
/// plugins dir for the given `paths`.  Returns the plugin directory.
fn write_fixture_plugin_with_options(paths: &AppPaths, id: &str) -> PathBuf {
    let plugin_dir = paths.user_plugins_dir().join(id);
    fs::create_dir_all(&plugin_dir).expect("plugin dir should be created");
    let manifest_json = serde_json::json!({
        "schema_version": linsync_core::CURRENT_PLUGIN_SCHEMA_VERSION,
        "id": id,
        "name": "Options Test Plugin",
        "version": "1.0.0",
        "license": "MIT",
        "entry": ["run.sh"],
        "classes": ["prediffer"],
        "mime_types": [],
        "extensions": [],
        "capabilities": [],
        "deterministic": false,
        "sandbox": {},
        "options_schema": [
            { "key": "level", "label": "Quality Level", "kind": "int", "default": 5, "choices": [] },
            { "key": "mode", "label": "Mode", "kind": "enum", "default": null, "choices": ["fast", "slow"] },
            { "key": "verbose", "label": "Verbose", "kind": "bool", "default": false, "choices": [] },
        ]
    });
    fs::write(
        plugin_dir.join(linsync_core::plugin::PLUGIN_MANIFEST_FILE),
        serde_json::to_string_pretty(&manifest_json).unwrap(),
    )
    .unwrap();
    plugin_dir
}

#[test]
fn bridge_plugin_options_get_returns_schema_and_empty_values_by_default() {
    let paths = test_app_paths("bridge-plugin-opts-get-default");
    let state = test_bridge_state(None);
    write_fixture_plugin_with_options(&paths, "test.opts-plugin");

    let response = String::from_utf8(bridge_response(
        "GET /plugins/options/get?id=test.opts-plugin HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "expected 200 OK; got: {response}"
    );
    let body = json_response_body(&response);
    let schema = body["schema"].as_array().expect("schema should be array");
    assert_eq!(schema.len(), 3, "plugin declares 3 options");
    assert_eq!(schema[0]["key"], "level");
    assert_eq!(schema[0]["kind"], "int");
    assert_eq!(schema[1]["key"], "mode");
    assert_eq!(schema[1]["kind"], "enum");
    assert_eq!(schema[2]["key"], "verbose");
    assert_eq!(schema[2]["kind"], "bool");
    // No values persisted yet — values object should be empty.
    assert!(
        body["values"]
            .as_object()
            .expect("values should be object")
            .is_empty(),
        "values should be empty before any set"
    );
}

#[test]
fn bridge_plugin_options_set_then_get_round_trips() {
    let paths = test_app_paths("bridge-plugin-opts-rt");
    let state = test_bridge_state(None);
    write_fixture_plugin_with_options(&paths, "test.opts-rt");

    // Set `level` to 8.
    let set_resp = String::from_utf8(bridge_response(
        "GET /plugins/options/set?id=test.opts-rt&key=level&value=8 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let set_body = json_response_body(&set_resp);
    assert_eq!(set_body["ok"], true, "set should return ok:true");

    // Set `verbose` to true.
    let _ = bridge_response(
        "GET /plugins/options/set?id=test.opts-rt&key=verbose&value=true HTTP/1.1\r\n",
        &paths,
        &state,
    );

    // Get should return schema + merged values.
    let get_resp = String::from_utf8(bridge_response(
        "GET /plugins/options/get?id=test.opts-rt HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let get_body = json_response_body(&get_resp);
    let values = get_body["values"]
        .as_object()
        .expect("values should be object");
    assert_eq!(values.get("level").and_then(|v| v.as_i64()), Some(8));
    assert_eq!(values.get("verbose").and_then(|v| v.as_bool()), Some(true));
    // `mode` was not set, so it should not appear in values.
    assert!(
        values.get("mode").is_none(),
        "unset option should not appear in values"
    );
}

#[test]
fn bridge_plugin_options_get_returns_empty_schema_for_missing_plugin() {
    let paths = test_app_paths("bridge-plugin-opts-no-plugin");
    let state = test_bridge_state(None);
    // No plugin installed; the endpoint should still return 200 with empty schema.
    let response = String::from_utf8(bridge_response(
        "GET /plugins/options/get?id=nonexistent.plugin HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(response.contains("HTTP/1.1 200 OK"));
    let body = json_response_body(&response);
    assert!(
        body["schema"].as_array().expect("schema array").is_empty(),
        "missing plugin should have empty schema"
    );
}

#[test]
fn bridge_plugin_options_set_requires_all_params() {
    let paths = test_app_paths("bridge-plugin-opts-bad-params");
    let state = test_bridge_state(None);
    // Missing id.
    let resp = String::from_utf8(bridge_response(
        "GET /plugins/options/set?key=level&value=3 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 400 Bad Request"),
        "missing id → 400"
    );

    // Missing key.
    let resp = String::from_utf8(bridge_response(
        "GET /plugins/options/set?id=any.plugin&value=3 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 400 Bad Request"),
        "missing key → 400"
    );

    // Missing value.
    let resp = String::from_utf8(bridge_response(
        "GET /plugins/options/set?id=any.plugin&key=level HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 400 Bad Request"),
        "missing value → 400"
    );
}

// ────────────────────────────────────────────────────────────────────────
// Phase 0 drift regressions.
//
// These tests describe the contracts the GUI bridge must uphold. All are
// active and passing — they serve as the ongoing regression guard.
// ────────────────────────────────────────────────────────────────────────

#[test]
fn compare_document_endpoint_is_routed() {
    let paths = test_app_paths("drift-doc-routed");
    let state = test_bridge_state(None);
    // The dispatcher must not 404 on this endpoint. The handler exists in
    // apps/linsync-gui/src/lib.rs::document_compare_bridge_response but is
    // not currently registered in main.rs::bridge_response_with_token.
    let resp = String::from_utf8(bridge_response(
        "GET /compare/document?left=/tmp/a.pdf&right=/tmp/b.pdf&mode=text HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        !resp.contains("HTTP/1.1 404 Not Found"),
        "/compare/document should be dispatched (got 404). Response: {resp}"
    );
}

#[test]
fn compare_text_bridge_honours_ignore_case() {
    // The bridge should thread an ignore_case query param through to
    // TextCompareOptions. Today /compare calls compare_text_files with
    // TextCompareOptions::default(), so this case-insensitive contract
    // fails. When Phase 1 (Compare profiles) lands, the bridge must
    // accept ignore_case (or read it via the active profile) and route
    // it through to the core.
    let root = test_file_root("drift-ignore-case");
    let left_path = root.join("left.txt");
    let right_path = root.join("right.txt");
    std::fs::write(&left_path, "FOO\nBAR\n").unwrap();
    std::fs::write(&right_path, "foo\nbar\n").unwrap();

    let paths = test_app_paths("drift-ignore-case-paths");
    let state = test_bridge_state(None);
    let query = format!(
        "left={}&right={}&ignore_case=true",
        urlencoding::encode(left_path.to_str().unwrap()),
        urlencoding::encode(right_path.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"), "compare should succeed");
    let body = json_response_body(&resp);
    // With ignore_case=true, FOO vs foo must compare equal: every
    // tab row should report state="equal" and difference_count
    // should be zero.
    let active_tab = body
        .get("session")
        .and_then(|s| s.get("tabs"))
        .and_then(|t| t.as_array())
        .and_then(|tabs| tabs.first())
        .expect("expected one tab in session");
    let differences = active_tab
        .get("difference_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);
    assert_eq!(
        differences, 0,
        "with ignore_case=true, FOO vs foo should compare equal; body={body}"
    );
    for side in ["left_rows", "right_rows"] {
        for row in active_tab
            .get(side)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let state = row.get("state").and_then(|s| s.as_str()).unwrap_or("");
            assert_eq!(
                state, "equal",
                "row state for {side} should be 'equal'; row={row}"
            );
        }
    }
}

#[test]
fn compare_text_bridge_applies_view_find_syntax_options() {
    let root = test_file_root("text-view-options");
    let left_path = root.join("left.rs");
    let right_path = root.join("right.rs");
    std::fs::write(&left_path, "fn main() {}\nlet value = 1;\n").unwrap();
    std::fs::write(&right_path, "fn main() {}\nlet value = 2;\n").unwrap();

    let paths = test_app_paths("text-view-options-paths");
    let state = test_bridge_state(None);
    let query = format!(
        "left={}&right={}&mode=Text&context_lines=0&syntax=rust&find=value&regex_rule_set=volatile&encoding=utf8&bookmark=left:2:mark",
        urlencoding::encode(left_path.to_str().unwrap()),
        urlencoding::encode(right_path.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"), "compare should succeed");
    let body = json_response_body(&resp);
    let rows = body["session"]["tabs"][0]["left_rows"]
        .as_array()
        .expect("left rows");

    assert!(
        rows.iter()
            .any(|row| row["folded_count"].as_u64().is_some()),
        "context_lines=0 should create folded rows; body={body}"
    );
    assert!(
        rows.iter().any(|row| row["has_find_match"] == true),
        "find=value should mark matching rows; body={body}"
    );
    assert!(
        rows.iter().any(|row| row["syntax_spans"]
            .as_array()
            .is_some_and(|spans| !spans.is_empty())),
        "syntax=rust should attach syntax spans; body={body}"
    );
    assert!(
        rows.iter().any(|row| row["bookmarked"] == true),
        "bookmark query should mark matching rows; body={body}"
    );
}

#[test]
fn compare_text_bridge_accepts_python_syntax_mode() {
    let root = test_file_root("text-python-syntax");
    let left_path = root.join("left.py");
    let right_path = root.join("right.py");
    std::fs::write(&left_path, "def main():\n    value = 1\n").unwrap();
    std::fs::write(&right_path, "def main():\n    value = 2\n").unwrap();

    let paths = test_app_paths("text-python-syntax-paths");
    let state = test_bridge_state(None);
    let query = format!(
        "left={}&right={}&mode=Text&syntax=python",
        urlencoding::encode(left_path.to_str().unwrap()),
        urlencoding::encode(right_path.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"), "compare should succeed");
    let body = json_response_body(&resp);
    let rows = body["session"]["tabs"][0]["left_rows"]
        .as_array()
        .expect("left rows");
    assert!(
        rows.iter().any(|row| row["syntax_spans"]
            .as_array()
            .is_some_and(|spans| spans.iter().any(|span| span["class"] == "keyword"))),
        "syntax=python should attach keyword syntax spans; body={body}"
    );
}

#[test]
fn webpage_unsupported_mode_returns_clear_error() {
    // Regression guard: direct or stale requests for a webpage mode the
    // bridge doesn't implement must receive an actionable error instead
    // of a silent success or panic.
    let paths = test_app_paths("drift-webpage-unsupported");
    let body = linsync::webpage_compare_bridge_response(
        "left=http://example.com/a&right=http://example.com/b&mode=rendered",
        &paths,
    );
    let v: serde_json::Value = serde_json::from_str(&body).expect("body is JSON");
    assert_eq!(
        v["schema_version"],
        serde_json::json!(RESPONSE_SCHEMA_VERSION)
    );
    assert!(
        v.get("error").is_some(),
        "unsupported webpage mode must surface as {{\"error\":...}} — got: {body}"
    );
}

#[test]
fn webpage_bridge_requires_confirmed_query_param() {
    // The consent gate lives in QML; direct HTTP requests without the
    // confirmed=1 token must be rejected before any network fetch.
    let paths = test_app_paths("drift-webpage-confirm");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /compare/webpage?left=http://example.com/a&right=http://example.com/b&mode=html HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.starts_with("HTTP/1.1 400"),
        "missing confirmed=1 must return 400: {resp}"
    );
}

// ────────────────────────────────────────────────────────────────────────
// Profile bridge endpoints (Phase 1)
// ────────────────────────────────────────────────────────────────────────

#[test]
fn profiles_list_returns_every_builtin() {
    let paths = test_app_paths("profiles-list");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /profiles/list HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    let ids: Vec<String> = body
        .get("profiles")
        .and_then(|v| v.as_array())
        .expect("profiles array")
        .iter()
        .filter_map(|p| p.get("id").and_then(|v| v.as_str()).map(|s| s.to_owned()))
        .collect();
    for expected in [
        "default",
        "strict-bytes",
        "ignore-formatting",
        "code-review",
        "prose-review",
        "folder-sync-preview",
        "webpage-source-safe",
    ] {
        assert!(ids.iter().any(|id| id == expected), "missing {expected}");
    }
}

#[test]
fn profiles_active_round_trip() {
    let paths = test_app_paths("profiles-active");
    let state = test_bridge_state(None);
    // No active pointer yet.
    let resp = String::from_utf8(bridge_response(
        "GET /profiles/active/get HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    assert!(body["active"].is_null(), "no active pointer initially");

    // Set the active profile to a built-in.
    let resp = String::from_utf8(bridge_response(
        "GET /profiles/active/set?id=code-review HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));

    // Read it back.
    let resp = String::from_utf8(bridge_response(
        "GET /profiles/active/get HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    assert_eq!(body["active"].as_str(), Some("code-review"));
}

#[test]
fn profiles_active_set_rejects_unknown_id() {
    let paths = test_app_paths("profiles-active-unknown");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /profiles/active/set?id=does-not-exist HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 404"),
        "unknown id should 404: {resp}"
    );
}

#[test]
fn stale_active_pointer_is_cleared_at_startup() {
    let paths = test_app_paths("stale-pointer-cleanup");
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    // Point the active selection at a user profile that does not exist.
    store
        .save_active_pointer(&ProfileId::new("ghost-profile").unwrap())
        .unwrap();
    assert!(store.load_active_pointer().unwrap().is_some());

    let cleared = cleanup_stale_active_pointer(&paths);
    assert!(cleared, "a dangling pointer must be cleared");
    assert!(
        store.load_active_pointer().unwrap().is_none(),
        "pointer file should be gone after cleanup"
    );
    // The resolver now returns the built-in default without a stale pointer.
    let profile = resolve_profile_for_request(&paths, &[]).unwrap();
    assert_eq!(profile.id.as_str(), "default");
}

#[test]
fn valid_active_pointer_is_not_cleared() {
    let paths = test_app_paths("valid-pointer-keep");
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    // A built-in id is always valid and must be preserved.
    store
        .save_active_pointer(&ProfileId::new("code-review").unwrap())
        .unwrap();

    let cleared = cleanup_stale_active_pointer(&paths);
    assert!(!cleared, "a valid built-in pointer must be kept");
    assert_eq!(
        store.load_active_pointer().unwrap().unwrap().as_str(),
        "code-review"
    );
}

#[test]
fn compare_per_request_profile_overrides_active() {
    // Active profile is strict-bytes (case-sensitive). A
    // ?profile=ignore-formatting query override should equate
    // FOO with foo for that single request without changing the
    // persisted active pointer.
    let root = test_file_root("per-request-override");
    let left_path = root.join("left.txt");
    let right_path = root.join("right.txt");
    std::fs::write(&left_path, "FOO\n").unwrap();
    std::fs::write(&right_path, "foo\n").unwrap();

    let paths = test_app_paths("per-request-override-paths");
    let state = test_bridge_state(None);

    // Set active = strict-bytes.
    bridge_response(
        "GET /profiles/active/set?id=strict-bytes HTTP/1.1\r\n",
        &paths,
        &state,
    );

    // Override per-request with ignore-formatting.
    let query = format!(
        "left={}&right={}&profile=ignore-formatting",
        urlencoding::encode(left_path.to_str().unwrap()),
        urlencoding::encode(right_path.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let differences = body["session"]["tabs"][0]["difference_count"]
        .as_u64()
        .unwrap_or(u64::MAX);
    assert_eq!(
        differences, 0,
        "per-request ?profile=ignore-formatting should fold case; body={body}"
    );

    // The persisted active pointer must NOT have changed.
    let resp = String::from_utf8(bridge_response(
        "GET /profiles/active/get HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    assert_eq!(
        body["active"].as_str(),
        Some("strict-bytes"),
        "per-request profile override must not mutate active pointer"
    );
}

#[test]
fn compare_unknown_profile_id_returns_400() {
    let root = test_file_root("unknown-profile-400");
    let left_path = root.join("left.txt");
    let right_path = root.join("right.txt");
    std::fs::write(&left_path, "x\n").unwrap();
    std::fs::write(&right_path, "y\n").unwrap();

    let paths = test_app_paths("unknown-profile-400-paths");
    let state = test_bridge_state(None);

    let query = format!(
        "left={}&right={}&profile=this-does-not-exist",
        urlencoding::encode(left_path.to_str().unwrap()),
        urlencoding::encode(right_path.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        resp.contains("HTTP/1.1 400"),
        "unknown ?profile= must return 400, not silent fallback: {resp}"
    );
}

#[test]
fn compare_honours_active_profile() {
    // With the active profile set to ignore-formatting, FOO vs foo
    // should compare equal even without per-request override flags.
    let root = test_file_root("active-profile-honoured");
    let left_path = root.join("left.txt");
    let right_path = root.join("right.txt");
    std::fs::write(&left_path, "FOO\nBAR\n").unwrap();
    std::fs::write(&right_path, "foo\nbar\n").unwrap();

    let paths = test_app_paths("active-profile-honoured-paths");
    let state = test_bridge_state(None);

    // Set active profile.
    bridge_response(
        "GET /profiles/active/set?id=ignore-formatting HTTP/1.1\r\n",
        &paths,
        &state,
    );

    // Run a compare — no per-request overrides.
    let query = format!(
        "left={}&right={}",
        urlencoding::encode(left_path.to_str().unwrap()),
        urlencoding::encode(right_path.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let differences = body["session"]["tabs"][0]["difference_count"]
        .as_u64()
        .unwrap_or(u64::MAX);
    assert_eq!(
        differences, 0,
        "active profile ignore-formatting should fold case; body={body}"
    );
}

#[test]
fn compare_table_uses_profile_delimiter() {
    let root = test_file_root("table-profile-delimiter");
    let left = root.join("left.csv");
    let right = root.join("right.csv");
    fs::write(&left, "a;b;c\n1;2;3\n").unwrap();
    fs::write(&right, "a;b;c\n1;9;3\n").unwrap();

    let paths = test_app_paths("table-profile-delimiter-paths");
    let state = test_bridge_state(None);

    // Create a user profile with semicolon delimiter and set it active.
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "id": "semi-table",
        "name": "Semicolon table",
        "description": "Uses ; delimiter",
        "table": { "delimiter": ";" }
    });
    let profile_dir = paths.profiles_dir();
    fs::create_dir_all(&profile_dir).unwrap();
    fs::write(
        profile_dir.join("semi-table.json"),
        profile_json.to_string(),
    )
    .unwrap();
    bridge_response(
        "GET /profiles/active/set?id=semi-table HTTP/1.1\r\n",
        &paths,
        &state,
    );

    let query = format!(
        "left={}&right={}&mode=Table",
        urlencoding::encode(left.to_str().unwrap()),
        urlencoding::encode(right.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let diff_count = body["session"]["tabs"][0]["difference_count"]
        .as_u64()
        .unwrap_or(u64::MAX);
    assert_eq!(
        diff_count, 1,
        "profile semicolon delimiter should parse the CSV into cells; body={body}"
    );
}

#[test]
fn compare_binary_uses_profile_bytes_per_row() {
    let root = test_file_root("binary-profile-bpr");
    let left = root.join("left.bin");
    let right = root.join("right.bin");
    fs::write(&left, b"\x00\x01\x02\x03\x04\x05\x06\x07\x08").unwrap();
    fs::write(&right, b"\x00\x01\x02\x03\x04\x05\x06\x07\xFF").unwrap();

    let paths = test_app_paths("binary-profile-bpr-paths");
    let state = test_bridge_state(None);

    // Create a profile with 4 bytes per row (default is 16).
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "id": "hex-4bpr",
        "name": "Hex 4 BPR",
        "description": "4 bytes per hex row",
        "binary": { "bytes_per_row": 4 }
    });
    let profile_dir = paths.profiles_dir();
    fs::create_dir_all(&profile_dir).unwrap();
    fs::write(profile_dir.join("hex-4bpr.json"), profile_json.to_string()).unwrap();
    bridge_response(
        "GET /profiles/active/set?id=hex-4bpr HTTP/1.1\r\n",
        &paths,
        &state,
    );

    let query = format!(
        "left={}&right={}&mode=Hex",
        urlencoding::encode(left.to_str().unwrap()),
        urlencoding::encode(right.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let tab = &body["session"]["tabs"][0];
    let left_rows = tab["left_rows"].as_array().expect("left_rows array");
    // 9 bytes / 4 bytes-per-row = 3 rows (ceil).
    assert_eq!(
        left_rows.len(),
        3,
        "4 BPR should produce 3 rows for 9 bytes; body={body}"
    );
}

#[test]
fn compare_query_override_table_delimiter() {
    let root = test_file_root("table-query-delimiter");
    let left = root.join("left.csv");
    let right = root.join("right.csv");
    fs::write(&left, "a;b;c\n1;2;3\n").unwrap();
    fs::write(&right, "a;b;c\n1;9;3\n").unwrap();

    let paths = test_app_paths("table-query-delimiter-paths");
    let state = test_bridge_state(None);

    let query = format!(
        "left={}&right={}&mode=Table&delimiter=;",
        urlencoding::encode(left.to_str().unwrap()),
        urlencoding::encode(right.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let diff_count = body["session"]["tabs"][0]["difference_count"]
        .as_u64()
        .unwrap_or(u64::MAX);
    assert_eq!(
        diff_count, 1,
        "query ?delimiter=; should parse the semicolon-delimited file; body={body}"
    );
}

#[test]
fn compare_folder_uses_profile_recursive() {
    let root = test_file_root("folder-profile-recursive");
    let left_dir = root.join("left");
    let right_dir = root.join("right");
    let left_sub = left_dir.join("sub");
    let right_sub = right_dir.join("sub");
    fs::create_dir_all(&left_sub).unwrap();
    fs::create_dir_all(&right_sub).unwrap();
    fs::write(left_sub.join("a.txt"), "hello").unwrap();
    fs::write(right_sub.join("a.txt"), "world").unwrap();

    let paths = test_app_paths("folder-profile-recursive-paths");
    let state = test_bridge_state(None);

    // Create a profile with recursive=false.
    let profile_json = serde_json::json!({
        "schema_version": 1,
        "id": "flat-folder",
        "name": "Flat folder",
        "description": "Non-recursive folder compare",
        "folder": { "recursive": false }
    });
    let profile_dir = paths.profiles_dir();
    fs::create_dir_all(&profile_dir).unwrap();
    fs::write(
        profile_dir.join("flat-folder.json"),
        profile_json.to_string(),
    )
    .unwrap();
    bridge_response(
        "GET /profiles/active/set?id=flat-folder HTTP/1.1\r\n",
        &paths,
        &state,
    );

    let query = format!(
        "left={}&right={}",
        urlencoding::encode(left_dir.to_str().unwrap()),
        urlencoding::encode(right_dir.to_str().unwrap()),
    );
    let resp = String::from_utf8(bridge_response(
        &format!("GET /compare?{query} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let tab = &body["session"]["tabs"][0];
    // Non-recursive should not descend into sub/, so both dirs look identical
    // (empty at the top level since only subdirs exist, no direct files).
    let _compared = tab["summary"]
        .as_array()
        .and_then(|s| s.iter().find(|item| item["label"] == "Compared"))
        .and_then(|item| item["value"].as_u64());
    // With recursive=false, the subdirectory is not entered so the two
    // empty-at-top-level folders compare as identical.
    let differences = tab["difference_count"].as_u64().unwrap_or(u64::MAX);
    assert_eq!(
        differences, 0,
        "non-recursive profile should skip subdirectory; body={body}"
    );
}

// ── Phase 3: versioned bridge contract + response-shape tests ─────────────

#[test]
fn health_includes_bridge_version_field() {
    let paths = test_app_paths("bridge-ver");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response("GET /health HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    let body = json_response_body(&resp);
    assert!(body["ok"].as_bool().unwrap());
    assert_eq!(
        body["bridge_version"].as_u64().unwrap(),
        BRIDGE_VERSION as u64
    );
}

#[test]
fn session_shape_has_active_tab_id_tabs_recent_paths() {
    let paths = test_app_paths("shape-session");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response("GET /session HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    let body = json_response_body(&resp);
    let session = &body["session"];
    assert!(session["active_tab_id"].is_number());
    assert!(session["tabs"].is_array());
    assert!(session["recent_paths"].is_array());
}

#[test]
fn settings_shape_is_json_object() {
    let paths = test_app_paths("shape-settings");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /settings HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    assert!(body.is_object());
}

#[test]
fn filters_list_shape_has_entries_array() {
    let paths = test_app_paths("shape-filters");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /filters/list HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    assert!(body["filters"].is_array());
}

#[test]
fn plugins_list_shape_has_plugins_array() {
    let paths = test_app_paths("shape-plugins");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /plugins/list HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    assert!(body["plugins"].is_array());
}

#[test]
fn profiles_list_shape_has_active_and_profiles() {
    let paths = test_app_paths("shape-profiles");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /profiles/list HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    assert!(body["active"].is_string() || body["active"].is_null());
    assert!(body["profiles"].is_array());
    for p in body["profiles"].as_array().unwrap() {
        assert!(p["id"].is_string());
        assert!(p["name"].is_string());
        assert!(p["builtin"].is_boolean());
    }
}

#[test]
fn compare_tab_shape_has_all_required_fields() {
    let root = test_file_root("shape-compare");
    let left = root.join("l.txt");
    let right = root.join("r.txt");
    fs::write(&left, "hello\n").unwrap();
    fs::write(&right, "world\n").unwrap();
    let paths = test_app_paths("shape-compare");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Text HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let tab = &json_response_body(&resp)["session"]["tabs"][0];
    for key in [
        "id",
        "title",
        "mode",
        "left_path",
        "right_path",
        "status",
        "difference_count",
        "left_rows",
        "right_rows",
        "validation",
        "summary",
    ] {
        assert!(
            tab[key].is_object()
                || tab[key].is_array()
                || tab[key].is_string()
                || tab[key].is_number()
                || tab[key].is_boolean(),
            "tab[{key}] should be present, got: {}",
            tab[key]
        );
    }
}

#[test]
fn compare_row_shape_has_row_id_number_text_state_block_kind() {
    let root = test_file_root("shape-row");
    let left = root.join("l.txt");
    let right = root.join("r.txt");
    fs::write(&left, "aaa\nbbb\n").unwrap();
    fs::write(&right, "aaa\nxxx\n").unwrap();
    let paths = test_app_paths("shape-row");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Text HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let rows = body["session"]["tabs"][0]["left_rows"].as_array().unwrap();
    let row = &rows[0];
    for key in ["row_id", "number", "text", "state", "block_kind"] {
        assert!(
            row[key].is_string() || row[key].is_number(),
            "row[{key}] should be present"
        );
    }
}

#[test]
fn raw_compare_returns_text_diff() {
    let paths = test_app_paths("raw-compare-route");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /raw-compare?left_text=hello&right_text=world&left_name=L&right_name=R HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    let tabs = body["session"]["tabs"].as_array().unwrap();
    assert!(!tabs.is_empty());
    assert!(tabs[0]["difference_count"].as_u64().unwrap() > 0);
}

#[test]
fn raw_compare_accepts_post_json_body() {
    let paths = test_app_paths("raw-compare-post");
    let state = test_bridge_state(None);
    let body = br#"{"left_text":"hello","right_text":"world","left_name":"L","right_name":"R"}"#;
    let resp = String::from_utf8(bridge_response_with_token(
        "POST /raw-compare HTTP/1.1\r\n",
        body,
        &paths,
        &state,
        None,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    let tabs = body["session"]["tabs"].as_array().unwrap();
    assert!(!tabs.is_empty());
    assert!(tabs[0]["difference_count"].as_u64().unwrap() > 0);
}

#[test]
fn raw_compare_preview_does_not_mutate_session() {
    let paths = test_app_paths("raw-compare-preview");
    let state = test_bridge_state(None);

    // Preview computes rows but must not create or replace a compare tab.
    let body = br#"{"left_text":"hello","right_text":"world","left_name":"L","right_name":"R"}"#;
    let resp = String::from_utf8(bridge_response_with_token(
        "POST /raw-compare/preview HTTP/1.1\r\n",
        body,
        &paths,
        &state,
        None,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    assert!(body["difference_count"].as_u64().unwrap() > 0);
    assert!(
        body.get("session").is_none(),
        "preview must not include session state"
    );

    // And the bridge state must have no tabs.
    let session_resp =
        String::from_utf8(bridge_response("GET /session HTTP/1.1\r\n", &paths, &state))
            .expect("utf-8 response");
    let session_body = json_response_body(&session_resp);
    assert!(
        session_body["session"]["tabs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn raw_compare_accepts_one_empty_side() {
    let paths = test_app_paths("raw-compare-empty-side");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /raw-compare?left_text=&right_text=world&left_name=L&right_name=R HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    assert!(
        body["session"]["tabs"][0]["difference_count"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn raw_compare_rejects_missing_left_text() {
    let paths = test_app_paths("raw-compare-missing");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /raw-compare?right_text=world HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("400"));
}

#[test]
fn sessions_reopen_rejects_out_of_range_index() {
    let paths = test_app_paths("reopen-range");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /sessions/reopen?index=999 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("404"));
}

#[test]
fn folder_op_plan_returns_http_response() {
    let root = test_file_root("folder-op-plan");
    let left = root.join("left");
    let right = root.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    let paths = test_app_paths("folder-op-plan-2");
    let state = test_bridge_state(None);
    let _ = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={} HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8");
    let resp = String::from_utf8(bridge_response(
        "GET /folder/op/plan?kind=copy-left-to-right HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1"));
}

#[test]
fn folder_op_execute_rejects_missing_kind() {
    let paths = test_app_paths("folder-op-exec");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /folder/op/execute HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("400") || resp.contains("error"));
}

/// Builds a Folder compare tab over a temp left/right pair where only the
/// right side holds `victim.txt`, so `kind=delete_right` plans one delete.
fn folder_delete_fixture(name: &str) -> (AppPaths, Arc<Mutex<GuiBridgeState>>, PathBuf) {
    let root = test_file_root(name);
    let left = root.join("left");
    let right = root.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    let victim = right.join("victim.txt");
    fs::write(&victim, "doomed").unwrap();
    let paths = test_app_paths(&format!("{name}-paths"));
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={} HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.starts_with("HTTP/1.1 200"), "compare failed: {resp}");
    (paths, state, victim)
}

fn set_delete_preference(paths: &AppPaths, preference: DeletePreference) {
    let store = SettingsStore::new(paths.settings_file());
    let mut settings = store.load_or_default().expect("settings load");
    settings.delete_preference = preference;
    store.save(&settings).expect("settings save");
}

#[test]
fn folder_op_plan_reports_permanent_delete_when_trash_disabled() {
    let (paths, state, _victim) = folder_delete_fixture("folder-op-plan-permanent");
    set_delete_preference(&paths, DeletePreference::Permanent);
    let resp = String::from_utf8(bridge_response(
        "GET /folder/op/plan?kind=delete_right HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.starts_with("HTTP/1.1 200"), "plan failed: {resp}");
    let body = json_response_body(&resp);
    assert_eq!(body["permanent_delete"], serde_json::json!(true));
    let warning = body["permanent_warning"]
        .as_str()
        .expect("permanent_warning should be a string");
    assert!(
        warning.contains("Permanently deleting"),
        "unexpected warning wording: {warning}"
    );
}

#[test]
fn folder_op_plan_reports_trash_delete_as_non_permanent() {
    let (paths, state, _victim) = folder_delete_fixture("folder-op-plan-trash");
    // Default settings keep delete_preference == MoveToTrash.
    let resp = String::from_utf8(bridge_response(
        "GET /folder/op/plan?kind=delete_right HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.starts_with("HTTP/1.1 200"), "plan failed: {resp}");
    let body = json_response_body(&resp);
    assert_eq!(body["permanent_delete"], serde_json::json!(false));
    assert!(body["permanent_warning"].is_null());
}

#[test]
fn folder_op_execute_permanent_delete_without_confirmation_is_409() {
    let (paths, state, victim) = folder_delete_fixture("folder-op-exec-perm-noconfirm");
    set_delete_preference(&paths, DeletePreference::Permanent);
    let resp = String::from_utf8(bridge_response(
        "GET /folder/op/execute?kind=delete_right HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.starts_with("HTTP/1.1 409"), "expected 409: {resp}");
    let body = json_response_body(&resp);
    let message = body["error"].as_str().expect("error message");
    assert!(
        message.contains("confirmation"),
        "unexpected error wording: {message}"
    );
    assert!(victim.exists(), "file must survive an unconfirmed delete");
}

#[test]
fn folder_op_execute_permanent_delete_with_confirmation_deletes() {
    let (paths, state, victim) = folder_delete_fixture("folder-op-exec-perm-confirmed");
    set_delete_preference(&paths, DeletePreference::Permanent);
    let resp = String::from_utf8(bridge_response(
        "GET /folder/op/execute?kind=delete_right&confirm_permanent=1 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.starts_with("HTTP/1.1 200"), "execute failed: {resp}");
    assert!(!victim.exists(), "confirmed delete must remove the file");
}

#[test]
fn folder_op_execute_trash_delete_needs_no_confirmation() {
    let (paths, state, victim) = folder_delete_fixture("folder-op-exec-trash");
    // Default settings keep delete_preference == MoveToTrash.
    let resp = String::from_utf8(bridge_response(
        "GET /folder/op/execute?kind=delete_right HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.starts_with("HTTP/1.1 200"), "execute failed: {resp}");
    assert!(!victim.exists(), "trash delete should move the file away");
}

#[test]
fn reveal_rejects_missing_path() {
    let paths = test_app_paths("reveal-missing");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response("GET /reveal HTTP/1.1\r\n", &paths, &state))
        .expect("utf-8 response");
    assert!(resp.contains("400"));
}

#[test]
fn reveal_rejects_nonexistent_path() {
    let paths = test_app_paths("reveal-noexist");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /reveal?path=/no/such/path/at/all HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("404"));
}

#[test]
fn open_external_rejects_missing_path() {
    let paths = test_app_paths("openext-missing");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /open-external HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("400"));
}

#[test]
fn copy_clipboard_rejects_missing_text() {
    let paths = test_app_paths("clip-missing");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /copy-clipboard HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("400"));
}

#[test]
fn image_mode_creates_session_tab() {
    let root = test_file_root("image-session-tab");
    let left = root.join("a.png");
    let right = root.join("b.png");
    let mut left_data = vec![0u8; 1024];
    left_data[0] = 0x89;
    left_data[1] = 0x50;
    left_data[2] = 0x4e;
    left_data[3] = 0x47;
    let mut right_data = left_data.clone();
    right_data[100] = 0xff;
    fs::write(&left, &left_data).unwrap();
    fs::write(&right, &right_data).unwrap();
    let paths = test_app_paths("image-session-tab");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Image HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    let tab = &body["session"]["tabs"][0];
    assert_eq!(tab["mode"].as_str().unwrap(), "Image");
    assert!(tab["summary"].is_array());
}

#[test]
fn image_compare_endpoint_updates_session_tab() {
    let root = test_file_root("image-endpoint-session-tab");
    let left = root.join("left.png");
    let right = root.join("right.png");
    let left_img: image::RgbaImage =
        image::ImageBuffer::from_fn(4, 4, |_, _| image::Rgba([255, 0, 0, 255]));
    let right_img: image::RgbaImage =
        image::ImageBuffer::from_fn(4, 4, |_, _| image::Rgba([0, 0, 255, 255]));
    left_img.save(&left).unwrap();
    right_img.save(&right).unwrap();
    let paths = test_app_paths("image-endpoint-session-tab");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /compare/image?left={}&right={}&mode=exact HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    assert_eq!(
        body["schema_version"],
        serde_json::json!(RESPONSE_SCHEMA_VERSION)
    );
    assert_eq!(body["session"]["tabs"][0]["mode"], "Image");
    assert_eq!(body["session"]["tabs"][0]["difference_count"], 1);
}

#[test]
fn image_formats_endpoint_reports_supported_decoder_filters() {
    let paths = test_app_paths("image-formats");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /compare/image/formats HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    let globs: Vec<&str> = body["extension_globs"]
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
    assert!(!globs.contains(&"*.bmp"));
    // GIF (animation) and HDR/EXR decoders are now compiled in.
    assert!(globs.contains(&"*.gif"));
    assert!(globs.contains(&"*.hdr"));
    assert!(globs.contains(&"*.exr"));
}

#[test]
fn image_save_overlay_endpoint_copies_last_overlay_png() {
    let root = test_file_root("image-save-overlay");
    let left = root.join("left.png");
    let right = root.join("right.png");
    let saved = root.join("saved-overlay.png");
    let left_img: image::RgbaImage =
        image::ImageBuffer::from_fn(4, 4, |_, _| image::Rgba([255, 0, 0, 255]));
    let right_img: image::RgbaImage =
        image::ImageBuffer::from_fn(4, 4, |_, _| image::Rgba([0, 0, 255, 255]));
    left_img.save(&left).unwrap();
    right_img.save(&right).unwrap();

    let paths = test_app_paths("image-save-overlay");
    let state = test_bridge_state(None);
    let compare = String::from_utf8(bridge_response(
        &format!(
            "GET /compare/image?left={}&right={}&mode=exact&overlay=true HTTP/1.1\r\n",
            url_encode(&left),
            url_encode(&right)
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(compare.contains("HTTP/1.1 200"));
    let compare_body = json_response_body(&compare);
    assert!(
        compare_body["overlay_path"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://")),
        "compare should create an overlay artifact: {compare_body}"
    );

    let save = String::from_utf8(bridge_response(
        &format!(
            "GET /compare/image/save-overlay?path={} HTTP/1.1\r\n",
            url_encode(&saved)
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(save.contains("HTTP/1.1 200"));
    let save_body = json_response_body(&save);
    assert_eq!(save_body["ok"], serde_json::json!(true));
    assert!(saved.exists());

    let overlay = image::open(&saved)
        .expect("saved overlay should decode")
        .to_rgba8();
    assert!(
        overlay
            .pixels()
            .any(|pixel| pixel.0[3] != 0 || pixel.0[0] != 0),
        "saved overlay should contain visible diff pixels"
    );
}

#[test]
fn document_response_builds_session_tab_shape() {
    let response = serde_json::json!({
        "equal": false,
        "left_extractor": "fixture",
        "right_extractor": "fixture",
        "differing_lines": 1,
        "left_text": "alpha\nsame\n",
        "right_text": "beta\nsame\n"
    });
    let tab = document_tab_from_response(
        "/tmp/left.pdf".to_owned(),
        "/tmp/right.pdf".to_owned(),
        &response,
    )
    .expect("document response should produce a tab");
    assert_eq!(tab.mode, "Document");
    assert_eq!(tab.difference_count, 1);
    assert_eq!(tab.left_rows.len(), tab.right_rows.len());
}

#[test]
fn document_mode_compare_endpoint_builds_extracted_session_tab() {
    if !command_available("bash")
        || !command_available("python3")
        || !command_available("pdftotext")
    {
        eprintln!("SKIP: bash, python3, or pdftotext not on PATH");
        return;
    }
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let left = fixture_root.join("tests/fixtures/document/simple.pdf");
    let right = fixture_root.join("tests/fixtures/document/simple-changed.pdf");
    let paths = test_app_paths("document-main-compare");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Document HTTP/1.1\r\n",
            urlencoding::encode(left.to_str().unwrap()),
            urlencoding::encode(right.to_str().unwrap())
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"), "{resp}");
    let body = json_response_body(&resp);
    let tab = &body["session"]["tabs"][0];
    assert_eq!(tab["mode"], "Document");
    assert!(
        tab["left_rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "document tab should contain extracted text rows: {body}"
    );
    assert!(
        tab["difference_count"].as_u64().unwrap_or_default() > 0,
        "changed PDFs should produce document differences: {body}"
    );
}

#[test]
fn webpage_response_builds_session_tab_shape() {
    let response = serde_json::json!({
        "summary": "different (1 diff blocks)",
        "equal": false,
        "truncated": false,
        "rows": [
            {"s":"equal","ln":1,"rn":1,"l":"same","r":"same"},
            {"s":"changed","ln":2,"rn":2,"l":"left","r":"right"}
        ]
    });
    let tab = webpage_tab_from_response(
        "https://left.example/".to_owned(),
        "https://right.example/".to_owned(),
        "html",
        &response,
    )
    .expect("webpage response should produce a tab");
    assert_eq!(tab.mode, "Webpage");
    assert_eq!(tab.difference_count, 1);
    assert_eq!(tab.left_rows.len(), 2);
}

#[test]
fn webpage_summary_only_response_preserves_difference_count() {
    let response = serde_json::json!({
        "summary": "different (left_only=1 right_only=0 different=0)",
        "equal": false,
    });
    let tab = webpage_tab_from_response(
        "https://left.example/".to_owned(),
        "https://right.example/".to_owned(),
        "tree",
        &response,
    )
    .expect("summary-only webpage response should produce a tab");
    assert_eq!(tab.mode, "Webpage");
    assert_eq!(tab.difference_count, 1);
    assert!(tab.left_rows.is_empty());
}

#[test]
fn webpage_recent_session_reopens_from_tab_snapshot() {
    let paths = test_app_paths("webpage-session-restore");
    let response = serde_json::json!({
        "summary": "different (1 diff blocks)",
        "equal": false,
        "truncated": false,
        "rows": [
            {"s":"changed","ln":1,"rn":1,"l":"left html","r":"right html"}
        ]
    });
    let tab = webpage_tab_from_response(
        "https://left.example/".to_owned(),
        "https://right.example/".to_owned(),
        "html",
        &response,
    )
    .expect("webpage response should produce a tab");
    let context = GuiLaunchContext::single_tab(tab);
    record_recent_context(&paths, &context);

    let recent = RecentSessionStore::new(paths.recent_sessions_file(), 20)
        .load_or_default()
        .expect("recent sessions should load");
    assert_eq!(recent.sessions[0].selected_view, CompareViewMode::Webpage);
    assert!(recent.sessions[0].layout.selected_view_state.is_some());

    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /sessions/reopen?index=0 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let tab = &body["session"]["tabs"][0];
    assert_eq!(tab["mode"], "Webpage");
    assert_eq!(tab["left_path"], "https://left.example/");
    assert_eq!(tab["left_rows"].as_array().unwrap().len(), 1);
}

#[test]
fn document_recent_session_reopens_extracted_rows_from_snapshot() {
    let paths = test_app_paths("document-session-restore");
    let response = serde_json::json!({
        "equal": false,
        "left_extractor": "fixture",
        "right_extractor": "fixture",
        "differing_lines": 1,
        "left_text": "alpha\nsame\n",
        "right_text": "beta\nsame\n"
    });
    let tab = document_tab_from_response(
        "/tmp/linsync-left-missing.pdf".to_owned(),
        "/tmp/linsync-right-missing.pdf".to_owned(),
        &response,
    )
    .expect("document response should produce a tab");
    let context = GuiLaunchContext::single_tab(tab);
    record_recent_context(&paths, &context);

    let recent = RecentSessionStore::new(paths.recent_sessions_file(), 20)
        .load_or_default()
        .expect("recent sessions should load");
    assert_eq!(recent.sessions[0].selected_view, CompareViewMode::Document);
    assert!(recent.sessions[0].layout.selected_view_state.is_some());

    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /sessions/reopen?index=0 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    let body = json_response_body(&resp);
    let tab = &body["session"]["tabs"][0];
    assert_eq!(tab["mode"], "Document");
    assert_eq!(tab["difference_count"], 1);
    assert!(tab["left_rows"].as_array().unwrap().len() >= 2);
}

#[test]
fn text_tab_updates_progress_snapshot() {
    let root = test_file_root("text-progress");
    let left = root.join("left.txt");
    let right = root.join("right.txt");
    fs::write(&left, "a\nb\nc\n").unwrap();
    fs::write(&right, "a\nx\nc\n").unwrap();
    let progress = Arc::new(Mutex::new(CompareProgress {
        phase: "starting".to_owned(),
        current: 0,
        total: 0,
        message: String::new(),
    }));
    let tab = text_tab_cancellable(
        &left,
        &right,
        left.display().to_string(),
        right.display().to_string(),
        &GuiCompareOptions::default(),
        &|| false,
        Some(Arc::clone(&progress)),
    )
    .expect("text tab should build");
    assert_eq!(tab.mode, "Text");
    let progress = progress.lock().unwrap();
    assert_eq!(progress.phase, "done");
    assert!(progress.total > 0);
}

#[test]
fn report_json_returns_active_tab_summary() {
    let root = test_file_root("report-json");
    let left = root.join("a.txt");
    let right = root.join("b.txt");
    fs::write(&left, "hello\n").unwrap();
    fs::write(&right, "world\n").unwrap();
    let paths = test_app_paths("report-json");
    let state = test_bridge_state(None);
    let _ = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Text HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8");
    let resp = String::from_utf8(bridge_response(
        "GET /report?format=json HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    assert!(body["tab"]["mode"].is_string());
    assert!(body["tab"]["difference_count"].is_number());
}

#[test]
fn report_unified_produces_text_output() {
    let root = test_file_root("report-unified");
    let left = root.join("a.txt");
    let right = root.join("b.txt");
    fs::write(&left, "same\ndifferent\n").unwrap();
    fs::write(&right, "same\nchanged\n").unwrap();
    let paths = test_app_paths("report-unified");
    let state = test_bridge_state(None);
    let _ = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Text HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8");
    let resp = String::from_utf8(bridge_response(
        "GET /report?format=unified HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("application/json"));
    assert!(resp.contains("--- "));
    assert!(resp.contains("+++ "));
}

#[test]
fn sessions_save_persists_session() {
    let root = test_file_root("session-save");
    let left = root.join("a.txt");
    let right = root.join("b.txt");
    fs::write(&left, "x\n").unwrap();
    fs::write(&right, "y\n").unwrap();
    let paths = test_app_paths("session-save");
    let state = test_bridge_state(None);
    let _ = String::from_utf8(bridge_response(
        &format!(
            "GET /compare?left={}&right={}&mode=Text HTTP/1.1\r\n",
            left.display(),
            right.display()
        ),
        &paths,
        &state,
    ))
    .expect("utf-8");
    let resp = String::from_utf8(bridge_response(
        "GET /sessions/save?title=TestSession HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));
    let body = json_response_body(&resp);
    assert!(body["ok"].as_bool().unwrap());
}

#[test]
fn sessions_save_caps_and_deduplicates_recent_sessions() {
    let root = test_file_root("session-save-cap");
    let paths = test_app_paths("session-save-cap");
    let state = test_bridge_state(None);

    let settings = Settings {
        recent_limit: 3,
        // Disable automatic recent-session recording from /compare so the store
        // contains only the explicit /sessions/save entries we are testing.
        persist_recent_paths: false,
        ..Default::default()
    };
    SettingsStore::new(paths.settings_file())
        .save(&settings)
        .unwrap();

    for i in 0..5 {
        let left = root.join(format!("a{i}.txt"));
        let right = root.join(format!("b{i}.txt"));
        fs::write(&left, "x\n").unwrap();
        fs::write(&right, "y\n").unwrap();
        let left_str = left.to_string_lossy();
        let right_str = right.to_string_lossy();
        let left_url = urlencoding::encode(&left_str);
        let right_url = urlencoding::encode(&right_str);
        let _ = String::from_utf8(bridge_response(
            &format!("GET /compare?left={left_url}&right={right_url}&mode=Text HTTP/1.1\r\n"),
            &paths,
            &state,
        ))
        .expect("utf-8");
        let resp = String::from_utf8(bridge_response(
            &format!("GET /sessions/save?title=Session{i} HTTP/1.1\r\n"),
            &paths,
            &state,
        ))
        .expect("utf-8 response");
        assert!(resp.contains("HTTP/1.1 200"), "save should succeed: {resp}");
    }

    let recent = RecentSessionStore::new(paths.recent_sessions_file(), 3)
        .load_or_default()
        .unwrap();
    assert_eq!(
        recent.sessions.len(),
        3,
        "recent sessions must be capped to recent_limit"
    );
    assert_eq!(recent.sessions[0].session.title, "Session4");
    assert_eq!(recent.sessions[1].session.title, "Session3");
    assert_eq!(recent.sessions[2].session.title, "Session2");

    // Re-save an existing session (same paths, same title) and verify
    // deduplication moves it to the front without growing the list.
    let left = root.join("a2.txt");
    let right = root.join("b2.txt");
    let left_str = left.to_string_lossy();
    let right_str = right.to_string_lossy();
    let left_url = urlencoding::encode(&left_str);
    let right_url = urlencoding::encode(&right_str);
    let _ = String::from_utf8(bridge_response(
        &format!("GET /compare?left={left_url}&right={right_url}&mode=Text HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8");
    let resp = String::from_utf8(bridge_response(
        "GET /sessions/save?title=Session2 HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"));

    let recent = RecentSessionStore::new(paths.recent_sessions_file(), 3)
        .load_or_default()
        .unwrap();
    assert_eq!(recent.sessions.len(), 3);
    assert_eq!(recent.sessions[0].session.title, "Session2");
    assert!(
        recent
            .sessions
            .iter()
            .any(|s| s.session.title == "Session4")
    );
    assert!(
        recent
            .sessions
            .iter()
            .any(|s| s.session.title == "Session3")
    );
    assert!(
        !recent
            .sessions
            .iter()
            .any(|s| s.session.title == "Session1")
    );
}

#[test]
fn cancellable_request_guard_cleans_up_on_drop() {
    let state = test_bridge_state(None);
    let params = vec![("request_id".to_owned(), "req-drop".to_owned())];
    let req = register_cancellable_request(&params, &state, "test", 1, "testing");
    {
        let s = state.lock().unwrap();
        assert!(s.compare_cancels.contains_key("req-drop"));
        assert!(s.compare_progress.contains_key("req-drop"));
    }
    drop(req);
    let s = state.lock().unwrap();
    assert!(
        !s.compare_cancels.contains_key("req-drop"),
        "cancel entry should be removed on drop"
    );
    assert!(
        !s.compare_progress.contains_key("req-drop"),
        "progress entry should be removed on drop"
    );
}

#[test]
fn cancellable_request_guard_cleans_up_on_panic() {
    let state = test_bridge_state(None);
    let state2 = Arc::clone(&state);
    let result = std::thread::spawn(move || {
        let params = vec![("request_id".to_owned(), "req-panic".to_owned())];
        let _req = register_cancellable_request(&params, &state2, "test", 1, "testing");
        {
            let s = state2.lock().unwrap();
            assert!(s.compare_cancels.contains_key("req-panic"));
            assert!(s.compare_progress.contains_key("req-panic"));
        }
        panic!("intentional panic to test RAII cleanup");
    })
    .join();
    assert!(result.is_err(), "thread should have panicked");
    let s = state.lock().unwrap();
    assert!(
        !s.compare_cancels.contains_key("req-panic"),
        "cancel entry should be removed during panic unwinding"
    );
    assert!(
        !s.compare_progress.contains_key("req-panic"),
        "progress entry should be removed during panic unwinding"
    );
}

#[test]
fn bridge_rejects_over_limit_with_503() {
    let paths = test_app_paths("bridge-limit");
    let bridge = start_bridge_server(paths, None).expect("bridge server should start");
    let base = bridge.base_url.strip_prefix("http://").unwrap();
    let (addr, token_path) = base
        .split_once('/')
        .expect("base_url should contain token path");
    let token = token_path.strip_prefix('/').unwrap_or(token_path);

    // Open enough POST requests with an announced body that is never sent to
    // occupy every concurrent handler slot. The handler will block reading the
    // body, keeping the active-connection count high.
    const LIMIT: usize = 16;
    let mut hold: Vec<TcpStream> = Vec::with_capacity(LIMIT);
    for _ in 0..LIMIT {
        let mut stream = TcpStream::connect(addr).expect("should connect to bridge");
        stream
            .write_all(
                format!(
                    "POST /{token}/health HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100000\r\n\r\n"
                )
                .as_bytes(),
            )
            .unwrap();
        hold.push(stream);
    }

    // Give the accept loop a moment to spawn all holding handlers before
    // probing the limit.
    std::thread::sleep(Duration::from_millis(100));

    // The next complete request must be rejected with a 503 instead of hanging.
    let mut overflow = TcpStream::connect(addr).expect("should connect to bridge");
    overflow
        .write_all(format!("GET /{token}/health HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
        .unwrap();
    overflow
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut buf = [0_u8; 1024];
    let n = overflow
        .read(&mut buf)
        .expect("should receive 503 response bytes");
    let resp = String::from_utf8_lossy(&buf[..n]);
    assert!(
        resp.starts_with("HTTP/1.1 503"),
        "expected 503 when connection limit reached, got: {resp}"
    );

    // Clean up the holding connections so the server threads can exit.
    drop(hold);
}

// ── Phase 5: Archive member edit bridge endpoints ────────────────────────
fn make_test_zip(root: &Path, entries: &[(String, String)]) -> PathBuf {
    let zip_path = root.join("test.zip");
    for (name, content) in entries {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }
    let mut cmd = Command::new("zip");
    cmd.arg("-q").arg(&zip_path);
    for (name, _) in entries {
        cmd.arg(name);
    }
    cmd.current_dir(root);
    let status = cmd.status().expect("zip command should be available");
    assert!(status.success(), "zip command failed");
    zip_path
}

#[test]
fn bridge_archive_member_edit_returns_token_and_staged_path() {
    if !command_available("zip") || !command_available("unzip") {
        return;
    }
    let root = test_file_root("archive-edit-bridge");
    let zip = make_test_zip(&root, &[("file.txt".to_owned(), "hello".to_owned())]);
    let paths = test_app_paths("archive-edit-bridge");
    let state = test_bridge_state(None);
    let zip_str = zip.to_string_lossy().to_string();
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /archive/member/edit?archive={}&member=file.txt HTTP/1.1\r\n",
            urlencoding::encode(&zip_str)
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"), "expected 200, got: {resp}");
    let body = json_response_body(&resp);
    assert!(body["ok"].as_bool().unwrap_or(false));
    assert!(body["token"].as_str().unwrap_or("").len() >= 16);
    assert!(
        PathBuf::from(body["staged_path"].as_str().unwrap_or("")).exists(),
        "staged file should exist"
    );
}

#[test]
fn bridge_archive_can_edit_true_for_supported_formats() {
    let paths = test_app_paths("archive-can-edit-true");
    for path in ["x.zip", "x.tar", "x.tar.gz", "x.7z"] {
        let resp = String::from_utf8(bridge_response(
            &format!(
                "GET /archive/can-edit?path={} HTTP/1.1\r\n",
                urlencoding::encode(path)
            ),
            &paths,
            &test_bridge_state(None),
        ))
        .expect("utf-8 response");
        assert!(
            resp.contains("HTTP/1.1 200"),
            "expected 200 for {path}: {resp}"
        );
        let body = json_response_body(&resp);
        assert!(
            body["editable"].as_bool().unwrap_or(false),
            "expected editable=true for {path}"
        );
    }
}

#[test]
fn bridge_archive_can_edit_false_for_unsupported() {
    let paths = test_app_paths("archive-can-edit-false");
    let resp = String::from_utf8(bridge_response(
        &format!(
            "GET /archive/can-edit?path={} HTTP/1.1\r\n",
            urlencoding::encode("x.txt")
        ),
        &paths,
        &test_bridge_state(None),
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 200"), "expected 200, got: {resp}");
    let body = json_response_body(&resp);
    assert_eq!(body["editable"].as_bool(), Some(false));
}

#[test]
fn bridge_archive_member_commit_rejects_invalid_token() {
    let paths = test_app_paths("archive-edit-commit-invalid");
    let state = test_bridge_state(None);
    let resp = String::from_utf8(bridge_response(
        "GET /archive/member/commit?token=nosuchtoken HTTP/1.1\r\n",
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(resp.contains("HTTP/1.1 400"), "expected 400, got: {resp}");
    let body = json_response_body(&resp);
    assert!(body["error"].as_str().unwrap_or("").contains("invalid"));
}

#[test]
fn bridge_archive_member_edit_rejects_concurrent_edit_for_same_archive() {
    if !command_available("zip") || !command_available("unzip") {
        return;
    }
    let root = test_file_root("archive-edit-concurrent");
    let zip = make_test_zip(
        &root,
        &[
            ("a.txt".to_owned(), "a".to_owned()),
            ("b.txt".to_owned(), "b".to_owned()),
        ],
    );
    let paths = test_app_paths("archive-edit-concurrent");
    let state = test_bridge_state(None);
    let zip_str = zip.to_string_lossy().to_string();
    let enc = urlencoding::encode(&zip_str);
    let first = String::from_utf8(bridge_response(
        &format!(
            "GET /archive/member/edit?archive={}&member=a.txt HTTP/1.1\r\n",
            enc
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(first.contains("HTTP/1.1 200"), "first edit should succeed");

    let second = String::from_utf8(bridge_response(
        &format!(
            "GET /archive/member/edit?archive={}&member=b.txt HTTP/1.1\r\n",
            enc
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        second.contains("HTTP/1.1 409"),
        "second edit for same archive should be rejected: {second}"
    );
}

#[test]
fn bridge_archive_member_commit_failure_preserves_staged_edit_and_token() {
    if !command_available("zip") || !command_available("unzip") {
        return;
    }
    let root = test_file_root("archive-edit-stale");
    let zip = make_test_zip(&root, &[("file.txt".to_owned(), "original".to_owned())]);
    let paths = test_app_paths("archive-edit-stale");
    let state = test_bridge_state(None);
    let zip_str = zip.to_string_lossy().to_string();
    let enc = urlencoding::encode(&zip_str);
    let edit = String::from_utf8(bridge_response(
        &format!(
            "GET /archive/member/edit?archive={}&member=file.txt HTTP/1.1\r\n",
            enc
        ),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(edit.contains("HTTP/1.1 200"), "edit should succeed: {edit}");
    let edit_body = json_response_body(&edit);
    let token = edit_body["token"].as_str().unwrap().to_owned();
    let staged = PathBuf::from(edit_body["staged_path"].as_str().unwrap());
    fs::write(&staged, "user's edited bytes").unwrap();

    // Make the archive stale (external modification between edit and commit).
    let mut bytes = fs::read(&zip).unwrap();
    bytes.push(0);
    fs::write(&zip, bytes).unwrap();

    let commit = String::from_utf8(bridge_response(
        &format!("GET /archive/member/commit?token={token} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        commit.contains("HTTP/1.1 409"),
        "stale commit should 409: {commit}"
    );
    let body = json_response_body(&commit);
    assert_eq!(body["retryable"].as_bool(), Some(false));
    assert_eq!(body["token_retained"].as_bool(), Some(true));
    // The user's edit must survive the failed commit.
    assert_eq!(
        body["staged_path"].as_str(),
        Some(staged.to_string_lossy().as_ref())
    );
    assert_eq!(
        fs::read_to_string(&staged).unwrap(),
        "user's edited bytes",
        "failed commit must not destroy the staged edit"
    );

    // The retained token still owns the edit: discard cleans up staging.
    let discard = String::from_utf8(bridge_response(
        &format!("GET /archive/member/discard?token={token} HTTP/1.1\r\n"),
        &paths,
        &state,
    ))
    .expect("utf-8 response");
    assert!(
        discard.contains("HTTP/1.1 200"),
        "discard with retained token should succeed: {discard}"
    );
    assert!(!staged.exists(), "discard should clean up staging");
}
