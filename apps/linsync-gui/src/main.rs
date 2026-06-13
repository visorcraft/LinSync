use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{self, Command, ExitCode};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use linsync::{apply_gui_setting, parse_bool_setting};
use linsync_core::plugin::{PluginClass, PluginExecutionOptions};
use linsync_core::{
    AppPaths, BinaryCompareOptions, CompareOptions, CompareProfile, CompareSession, CompareSide,
    CompareViewMode, ConflictId, DeletePreference, DiffBlockKind, DiffLine, DiffLineKind,
    DiscoveredPlugin, DocumentCompareMode, DocumentCompareOptions, EncodingSummary, FileFilter,
    FilterStore, FolderCompareControl, FolderCompareEvent, FolderCompareOptions, FolderEntryDiff,
    FolderEntryState, FolderOperationKind, FolderOperationOutcome, FolderOperationStatus,
    ImageCompareOptions, ImageCompareResult, MergeAction, MergeChoice, NamedFilters, ProfileId,
    ProfileStore, RecentPathStore, RecentSessionStore, RecentSessions, SessionFile, Settings,
    SettingsStore, TableCompareOptions, TextBookmark, TextCompareOptions, TextCompareResult,
    TextDocument, TextFindOptions, TextInputEncoding, TextRenderMode, TextSyntaxMode,
    ThemePreference, ThreeWayMergeState, TwoWayMergeState, TypedValueKind, builtin_profiles,
    cleanup_artifacts, compare_binary, compare_binary_files, compare_documents,
    compare_documents_cancellable, compare_folders, compare_folders_with_progress, compare_images,
    compare_table_files, compare_text, compare_text_files_with_prediffer, create_save_plan,
    discover_installed_plugins, execute_folder_operation_plan, find_builtin, is_likely_binary,
    permanent_delete_warning, plan_folder_operation, save_artifact, write_encoded_text_with_plan,
};
use serde::{Deserialize, Serialize};

const BRIDGE_VERSION: u32 = 1;
const RESPONSE_SCHEMA_VERSION: u32 = 1;
const GUI_TAB_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
/// Key under `SessionLayout.extra` holding the multi-tab snapshot JSON.
const GUI_TABS_SNAPSHOT_KEY: &str = "gui_tabs_snapshot";

#[cfg(feature = "cxxqt-app")]
mod cxxqt_session;
#[cfg(feature = "cxxqt-smoke")]
mod cxxqt_smoke;
#[cfg(feature = "cxxqt-app")]
mod icon_theme;

fn main() -> ExitCode {
    let paths = linsync_core::AppPaths::from_env();
    if let Err(err) = linsync_core::init_file_logging(&paths) {
        eprintln!("warning: failed to initialize LinSync logging: {err}");
    }
    if let Err(err) = linsync_core::install_panic_log_hook(&paths) {
        eprintln!("warning: failed to install LinSync panic log hook: {err}");
    }

    tracing::info!(
        log_file = %paths.log_file.display(),
        "LinSync GUI shell started"
    );

    match run(&paths, env::args_os().skip(1).collect()) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

fn run(paths: &AppPaths, args: Vec<OsString>) -> Result<ExitCode, String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(ExitCode::SUCCESS);
    }

    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("linsync {}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }

    let qml_file = resolve_qml_file()?;
    if args.iter().any(|arg| arg == "--print-qml-path") {
        println!("{}", qml_file.display());
        return Ok(ExitCode::SUCCESS);
    }
    let mut launch_context = build_launch_context(&args);

    // On a bare launch we never auto-populate the Compare page's Left/Right
    // path fields (or its diff state) from any previous session — that was the
    // source of "defaults to fake folder names" (/tmp/bigfolder etc.). Prior
    // work is resumed explicitly from the Sessions sidebar; the /sessions/*
    // responders filter leftover test-fixture entries themselves.

    // A Git-mergetool launch (LINSYNC_MERGE_* env) takes priority: open the
    // Merge workspace with the three inputs and the predetermined output path.
    if let Some(merge) = merge_launch_from_env() {
        let ctx = launch_context.get_or_insert_with(GuiLaunchContext::empty);
        ctx.startup_section = Some("merge".to_owned());
        ctx.merge = Some(merge);
    }

    // Bridge LINSYNC_STARTUP_SECTION into the launch context so QML can read
    // it on Component.onCompleted. This is the canonical path for screenshot
    // capture (post-`--` argv to qml6 gets eaten as file paths, see #1).
    if let Ok(section) = env::var("LINSYNC_STARTUP_SECTION")
        && !section.is_empty()
    {
        let ctx = launch_context.get_or_insert_with(GuiLaunchContext::empty);
        ctx.startup_section = Some(section);
    }
    let launch_context_path = match launch_context.as_ref() {
        Some(context) => Some(write_launch_context(paths, context)?),
        None => None,
    };

    #[cfg(feature = "cxxqt-app")]
    if use_cxxqt_host() {
        return run_cxxqt_host(
            paths,
            &qml_file,
            launch_context_path.as_deref(),
            launch_context.clone(),
        );
    }

    let bridge = start_bridge_server(paths.clone(), launch_context.clone())?;

    let runner = resolve_qml_runner().ok_or_else(|| {
        "could not find a Qt QML runner; install qml6 or set LINSYNC_QML_RUNNER".to_owned()
    })?;

    tracing::info!(
        qml_runner = %runner.display(),
        qml_file = %qml_file.display(),
        "Launching LinSync QML shell"
    );

    let qml_root = qml_file
        .parent()
        .ok_or_else(|| format!("invalid QML file path '{}'", qml_file.display()))?;
    let mut command = Command::new(&runner);
    if let Some(icon_file) = resolve_window_icon_file(&qml_file) {
        command.arg("--qwindowicon").arg(icon_file);
    }
    command.arg("-I").arg(qml_root).arg("-f").arg(&qml_file);

    // Write bridge info to a well-known temp path so QML can read via
    // XMLHttpRequest. qml6 treats everything after the QML file as
    // additional files to load, so `--` arg separation doesn't work.
    let bridge_info = serde_json::json!({
        "bridge_url": &bridge.base_url,
        "version": env!("CARGO_PKG_VERSION"),
        "context_path": launch_context_path.as_ref().map(|p| p.display().to_string()),
        "section": env::var("LINSYNC_STARTUP_SECTION").ok().filter(|s| !s.is_empty()),
    });
    let payload = serde_json::to_string(&bridge_info).unwrap();
    if let Some(path) = write_bridge_info_file(payload.as_bytes()) {
        command.env("LINSYNC_BRIDGE_INFO", path.display().to_string());
    } else {
        tracing::warn!("bridge info sidecar not written; GUI will run without the HTTP bridge");
    }
    // Qt6 disables HTTP GET on file:/// by default for local QML.
    // Must opt in so the QML can read the bridge info sidecar.
    command.env("QML_XHR_ALLOW_FILE_READ", "1");
    // Use Fusion style to avoid Breeze QML theme TextArea crash on Qt 6.11.
    // The user can still override via their own env or style sheet.
    command.env("QT_QUICK_CONTROLS_STYLE", "Fusion");

    let status = command.status().map_err(|err| {
        format!(
            "failed to launch Qt QML runner '{}': {err}",
            runner.display()
        )
    })?;

    if status.success() {
        return Ok(ExitCode::SUCCESS);
    }

    Ok(ExitCode::from(
        status.code().unwrap_or(2).clamp(1, u8::MAX as i32) as u8,
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiLaunchContext {
    session: GuiSessionState,
    /// Optional initial sidebar section name (used by screenshot captures).
    /// Honoured by Main.qml on Component.onCompleted; populated from the
    /// LINSYNC_STARTUP_SECTION env var when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    startup_section: Option<String>,
    /// Three-way merge launch (Git mergetool). When present, Main.qml opens the
    /// Merge workspace with these inputs and a predetermined output path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    merge: Option<GuiMergeLaunch>,
}

/// A Git-mergetool launch request: the three inputs plus the output path the
/// resolved merge must be written to (so `linsync-cli mergetool` can validate
/// it after the GUI exits).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiMergeLaunch {
    base: String,
    /// "local" side (Git's $LOCAL) — shown as the left column.
    left: String,
    /// "remote" side (Git's $REMOTE) — shown as the right column.
    right: String,
    /// Where the resolved output must be written ($MERGED).
    output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiSessionState {
    active_tab_id: u64,
    tabs: Vec<GuiCompareTab>,
    recent_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiCompareTab {
    id: u64,
    title: String,
    mode: String,
    left_path: String,
    right_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_path: Option<String>,
    status: String,
    difference_count: usize,
    left_dirty: bool,
    right_dirty: bool,
    #[serde(default)]
    can_undo: bool,
    #[serde(default)]
    can_redo: bool,
    validation: GuiOpenValidation,
    summary: Vec<GuiSummaryItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    left_rows: Vec<GuiLineRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    right_rows: Vec<GuiLineRow>,
    /// When a text diff exceeds [`TEXT_WINDOW_THRESHOLD`] rows it is served in
    /// windows: `left_rows`/`right_rows` carry only the first window and this is
    /// the full row count, so the GUI fetches the rest on demand via
    /// `/compare/text/window`. `None` (the default, omitted on the wire) means
    /// every row is embedded as before — small/medium diffs are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total_rows: Option<usize>,
    /// Full change-row index list for a windowed text diff so next/prev-change
    /// navigation reaches differences outside the loaded window. Empty (and
    /// omitted) for non-windowed diffs, where the GUI derives it from the rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    diff_row_indexes: Vec<usize>,
    /// Full find-match row index list for a windowed text diff that was compared
    /// with an active find, so find navigation reaches matches outside the
    /// loaded window. Empty (and omitted) when no find is active or the diff
    /// is not windowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    search_row_indexes: Vec<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    folder_entries: Vec<GuiFolderEntry>,
    /// When a folder comparison exceeds [`FOLDER_WINDOW_THRESHOLD`] entries it is
    /// served windowed: `folder_entries` carries only the first page and this is
    /// the full entry count, so the GUI pages the rest through `/folder/query`
    /// (sorting/filtering server-side). `None` (the default, omitted on the
    /// wire) means every entry is embedded — small folders are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder_total: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    encoding_metadata: Option<EncodingSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    table_headers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    table_cells: Option<Vec<linsync_core::TableRowDiff>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    artifacts: Vec<linsync_core::CompareArtifact>,
    /// Rendered page summaries for document compare in rendered mode.
    /// Transient: `file://` URIs point to per-process temp cache directories
    /// that do not survive session restore, so they must not be serialized.
    #[serde(skip)]
    rendered_pages: Option<Vec<GuiRenderedPage>>,
    /// Options used to build this tab, so merge-copy, recompare, and window
    /// fetches can honor the same profile / per-request overrides instead of
    /// falling back to defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    options: Option<GuiCompareOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiTabSnapshot {
    schema_version: u32,
    tab: GuiCompareTab,
}

/// Snapshot of every open tab, persisted in the recent session's
/// `layout.extra["gui_tabs_snapshot"]` so the GUI can restore a multi-tab
/// workspace (not just the active tab) on next launch. Stored in the
/// forward-compat extra map, so it needs no core storage-schema change.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiMultiTabSnapshot {
    schema_version: u32,
    active_tab_id: u64,
    tabs: Vec<GuiCompareTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiOpenValidation {
    compatible: bool,
    path_kind: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiSummaryItem {
    label: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiLineRow {
    row_id: String,
    number: Option<usize>,
    text: String,
    state: String,
    /// Block-level kind: "equal", "difference", or "moved".
    /// Defaults to an empty string for rows produced without block info.
    #[serde(default)]
    block_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folded_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    syntax_spans: Vec<linsync_core::SyntaxSpan>,
    #[serde(default)]
    has_find_match: bool,
    #[serde(default)]
    bookmarked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiRenderedPage {
    page: usize,
    equal: bool,
    diff_ratio: f64,
    left_uri: Option<String>,
    right_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuiFolderEntry {
    path: String,
    is_dir: bool,
    /// Entry kind for type filtering: "file" / "directory" / "symlink" / "special".
    entry_type: String,
    state: String,
    left_size: Option<u64>,
    right_size: Option<u64>,
    left_modified: Option<String>,
    right_modified: Option<String>,
    method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuiSettings {
    theme_preference: ThemePreference,
    font_size: u8,
    font_family: String,
    tab_width: u8,
    show_line_numbers: bool,
    show_whitespace: bool,
    word_wrap: bool,
    ignore_case: bool,
    ignore_whitespace: bool,
    ignore_blank_lines: bool,
    ignore_eol: bool,
    detect_moves: bool,
    eol_normalization: String,
    default_compare_mode: String,
    open_last_session: bool,
    confirm_on_close: bool,
    persist_recent_paths: bool,
    max_recent_paths: usize,
    reduce_motion: bool,
    keep_archive_backup: bool,
}

impl From<&Settings> for GuiSettings {
    fn from(settings: &Settings) -> Self {
        Self {
            theme_preference: settings.theme_preference,
            font_size: settings.pane_font_size,
            font_family: settings.pane_font_family.clone(),
            tab_width: settings.pane_tab_width,
            show_line_numbers: settings.show_line_numbers,
            show_whitespace: settings.show_whitespace,
            word_wrap: settings.word_wrap,
            ignore_case: settings.ignore_case,
            ignore_whitespace: settings.ignore_whitespace,
            ignore_blank_lines: settings.ignore_blank_lines,
            ignore_eol: settings.ignore_eol,
            detect_moves: settings.detect_moves,
            eol_normalization: settings.eol_normalization.clone(),
            default_compare_mode: settings.default_compare_mode.clone(),
            open_last_session: settings.open_last_session,
            confirm_on_close: settings.confirm_on_close,
            persist_recent_paths: settings.persist_recent_paths,
            max_recent_paths: settings.recent_limit,
            reduce_motion: settings.reduce_motion,
            keep_archive_backup: settings.keep_archive_backup,
        }
    }
}

struct BridgeServer {
    base_url: String,
}

struct GuiBridgeState {
    session: GuiSessionState,
    next_tab_id: u64,
    undo_stacks: HashMap<u64, Vec<GuiCompareTab>>,
    redo_stacks: HashMap<u64, Vec<GuiCompareTab>>,
    three_way_session: Option<ThreeWayMergeState>,
    /// In-memory plugin-enabled map protected by its own lock so that concurrent
    /// toggle operations are atomic relative to each other and to list reads.
    plugin_enabled: Arc<Mutex<HashMap<String, bool>>>,
    /// Cancellation flags for in-flight `/compare` requests, keyed by the
    /// `request_id` the QML supplies. `/cancel?id=X` flips the flag; the compare
    /// polls it and aborts. Inserted/removed under the state lock, but the flag
    /// itself is atomic so `/cancel` never blocks on the running compare.
    compare_cancels: HashMap<String, Arc<AtomicBool>>,
    /// Progress snapshots for in-flight `/compare` requests, keyed by
    /// `request_id`. Updated by the compare thread, read by `/progress?id=X`.
    compare_progress: HashMap<String, Arc<Mutex<CompareProgress>>>,
    last_image_result: Option<ImageCompareResult>,
    last_image_overlay_path: Option<PathBuf>,
    /// In-progress archive member edits, keyed by opaque token. The bridge
    /// holds the [`MemberEditContext`] so clients never supply paths to commit.
    archive_edit_tokens: HashMap<String, linsync_core::MemberEditContext>,
    /// Owner-only temp directories holding rendered page PNGs for document
    /// compare tabs. Cleaned up when the tab is closed or overwritten.
    rendered_page_cache_dirs: HashMap<u64, PathBuf>,
    /// Extracted archive cache directories, keyed by tab id. Cleaned up when
    /// the tab is closed.
    archive_extract_dirs: HashMap<u64, PathBuf>,
}

struct CompareProgress {
    phase: String,
    current: usize,
    total: usize,
    message: String,
}

fn set_progress(
    progress: &Option<Arc<Mutex<CompareProgress>>>,
    phase: &str,
    current: usize,
    total: usize,
    message: String,
) {
    if let Some(progress) = progress
        && let Ok(mut progress) = progress.lock()
    {
        progress.phase = phase.to_owned();
        progress.current = current;
        progress.total = total;
        progress.message = message;
    }
}

fn register_progress_request(
    params: &[(String, String)],
    state: &Arc<Mutex<GuiBridgeState>>,
    phase: &str,
    total: usize,
    message: &str,
) -> (Option<String>, Option<Arc<Mutex<CompareProgress>>>) {
    let Some(request_id) = query_value(params, "request_id").map(str::to_owned) else {
        return (None, None);
    };
    let progress = Arc::new(Mutex::new(CompareProgress {
        phase: phase.to_owned(),
        current: 0,
        total,
        message: message.to_owned(),
    }));
    if let Ok(mut state) = state.lock() {
        state
            .compare_progress
            .insert(request_id.clone(), Arc::clone(&progress));
    }
    (Some(request_id), Some(progress))
}

fn remove_progress_request(request_id: Option<&str>, state: &Arc<Mutex<GuiBridgeState>>) {
    if let Some(id) = request_id
        && let Ok(mut state) = state.lock()
    {
        state.compare_progress.remove(id);
    }
}

const GUI_HISTORY_LIMIT: usize = 32;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct GuiCompareOptions {
    text: TextCompareOptions,
    folder: FolderCompareOptions,
    table: TableCompareOptions,
    binary: BinaryCompareOptions,
    image: ImageCompareOptions,
    document: DocumentCompareOptions,
}

impl GuiLaunchContext {
    fn single_tab(tab: GuiCompareTab) -> Self {
        let recent_paths = unique_recent_paths([tab.left_path.clone(), tab.right_path.clone()]);
        Self {
            session: GuiSessionState {
                active_tab_id: tab.id,
                tabs: vec![tab],
                recent_paths,
            },
            startup_section: None,
            merge: None,
        }
    }

    /// Build a multi-tab context from a saved snapshot. `active_tab_id` is
    /// clamped to the first tab when it does not match any restored tab.
    fn from_tabs(tabs: Vec<GuiCompareTab>, active_tab_id: u64) -> Self {
        let recent_paths = unique_recent_paths(
            tabs.iter()
                .flat_map(|tab| [tab.left_path.clone(), tab.right_path.clone()]),
        );
        let active_tab_id = if tabs.iter().any(|tab| tab.id == active_tab_id) {
            active_tab_id
        } else {
            tabs.first().map(|tab| tab.id).unwrap_or(0)
        };
        Self {
            session: GuiSessionState {
                active_tab_id,
                tabs,
                recent_paths,
            },
            startup_section: None,
            merge: None,
        }
    }

    fn empty() -> Self {
        Self {
            session: GuiSessionState {
                active_tab_id: 0,
                tabs: vec![],
                recent_paths: vec![],
            },
            startup_section: None,
            merge: None,
        }
    }

    fn active_tab(&self) -> Option<&GuiCompareTab> {
        self.session
            .tabs
            .iter()
            .find(|tab| tab.id == self.session.active_tab_id)
            .or_else(|| self.session.tabs.first())
    }
}

impl GuiBridgeState {
    fn new(initial_context: Option<GuiLaunchContext>) -> Self {
        let session = initial_context
            .map(|context| context.session)
            .unwrap_or_else(|| GuiSessionState {
                active_tab_id: 0,
                tabs: Vec::new(),
                recent_paths: Vec::new(),
            });
        let next_tab_id = session.tabs.iter().map(|tab| tab.id).max().unwrap_or(0) + 1;
        Self {
            session,
            next_tab_id,
            undo_stacks: HashMap::new(),
            redo_stacks: HashMap::new(),
            three_way_session: None,
            plugin_enabled: Arc::new(Mutex::new(HashMap::new())),
            compare_cancels: HashMap::new(),
            compare_progress: HashMap::new(),
            last_image_result: None,
            last_image_overlay_path: None,
            archive_edit_tokens: HashMap::new(),
            rendered_page_cache_dirs: HashMap::new(),
            archive_extract_dirs: HashMap::new(),
        }
    }

    fn context(&self) -> GuiLaunchContext {
        let mut session = self.session.clone();
        for tab in &mut session.tabs {
            tab.can_undo = self
                .undo_stacks
                .get(&tab.id)
                .is_some_and(|stack| !stack.is_empty());
            tab.can_redo = self
                .redo_stacks
                .get(&tab.id)
                .is_some_and(|stack| !stack.is_empty());
        }
        GuiLaunchContext {
            session,
            startup_section: None,
            merge: None,
        }
    }

    fn apply_compare(&mut self, mut tab: GuiCompareTab, new_tab: bool) -> GuiLaunchContext {
        if new_tab || self.session.tabs.is_empty() {
            tab.id = self.next_tab_id;
            self.next_tab_id += 1;
            self.session.active_tab_id = tab.id;
            self.undo_stacks.remove(&tab.id);
            self.redo_stacks.remove(&tab.id);
            self.session.tabs.push(tab);
        } else {
            tab.id = self.session.active_tab_id;
            self.undo_stacks.remove(&tab.id);
            self.redo_stacks.remove(&tab.id);
            if let Some(existing) = self
                .session
                .tabs
                .iter_mut()
                .find(|existing| existing.id == self.session.active_tab_id)
            {
                *existing = tab;
            } else {
                self.session.active_tab_id = tab.id;
                self.session.tabs.push(tab);
            }
        }

        self.refresh_recent_paths();
        self.context()
    }

    fn close_tab(&mut self, tab_id: u64) -> GuiLaunchContext {
        self.session.tabs.retain(|tab| tab.id != tab_id);
        self.undo_stacks.remove(&tab_id);
        self.redo_stacks.remove(&tab_id);
        if let Some(dir) = self.rendered_page_cache_dirs.remove(&tab_id) {
            let _ = fs::remove_dir_all(dir);
        }
        if let Some(dir) = self.archive_extract_dirs.remove(&tab_id) {
            let _ = fs::remove_dir_all(dir);
        }
        if self.session.active_tab_id == tab_id {
            self.session.active_tab_id = self
                .session
                .tabs
                .last()
                .map(|tab| tab.id)
                .unwrap_or_default();
        }
        self.refresh_recent_paths();
        self.context()
    }

    fn activate_tab(&mut self, tab_id: u64) -> Result<GuiLaunchContext, String> {
        if self.session.tabs.iter().any(|tab| tab.id == tab_id) {
            self.session.active_tab_id = tab_id;
            Ok(self.context())
        } else {
            Err(format!("unknown tab id: {tab_id}"))
        }
    }

    fn set_bookmark(&mut self, row: usize, bookmarked: bool) -> Result<GuiLaunchContext, String> {
        let active_tab_id = self.session.active_tab_id;
        let tab = self
            .session
            .tabs
            .iter_mut()
            .find(|tab| tab.id == active_tab_id)
            .ok_or_else(|| "no active compare tab".to_owned())?;
        if row >= tab.left_rows.len() && row >= tab.right_rows.len() {
            return Err(format!("row index {row} out of range"));
        }
        if let Some(left) = tab.left_rows.get_mut(row) {
            left.bookmarked = bookmarked;
        }
        if let Some(right) = tab.right_rows.get_mut(row) {
            right.bookmarked = bookmarked;
        }
        Ok(self.context())
    }

    fn copy_row(&mut self, row: usize, direction: &str) -> Result<GuiLaunchContext, String> {
        let active_tab_id = self.session.active_tab_id;
        let snapshot = self
            .active_tab()
            .ok_or_else(|| "no active compare tab".to_owned())?
            .clone();
        let tab = self
            .session
            .tabs
            .iter_mut()
            .find(|tab| tab.id == active_tab_id)
            .ok_or_else(|| "no active compare tab".to_owned())?;

        copy_tab_row(tab, row, direction)?;
        self.push_undo_snapshot(active_tab_id, snapshot);
        self.redo_stacks.remove(&active_tab_id);
        Ok(self.context())
    }

    fn copy_all(&mut self, direction: &str) -> Result<GuiLaunchContext, String> {
        let active_tab_id = self.session.active_tab_id;
        let snapshot = self
            .active_tab()
            .ok_or_else(|| "no active compare tab".to_owned())?
            .clone();
        let tab = self
            .session
            .tabs
            .iter_mut()
            .find(|tab| tab.id == active_tab_id)
            .ok_or_else(|| "no active compare tab".to_owned())?;

        copy_tab_all(tab, direction)?;
        self.push_undo_snapshot(active_tab_id, snapshot);
        self.redo_stacks.remove(&active_tab_id);
        Ok(self.context())
    }

    fn undo(&mut self) -> Result<GuiLaunchContext, String> {
        let active_tab_id = self.session.active_tab_id;
        let current = self
            .active_tab()
            .ok_or_else(|| "no active compare tab".to_owned())?
            .clone();
        let Some(snapshot) = self
            .undo_stacks
            .get_mut(&active_tab_id)
            .and_then(|stack| stack.pop())
        else {
            return Err("nothing to undo".to_owned());
        };

        let tab = self
            .session
            .tabs
            .iter_mut()
            .find(|tab| tab.id == active_tab_id)
            .ok_or_else(|| "no active compare tab".to_owned())?;
        *tab = snapshot;
        tab.status = "Undid last merge action".to_owned();
        self.push_redo_snapshot(active_tab_id, current);
        Ok(self.context())
    }

    fn redo(&mut self) -> Result<GuiLaunchContext, String> {
        let active_tab_id = self.session.active_tab_id;
        let current = self
            .active_tab()
            .ok_or_else(|| "no active compare tab".to_owned())?
            .clone();
        let Some(snapshot) = self
            .redo_stacks
            .get_mut(&active_tab_id)
            .and_then(|stack| stack.pop())
        else {
            return Err("nothing to redo".to_owned());
        };

        let tab = self
            .session
            .tabs
            .iter_mut()
            .find(|tab| tab.id == active_tab_id)
            .ok_or_else(|| "no active compare tab".to_owned())?;
        *tab = snapshot;
        tab.status = "Redid last merge action".to_owned();
        self.push_undo_snapshot(active_tab_id, current);
        Ok(self.context())
    }

    fn save_side(&mut self, side: &str) -> Result<GuiLaunchContext, String> {
        let active_tab_id = self.session.active_tab_id;
        let tab = self
            .session
            .tabs
            .iter_mut()
            .find(|tab| tab.id == active_tab_id)
            .ok_or_else(|| "no active compare tab".to_owned())?;

        save_tab_side(tab, side)?;
        Ok(self.context())
    }

    fn refresh_recent_paths(&mut self) {
        self.session.recent_paths = unique_recent_paths(
            self.session
                .tabs
                .iter()
                .flat_map(|tab| [tab.left_path.clone(), tab.right_path.clone()]),
        );
    }

    fn active_tab(&self) -> Option<&GuiCompareTab> {
        let active_tab_id = self.session.active_tab_id;
        self.session.tabs.iter().find(|tab| tab.id == active_tab_id)
    }

    fn push_undo_snapshot(&mut self, tab_id: u64, mut snapshot: GuiCompareTab) {
        snapshot.can_undo = false;
        snapshot.can_redo = false;
        push_limited_snapshot(self.undo_stacks.entry(tab_id).or_default(), snapshot);
    }

    fn push_redo_snapshot(&mut self, tab_id: u64, mut snapshot: GuiCompareTab) {
        snapshot.can_undo = false;
        snapshot.can_redo = false;
        push_limited_snapshot(self.redo_stacks.entry(tab_id).or_default(), snapshot);
    }
}

fn push_limited_snapshot(stack: &mut Vec<GuiCompareTab>, snapshot: GuiCompareTab) {
    stack.push(snapshot);
    if stack.len() > GUI_HISTORY_LIMIT {
        stack.remove(0);
    }
}

fn save_tab_side(tab: &mut GuiCompareTab, side: &str) -> Result<(), String> {
    if tab.mode != "Text" {
        return Err("save currently supports text compare tabs only".to_owned());
    }

    match side {
        "left" => save_tab_rows(
            "left",
            &tab.left_path,
            &tab.left_rows,
            &mut tab.left_dirty,
            &mut tab.status,
        ),
        "right" => save_tab_rows(
            "right",
            &tab.right_path,
            &tab.right_rows,
            &mut tab.right_dirty,
            &mut tab.status,
        ),
        "dirty" | "all" => {
            let mut saved = Vec::new();
            if tab.left_dirty {
                save_tab_rows(
                    "left",
                    &tab.left_path,
                    &tab.left_rows,
                    &mut tab.left_dirty,
                    &mut tab.status,
                )?;
                saved.push("left");
            }
            if tab.right_dirty {
                save_tab_rows(
                    "right",
                    &tab.right_path,
                    &tab.right_rows,
                    &mut tab.right_dirty,
                    &mut tab.status,
                )?;
                saved.push("right");
            }
            tab.status = if saved.is_empty() {
                "No dirty sides to save".to_owned()
            } else {
                format!("Saved {}", saved.join(" and "))
            };
            Ok(())
        }
        _ => Err(format!("unsupported save side: {side}")),
    }
}

fn save_tab_rows(
    side: &str,
    path: &str,
    rows: &[GuiLineRow],
    dirty: &mut bool,
    status: &mut String,
) -> Result<(), String> {
    if !*dirty {
        *status = format!("{side} side already clean");
        return Ok(());
    }
    if path.is_empty() {
        return Err(format!("cannot save {side} side without a path"));
    }

    let target = PathBuf::from(path);
    let document = TextDocument::from_path(&target)
        .map_err(|err| format!("failed to read {side} side before save: {err}"))?;
    if document.read_only {
        return Err(format!("cannot save read-only {side} side"));
    }
    let contents = rows_to_document_text(rows, &document);
    let plan = create_save_plan(&target, true);
    write_encoded_text_with_plan(&plan, &contents, document.encoding)
        .map_err(|err| format!("failed to save {side} side: {err}"))?;

    *dirty = false;
    *status = format!("Saved {side} side with backup");
    Ok(())
}

fn rows_to_document_text(rows: &[GuiLineRow], document: &TextDocument) -> String {
    let ending = line_ending_text(document.line_ending);
    let mut text = rows
        .iter()
        .filter(|row| row.number.is_some())
        .map(|row| row.text.as_str())
        .collect::<Vec<_>>()
        .join(ending);
    if document
        .lines
        .last()
        .and_then(|line| line.newline)
        .is_some()
    {
        text.push_str(ending);
    }
    text
}

fn line_ending_text(ending: linsync_core::LineEnding) -> &'static str {
    match ending {
        linsync_core::LineEnding::Crlf => "\r\n",
        linsync_core::LineEnding::Cr => "\r",
        linsync_core::LineEnding::Lf
        | linsync_core::LineEnding::Mixed
        | linsync_core::LineEnding::None => "\n",
    }
}

fn copy_tab_row(tab: &mut GuiCompareTab, row: usize, direction: &str) -> Result<(), String> {
    if tab.mode == "Text" && copy_text_diff_block(tab, row, direction)? {
        return Ok(());
    }

    ensure_row_pair(tab, row)?;
    let left = tab.left_rows[row].clone();
    let right = tab.right_rows[row].clone();

    match direction {
        "left_to_right" => {
            tab.right_rows[row] = GuiLineRow {
                row_id: first_non_empty(&right.row_id, &left.row_id),
                number: left.number,
                text: left.text,
                state: "equal".to_owned(),
                block_kind: "equal".to_owned(),
                folded_count: None,
                syntax_spans: Vec::new(),
                has_find_match: false,
                bookmarked: false,
            };
            tab.right_dirty = true;
            tab.status = "Copied left to right".to_owned();
        }
        "right_to_left" => {
            tab.left_rows[row] = GuiLineRow {
                row_id: first_non_empty(&left.row_id, &right.row_id),
                number: right.number,
                text: right.text,
                state: "equal".to_owned(),
                block_kind: "equal".to_owned(),
                folded_count: None,
                syntax_spans: Vec::new(),
                has_find_match: false,
                bookmarked: false,
            };
            tab.left_dirty = true;
            tab.status = "Copied right to left".to_owned();
        }
        _ => return Err(format!("unsupported copy direction: {direction}")),
    }

    normalize_tab_row(tab, row);
    tab.difference_count = tab_difference_count(tab);
    Ok(())
}

fn copy_text_diff_block(
    tab: &mut GuiCompareTab,
    row: usize,
    direction: &str,
) -> Result<bool, String> {
    let compare = compare_tab_text_rows(tab);
    let Some(block_index) = diff_block_index_for_row(&compare, row) else {
        return Ok(false);
    };

    let mut state = TwoWayMergeState::new(compare);
    let action = match direction {
        "left_to_right" => MergeAction::CopyLeftToRight { block_index },
        "right_to_left" => MergeAction::CopyRightToLeft { block_index },
        _ => return Err(format!("unsupported copy direction: {direction}")),
    };

    state
        .apply(action)
        .map_err(|err| format!("failed to apply text merge: {err}"))?;
    state.recompute(&tab_text_options(tab));

    apply_text_merge_state(tab, &state);
    tab.status = match direction {
        "left_to_right" => "Copied left to right".to_owned(),
        "right_to_left" => "Copied right to left".to_owned(),
        _ => unreachable!("direction checked above"),
    };
    Ok(true)
}

fn copy_tab_all(tab: &mut GuiCompareTab, direction: &str) -> Result<(), String> {
    if tab.mode != "Text" {
        return Err("copy all currently supports text compare tabs only".to_owned());
    }

    let compare = compare_tab_text_rows(tab);
    let diff_blocks = compare
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            matches!(block.kind, DiffBlockKind::Difference).then_some(index)
        })
        .rev()
        .collect::<Vec<_>>();
    if diff_blocks.is_empty() {
        tab.status = "No differences to copy".to_owned();
        return Ok(());
    }

    let mut state = TwoWayMergeState::new(compare);
    for block_index in diff_blocks {
        let action = match direction {
            "left_to_right" => MergeAction::CopyLeftToRight { block_index },
            "right_to_left" => MergeAction::CopyRightToLeft { block_index },
            _ => return Err(format!("unsupported copy direction: {direction}")),
        };
        state
            .apply(action)
            .map_err(|err| format!("failed to apply text merge: {err}"))?;
    }
    state.recompute(&tab_text_options(tab));

    apply_text_merge_state(tab, &state);
    tab.status = match direction {
        "left_to_right" => "Copied all left to right".to_owned(),
        "right_to_left" => "Copied all right to left".to_owned(),
        _ => unreachable!("direction checked above"),
    };
    Ok(())
}

fn apply_text_merge_state(tab: &mut GuiCompareTab, state: &TwoWayMergeState) {
    let (left_rows, right_rows) = text_rows_for_gui(&state.compare.lines, &state.compare.blocks);
    tab.left_rows = left_rows;
    tab.right_rows = right_rows;
    tab.summary = text_summary_items(&state.compare);
    tab.difference_count = state.compare.summary.differences;
    tab.left_dirty = state.left.dirty || tab.left_dirty;
    tab.right_dirty = state.right.dirty || tab.right_dirty;
}

fn compare_tab_text_rows(tab: &GuiCompareTab) -> TextCompareResult {
    let left = rows_plain_text(&tab.left_rows);
    let right = rows_plain_text(&tab.right_rows);
    let left_document = TextDocument::from_text(&tab.left_path, &left);
    let right_document = TextDocument::from_text(&tab.right_path, &right);
    compare_documents(left_document, right_document, &tab_text_options(tab))
}

fn tab_text_options(tab: &GuiCompareTab) -> TextCompareOptions {
    tab.options
        .as_ref()
        .map(|o| o.text.clone())
        .unwrap_or_default()
}

fn rows_plain_text(rows: &[GuiLineRow]) -> String {
    let mut text = rows
        .iter()
        .filter(|row| row.number.is_some())
        .map(|row| row.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

fn diff_block_index_for_row(compare: &TextCompareResult, selected_row: usize) -> Option<usize> {
    let selected = compare.lines.get(selected_row)?;
    if matches!(selected.kind, DiffLineKind::Equal) {
        return None;
    }

    let mut current_block = 0;
    let mut current_kind = compare
        .lines
        .first()
        .map(|line| gui_diff_block_kind(line.kind))?;
    for (row_index, line) in compare.lines.iter().enumerate() {
        let kind = gui_diff_block_kind(line.kind);
        if row_index > 0 && kind != current_kind {
            current_block += 1;
            current_kind = kind;
        }
        if row_index == selected_row {
            return Some(current_block);
        }
    }
    None
}

fn gui_diff_block_kind(kind: DiffLineKind) -> &'static str {
    match kind {
        DiffLineKind::Equal => "equal",
        DiffLineKind::Changed | DiffLineKind::LeftOnly | DiffLineKind::RightOnly => "difference",
    }
}

fn text_summary_items(result: &TextCompareResult) -> Vec<GuiSummaryItem> {
    vec![
        summary_item("Diff blocks", result.summary.diff_blocks),
        summary_item("Changed lines", result.summary.changed_lines),
        summary_item("Left-only lines", result.summary.left_only_lines),
        summary_item("Right-only lines", result.summary.right_only_lines),
    ]
}

fn ensure_row_pair(tab: &mut GuiCompareTab, row: usize) -> Result<(), String> {
    // Cap the row index against the current pane size + a small headroom so a
    // malformed `row=` query cannot push the process into an unbounded
    // allocation loop.
    let current = tab.left_rows.len().max(tab.right_rows.len());
    let limit = current.saturating_add(MAX_ROW_GROWTH);
    if row > limit {
        return Err(format!(
            "row index {row} exceeds the current pane size by more than {MAX_ROW_GROWTH}"
        ));
    }

    while tab.left_rows.len() <= row {
        tab.left_rows.push(blank_gui_row(tab.left_rows.len()));
    }
    while tab.right_rows.len() <= row {
        tab.right_rows.push(blank_gui_row(tab.right_rows.len()));
    }
    Ok(())
}

const MAX_ROW_GROWTH: usize = 1024;

fn blank_gui_row(index: usize) -> GuiLineRow {
    GuiLineRow {
        row_id: format!("blank:{index}"),
        number: None,
        text: String::new(),
        state: "empty".to_owned(),
        block_kind: String::new(),
        folded_count: None,
        syntax_spans: Vec::new(),
        has_find_match: false,
        bookmarked: false,
    }
}

fn first_non_empty(primary: &str, fallback: &str) -> String {
    if primary.is_empty() {
        fallback.to_owned()
    } else {
        primary.to_owned()
    }
}

fn normalize_tab_row(tab: &mut GuiCompareTab, row: usize) {
    if tab.left_rows[row].text == tab.right_rows[row].text {
        tab.left_rows[row].state = "equal".to_owned();
        tab.right_rows[row].state = "equal".to_owned();
    }
}

fn tab_difference_count(tab: &GuiCompareTab) -> usize {
    let rows = tab.left_rows.len().max(tab.right_rows.len());
    (0..rows)
        .filter(|index| {
            let left = tab.left_rows.get(*index).map(|row| row.state.as_str());
            let right = tab.right_rows.get(*index).map(|row| row.state.as_str());
            left.is_some_and(is_gui_difference_state) || right.is_some_and(is_gui_difference_state)
        })
        .count()
}

fn is_gui_difference_state(state: &str) -> bool {
    matches!(
        state,
        "changed" | "left_only" | "right_only" | "error" | "aborted"
    )
}

fn unique_recent_paths(paths: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut recent_paths = Vec::new();
    for path in paths {
        if !path.is_empty() && !recent_paths.iter().any(|candidate| candidate == &path) {
            recent_paths.push(path);
        }
    }
    recent_paths
}

fn write_launch_context(paths: &AppPaths, context: &GuiLaunchContext) -> Result<PathBuf, String> {
    record_recent_context(paths, context);

    let context_dir = paths.cache_dir.join("gui");
    fs::create_dir_all(&context_dir).map_err(|err| {
        format!(
            "failed to create GUI context directory '{}': {err}",
            context_dir.display()
        )
    })?;
    let context_file = context_dir.join(format!("launch-{}.json", process::id()));
    let data = serde_json::to_vec_pretty(&context)
        .map_err(|err| format!("failed to serialize GUI context: {err}"))?;
    write_owner_only(&context_file, &data).map_err(|err| {
        format!(
            "failed to write GUI context '{}': {err}",
            context_file.display()
        )
    })?;
    Ok(context_file)
}

/// Write the bridge-info JSON (which embeds the loopback bridge's auth token)
/// to a per-user-private sidecar file and return its path.
///
/// The token must never be readable by other local users, so this:
/// 1. ensures the parent dir is a real (non-symlink) directory owned by the
///    current user and locked to 0700, refusing to write if another user
///    controls it (e.g. an attacker who pre-created `/tmp/linsync`);
/// 2. writes the file 0600 with `O_NOFOLLOW` so a symlink planted at the path
///    is not followed.
///
/// Returns `None` (after logging) if the location cannot be secured, so the
/// token is never dropped into an attacker-controlled location.
///
/// Uses a hardcoded `/tmp/linsync` path (not `std::env::temp_dir()`) to agree
/// with the QML sidecar reader in `Main.qml` which reads from a hardcoded
/// `file:///tmp/linsync/bridge-info.json`. If `std::env::temp_dir()` were used,
/// the paths would desync when `$TMPDIR` is set (CI, containers).
#[cfg(unix)]
fn write_bridge_info_file(payload: &[u8]) -> Option<PathBuf> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

    let dir = Path::new("/tmp").join("linsync");
    if let Err(err) = fs::create_dir_all(&dir) {
        tracing::warn!(dir = %dir.display(), error = %err, "cannot create bridge info dir");
        return None;
    }
    // The directory must be a real directory owned by us, so no other local
    // user can read our token or plant a fake bridge sidecar.
    let meta = match fs::symlink_metadata(&dir) {
        Ok(m) => m,
        Err(err) => {
            tracing::warn!(dir = %dir.display(), error = %err, "cannot stat bridge info dir");
            return None;
        }
    };
    let euid = unsafe { libc::geteuid() };
    if !meta.is_dir() || meta.uid() != euid {
        tracing::warn!(
            dir = %dir.display(),
            owner = meta.uid(),
            euid,
            "bridge info dir is not a directory owned by the current user; refusing to write token"
        );
        return None;
    }
    // Lock the directory to owner-only and confirm no group/other access.
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    if let Ok(m2) = fs::symlink_metadata(&dir)
        && m2.mode() & 0o077 != 0
    {
        tracing::warn!(
            dir = %dir.display(),
            mode = format!("{:o}", m2.mode()),
            "bridge info dir is group/other accessible; refusing to write token"
        );
        return None;
    }

    let path = dir.join("bridge-info.json");
    // O_NOFOLLOW: never write through a symlink planted at the file path.
    let mut options = fs::OpenOptions::new();
    options
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW);
    let mut file = match options.open(&path) {
        Ok(f) => f,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "cannot open bridge info file");
            return None;
        }
    };
    let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    if let Err(err) = file.write_all(payload) {
        tracing::warn!(path = %path.display(), error = %err, "cannot write bridge info file");
        return None;
    }
    if let Err(err) = file.flush() {
        tracing::warn!(path = %path.display(), error = %err, "cannot flush bridge info file");
        return None;
    }
    Some(path)
}

#[cfg(not(unix))]
fn write_bridge_info_file(payload: &[u8]) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("linsync");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("bridge-info.json");
    match fs::write(&path, payload) {
        Ok(()) => Some(path),
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "cannot write bridge info file");
            None
        }
    }
}

fn write_owner_only(path: &Path, data: &[u8]) -> std::io::Result<()> {
    // The launch context records every absolute path the user has open. It
    // lives under $XDG_CACHE_HOME, which is usually 0o755, so use 0o600 on the
    // file itself to keep other local users from reading the recent-paths list.
    use std::fs::OpenOptions;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(data)?;
    file.flush()
}

/// Serialize a launch context to a JSON value, windowing any large text tab for
/// the wire so the GUI never receives every row at once. The canonical
/// server-side context stays full (it backs merge-copy/undo/export); only clone
/// when something actually needs windowing so the common small-diff path
/// serializes the borrow directly with no extra allocation.
fn context_to_value(context: &GuiLaunchContext) -> Result<serde_json::Value, serde_json::Error> {
    if context.session.tabs.iter().any(tab_needs_windowing) {
        let mut windowed = context.clone();
        for tab in &mut windowed.session.tabs {
            apply_text_windowing(tab);
            apply_folder_windowing(tab);
            apply_binary_windowing(tab);
            apply_table_windowing(tab);
        }
        serde_json::to_value(&windowed)
    } else {
        serde_json::to_value(context)
    }
}

fn context_to_json(context: &GuiLaunchContext) -> Result<String, String> {
    let mut value = context_to_value(context)
        .map_err(|err| format!("failed to serialize GUI context: {err}"))?;
    insert_response_schema_version(&mut value);
    serde_json::to_string(&value).map_err(|err| format!("failed to serialize GUI context: {err}"))
}

fn insert_response_schema_version(value: &mut serde_json::Value) {
    if let Some(object) = value.as_object_mut() {
        object
            .entry("schema_version".to_owned())
            .or_insert_with(|| serde_json::json!(RESPONSE_SCHEMA_VERSION));
    }
}

fn attach_session_to_response_body(
    body: String,
    tab: Option<GuiCompareTab>,
    new_tab: bool,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> String {
    let Some(tab) = tab else {
        return body;
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&body) else {
        return body;
    };
    let Some(object) = value.as_object_mut() else {
        return body;
    };
    if object.get("error").is_some() {
        return body;
    }
    let context = match state.lock() {
        Ok(mut state) => state.apply_compare(tab, new_tab),
        Err(_) => return body,
    };
    record_recent_context(paths, &context);
    if let Ok(context_value) = context_to_value(&context)
        && let Some(context_object) = context_value.as_object()
    {
        for (key, value) in context_object {
            object.insert(key.clone(), value.clone());
        }
    }
    serde_json::to_string(&value).unwrap_or(body)
}

fn recent_limit(paths: &AppPaths) -> usize {
    SettingsStore::new(paths.settings_file())
        .load_or_default()
        .map(|settings| settings.recent_limit)
        .unwrap_or_else(|err| {
            tracing::warn!(error = %err, "failed to load recent item limit");
            20
        })
}

fn load_gui_settings(paths: &AppPaths) -> Result<GuiSettings, String> {
    SettingsStore::new(paths.settings_file())
        .load_or_default()
        .map(|settings| GuiSettings::from(&settings))
        .map_err(|err| format!("failed to load settings: {err}"))
}

fn load_gui_settings_json(paths: &AppPaths) -> Result<String, String> {
    serde_json::to_string(&load_gui_settings(paths)?)
        .map_err(|err| format!("failed to serialize settings: {err}"))
}

fn save_gui_setting(paths: &AppPaths, key: &str, value: &str) -> Result<GuiSettings, String> {
    let store = SettingsStore::new(paths.settings_file());
    let mut settings = store
        .load_or_default()
        .map_err(|err| format!("failed to load settings: {err}"))?;
    apply_gui_setting(&mut settings, key, value)?;
    store
        .save(&settings)
        .map_err(|err| format!("failed to save settings: {err}"))?;
    Ok(GuiSettings::from(&settings))
}

fn save_gui_setting_json(paths: &AppPaths, key: &str, value: &str) -> Result<String, String> {
    serde_json::to_string(&save_gui_setting(paths, key, value)?)
        .map_err(|err| format!("failed to serialize settings: {err}"))
}

fn reset_gui_settings(paths: &AppPaths) -> Result<GuiSettings, String> {
    let store = SettingsStore::new(paths.settings_file());
    store
        .reset_to_default()
        .map(|settings| GuiSettings::from(&settings))
        .map_err(|err| format!("failed to reset settings: {err}"))
}

fn reset_gui_settings_json(paths: &AppPaths) -> Result<String, String> {
    serde_json::to_string(&reset_gui_settings(paths)?)
        .map_err(|err| format!("failed to serialize settings: {err}"))
}

fn record_recent_context(paths: &AppPaths, context: &GuiLaunchContext) {
    // Privacy control: when the user has turned off persisting recent paths,
    // remember nothing about what was compared — neither the recent-paths list
    // nor the recent-session history (both store the compared paths).
    if !recent_persistence_enabled(paths) {
        return;
    }
    record_recent_paths(paths, context);
    record_recent_session(paths, context);
}

/// Whether the user permits persisting comparison history (recent paths and
/// sessions). Defaults to enabled when the setting cannot be read.
fn recent_persistence_enabled(paths: &AppPaths) -> bool {
    SettingsStore::new(paths.settings_file())
        .load_or_default()
        .map(|settings| settings.persist_recent_paths)
        .unwrap_or(true)
}

fn record_recent_paths(paths: &AppPaths, context: &GuiLaunchContext) {
    let Some(tab) = context.active_tab() else {
        return;
    };
    if !tab_has_persistable_paths(tab) {
        return;
    }

    let store = RecentPathStore::new(paths.recent_paths_file(), recent_limit(paths));
    for path in [&tab.left_path, &tab.right_path] {
        if !path.is_empty()
            && let Err(err) = store.add(PathBuf::from(path))
        {
            tracing::warn!(path, error = %err, "failed to record recent GUI path");
        }
    }
}

fn record_recent_session(paths: &AppPaths, context: &GuiLaunchContext) {
    let Some(tab) = context.active_tab() else {
        return;
    };
    if !tab_has_persistable_paths(tab) {
        return;
    }

    let mut session = session_file_from_tab(tab);
    persist_multi_tab_snapshot(&mut session, context);

    if let Err(err) =
        RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths)).add(session)
    {
        tracing::warn!(error = %err, "failed to record recent GUI session");
    }
}

/// Build a persisted [`SessionFile`] from a single GUI tab (paths, view mode,
/// and — for Image/Document/Webpage — the per-tab view snapshot).
fn session_file_from_tab(tab: &GuiCompareTab) -> SessionFile {
    let mut session = SessionFile::new(CompareSession {
        title: tab.title.clone(),
        left: PathBuf::from(&tab.left_path),
        base: None,
        right: PathBuf::from(&tab.right_path),
        options: CompareOptions::default(),
    });
    session.selected_view = compare_view_mode(&tab.mode);
    session.last_result = Some(linsync_core::SessionResultSummary {
        equal: tab.difference_count == 0,
        difference_count: tab.difference_count,
    });
    persist_tab_snapshot(&mut session, tab);
    session
}

/// Embed a snapshot of *every* persistable open tab into the session's
/// forward-compat `layout.extra` map, so the next launch can restore the whole
/// workspace rather than only the active tab. Only stores the snapshot when
/// more than one persistable tab is open (a single tab already round-trips
/// through the `session` + `selected_view_state` fields).
fn persist_multi_tab_snapshot(session: &mut SessionFile, context: &GuiLaunchContext) {
    let tabs: Vec<GuiCompareTab> = context
        .session
        .tabs
        .iter()
        .filter(|tab| tab_has_persistable_paths(tab))
        .cloned()
        .collect();
    if tabs.len() < 2 {
        return;
    }
    let snapshot = GuiMultiTabSnapshot {
        schema_version: GUI_TAB_SNAPSHOT_SCHEMA_VERSION,
        active_tab_id: context.session.active_tab_id,
        tabs,
    };
    match serde_json::to_value(&snapshot) {
        Ok(value) => {
            session
                .layout
                .extra
                .insert(GUI_TABS_SNAPSHOT_KEY.to_owned(), value);
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to serialize multi-tab snapshot");
        }
    }
}

/// Rebuild a multi-tab launch context from a recent session's snapshot, if it
/// carries one. Used by explicit `/sessions/reopen` so a saved multi-tab
/// workspace comes back whole. (Bare-launch auto-restore was intentionally
/// removed — prior work is only ever resumed explicitly.)
fn restore_multi_tab_context(session: &SessionFile) -> Option<GuiLaunchContext> {
    let value = session.layout.extra.get(GUI_TABS_SNAPSHOT_KEY)?;
    let snapshot: GuiMultiTabSnapshot = serde_json::from_value(value.clone()).ok()?;
    if snapshot.schema_version != GUI_TAB_SNAPSHOT_SCHEMA_VERSION || snapshot.tabs.len() < 2 {
        return None;
    }
    Some(GuiLaunchContext::from_tabs(
        snapshot.tabs,
        snapshot.active_tab_id,
    ))
}

/// Heuristic: never treat paths under the source tree's tests/fixtures/ as
/// persistable "recent" entries (for recording or for display in the Sessions
/// page / reopen). These fixtures are used by gui-smoke.sh, release-smoke,
/// unit tests, and manual `cargo run -p linsync -- <fixture>` invocations
/// during development. We also never auto-restore *any* previous session's
/// paths into the main Compare page's Left/Right input fields on a bare launch
/// (no CLI args). Pre-filling those fields from "last session" (even real
/// user data or /tmp/ dev folders like bigfolder) produced a horrible
/// experience of "defaults" the user didn't choose. The Sessions page and
/// explicit re-open / project open are the way to resume prior work.
fn path_looks_like_internal_test_fixture(p: &Path) -> bool {
    let s = p.to_string_lossy().to_ascii_lowercase();
    // Only match *this project's* fixtures (a path component containing
    // "linsync" somewhere above tests/fixtures), not any project's
    // tests/fixtures tree — developers legitimately diff their own golden
    // files (e.g. /home/dev/myapp/tests/fixtures/{expected,actual}) and those
    // must record/persist like any other compare. Covers /tests/fixtures/
    // (unix), \tests\fixtures\ (windows), and trailing cases.
    if !s.contains("linsync") {
        return false;
    }
    s.contains("/tests/fixtures/")
        || s.contains("\\tests\\fixtures\\")
        || s.ends_with("/tests/fixtures")
        || s.ends_with("\\tests\\fixtures")
}

/// Drop recent-session entries that point at internal test fixtures (leftover
/// pollution from dev / smoke runs). Every endpoint that loads the recent store
/// MUST apply this before indexing into `sessions`: the Sessions page receives
/// indices into the pruned list, so an unpruned endpoint would address the
/// wrong entry whenever a hidden fixture entry exists on disk.
fn prune_internal_fixture_sessions(recent: &mut RecentSessions) {
    recent.sessions.retain(|s| {
        !path_looks_like_internal_test_fixture(&s.session.left)
            && !path_looks_like_internal_test_fixture(&s.session.right)
    });
}

fn tab_has_persistable_paths(tab: &GuiCompareTab) -> bool {
    if !tab.validation.compatible || tab.left_path.is_empty() || tab.right_path.is_empty() {
        return false;
    }
    if path_looks_like_internal_test_fixture(Path::new(&tab.left_path))
        || path_looks_like_internal_test_fixture(Path::new(&tab.right_path))
    {
        return false;
    }
    true
}

fn persist_tab_snapshot(session: &mut SessionFile, tab: &GuiCompareTab) {
    if !matches!(tab.mode.as_str(), "Image" | "Document" | "Webpage") {
        return;
    }
    let snapshot = GuiTabSnapshot {
        schema_version: GUI_TAB_SNAPSHOT_SCHEMA_VERSION,
        tab: tab.clone(),
    };
    match serde_json::to_string(&snapshot) {
        Ok(raw) => session.layout.selected_view_state = Some(raw),
        Err(err) => {
            tracing::warn!(mode = tab.mode, error = %err, "failed to serialize GUI tab snapshot");
        }
    }
}

fn restore_tab_snapshot(session: &SessionFile) -> Option<GuiCompareTab> {
    let raw = session.layout.selected_view_state.as_ref()?;
    let snapshot: GuiTabSnapshot = serde_json::from_str(raw).ok()?;
    if snapshot.schema_version != GUI_TAB_SNAPSHOT_SCHEMA_VERSION {
        return None;
    }
    let tab = snapshot.tab;
    if compare_view_mode(&tab.mode) != session.selected_view {
        return None;
    }
    if session.session.left.as_os_str() != std::ffi::OsStr::new(&tab.left_path)
        || session.session.right.as_os_str() != std::ffi::OsStr::new(&tab.right_path)
    {
        return None;
    }
    tab_has_persistable_paths(&tab).then_some(tab)
}

/// Rebuild a compare tab for a saved session. Non-text options come from
/// `base` (callers resolve the active profile); the session file's own saved
/// text options overlay it so a reopened session reproduces the text compare
/// it was saved with.
fn build_tab_for_session_file(session: &SessionFile, base: &GuiCompareOptions) -> GuiCompareTab {
    let mut options = base.clone();
    options.text = session.session.options.text.clone();
    restore_tab_snapshot(session).unwrap_or_else(|| {
        let mode = Some(compare_view_mode_label(session.selected_view));
        build_tab_for_paths_with_mode(
            &session.session.left,
            &session.session.right,
            mode,
            &options,
        )
    })
}

fn compare_view_mode(mode: &str) -> CompareViewMode {
    match mode {
        "Folder" => CompareViewMode::Folder,
        "Hex" => CompareViewMode::Binary,
        "Table" => CompareViewMode::Table,
        "Image" => CompareViewMode::Image,
        "Document" => CompareViewMode::Document,
        "Webpage" => CompareViewMode::Webpage,
        _ => CompareViewMode::Text,
    }
}

fn compare_view_mode_label(mode: CompareViewMode) -> &'static str {
    match mode {
        CompareViewMode::Folder => "Folder",
        CompareViewMode::Binary => "Hex",
        CompareViewMode::Table => "Table",
        CompareViewMode::Image => "Image",
        CompareViewMode::Document => "Document",
        CompareViewMode::Archive => "Archive",
        CompareViewMode::Webpage => "Webpage",
        CompareViewMode::Text => "Text",
    }
}

fn build_launch_context(args: &[OsString]) -> Option<GuiLaunchContext> {
    let paths = positional_paths(args)?;
    Some(build_context_for_paths(&paths[0], &paths[1]))
}

/// Read a Git-mergetool launch from the `LINSYNC_MERGE_*` environment, set by
/// `linsync-cli mergetool` when it launches the GUI for interactive resolution.
/// All four paths must be present; otherwise this returns `None`.
fn merge_launch_from_env() -> Option<GuiMergeLaunch> {
    let read = |key: &str| env::var(key).ok().filter(|v| !v.is_empty());
    Some(GuiMergeLaunch {
        base: read("LINSYNC_MERGE_BASE")?,
        left: read("LINSYNC_MERGE_LOCAL")?,
        right: read("LINSYNC_MERGE_REMOTE")?,
        output: read("LINSYNC_MERGE_MERGED")?,
    })
}

fn build_context_for_paths(left: &Path, right: &Path) -> GuiLaunchContext {
    GuiLaunchContext::single_tab(build_tab_for_paths(left, right))
}

fn build_tab_for_paths(left: &Path, right: &Path) -> GuiCompareTab {
    build_tab_for_paths_with_mode(left, right, None, &GuiCompareOptions::default())
}

fn build_tab_for_paths_with_mode(
    left: &Path,
    right: &Path,
    mode: Option<&str>,
    options: &GuiCompareOptions,
) -> GuiCompareTab {
    build_tab_for_paths_with_mode_cancellable(left, right, mode, options, &|| false, None)
        .expect("a non-cancelling build always yields a tab")
}

/// Cancellable variant of [`build_tab_for_paths_with_mode`]. `should_cancel` is
/// polled during the long folder/text compares; when it reports `true` the
/// compare aborts and this returns `None` (the bridge then responds with
/// `{"cancelled":true}` without mutating the session). Fast modes (table, hex,
/// validation errors) are unaffected.
fn build_tab_for_paths_with_mode_cancellable(
    left: &Path,
    right: &Path,
    mode: Option<&str>,
    options: &GuiCompareOptions,
    should_cancel: &dyn Fn() -> bool,
    progress: Option<Arc<Mutex<CompareProgress>>>,
) -> Option<GuiCompareTab> {
    build_tab_for_paths_with_mode_cancellable_and_artifacts(
        left,
        right,
        mode,
        options,
        should_cancel,
        progress,
    )
    .map(|(tab, _)| tab)
}

fn build_tab_for_paths_with_mode_cancellable_and_artifacts(
    left: &Path,
    right: &Path,
    mode: Option<&str>,
    options: &GuiCompareOptions,
    should_cancel: &dyn Fn() -> bool,
    progress: Option<Arc<Mutex<CompareProgress>>>,
) -> Option<(GuiCompareTab, Vec<PathBuf>)> {
    if should_cancel() {
        return None;
    }
    let left_path = left.display().to_string();
    let right_path = right.display().to_string();

    if let Some(mode) = mode.map(str::trim).filter(|mode| !mode.is_empty()) {
        return match GuiCompareMode::from_label(mode) {
            Some(mode) => {
                let (tab, dirs) = explicit_tab_for_paths_cancellable(
                    mode,
                    left,
                    right,
                    left_path,
                    right_path,
                    options,
                    should_cancel,
                    progress,
                );
                tab.map(|tab| (tab, dirs))
            }
            None => Some((
                invalid_compare_tab(
                    "Text",
                    left_path,
                    right_path,
                    format!("Unsupported compare mode '{mode}'"),
                ),
                Vec::new(),
            )),
        };
    }

    match classify_context_paths(left, right) {
        Ok(ContextPathKind::Folders) => folder_tab_cancellable(
            left,
            right,
            left_path,
            right_path,
            options,
            should_cancel,
            progress,
        )
        .map(|tab| (tab, Vec::new())),
        Ok(ContextPathKind::Files) => file_tab_cancellable(
            left,
            right,
            left_path,
            right_path,
            options,
            should_cancel,
            progress,
        )
        .map(|tab| (tab, Vec::new())),
        Err(status) => Some((
            invalid_compare_tab("Text", left_path, right_path, status),
            Vec::new(),
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuiCompareMode {
    Text,
    Folder,
    Table,
    Hex,
    Image,
    Document,
    Webpage,
    Archive,
}

impl GuiCompareMode {
    fn from_label(label: &str) -> Option<Self> {
        match label {
            "Text" | "text" => Some(Self::Text),
            "Folder" | "folder" => Some(Self::Folder),
            "Table" | "table" => Some(Self::Table),
            "Hex" | "hex" | "Binary" | "binary" => Some(Self::Hex),
            "Image" | "image" => Some(Self::Image),
            "Document" | "document" => Some(Self::Document),
            "Webpage" | "webpage" => Some(Self::Webpage),
            "Archive" | "archive" => Some(Self::Archive),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Folder => "Folder",
            Self::Table => "Table",
            Self::Hex => "Hex",
            Self::Image => "Image",
            Self::Document => "Document",
            Self::Webpage => "Webpage",
            Self::Archive => "Archive",
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn explicit_tab_for_paths_cancellable(
    mode: GuiCompareMode,
    left: &Path,
    right: &Path,
    left_path: String,
    right_path: String,
    options: &GuiCompareOptions,
    should_cancel: &dyn Fn() -> bool,
    progress: Option<Arc<Mutex<CompareProgress>>>,
) -> (Option<GuiCompareTab>, Vec<PathBuf>) {
    match classify_context_paths(left, right) {
        Ok(ContextPathKind::Folders) if mode == GuiCompareMode::Folder => (
            folder_tab_cancellable(
                left,
                right,
                left_path,
                right_path,
                options,
                should_cancel,
                progress,
            ),
            Vec::new(),
        ),
        Ok(ContextPathKind::Files) => match mode {
            GuiCompareMode::Text => (
                text_tab_cancellable(
                    left,
                    right,
                    left_path,
                    right_path,
                    options,
                    should_cancel,
                    progress,
                ),
                Vec::new(),
            ),
            GuiCompareMode::Table => (
                Some(table_tab(left, right, left_path, right_path, options)),
                Vec::new(),
            ),
            GuiCompareMode::Hex => (
                Some(binary_tab(left, right, left_path, right_path, options)),
                Vec::new(),
            ),
            GuiCompareMode::Folder => (
                Some(invalid_compare_tab(
                    mode.label(),
                    left_path,
                    right_path,
                    "Selected folder compare requires two folders".to_owned(),
                )),
                Vec::new(),
            ),
            GuiCompareMode::Image => (
                Some(image_tab(left, right, left_path, right_path, options)),
                Vec::new(),
            ),
            GuiCompareMode::Document => document_tab(
                left,
                right,
                left_path,
                right_path,
                options,
                should_cancel,
                progress,
            ),
            GuiCompareMode::Webpage => (
                Some(invalid_compare_tab(
                    mode.label(),
                    left_path,
                    right_path,
                    "Webpage compare uses the dedicated Webpage Compare page".to_owned(),
                )),
                Vec::new(),
            ),
            GuiCompareMode::Archive => {
                if linsync_core::is_builtin_archive_format(left)
                    && linsync_core::is_builtin_archive_format(right)
                {
                    builtin_archive_tab(
                        left,
                        right,
                        left_path,
                        right_path,
                        options,
                        &AppPaths::from_env(),
                    )
                } else {
                    (
                        Some(invalid_compare_tab(
                            mode.label(),
                            left_path,
                            right_path,
                            "Built-in archive compare requires two supported archives".to_owned(),
                        )),
                        Vec::new(),
                    )
                }
            }
        },
        Ok(ContextPathKind::Folders) => (
            Some(invalid_compare_tab(
                mode.label(),
                left_path,
                right_path,
                format!("Selected {} compare requires two files", mode.label()),
            )),
            Vec::new(),
        ),
        Err(status) => (
            Some(invalid_compare_tab(
                mode.label(),
                left_path,
                right_path,
                status,
            )),
            Vec::new(),
        ),
    }
}

fn invalid_compare_tab(
    mode: &str,
    left_path: String,
    right_path: String,
    status: String,
) -> GuiCompareTab {
    compare_tab(
        mode,
        (left_path, right_path),
        status.clone(),
        0,
        GuiOpenValidation {
            compatible: false,
            path_kind: "Invalid".to_owned(),
            message: status,
        },
        Vec::new(),
        (Vec::new(), Vec::new()),
        Vec::new(),
        None,
        None,
        Vec::new(),
        None,
    )
}

fn positional_paths(args: &[OsString]) -> Option<[PathBuf; 2]> {
    let mut values = args.iter();
    let first = values.next()?;
    let left = if first == "--" { values.next()? } else { first };
    let right = values.next()?;
    values.next().is_none().then(|| {
        [
            PathBuf::from(left.as_os_str()),
            PathBuf::from(right.as_os_str()),
        ]
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextPathKind {
    Files,
    Folders,
}

fn classify_context_paths(left: &Path, right: &Path) -> Result<ContextPathKind, String> {
    let left_metadata = fs::metadata(left)
        .map_err(|err| format!("Cannot access left path '{}': {err}", left.display()))?;
    let right_metadata = fs::metadata(right)
        .map_err(|err| format!("Cannot access right path '{}': {err}", right.display()))?;

    match (left_metadata.is_dir(), right_metadata.is_dir()) {
        (true, true) => Ok(ContextPathKind::Folders),
        (false, false) => Ok(ContextPathKind::Files),
        _ => Err("Select two files or two folders".to_owned()),
    }
}

fn folder_tab_cancellable(
    left: &Path,
    right: &Path,
    left_path: String,
    right_path: String,
    options: &GuiCompareOptions,
    should_cancel: &dyn Fn() -> bool,
    progress: Option<Arc<Mutex<CompareProgress>>>,
) -> Option<GuiCompareTab> {
    let folder_options = &options.folder;
    let mut discovered_total: usize = 0;
    let mut compared_count: usize = 0;
    let result = compare_folders_with_progress(left, right, folder_options, |event| {
        match &event {
            FolderCompareEvent::Discovered { .. } => {
                discovered_total += 1;
                if let Some(p) = &progress
                    && let Ok(mut p) = p.lock()
                {
                    p.phase = "walking".to_owned();
                    p.total = discovered_total;
                    p.message = format!("Discovered {discovered_total} entries…");
                }
            }
            FolderCompareEvent::Compared { relative_path, .. }
            | FolderCompareEvent::Skipped { relative_path, .. }
            | FolderCompareEvent::Error { relative_path, .. } => {
                compared_count += 1;
                if let Some(p) = &progress
                    && let Ok(mut p) = p.lock()
                {
                    p.phase = "comparing".to_owned();
                    p.current = compared_count;
                    p.total = discovered_total.max(compared_count);
                    p.message = relative_path.display().to_string();
                }
            }
            FolderCompareEvent::Completed { .. } | FolderCompareEvent::Cancelled { .. } => {
                if let Some(p) = &progress
                    && let Ok(mut p) = p.lock()
                {
                    p.phase = "done".to_owned();
                    p.current = compared_count;
                    p.total = discovered_total.max(compared_count);
                    p.message.clear();
                }
            }
        }
        if should_cancel() {
            FolderCompareControl::Cancel
        } else {
            FolderCompareControl::Continue
        }
    });
    // If the user cancelled, abort rather than surfacing a partial/aborted tab.
    if should_cancel() {
        return None;
    }
    Some(match result {
        Ok(result) => {
            let difference_count = result.summary.different_count
                + result.summary.one_sided_count
                + result.summary.errors_count
                + result.summary.aborted_count;
            let folder_entries = folder_entries_for_gui(&result.entries);
            compare_tab(
                "Folder",
                (left_path, right_path),
                "Folder compare complete".to_owned(),
                difference_count,
                GuiOpenValidation {
                    compatible: true,
                    path_kind: "Folders".to_owned(),
                    message: "Validated two folders".to_owned(),
                },
                vec![
                    summary_item("Compared", result.summary.compared_count),
                    summary_item("Identical", result.summary.identical_count),
                    summary_item("Different", result.summary.different_count),
                    summary_item("One-sided", result.summary.one_sided_count),
                    summary_item("Skipped", result.summary.skipped_count),
                    summary_item("Errors", result.summary.errors_count),
                ],
                (Vec::new(), Vec::new()),
                folder_entries,
                None,
                None,
                Vec::new(),
                Some(options.clone()),
            )
        }
        Err(err) => compare_tab(
            "Folder",
            (left_path, right_path),
            format!("Folder compare failed: {err}"),
            0,
            GuiOpenValidation {
                compatible: true,
                path_kind: "Folders".to_owned(),
                message: "Validated two folders; compare failed".to_owned(),
            },
            Vec::new(),
            (Vec::new(), Vec::new()),
            vec![],
            None,
            None,
            Vec::new(),
            Some(options.clone()),
        ),
    })
}

/// The active profile's per-plugin enable/disable overrides, or an empty map
/// when no user profile is selected (a built-in or no active profile has no
/// overrides). Threaded into plugin resolution so a profile that disables a
/// plugin is honored in the GUI exactly as in the CLI.
fn active_profile_plugin_overrides(paths: &AppPaths) -> std::collections::BTreeMap<String, bool> {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    match store.load_active_pointer() {
        Ok(Some(id)) => store
            .load(&id)
            .map(|profile| profile.plugin_enablement)
            .unwrap_or_default(),
        _ => std::collections::BTreeMap::new(),
    }
}

/// If `left` and `right` are two files of the same archive extension for which
/// an *enabled* unpacker/virtualizer plugin is installed, return that plugin.
/// This is what lets the GUI auto-route an archive pair to a folder-style diff.
fn archive_pair_unpacker(
    left: &Path,
    right: &Path,
    paths: &AppPaths,
) -> Option<linsync_core::DiscoveredPlugin> {
    if !left.is_file() || !right.is_file() {
        return None;
    }
    let ext = left
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    let right_ext = right
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if right_ext.as_deref() != Some(ext.as_str()) {
        return None;
    }
    let overrides = active_profile_plugin_overrides(paths);
    linsync_core::resolve_enabled_virtualizer_for_extension(paths, &ext, Some(&overrides))
}

/// Compare two archives via `plugin` (unpack + nested recursion one level) and
/// present the result through the folder view (tab mode "Folder"), titled as an
/// archive compare. Nested-archive members appear as `"<member>!/…"` entries.
fn archive_tab(
    left_path: String,
    right_path: String,
    plugin: &linsync_core::DiscoveredPlugin,
    options: &GuiCompareOptions,
) -> GuiCompareTab {
    let exec = PluginExecutionOptions {
        timeout: std::time::Duration::from_secs(60),
        ..PluginExecutionOptions::default()
    };
    match linsync_core::compare_archives_with_unpacker_recursive(
        &plugin.root,
        &plugin.manifest,
        &left_path,
        &right_path,
        1,
        &exec,
    ) {
        Ok(result) => {
            let difference_count = result.summary.different_count
                + result.summary.one_sided_count
                + result.summary.errors_count
                + result.summary.aborted_count;
            let folder_entries = folder_entries_for_gui(&result.entries);
            compare_tab(
                "Folder",
                (left_path, right_path),
                "Archive compare complete".to_owned(),
                difference_count,
                GuiOpenValidation {
                    compatible: true,
                    path_kind: "Archives".to_owned(),
                    message: "Compared two archives as folders".to_owned(),
                },
                vec![
                    summary_item("Compared", result.summary.compared_count),
                    summary_item("Identical", result.summary.identical_count),
                    summary_item("Different", result.summary.different_count),
                    summary_item("One-sided", result.summary.one_sided_count),
                ],
                (Vec::new(), Vec::new()),
                folder_entries,
                None,
                None,
                Vec::new(),
                Some(options.clone()),
            )
        }
        Err(err) => invalid_compare_tab(
            "Text",
            left_path,
            right_path,
            format!("Archive compare failed: {err}"),
        ),
    }
}

/// Unique per-request extraction cache directory under the app cache.
fn archive_extract_cache_dir(paths: &AppPaths) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let id = format!(
        "{}-{}-{}",
        std::process::id(),
        now,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    paths.cache_dir.join("archive-extract").join(id)
}

/// Compare two built-in-format archives by extracting them to a persistent
/// cache directory and presenting the result through the folder view. Returns
/// the tab plus the cache directory so the bridge can clean it up on tab close.
fn builtin_archive_tab(
    left: &Path,
    right: &Path,
    left_path: String,
    right_path: String,
    options: &GuiCompareOptions,
    paths: &AppPaths,
) -> (Option<GuiCompareTab>, Vec<PathBuf>) {
    let cache_dir = archive_extract_cache_dir(paths);
    let left_extract = cache_dir.join("left");
    let right_extract = cache_dir.join("right");

    let cleanup = || {
        let _ = fs::remove_dir_all(&cache_dir);
    };

    match linsync_core::compare_builtin_archives_with_dirs(
        left,
        right,
        &left_extract,
        &right_extract,
        &options.folder,
    ) {
        Ok(result) => {
            let difference_count = result.summary.different_count
                + result.summary.one_sided_count
                + result.summary.errors_count
                + result.summary.aborted_count;
            let folder_entries = folder_entries_for_gui(&result.entries);
            let tab = compare_tab(
                "Folder",
                (left_path, right_path),
                "Archive compare complete".to_owned(),
                difference_count,
                GuiOpenValidation {
                    compatible: true,
                    path_kind: "Archives".to_owned(),
                    message: "Compared two archives as folders".to_owned(),
                },
                vec![
                    summary_item("Compared", result.summary.compared_count),
                    summary_item("Identical", result.summary.identical_count),
                    summary_item("Different", result.summary.different_count),
                    summary_item("One-sided", result.summary.one_sided_count),
                ],
                (Vec::new(), Vec::new()),
                folder_entries,
                None,
                None,
                Vec::new(),
                Some(options.clone()),
            );
            (Some(tab), vec![cache_dir])
        }
        Err(err) => {
            cleanup();
            (
                Some(invalid_compare_tab(
                    "Text",
                    left_path,
                    right_path,
                    format!("Archive compare failed: {err}"),
                )),
                Vec::new(),
            )
        }
    }
}

fn folder_entries_for_gui(entries: &[FolderEntryDiff]) -> Vec<GuiFolderEntry> {
    entries
        .iter()
        .map(|entry| {
            let method_label = entry
                .effective_method
                .map(|m| format!("{m:?}"))
                .unwrap_or_default();
            GuiFolderEntry {
                path: entry.relative_path.display().to_string(),
                is_dir: entry.is_dir,
                entry_type: entry.entry_type.as_str().to_owned(),
                state: gui_folder_state(entry.state).to_owned(),
                left_size: entry.left_size,
                right_size: entry.right_size,
                left_modified: entry.left_modified.map(|t| format!("{t:?}")),
                right_modified: entry.right_modified.map(|t| format!("{t:?}")),
                method: method_label,
            }
        })
        .collect()
}

fn gui_folder_state(state: FolderEntryState) -> &'static str {
    match state {
        FolderEntryState::Identical => "equal",
        FolderEntryState::Different => "changed",
        FolderEntryState::LeftOnly => "left_only",
        FolderEntryState::RightOnly => "right_only",
        FolderEntryState::Skipped => "skipped",
        FolderEntryState::Error => "error",
        FolderEntryState::Aborted => "aborted",
    }
}

fn file_tab_cancellable(
    left: &Path,
    right: &Path,
    left_path: String,
    right_path: String,
    options: &GuiCompareOptions,
    should_cancel: &dyn Fn() -> bool,
    progress: Option<Arc<Mutex<CompareProgress>>>,
) -> Option<GuiCompareTab> {
    if is_table_path(left) && is_table_path(right) {
        let mut table_opts = options.table.clone();
        if table_opts.delimiter == ',' && is_tsv_path(left) && is_tsv_path(right) {
            table_opts.delimiter = '\t';
        }
        let mut opts = options.clone();
        opts.table = table_opts;
        return Some(table_tab(left, right, left_path, right_path, &opts));
    }

    let left_bytes = fs::read(left).unwrap_or_default();
    let right_bytes = fs::read(right).unwrap_or_default();
    if is_likely_binary(&left_bytes) || is_likely_binary(&right_bytes) {
        return Some(binary_tab(left, right, left_path, right_path, options));
    }

    text_tab_cancellable(
        left,
        right,
        left_path,
        right_path,
        options,
        should_cancel,
        progress,
    )
}

fn text_tab_cancellable(
    left: &Path,
    right: &Path,
    left_path: String,
    right_path: String,
    options: &GuiCompareOptions,
    should_cancel: &dyn Fn() -> bool,
    progress: Option<Arc<Mutex<CompareProgress>>>,
) -> Option<GuiCompareTab> {
    let text_options = &options.text;
    set_progress(&progress, "reading", 0, 0, "Reading text files".to_owned());
    let result = try_prediffer_compare(left, right, text_options).or_else(|| {
        let left_document = match TextDocument::from_path_with_encoding(left, text_options.encoding)
        {
            Ok(document) => document,
            Err(_) => return None,
        };
        let right_document =
            match TextDocument::from_path_with_encoding(right, text_options.encoding) {
                Ok(document) => document,
                Err(_) => return None,
            };
        let total = left_document
            .lines
            .len()
            .max(right_document.lines.len())
            .max(1);
        set_progress(
            &progress,
            "comparing",
            0,
            total,
            "Comparing text rows".to_owned(),
        );
        let ticks = AtomicUsize::new(0);
        let progress_for_compare = progress.clone();
        compare_documents_cancellable(left_document, right_document, text_options, &|| {
            let current = ticks
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1)
                .min(total);
            if current == 1 || current % 32 == 0 || current == total {
                set_progress(
                    &progress_for_compare,
                    "comparing",
                    current,
                    total,
                    format!("Compared {current}/{total} text rows"),
                );
            }
            should_cancel()
        })
    });
    match result {
        Some(result) => {
            set_progress(
                &progress,
                "rendering",
                result.lines.len(),
                result.lines.len().max(1),
                "Building text view".to_owned(),
            );
            let encoding = Some(result.encoding_summary());
            let (left_rows, right_rows) = text_rows_for_gui_with_options(&result, text_options);
            set_progress(
                &progress,
                "done",
                result.lines.len(),
                result.lines.len().max(1),
                String::new(),
            );
            Some(compare_tab(
                "Text",
                (left_path, right_path),
                "Text compare complete".to_owned(),
                result.summary.differences,
                GuiOpenValidation {
                    compatible: true,
                    path_kind: "Files".to_owned(),
                    message: "Validated two files".to_owned(),
                },
                vec![
                    summary_item("Diff blocks", result.summary.diff_blocks),
                    summary_item("Changed lines", result.summary.changed_lines),
                    summary_item("Left-only lines", result.summary.left_only_lines),
                    summary_item("Right-only lines", result.summary.right_only_lines),
                ],
                (left_rows, right_rows),
                vec![],
                encoding,
                None,
                Vec::new(),
                Some(options.clone()),
            ))
        }
        None => None,
    }
}

fn try_prediffer_compare(
    left: &Path,
    right: &Path,
    text_options: &TextCompareOptions,
) -> Option<TextCompareResult> {
    let paths = linsync_core::paths::AppPaths::from_env();
    let discovery = discover_installed_plugins(&paths);
    // Honor plugin enablement: a prediffer disabled globally (plugins.json) or
    // by the active profile's per-plugin override must not auto-apply.
    let global_enabled = linsync_core::load_plugin_enabled_map(&paths);
    let overrides = active_profile_plugin_overrides(&paths);
    let ext = left
        .extension()
        .or_else(|| right.extension())?
        .to_str()?
        .to_lowercase();
    let matched = discovery.plugins.iter().find(|p| {
        p.manifest.classes.contains(&PluginClass::Prediffer)
            && linsync_core::is_plugin_enabled_for_profile(
                &global_enabled,
                &overrides,
                &p.manifest.id,
            )
            && p.manifest
                .extensions
                .iter()
                .any(|e| e.to_lowercase() == ext)
    })?;
    let exec_opts = PluginExecutionOptions::default();
    compare_text_files_with_prediffer(
        left,
        right,
        text_options,
        Some(&matched.root),
        Some(&matched.manifest),
        &exec_opts,
    )
    .ok()
}

fn text_rows_for_gui(
    lines: &[DiffLine],
    blocks: &[linsync_core::DiffBlock],
) -> (Vec<GuiLineRow>, Vec<GuiLineRow>) {
    use linsync_core::MoveDirection;

    // No hard cap — large files show all rows. The QML ListView virtualizes
    // via its delegate model, so memory is O(visible rows) not O(total rows).
    let mut line_block_kinds: Vec<&str> = Vec::with_capacity(lines.len());
    {
        let mut block_iter = blocks.iter();
        let mut current_block = block_iter.next();
        for line in lines.iter() {
            // Advance past blocks that this line is beyond.
            while let Some(blk) = current_block {
                let past_left = blk
                    .left_start
                    .is_some_and(|s| line.left_line.is_some_and(|n| n >= s + blk.left_len))
                    && blk.left_len > 0;
                let past_right = blk
                    .right_start
                    .is_some_and(|s| line.right_line.is_some_and(|n| n >= s + blk.right_len))
                    && blk.right_len > 0;
                let past_equal = matches!(blk.kind, linsync_core::DiffBlockKind::Equal)
                    && past_left
                    && past_right;
                let past_diff = !matches!(blk.kind, linsync_core::DiffBlockKind::Equal)
                    && (past_left || past_right);
                if past_equal || past_diff {
                    current_block = block_iter.next();
                } else {
                    break;
                }
            }
            let kind = match current_block.map(|b| &b.kind) {
                Some(linsync_core::DiffBlockKind::Moved {
                    direction: MoveDirection::LeftToRight,
                    ..
                })
                | Some(linsync_core::DiffBlockKind::Moved {
                    direction: MoveDirection::RightToLeft,
                    ..
                }) => "moved",
                Some(linsync_core::DiffBlockKind::Difference) => "difference",
                _ => "equal",
            };
            line_block_kinds.push(kind);
        }
    }

    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let state = gui_line_state(line.kind);
            let block_kind = line_block_kinds
                .get(index)
                .copied()
                .unwrap_or("equal")
                .to_owned();
            let row_id = format!(
                "text:{}:{}:{}",
                line.left_line.unwrap_or(0),
                line.right_line.unwrap_or(0),
                index
            );
            (
                GuiLineRow {
                    row_id: row_id.clone(),
                    number: line.left_line,
                    text: line.left.clone().unwrap_or_default(),
                    state: state.to_owned(),
                    block_kind: block_kind.clone(),
                    folded_count: None,
                    syntax_spans: Vec::new(),
                    has_find_match: false,
                    bookmarked: false,
                },
                GuiLineRow {
                    row_id,
                    number: line.right_line,
                    text: line.right.clone().unwrap_or_default(),
                    state: state.to_owned(),
                    block_kind,
                    folded_count: None,
                    syntax_spans: Vec::new(),
                    has_find_match: false,
                    bookmarked: false,
                },
            )
        })
        .unzip()
}

fn text_rows_for_gui_with_options(
    result: &TextCompareResult,
    options: &TextCompareOptions,
) -> (Vec<GuiLineRow>, Vec<GuiLineRow>) {
    if options.render_mode != TextRenderMode::SideBySide {
        let rendered = result.render_text(options);
        let rows = rendered
            .lines()
            .enumerate()
            .map(|(index, text)| {
                let state = if text.starts_with('+') {
                    "right_only"
                } else if text.starts_with('-') {
                    "left_only"
                } else if text.starts_with('!') || text.starts_with('~') {
                    "changed"
                } else {
                    "equal"
                };
                let block_kind = if state == "equal" {
                    "equal"
                } else {
                    "difference"
                };
                GuiLineRow {
                    row_id: format!("rendered:{index}"),
                    number: Some(index + 1),
                    text: text.to_owned(),
                    state: state.to_owned(),
                    block_kind: block_kind.to_owned(),
                    folded_count: None,
                    syntax_spans: Vec::new(),
                    has_find_match: false,
                    bookmarked: false,
                }
            })
            .collect::<Vec<_>>();
        let right_rows = rows
            .iter()
            .enumerate()
            .map(|(index, row)| GuiLineRow {
                row_id: format!("rendered-right:{index}"),
                number: row.number,
                text: String::new(),
                state: row.state.clone(),
                block_kind: row.block_kind.clone(),
                folded_count: row.folded_count,
                syntax_spans: Vec::new(),
                has_find_match: false,
                bookmarked: false,
            })
            .collect();
        return (rows, right_rows);
    }

    gui_rows_from_view_rows(result.view_rows(options))
}

/// Map core side-by-side view rows onto the bridge's `GuiLineRow` pairs.
/// Shared by the full `/compare` build and the `/compare/text/window` page
/// path so both produce byte-identical rows.
fn gui_rows_from_view_rows(
    rows: Vec<linsync_core::TextViewRow>,
) -> (Vec<GuiLineRow>, Vec<GuiLineRow>) {
    rows.into_iter()
        .map(|row| {
            let row_id = if row.folded_count.is_some() {
                format!("text-fold:{}", row.index)
            } else {
                format!(
                    "text:{}:{}:{}",
                    row.left_line.unwrap_or(0),
                    row.right_line.unwrap_or(0),
                    row.index
                )
            };
            let has_find_match = !row.find_matches.is_empty();
            let bookmarked = !row.bookmarks.is_empty();
            (
                GuiLineRow {
                    row_id: row_id.clone(),
                    number: row.left_line,
                    text: row.left,
                    state: row.state.clone(),
                    block_kind: row.block_kind.clone(),
                    folded_count: row.folded_count,
                    syntax_spans: row.left_syntax,
                    has_find_match,
                    bookmarked,
                },
                GuiLineRow {
                    row_id,
                    number: row.right_line,
                    text: row.right,
                    state: row.state,
                    block_kind: row.block_kind,
                    folded_count: row.folded_count,
                    syntax_spans: row.right_syntax,
                    has_find_match,
                    bookmarked,
                },
            )
        })
        .unzip()
}

fn gui_line_state(kind: DiffLineKind) -> &'static str {
    match kind {
        DiffLineKind::Equal => "equal",
        DiffLineKind::Changed => "changed",
        DiffLineKind::LeftOnly => "left_only",
        DiffLineKind::RightOnly => "right_only",
    }
}

/// Build aligned left/right GUI rows from a table compare result. Each
/// `TableRowDiff` becomes one row per side; cells are joined with ` | ` so the
/// existing line-oriented diff pane can render the table side-by-side. The row
/// state drives the diff highlight (left_only / right_only / changed / equal).
fn table_rows_for_gui(
    result: &linsync_core::TableCompareResult,
) -> (Vec<GuiLineRow>, Vec<GuiLineRow>) {
    let mut left_rows = Vec::with_capacity(result.rows.len());
    let mut right_rows = Vec::with_capacity(result.rows.len());
    for row in &result.rows {
        let left_text = row
            .cells
            .iter()
            .map(|c| c.left.clone().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" | ");
        let right_text = row
            .cells
            .iter()
            .map(|c| c.right.clone().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" | ");
        let has_left = row.cells.iter().any(|c| c.left.is_some());
        let has_right = row.cells.iter().any(|c| c.right.is_some());
        let state = if has_left && !has_right {
            "left_only"
        } else if has_right && !has_left {
            "right_only"
        } else if row.has_difference {
            "changed"
        } else {
            "equal"
        };
        let block_kind = if state == "equal" {
            "equal"
        } else {
            "difference"
        };
        let row_id = format!("table:{}", row.row_index);
        let number = Some(row.row_index + 1);
        left_rows.push(GuiLineRow {
            row_id: row_id.clone(),
            number,
            text: left_text,
            state: state.to_owned(),
            block_kind: block_kind.to_owned(),
            folded_count: None,
            syntax_spans: Vec::new(),
            has_find_match: false,
            bookmarked: false,
        });
        right_rows.push(GuiLineRow {
            row_id,
            number,
            text: right_text,
            state: state.to_owned(),
            block_kind: block_kind.to_owned(),
            folded_count: None,
            syntax_spans: Vec::new(),
            has_find_match: false,
            bookmarked: false,
        });
    }
    (left_rows, right_rows)
}

/// Build aligned left/right GUI rows from a binary compare result. Each
/// `HexRow` becomes a single formatted `OFFSET  HEX  ASCII` line per side, so
/// the diff pane renders a navigable hex view with differing rows highlighted.
fn hex_rows_for_gui(
    result: &linsync_core::BinaryCompareResult,
) -> (Vec<GuiLineRow>, Vec<GuiLineRow>) {
    let mut left_rows = Vec::with_capacity(result.rows.len());
    let mut right_rows = Vec::with_capacity(result.rows.len());
    for (index, row) in result.rows.iter().enumerate() {
        let state = if row.has_difference {
            "changed"
        } else {
            "equal"
        };
        let block_kind = if row.has_difference {
            "difference"
        } else {
            "equal"
        };
        let row_id = format!("hex:{:08x}", row.offset);
        let number = Some(index + 1);
        left_rows.push(GuiLineRow {
            row_id: row_id.clone(),
            number,
            text: format!("{:08x}  {}  {}", row.offset, row.left_hex, row.left_ascii),
            state: state.to_owned(),
            block_kind: block_kind.to_owned(),
            folded_count: None,
            syntax_spans: Vec::new(),
            has_find_match: false,
            bookmarked: false,
        });
        right_rows.push(GuiLineRow {
            row_id,
            number,
            text: format!("{:08x}  {}  {}", row.offset, row.right_hex, row.right_ascii),
            state: state.to_owned(),
            block_kind: block_kind.to_owned(),
            folded_count: None,
            syntax_spans: Vec::new(),
            has_find_match: false,
            bookmarked: false,
        });
    }
    (left_rows, right_rows)
}

fn table_tab(
    left: &Path,
    right: &Path,
    left_path: String,
    right_path: String,
    options: &GuiCompareOptions,
) -> GuiCompareTab {
    let table_options = &options.table;
    match compare_table_files(left, right, table_options) {
        Ok(result) => {
            let rows = table_rows_for_gui(&result);
            let summary = vec![
                summary_item("Rows", result.rows.len()),
                summary_item("Changed cells", result.changed_cells),
            ];
            let mut tab = compare_tab(
                "Table",
                (left_path, right_path),
                "Table compare complete".to_owned(),
                result.changed_cells,
                GuiOpenValidation {
                    compatible: true,
                    path_kind: "Files".to_owned(),
                    message: "Validated two table files".to_owned(),
                },
                summary,
                rows,
                vec![],
                None,
                Some(result.rows),
                Vec::new(),
                Some(options.clone()),
            );
            tab.table_headers = result.header.clone();
            tab
        }
        Err(err) => compare_tab(
            "Table",
            (left_path, right_path),
            format!("Table compare failed: {err}"),
            0,
            GuiOpenValidation {
                compatible: true,
                path_kind: "Files".to_owned(),
                message: "Validated two table files; compare failed".to_owned(),
            },
            Vec::new(),
            (Vec::new(), Vec::new()),
            vec![],
            None,
            None,
            Vec::new(),
            Some(options.clone()),
        ),
    }
}

fn binary_tab(
    left: &Path,
    right: &Path,
    left_path: String,
    right_path: String,
    options: &GuiCompareOptions,
) -> GuiCompareTab {
    let binary_options = &options.binary;
    match compare_binary_files(left, right, binary_options) {
        Ok(result) => compare_tab(
            "Hex",
            (left_path, right_path),
            "Binary compare complete".to_owned(),
            result.differences.len(),
            GuiOpenValidation {
                compatible: true,
                path_kind: "Files".to_owned(),
                message: "Validated two binary files".to_owned(),
            },
            vec![
                summary_item("Left bytes", result.left_len),
                summary_item("Right bytes", result.right_len),
                summary_item("Byte differences", result.differences.len()),
            ],
            hex_rows_for_gui(&result),
            vec![],
            None,
            None,
            Vec::new(),
            Some(options.clone()),
        ),
        Err(err) => compare_tab(
            "Hex",
            (left_path, right_path),
            format!("Binary compare failed: {err}"),
            0,
            GuiOpenValidation {
                compatible: true,
                path_kind: "Files".to_owned(),
                message: "Validated two binary files; compare failed".to_owned(),
            },
            Vec::new(),
            (Vec::new(), Vec::new()),
            vec![],
            None,
            None,
            Vec::new(),
            Some(options.clone()),
        ),
    }
}

fn image_tab(
    left: &Path,
    right: &Path,
    left_path: String,
    right_path: String,
    options: &GuiCompareOptions,
) -> GuiCompareTab {
    let image_options = &options.image;
    match compare_images(left, right, image_options) {
        Ok(result) => {
            let diff_count = if result.equal { 0 } else { 1 };
            compare_tab(
                "Image",
                (left_path, right_path),
                if result.equal {
                    "Images are identical".to_owned()
                } else {
                    format!(
                        "Images differ: {}/{} pixels ({:.3}%)",
                        result.differing_pixels,
                        result.total_pixels,
                        result.diff_ratio * 100.0
                    )
                },
                diff_count,
                GuiOpenValidation {
                    compatible: true,
                    path_kind: "Files".to_owned(),
                    message: "Validated two image files".to_owned(),
                },
                vec![
                    summary_item_string(
                        "Left dimensions",
                        format!("{}x{}", result.left_dims.0, result.left_dims.1),
                    ),
                    summary_item_string(
                        "Right dimensions",
                        format!("{}x{}", result.right_dims.0, result.right_dims.1),
                    ),
                    summary_item("Total pixels", result.total_pixels as usize),
                    summary_item("Differing pixels", result.differing_pixels as usize),
                    summary_item_string("Diff ratio", format!("{:.4}", result.diff_ratio)),
                ],
                (Vec::new(), Vec::new()),
                vec![],
                None,
                None,
                Vec::new(),
                Some(options.clone()),
            )
        }
        Err(err) => compare_tab(
            "Image",
            (left_path, right_path),
            format!("Image compare failed: {err}"),
            0,
            GuiOpenValidation {
                compatible: true,
                path_kind: "Files".to_owned(),
                message: "Validated two image files; compare failed".to_owned(),
            },
            Vec::new(),
            (Vec::new(), Vec::new()),
            vec![],
            None,
            None,
            Vec::new(),
            Some(options.clone()),
        ),
    }
}

fn document_mode_query_value(mode: DocumentCompareMode) -> &'static str {
    match mode {
        DocumentCompareMode::Text => "text",
        DocumentCompareMode::OcrText => "ocr_text",
        DocumentCompareMode::Rendered => "rendered",
    }
}

fn document_tab(
    left: &Path,
    right: &Path,
    left_path: String,
    right_path: String,
    options: &GuiCompareOptions,
    should_cancel: &dyn Fn() -> bool,
    progress: Option<Arc<Mutex<CompareProgress>>>,
) -> (Option<GuiCompareTab>, Vec<PathBuf>) {
    let document_options = &options.document;
    if should_cancel() {
        return (None, Vec::new());
    }
    set_progress(
        &progress,
        "extracting",
        1,
        3,
        "Running document extractor".to_owned(),
    );
    let query = format!(
        "left={}&right={}&mode={}&ocr_language={}",
        urlencoding::encode(&left.display().to_string()),
        urlencoding::encode(&right.display().to_string()),
        document_mode_query_value(document_options.mode),
        urlencoding::encode(&document_options.ocr_language),
    );
    let (body, artifact_dirs) =
        linsync::document_compare_bridge_response_with_profile_and_artifacts(
            &query,
            document_options,
        );
    set_progress(
        &progress,
        "finalizing",
        2,
        3,
        "Building document tab".to_owned(),
    );
    if should_cancel() {
        for dir in &artifact_dirs {
            let _ = fs::remove_dir_all(dir);
        }
        return (None, Vec::new());
    }
    let value = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(value) => value,
        Err(err) => {
            for dir in &artifact_dirs {
                let _ = fs::remove_dir_all(dir);
            }
            return (
                Some(invalid_compare_tab(
                    "Document",
                    left_path,
                    right_path,
                    format!("Document compare failed: {err}"),
                )),
                Vec::new(),
            );
        }
    };
    if let Some(mut tab) = document_tab_from_response(left_path.clone(), right_path.clone(), &value)
    {
        tab.options = Some(options.clone());
        set_progress(&progress, "done", 3, 3, String::new());
        return (Some(tab), artifact_dirs);
    }
    for dir in &artifact_dirs {
        let _ = fs::remove_dir_all(dir);
    }
    let message = value
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("document compare failed");
    set_progress(&progress, "done", 3, 3, String::new());
    (
        Some(compare_tab(
            "Document",
            (left_path, right_path),
            format!("Document compare failed: {message}"),
            0,
            GuiOpenValidation {
                compatible: true,
                path_kind: "Files".to_owned(),
                message: "Validated two document files; compare failed".to_owned(),
            },
            Vec::new(),
            (Vec::new(), Vec::new()),
            vec![],
            None,
            None,
            Vec::new(),
            Some(options.clone()),
        )),
        Vec::new(),
    )
}

fn image_tab_from_result(
    left_path: String,
    right_path: String,
    result: &ImageCompareResult,
    response: &serde_json::Value,
) -> GuiCompareTab {
    let diff_count = if result.equal { 0 } else { 1 };
    let mut artifacts = Vec::new();
    if let Some(uri) = response.get("overlay_path").and_then(|v| v.as_str())
        && let Some(path) = uri.strip_prefix("file://")
    {
        artifacts.push(linsync_core::CompareArtifact::ImageOverlay {
            path: PathBuf::from(path),
            width: result.left_dims.0.max(result.right_dims.0),
            height: result.left_dims.1.max(result.right_dims.1),
        });
    }
    compare_tab(
        "Image",
        (left_path, right_path),
        if result.equal {
            "Images are identical".to_owned()
        } else {
            format!(
                "Images differ: {}/{} pixels ({:.3}%)",
                result.differing_pixels,
                result.total_pixels,
                result.diff_ratio * 100.0
            )
        },
        diff_count,
        GuiOpenValidation {
            compatible: true,
            path_kind: "Files".to_owned(),
            message: "Validated two image files".to_owned(),
        },
        vec![
            summary_item_string(
                "Left dimensions",
                format!("{}x{}", result.left_dims.0, result.left_dims.1),
            ),
            summary_item_string(
                "Right dimensions",
                format!("{}x{}", result.right_dims.0, result.right_dims.1),
            ),
            summary_item("Total pixels", result.total_pixels as usize),
            summary_item("Differing pixels", result.differing_pixels as usize),
            summary_item_string("Diff ratio", format!("{:.4}", result.diff_ratio)),
        ],
        (Vec::new(), Vec::new()),
        vec![],
        None,
        None,
        artifacts,
        None,
    )
}

fn document_tab_from_response(
    left_path: String,
    right_path: String,
    response: &serde_json::Value,
) -> Option<GuiCompareTab> {
    if response.get("error").is_some() {
        return None;
    }
    let rendered_pages: Option<Vec<GuiRenderedPage>> = response
        .get("rendered_pages")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let extractor = response
        .get("left_extractor")
        .and_then(|v| v.as_str())
        .unwrap_or("document plugin")
        .to_owned();

    let (left_rows, right_rows, diff_count, encoding_metadata, summary, status) =
        if let Some(ref pages) = rendered_pages {
            let diff_count = pages.iter().filter(|p| !p.equal).count();
            let status = if diff_count == 0 {
                format!("Rendered pages are identical (extracted via {extractor})")
            } else {
                format!("{diff_count} differing rendered pages (extracted via {extractor})")
            };
            let summary = vec![
                summary_item("Rendered pages", pages.len()),
                summary_item("Differing pages", diff_count),
                summary_item_string("Extractor", extractor.clone()),
            ];
            (Vec::new(), Vec::new(), diff_count, None, summary, status)
        } else {
            let left_text = response
                .get("left_text")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let right_text = response
                .get("right_text")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let text_result = compare_text(
                &left_path,
                left_text,
                &right_path,
                right_text,
                &TextCompareOptions::default(),
            );
            let diff_count = response
                .get("differing_lines")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or_else(|| text_result.difference_count());
            let (left_rows, right_rows) =
                text_rows_for_gui(&text_result.lines, &text_result.blocks);
            let summary = vec![
                summary_item("Differing lines", diff_count),
                summary_item_string("Extractor", extractor.clone()),
            ];
            let status = if diff_count == 0 {
                format!("Documents are equal (extracted via {extractor})")
            } else {
                format!("{diff_count} differing document lines (extracted via {extractor})")
            };
            (
                left_rows,
                right_rows,
                diff_count,
                Some(text_result.encoding_summary()),
                summary,
                status,
            )
        };

    let mut tab = compare_tab(
        "Document",
        (left_path, right_path),
        status,
        diff_count,
        GuiOpenValidation {
            compatible: true,
            path_kind: "Files".to_owned(),
            message: "Validated two document files".to_owned(),
        },
        summary,
        (left_rows, right_rows),
        vec![],
        encoding_metadata,
        None,
        Vec::new(),
        None,
    );
    tab.rendered_pages = rendered_pages;
    Some(tab)
}

fn webpage_tab_from_response(
    left_url: String,
    right_url: String,
    mode: &str,
    response: &serde_json::Value,
) -> Option<GuiCompareTab> {
    if response.get("error").is_some() {
        return None;
    }
    let rows = response
        .get("rows")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut left_rows = Vec::with_capacity(rows.len());
    let mut right_rows = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let state = row.get("s").and_then(|v| v.as_str()).unwrap_or("equal");
        let block_kind = if state == "equal" {
            "equal"
        } else {
            "difference"
        };
        let row_id = format!("webpage:{index}");
        left_rows.push(GuiLineRow {
            row_id: row_id.clone(),
            number: row.get("ln").and_then(|v| v.as_u64()).map(|n| n as usize),
            text: row
                .get("l")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned(),
            state: if state == "right_only" {
                "equal".to_owned()
            } else {
                state.to_owned()
            },
            block_kind: block_kind.to_owned(),
            folded_count: None,
            syntax_spans: Vec::new(),
            has_find_match: false,
            bookmarked: false,
        });
        right_rows.push(GuiLineRow {
            row_id,
            number: row.get("rn").and_then(|v| v.as_u64()).map(|n| n as usize),
            text: row
                .get("r")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned(),
            state: if state == "left_only" {
                "equal".to_owned()
            } else {
                state.to_owned()
            },
            block_kind: block_kind.to_owned(),
            folded_count: None,
            syntax_spans: Vec::new(),
            has_find_match: false,
            bookmarked: false,
        });
    }
    let equal = response
        .get("equal")
        .and_then(|v| v.as_bool())
        .unwrap_or(rows.is_empty());
    let diff_count = rows
        .iter()
        .filter(|row| {
            row.get("s")
                .and_then(|v| v.as_str())
                .is_some_and(|state| state != "equal")
        })
        .count()
        .max((!equal) as usize);
    let summary = response
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("Compare complete")
        .to_owned();
    Some(compare_tab(
        "Webpage",
        (left_url, right_url),
        summary.clone(),
        diff_count,
        GuiOpenValidation {
            compatible: true,
            path_kind: "URLs".to_owned(),
            message: "Validated two webpage URLs".to_owned(),
        },
        vec![
            summary_item_string("Mode", mode.to_owned()),
            summary_item("Rows", rows.len()),
            summary_item_string(
                "Truncated",
                response
                    .get("truncated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                    .to_string(),
            ),
        ],
        (left_rows, right_rows),
        vec![],
        None,
        None,
        Vec::new(),
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
fn compare_tab(
    mode: &str,
    paths: (String, String),
    status: String,
    difference_count: usize,
    validation: GuiOpenValidation,
    summary: Vec<GuiSummaryItem>,
    rows: (Vec<GuiLineRow>, Vec<GuiLineRow>),
    folder_entries: Vec<GuiFolderEntry>,
    encoding_metadata: Option<EncodingSummary>,
    table_cells: Option<Vec<linsync_core::TableRowDiff>>,
    artifacts: Vec<linsync_core::CompareArtifact>,
    options: Option<GuiCompareOptions>,
) -> GuiCompareTab {
    let (left_path, right_path) = paths;
    let (left_rows, right_rows) = rows;
    GuiCompareTab {
        id: 1,
        title: compare_tab_title(mode, &left_path, &right_path),
        mode: mode.to_owned(),
        left_path,
        right_path,
        base_path: None,
        status,
        difference_count,
        left_dirty: false,
        right_dirty: false,
        can_undo: false,
        can_redo: false,
        validation,
        summary,
        left_rows,
        right_rows,
        total_rows: None,
        diff_row_indexes: Vec::new(),
        search_row_indexes: Vec::new(),
        folder_entries,
        folder_total: None,
        encoding_metadata,
        table_headers: None,
        table_cells,
        artifacts,
        rendered_pages: None,
        options,
    }
}

/// Text diffs with more than this many rows are served to the GUI in windows:
/// the compare response embeds only the first window, and the GUI fetches the
/// rest on demand via `/compare/text/window` as the user scrolls or jumps. Kept
/// well above a screenful so small/medium diffs are still embedded whole (zero
/// behavior change for the common case). Also the window size used per fetch.
const TEXT_WINDOW_THRESHOLD: usize = 2000;

/// Folder comparisons with more than this many entries are served to the GUI a
/// page at a time: the compare response embeds only the first page, and the GUI
/// pages + sorts + filters the rest through `/folder/query`. Kept high so the
/// common small/medium folder loads whole (client-side sort/filter, unchanged).
const FOLDER_WINDOW_THRESHOLD: usize = 5000;

/// Binary/hex comparisons with more than this many rows are served windowed:
/// the compare response embeds only the first page, and the GUI pages the rest
/// through `/binary/window`. Same rationale as text windowing.
const BINARY_WINDOW_THRESHOLD: usize = 2000;

/// Table comparisons with more than this many rows are served windowed:
/// the compare response embeds only the first page, and the GUI pages the rest
/// through `/compare/table/window`.
const TABLE_WINDOW_THRESHOLD: usize = 2000;
/// The window size used for table compare wire responses and fetches.
const TABLE_WINDOW_SIZE: usize = 2000;

/// Whether `tab` is a comparison large enough to serve windowed — a text diff
/// over [`TEXT_WINDOW_THRESHOLD`] rows, a folder over
/// [`FOLDER_WINDOW_THRESHOLD`] entries, a hex diff over
/// [`BINARY_WINDOW_THRESHOLD`] rows, or a table diff over
/// [`TABLE_WINDOW_THRESHOLD`] rows.
fn tab_needs_windowing(tab: &GuiCompareTab) -> bool {
    (tab.mode == "Text" && tab.left_rows.len().max(tab.right_rows.len()) > TEXT_WINDOW_THRESHOLD)
        || (tab.mode == "Folder" && tab.folder_entries.len() > FOLDER_WINDOW_THRESHOLD)
        || (tab.mode == "Hex"
            && tab.left_rows.len().max(tab.right_rows.len()) > BINARY_WINDOW_THRESHOLD)
        || (tab.mode == "Table"
            && tab
                .table_cells
                .as_ref()
                .is_some_and(|r| r.len() > TABLE_WINDOW_THRESHOLD))
}

/// Window a large folder `tab` for transmission: record the full entry count and
/// truncate the embedded entries to the first page. The GUI then pages the rest
/// through `/folder/query`. The canonical server-side tab stays full. A no-op
/// for folders below the threshold.
fn apply_folder_windowing(tab: &mut GuiCompareTab) {
    if tab.mode != "Folder" || tab.folder_entries.len() <= FOLDER_WINDOW_THRESHOLD {
        return;
    }
    tab.folder_total = Some(tab.folder_entries.len());
    tab.folder_entries.truncate(FOLDER_WINDOW_THRESHOLD);
}

/// Window a large text `tab` *for transmission to the GUI*: compute the full
/// change-row index list (so next/prev-change navigation reaches differences
/// outside the loaded window), record the total row count, and truncate the
/// embedded rows to the first window. Callers apply this to a throwaway clone —
/// the canonical server-side tab stays full so merge-copy, bookmarks, undo, and
/// report export still address every row. A no-op for tabs below the threshold.
fn apply_text_windowing(tab: &mut GuiCompareTab) {
    if !tab_needs_windowing(tab) {
        return;
    }
    let total = tab.left_rows.len().max(tab.right_rows.len());
    let mut diff_row_indexes = Vec::new();
    let mut search_row_indexes = Vec::new();
    for index in 0..total {
        let left = tab.left_rows.get(index);
        let right = tab.right_rows.get(index);
        let left_state = left.map(|row| row.state.as_str());
        let right_state = right.map(|row| row.state.as_str());
        if left_state.is_some_and(is_gui_difference_state)
            || right_state.is_some_and(is_gui_difference_state)
        {
            diff_row_indexes.push(index);
        }
        if left.is_some_and(|row| row.has_find_match) || right.is_some_and(|row| row.has_find_match)
        {
            search_row_indexes.push(index);
        }
    }
    tab.total_rows = Some(total);
    tab.diff_row_indexes = diff_row_indexes;
    tab.search_row_indexes = search_row_indexes;
    tab.left_rows.truncate(TEXT_WINDOW_THRESHOLD);
    tab.right_rows.truncate(TEXT_WINDOW_THRESHOLD);
}

/// Window a large binary/hex `tab` *for transmission to the GUI*: record the
/// total row count and truncate the embedded rows to the first window. The
/// GUI then pages the rest through `/binary/window`. A no-op for tabs below
/// the threshold.
fn apply_binary_windowing(tab: &mut GuiCompareTab) {
    if tab.mode != "Hex" || tab.left_rows.len().max(tab.right_rows.len()) <= BINARY_WINDOW_THRESHOLD
    {
        return;
    }
    let total = tab.left_rows.len().max(tab.right_rows.len());
    let mut diff_row_indexes = Vec::new();
    for index in 0..total {
        let left = tab.left_rows.get(index);
        let right = tab.right_rows.get(index);
        let left_state = left.map(|row| row.state.as_str());
        let right_state = right.map(|row| row.state.as_str());
        if left_state.is_some_and(is_gui_difference_state)
            || right_state.is_some_and(is_gui_difference_state)
        {
            diff_row_indexes.push(index);
        }
    }
    tab.total_rows = Some(total);
    tab.diff_row_indexes = diff_row_indexes;
    tab.left_rows.truncate(BINARY_WINDOW_THRESHOLD);
    tab.right_rows.truncate(BINARY_WINDOW_THRESHOLD);
}

/// Window a large table `tab` *for transmission to the GUI*: record the full
/// row count and truncate the embedded `table_cells` to the first window. The
/// GUI then pages the rest through `/compare/table/window`. A no-op for tables
/// below the threshold.
fn apply_table_windowing(tab: &mut GuiCompareTab) {
    if tab.mode != "Table" {
        return;
    }
    let Some(rows) = tab.table_cells.as_ref() else {
        return;
    };
    if rows.len() <= TABLE_WINDOW_THRESHOLD {
        return;
    }
    let total = rows.len();
    let window = rows.iter().take(TABLE_WINDOW_SIZE).cloned().collect();
    tab.total_rows = Some(total);
    tab.table_cells = Some(window);
}

fn compare_tab_title(mode: &str, left_path: &str, right_path: &str) -> String {
    let left_name = Path::new(left_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(left_path);
    let right_name = Path::new(right_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(right_path);
    format!("{mode}: {left_name} <-> {right_name}")
}

fn summary_item(label: &str, value: usize) -> GuiSummaryItem {
    GuiSummaryItem {
        label: label.to_owned(),
        value: value.to_string(),
    }
}

fn summary_item_string(label: &str, value: String) -> GuiSummaryItem {
    GuiSummaryItem {
        label: label.to_owned(),
        value,
    }
}

fn is_table_path(path: &Path) -> bool {
    is_csv_path(path) || is_tsv_path(path)
}

fn is_csv_path(path: &Path) -> bool {
    has_extension(path, "csv")
}

fn is_tsv_path(path: &Path) -> bool {
    has_extension(path, "tsv")
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn start_bridge_server(
    paths: AppPaths,
    initial_context: Option<GuiLaunchContext>,
) -> Result<BridgeServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|err| format!("failed to bind LinSync GUI bridge: {err}"))?;
    let address = listener
        .local_addr()
        .map_err(|err| format!("failed to read LinSync GUI bridge address: {err}"))?;
    let token = bridge_token()
        .map_err(|err| format!("failed to create LinSync GUI bridge token: {err}"))?;
    let base_url = format!("http://{address}/{token}");
    let server_token = token.clone();
    let state = Arc::new(Mutex::new(GuiBridgeState::new(initial_context)));

    // Pre-load the plugin-enabled map from disk so the in-memory copy is
    // authoritative from the first request onward.
    if let Ok(s) = state.lock()
        && let Ok(mut pe) = s.plugin_enabled.lock()
    {
        *pe = linsync_core::load_plugin_enabled_map(&paths);
    }

    // Clear a stale active-profile pointer once at startup (e.g. a user profile
    // deleted while selected) so the per-request resolver doesn't warn on every
    // request.
    cleanup_stale_active_pointer(&paths);

    // Reclaim archive-edit staging dirs and portal backups orphaned by a
    // crash or kill mid-edit (edit tokens live only in process memory, so
    // nothing else ever references them again). Age-gated so a concurrent
    // LinSync instance's live edit is never swept.
    sweep_orphaned_archive_edits(&paths);

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    // Handle each connection on its own thread so a `/cancel`
                    // request can be served while a `/compare` is still running
                    // (the accept loop must not block on a single request).
                    let paths = paths.clone();
                    let state = Arc::clone(&state);
                    let token = server_token.clone();
                    thread::spawn(move || {
                        if let Err(err) = handle_bridge_connection(stream, &paths, &state, &token) {
                            tracing::warn!(error = %err, "LinSync GUI bridge request failed");
                        }
                    });
                }
                Err(err) => {
                    tracing::warn!(error = %err, "LinSync GUI bridge accept failed");
                    break;
                }
            }
        }
    });

    Ok(BridgeServer { base_url })
}

const MAX_BRIDGE_REQUEST_BYTES: u64 = 256 * 1024; // 256 KB — bumped for raw-text paste via query params
const MAX_BRIDGE_HEADERS: usize = 64;

fn handle_bridge_connection(
    mut stream: TcpStream,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
    token: &str,
) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let mut reader = BufReader::new(stream.try_clone()?).take(MAX_BRIDGE_REQUEST_BYTES);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    // Reject any Origin: header from a non-loopback page.
    let mut origin: Option<String> = None;
    let mut headers_seen: usize = 0;
    loop {
        if headers_seen > MAX_BRIDGE_HEADERS {
            return Ok(());
        }
        headers_seen += 1;
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" {
            break;
        }
        if let Some(value) = header
            .split_once(':')
            .and_then(|(name, value)| name.eq_ignore_ascii_case("origin").then_some(value))
        {
            origin = Some(value.trim().to_owned());
        }
    }

    if let Some(value) = origin.as_deref()
        && !origin_is_loopback(value)
    {
        let response = bridge_error(403, "Forbidden", "cross-origin requests are not allowed");
        stream.write_all(&response)?;
        return stream.flush();
    }

    let response = bridge_response_with_token(&request_line, paths, state, Some(token));
    stream.write_all(&response)?;
    stream.flush()
}

fn origin_is_loopback(origin: &str) -> bool {
    let scheme_end = match origin.find("://") {
        Some(index) => index + 3,
        None => return false,
    };
    let host = &origin[scheme_end..];
    let host = host.split_once('/').map(|(host, _)| host).unwrap_or(host);
    let host = if let Some(rest) = host.strip_prefix('[') {
        let Some((address, after_bracket)) = rest.split_once(']') else {
            return false;
        };
        if !after_bracket.is_empty() && !after_bracket.starts_with(':') {
            return false;
        }
        address
    } else if host == "::1" {
        host
    } else {
        host.rsplit_once(':').map(|(host, _)| host).unwrap_or(host)
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
fn bridge_response(
    request_line: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    bridge_response_with_token(request_line, paths, state, None)
}

fn bridge_response_with_token(
    request_line: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
    required_token: Option<&str>,
) -> Vec<u8> {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();

    if method == "OPTIONS" {
        return http_response(204, "No Content", "application/json", b"{}".to_vec());
    }

    if method != "GET" {
        return bridge_error(405, "Method Not Allowed", "unsupported method");
    }

    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let path = match strip_required_bridge_token(path, required_token) {
        Ok(path) => path,
        Err(response) => return response,
    };
    match path {
        "/health" => http_response(
            200,
            "OK",
            "application/json",
            format!(r#"{{"ok":true,"bridge_version":{BRIDGE_VERSION}}}"#).into_bytes(),
        ),
        "/session" => session_bridge_response(state),
        "/settings" => settings_bridge_response(paths),
        "/settings/set" => settings_set_bridge_response(query, paths),
        "/settings/reset" => settings_reset_bridge_response(paths),
        "/file/read" => file_read_bridge_response(query, state),
        "/file/write" => file_write_bridge_response(query, state),
        "/compare" => compare_bridge_response(query, paths, state),
        "/cancel" => cancel_bridge_response(query, state),
        "/progress" => progress_bridge_response(query, state),
        "/copy" => copy_bridge_response(query, state),
        "/copy-all" => copy_all_bridge_response(query, state),
        "/undo" => undo_bridge_response(state),
        "/redo" => redo_bridge_response(state),
        "/save" => save_bridge_response(query, state),
        "/tab/activate" => activate_tab_bridge_response(query, state),
        "/tab/close" => close_tab_bridge_response(query, state),
        "/bookmark/set" => bookmark_set_bridge_response(query, state),
        "/folder/open" => folder_open_bridge_response(query, paths),
        "/sessions/recent" => sessions_recent_bridge_response(paths),
        "/sessions/reopen" => sessions_reopen_bridge_response(query, paths, state),
        "/sessions/delete" => sessions_delete_bridge_response(query, paths),
        "/sessions/rename" => sessions_rename_bridge_response(query, paths),
        "/filters/list" => filters_list_bridge_response(paths),
        "/filters/save" => filters_save_bridge_response(query, paths),
        "/filters/delete" => filters_delete_bridge_response(query, paths),
        "/filters/validate" => filters_validate_bridge_response(query),
        "/filters/migrate" => filters_migrate_bridge_response(query),
        "/walk" => walk_options_bridge_response(paths),
        "/walk/set" => walk_options_set_bridge_response(query, paths),
        "/plugins/list" => {
            let pe = match state.lock() {
                Ok(s) => Arc::clone(&s.plugin_enabled),
                Err(_) => {
                    return bridge_error(500, "Internal Server Error", "session state unavailable");
                }
            };
            plugins_list_bridge_response(paths, &pe)
        }
        "/plugins/toggle" => {
            let pe = match state.lock() {
                Ok(s) => Arc::clone(&s.plugin_enabled),
                Err(_) => {
                    return bridge_error(500, "Internal Server Error", "session state unavailable");
                }
            };
            plugins_toggle_bridge_response(query, paths, &pe)
        }
        "/plugins/options/get" => plugins_options_get_bridge_response(query, paths),
        "/plugins/options/set" => plugins_options_set_bridge_response(query, paths),
        "/plugins/diagnostic" => plugins_diagnostic_bridge_response(query, paths),
        "/plugins/install" => plugins_install_bridge_response(query, paths),
        "/plugins/remove" => plugins_remove_bridge_response(query, paths),
        "/plugins/trust" => plugins_trust_bridge_response(query, paths),
        "/capabilities" => capabilities_bridge_response(),
        "/folder/query" => folder_query_bridge_response(query, paths),
        "/compare/text/window" => text_window_bridge_response(query, paths),
        "/compare/table/window" => table_window_bridge_response(query, state),
        "/binary/window" => binary_window_bridge_response(query, state),
        "/folder/op/plan" => folder_op_plan_bridge_response(query, paths, state),
        "/folder/op/execute" => folder_op_execute_bridge_response(query, paths, state),
        "/archive/member/edit" => archive_member_edit_bridge_response(query, paths, state),
        "/archive/member/commit" => archive_member_commit_bridge_response(query, state),
        "/archive/member/discard" => archive_member_discard_bridge_response(query, state),
        "/merge/conflicts" => merge_conflicts_bridge_response(state),
        "/merge3/start" => merge3_start_bridge_response(query, paths, state),
        "/merge3/resolve" => merge3_resolve_bridge_response(query, state),
        "/merge3/save" => merge3_save_bridge_response(query, state),
        "/compare/document" => {
            let params = query_params(query);
            let profile = match resolve_profile_for_request(paths, &params) {
                Ok(p) => p,
                Err(err) => return bridge_error(400, "Bad Request", &err),
            };
            let (request_id, progress) =
                register_progress_request(&params, state, "extracting", 3, "Extracting text");
            set_progress(
                &progress,
                "extracting",
                1,
                3,
                "Running document extractor".to_owned(),
            );
            let (mut body, artifacts) =
                linsync::document_compare_bridge_response_with_profile_and_artifacts(
                    query,
                    &profile.document,
                );
            set_progress(
                &progress,
                "finalizing",
                2,
                3,
                "Building document tab".to_owned(),
            );
            if let (Some(left), Some(right), Ok(value)) = (
                query_value(&params, "left"),
                query_value(&params, "right"),
                serde_json::from_str::<serde_json::Value>(&body),
            ) {
                let tab = document_tab_from_response(left.to_owned(), right.to_owned(), &value);
                body = attach_session_to_response_body(
                    body,
                    tab,
                    query_bool(&params, "new_tab"),
                    paths,
                    state,
                );
                if !artifacts.is_empty()
                    && let Ok(mut state) = state.lock()
                {
                    let tab_id = state.session.active_tab_id;
                    if let Some(old) = state.rendered_page_cache_dirs.remove(&tab_id) {
                        let _ = fs::remove_dir_all(old);
                    }
                    if let Some(dir) = artifacts.into_iter().next() {
                        state.rendered_page_cache_dirs.insert(tab_id, dir);
                    }
                }
            }
            set_progress(&progress, "done", 3, 3, String::new());
            remove_progress_request(request_id.as_deref(), state);
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "/profiles/list" => profiles_list_bridge_response(paths),
        "/profiles/active/get" => profiles_active_get_bridge_response(paths),
        "/profiles/active/set" => profiles_active_set_bridge_response(query, paths),
        "/profiles/active/prediffer" => profiles_active_prediffer_bridge_response(query, paths),
        "/profiles/active/plugin-enabled" => {
            profiles_active_plugin_enabled_bridge_response(query, paths)
        }
        "/raw-compare" => raw_compare_bridge_response(query, paths, state),
        "/compare/image" => {
            let params = query_params(query);
            let profile = match resolve_profile_for_request(paths, &params) {
                Ok(p) => p,
                Err(err) => return bridge_error(400, "Bad Request", &err),
            };
            let (mut body, result) =
                linsync::image_compare_bridge_response_with_profile(query, &profile.image);
            let result_for_tab = result.clone();
            let overlay_path = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("overlay_path")
                        .and_then(|uri| uri.as_str())
                        .and_then(file_uri_to_path)
                });
            if let Ok(mut s) = state.lock() {
                s.last_image_result = result;
                s.last_image_overlay_path = overlay_path;
            }
            if let (Some(result), Some(left), Some(right), Ok(value)) = (
                result_for_tab,
                query_value(&params, "left"),
                query_value(&params, "right"),
                serde_json::from_str::<serde_json::Value>(&body),
            ) {
                let tab = image_tab_from_result(left.to_owned(), right.to_owned(), &result, &value);
                body = attach_session_to_response_body(
                    body,
                    Some(tab),
                    query_bool(&params, "new_tab"),
                    paths,
                    state,
                );
            }
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "/compare/image/regions" => image_regions_bridge_response(state),
        "/compare/image/save-overlay" => image_save_overlay_bridge_response(query, state),
        "/compare/image/formats" => http_response(
            200,
            "OK",
            "application/json",
            linsync::image_formats_bridge_response().into_bytes(),
        ),
        "/compare/webpage" => {
            let params = query_params(query);
            let profile = match resolve_profile_for_request(paths, &params) {
                Ok(p) => p,
                Err(err) => return bridge_error(400, "Bad Request", &err),
            };
            let (request_id, progress) =
                register_progress_request(&params, state, "fetching", 3, "Fetching webpages");
            set_progress(
                &progress,
                "fetching",
                1,
                3,
                "Fetching webpage content".to_owned(),
            );
            let mut body = linsync::webpage_compare_bridge_response_with_profile(
                query,
                paths,
                &profile.webpage,
            );
            set_progress(
                &progress,
                "finalizing",
                2,
                3,
                "Building webpage tab".to_owned(),
            );
            if let (Some(left), Some(right), Ok(value)) = (
                query_value(&params, "left"),
                query_value(&params, "right"),
                serde_json::from_str::<serde_json::Value>(&body),
            ) {
                let mode = query_value(&params, "mode").unwrap_or("html");
                let tab =
                    webpage_tab_from_response(left.to_owned(), right.to_owned(), mode, &value);
                body = attach_session_to_response_body(
                    body,
                    tab,
                    query_bool(&params, "new_tab"),
                    paths,
                    state,
                );
            }
            set_progress(&progress, "done", 3, 3, String::new());
            remove_progress_request(request_id.as_deref(), state);
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "/compare/webpage/clear-cache" => {
            let body = linsync::webpage_clear_cache_bridge_response(paths);
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "/binary/interpret" => binary_interpret_bridge_response(query, state),
        "/reveal" => reveal_bridge_response(query),
        "/open-external" => open_external_bridge_response(query),
        "/copy-clipboard" => copy_clipboard_bridge_response(query),
        "/report" => report_bridge_response(query, state, paths),
        "/project/save" => project_save_bridge_response(query, state, paths),
        "/project/open" => project_open_bridge_response(query, paths),
        "/project/recent" => project_recent_bridge_response(paths),
        "/sessions/save" => sessions_save_bridge_response(query, paths, state),
        "/artifacts/list" => artifacts_list_bridge_response(state),
        "/artifacts/cleanup" => artifacts_cleanup_bridge_response(query, paths),
        _ => bridge_error(404, "Not Found", "unknown bridge endpoint"),
    }
}

fn strip_required_bridge_token<'a>(
    path: &'a str,
    required_token: Option<&str>,
) -> Result<&'a str, Vec<u8>> {
    let Some(token) = required_token else {
        return Ok(path);
    };

    let expected_prefix = format!("/{token}");
    if path == expected_prefix {
        return Ok("/");
    }
    path.strip_prefix(&expected_prefix)
        .filter(|rest| rest.starts_with('/'))
        .ok_or_else(|| bridge_error(403, "Forbidden", "invalid bridge token"))
}

fn bridge_token() -> std::io::Result<String> {
    let mut bytes = [0_u8; 16];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn session_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let context = match state.lock() {
        Ok(state) => state.context(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn settings_bridge_response(paths: &AppPaths) -> Vec<u8> {
    match load_gui_settings_json(paths) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

fn settings_set_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(key) = query_value(&params, "key") else {
        return bridge_error(400, "Bad Request", "missing setting key");
    };
    let Some(value) = query_value(&params, "value") else {
        return bridge_error(400, "Bad Request", "missing setting value");
    };

    match save_gui_setting_json(paths, key, value) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(400, "Bad Request", &err),
    }
}

fn settings_reset_bridge_response(paths: &AppPaths) -> Vec<u8> {
    match reset_gui_settings_json(paths) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

// ── Profile bridge endpoints ────────────────────────────────────────────────

fn profiles_list_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let mut entries: Vec<serde_json::Value> = Vec::new();
    for p in builtin_profiles() {
        entries.push(serde_json::json!({
            "id": p.id.to_string(),
            "name": p.name,
            "description": p.description,
            "builtin": true,
        }));
    }
    let user_ids = match store.list_user_ids() {
        Ok(ids) => ids,
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    for id in user_ids {
        match store.load(&id) {
            Ok(p) => entries.push(serde_json::json!({
                "id": p.id.to_string(),
                "name": p.name,
                "description": p.description,
                "builtin": false,
            })),
            Err(err) => entries.push(serde_json::json!({
                "id": id.to_string(),
                "name": id.to_string(),
                "description": String::new(),
                "builtin": false,
                "error": err.to_string(),
            })),
        }
    }
    let active = store
        .load_active_pointer()
        .ok()
        .flatten()
        .map(|id| id.to_string());
    let body = serde_json::json!({
        "active": active,
        "profiles": entries,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn profiles_active_get_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let active = match store.load_active_pointer() {
        Ok(maybe) => maybe.map(|id| id.to_string()),
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    let body = serde_json::json!({ "active": active }).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn profiles_active_set_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(raw_id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id parameter");
    };
    let id = match ProfileId::new(raw_id.to_owned()) {
        Ok(id) => id,
        Err(err) => {
            return bridge_error(400, "Bad Request", &format!("invalid profile id: {err}"));
        }
    };
    // Reject ids that don't resolve to a built-in or stored user
    // profile. This prevents the GUI from quietly setting an active
    // pointer that subsequent compares would fall back away from.
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    if find_builtin(&id).is_none() && store.load(&id).is_err() {
        return bridge_error(
            404,
            "Not Found",
            &format!("profile '{id}' does not exist (built-in or user)"),
        );
    }
    if let Err(err) = store.save_active_pointer(&id) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({ "active": id.to_string() }).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Add or remove a prediffer plugin from the active profile's prediffer chain
/// (`?id=PLUGIN_ID&enabled=true|false`). Only user profiles are editable;
/// built-in profiles (and "no active profile") are rejected with 409 so the
/// caller can prompt the user to create/select a user profile first.
fn profiles_active_prediffer_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(plugin_id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id parameter");
    };
    let enabled = query_value(&params, "enabled")
        .map(|v| v != "false")
        .unwrap_or(true);

    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let active_id = match store.load_active_pointer() {
        Ok(Some(id)) => id,
        Ok(None) => {
            return bridge_error(
                409,
                "Conflict",
                "no active profile selected; select a user profile to edit its prediffers",
            );
        }
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    if find_builtin(&active_id).is_some() {
        return bridge_error(
            409,
            "Conflict",
            "built-in profiles are read-only; copy to a user profile to edit prediffers",
        );
    }
    let mut profile = match store.load(&active_id) {
        Ok(p) => p,
        Err(err) => return bridge_error(404, "Not Found", &err.to_string()),
    };
    // Apply the add/remove, keeping the list de-duplicated and order-stable.
    profile
        .text
        .prediffer_plugins
        .retain(|existing| existing != plugin_id);
    if enabled {
        profile.text.prediffer_plugins.push(plugin_id.to_owned());
    }
    if let Err(err) = store.save(&profile) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({
        "ok": true,
        "profile": active_id.to_string(),
        "prediffers": profile.text.prediffer_plugins,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Set or clear a per-plugin enable/disable override on the active profile
/// (`?id=PLUGIN_ID&enabled=true|false`). Unlike the global `plugins.json`
/// toggle, this override is scoped to the active profile and wins over the
/// global state when that profile drives a comparison. Only user profiles are
/// editable; built-in profiles (and "no active profile") are rejected with 409.
fn profiles_active_plugin_enabled_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(plugin_id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id parameter");
    };
    let Some(enabled_raw) = query_value(&params, "enabled") else {
        return bridge_error(400, "Bad Request", "missing enabled parameter");
    };
    let enabled = enabled_raw != "false";

    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let active_id = match store.load_active_pointer() {
        Ok(Some(id)) => id,
        Ok(None) => {
            return bridge_error(
                409,
                "Conflict",
                "no active profile selected; select a user profile to set per-profile plugin state",
            );
        }
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    if find_builtin(&active_id).is_some() {
        return bridge_error(
            409,
            "Conflict",
            "built-in profiles are read-only; copy to a user profile to set per-profile plugin state",
        );
    }
    let mut profile = match store.load(&active_id) {
        Ok(p) => p,
        Err(err) => return bridge_error(404, "Not Found", &err.to_string()),
    };
    // Record the override. We always store the explicit boolean (rather than
    // dropping back to "default") so the GUI can show a clear per-profile state;
    // the resolver treats a present entry as authoritative over the global map.
    //
    // This is an unsynchronized load-modify-write to the profile file, matching
    // the sibling /profiles/active/{set,prediffer} endpoints: the GUI drives one
    // edit at a time, so concurrent edits to the same profile are not a concern
    // here. If a multi-writer scenario ever arises, add file-level locking
    // across all profile-mutating endpoints rather than just this one.
    profile
        .plugin_enablement
        .insert(plugin_id.to_owned(), enabled);
    if let Err(err) = store.save(&profile) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({
        "ok": true,
        "profile": active_id.to_string(),
        "plugin_id": plugin_id,
        "enabled": enabled,
        "plugin_enablement": profile.plugin_enablement,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

// ── Profile resolution for bridge requests ──────────────────────────────────

/// Resolve the [`CompareProfile`] that should drive a single bridge
/// request:
///   1. If `?profile=<id>` is present:
///      - If the id resolves to a built-in or stored user profile, use it.
///      - Otherwise return `Err(...)` — the caller must surface a 400 so
///        the GUI cannot silently fall through to the wrong options. This
///        matches `/profiles/active/set`'s 404 semantics for unknown ids.
///   2. Otherwise read `active-profile.json`. If the id resolves to a
///      built-in or a stored user profile, use it.
///   3. As a last resort, return the `default` built-in.
fn resolve_profile_for_request(
    paths: &AppPaths,
    params: &[(String, String)],
) -> Result<CompareProfile, String> {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    if let Some(requested) = query_value(params, "profile") {
        let id = ProfileId::new(requested.to_owned())
            .map_err(|err| format!("invalid profile id '{requested}': {err}"))?;
        if let Some(p) = find_builtin(&id) {
            return Ok(p);
        }
        if let Ok(p) = store.load(&id) {
            return Ok(p);
        }
        return Err(format!("profile '{id}' does not exist (built-in or user)"));
    }
    if let Ok(Some(active_id)) = store.load_active_pointer() {
        if let Some(p) = find_builtin(&active_id) {
            return Ok(p);
        }
        if let Ok(p) = store.load(&active_id) {
            return Ok(p);
        }
        // Active pointer references a profile that no longer exists.
        // Fall through to the built-in default rather than fail; the
        // user may have removed a custom profile while it was still
        // selected. Logged so the GUI / CLI can surface a one-shot
        // notification later.
        eprintln!(
            "warning: active profile '{active_id}' no longer exists; using built-in 'default'"
        );
    }
    Ok(builtin_profiles()
        .into_iter()
        .next()
        .expect("at least one built-in profile is registered"))
}

/// Detect and clear a stale active-profile pointer once at startup.
///
/// If the active pointer references a profile that no longer exists (e.g. a
/// user profile deleted while it was selected), remove the pointer file so the
/// per-request resolver falls back to `default` cleanly — without emitting the
/// "active profile … no longer exists" warning on every request. Built-in ids
/// and live user profiles are left untouched. Returns `true` when a stale
/// pointer was cleared.
fn cleanup_stale_active_pointer(paths: &AppPaths) -> bool {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let Ok(Some(active_id)) = store.load_active_pointer() else {
        return false;
    };
    if find_builtin(&active_id).is_some() || store.load(&active_id).is_ok() {
        return false;
    }
    match store.clear_active_pointer() {
        Ok(()) => {
            eprintln!(
                "notice: cleared stale active profile pointer '{active_id}' (profile no longer exists); using built-in 'default'"
            );
            true
        }
        Err(err) => {
            eprintln!("warning: failed to clear stale active profile pointer '{active_id}': {err}");
            false
        }
    }
}

/// Minimum age before an unreferenced archive-edit staging dir or portal
/// backup is considered orphaned. Generous so an edit left open across a
/// long external-editor session (or owned by a concurrently running
/// instance) is never reclaimed out from under the user.
const ARCHIVE_EDIT_ORPHAN_MAX_AGE: std::time::Duration =
    std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// Remove archive-edit staging dirs (`cache_dir/archive-edits/<token>`) and
/// portal backups (`state_dir/archive-edit/<token>.bak`) older than
/// [`ARCHIVE_EDIT_ORPHAN_MAX_AGE`]. Edit tokens live only in process memory,
/// so entries from previous runs can never be committed or discarded again —
/// without this sweep a crash mid-edit leaks them forever.
fn sweep_orphaned_archive_edits(paths: &AppPaths) {
    let is_orphaned = |path: &Path| {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|mtime| mtime.elapsed().ok())
            .is_some_and(|age| age > ARCHIVE_EDIT_ORPHAN_MAX_AGE)
    };
    if let Ok(entries) = fs::read_dir(paths.cache_dir.join("archive-edits")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_orphaned(&path) {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
    if let Ok(entries) = fs::read_dir(paths.state_dir.join("archive-edit")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "bak") && is_orphaned(&path) {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// Build the `TextCompareOptions` for a single bridge request. Starts
/// from the resolved profile's text options, then applies per-request
/// query overrides (`ignore_case`, `ignore_whitespace`,
/// `ignore_blank_lines`, `ignore_eol`, `detect_moves`). Per the Phase 1
/// contract, an explicit `?ignore_case=true` always wins over the
/// profile's value; an absent flag leaves the profile value unchanged.
///
/// Returns `Err` when `?profile=` references an unknown id so the caller
/// can return 400 Bad Request rather than silently fall through.
fn resolve_text_options_for_request(
    paths: &AppPaths,
    params: &[(String, String)],
) -> Result<TextCompareOptions, String> {
    let profile = resolve_profile_for_request(paths, params)?;
    let mut opts = profile.text;
    apply_text_query_overrides(&mut opts, params)?;
    Ok(opts)
}

fn apply_text_query_overrides(
    opts: &mut TextCompareOptions,
    params: &[(String, String)],
) -> Result<(), String> {
    if let Some(v) = query_value(params, "ignore_case")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.ignore_case = parsed;
    }
    if let Some(v) = query_value(params, "ignore_whitespace")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.ignore_whitespace = parsed;
    }
    if let Some(v) = query_value(params, "ignore_blank_lines")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.ignore_blank_lines = parsed;
    }
    if let Some(v) = query_value(params, "ignore_eol")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.ignore_eol = parsed;
    }
    if let Some(v) = query_value(params, "detect_moves")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.detect_moves = parsed;
    }
    if let Some(v) = query_value(params, "diff_algorithm") {
        opts.diff_algorithm = match v {
            "lcs" => linsync_core::DiffAlgorithm::Lcs,
            "patience" => linsync_core::DiffAlgorithm::Patience,
            "myers" => linsync_core::DiffAlgorithm::Myers,
            _ => return Err(format!("unknown diff_algorithm '{v}'")),
        };
    }
    if let Some(v) = query_value(params, "inline_granularity") {
        opts.inline_granularity = match v {
            "char" => linsync_core::InlineGranularity::Char,
            "word" => linsync_core::InlineGranularity::Word,
            "grapheme" => linsync_core::InlineGranularity::Grapheme,
            _ => return Err(format!("unknown inline_granularity '{v}'")),
        };
    }
    for value in params
        .iter()
        .filter(|(key, _)| key == "regex_rule_set")
        .map(|(_, value)| value)
    {
        opts.regex_rule_sets.push(value.clone());
    }
    if let Some(v) = query_value(params, "context_lines") {
        opts.context_lines = Some(
            v.parse::<usize>()
                .map_err(|_| format!("invalid context_lines '{v}'"))?,
        );
    }
    if let Some(v) = query_value(params, "show_only_changes")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.show_only_changes = parsed;
    }
    if let Some(v) = query_value(params, "render_mode") {
        opts.render_mode = parse_text_render_mode_query(v)?;
    }
    if let Some(v) = query_value(params, "syntax") {
        opts.syntax_mode = parse_text_syntax_mode_query(v)?;
    }
    if let Some(v) = query_value(params, "encoding") {
        opts.encoding = parse_text_encoding_query(v)?;
    }
    if let Some(pattern) = query_value(params, "find") {
        opts.find = Some(TextFindOptions {
            pattern: pattern.to_owned(),
            regex: query_bool(params, "find_regex"),
            case_sensitive: query_bool(params, "find_case_sensitive"),
        });
    }
    for value in params
        .iter()
        .filter(|(key, _)| key == "bookmark")
        .map(|(_, value)| value)
    {
        opts.bookmarks.push(parse_text_bookmark_query(value)?);
    }
    opts.validate_rule_sets()
        .map_err(|err| format!("invalid text options: {err}"))?;
    opts.validate_regex_options()
        .map_err(|err| format!("invalid text regex option: {err}"))?;
    Ok(())
}

fn parse_bool_query_param(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_text_render_mode_query(value: &str) -> Result<TextRenderMode, String> {
    match value {
        "side-by-side" | "side_by_side" | "side" => Ok(TextRenderMode::SideBySide),
        "unified" => Ok(TextRenderMode::Unified),
        "context" => Ok(TextRenderMode::Context),
        "normal" => Ok(TextRenderMode::Normal),
        "html" => Ok(TextRenderMode::Html),
        _ => Err(format!("unknown render_mode '{value}'")),
    }
}

fn parse_text_syntax_mode_query(value: &str) -> Result<TextSyntaxMode, String> {
    // Token set lives in core (`TextSyntaxMode: FromStr`), shared with the
    // CLI's `--syntax` — same precedent as `FolderGrouping` / `group_by=`.
    value.parse()
}

fn parse_text_encoding_query(value: &str) -> Result<TextInputEncoding, String> {
    match value {
        "auto" => Ok(TextInputEncoding::Auto),
        "utf8" | "utf-8" => Ok(TextInputEncoding::Utf8),
        "utf8-bom" | "utf-8-bom" => Ok(TextInputEncoding::Utf8Bom),
        "utf16le" | "utf-16le" | "utf-16-le" => Ok(TextInputEncoding::Utf16Le),
        "utf16be" | "utf-16be" | "utf-16-be" => Ok(TextInputEncoding::Utf16Be),
        "lossy-utf8" | "lossy-utf-8" => Ok(TextInputEncoding::LossyUtf8),
        _ => Err(format!("unknown encoding '{value}'")),
    }
}

fn parse_text_bookmark_query(value: &str) -> Result<TextBookmark, String> {
    let mut parts = value.splitn(3, ':');
    let side = match parts.next().unwrap_or_default() {
        "left" | "l" => CompareSide::Left,
        "right" | "r" => CompareSide::Right,
        other => {
            return Err(format!(
                "bookmark side '{other}' must be left or right; expected SIDE:LINE[:LABEL]"
            ));
        }
    };
    let Some(line_raw) = parts.next() else {
        return Err("bookmark requires SIDE:LINE[:LABEL]".to_owned());
    };
    let line = line_raw
        .parse::<usize>()
        .map_err(|_| "bookmark line must be a positive integer".to_owned())?;
    if line == 0 {
        return Err("bookmark line must be a positive integer".to_owned());
    }
    let label = parts.next().unwrap_or_default().to_owned();
    Ok(TextBookmark { side, line, label })
}

/// Resolve `FolderCompareOptions` for a single bridge request: start
/// from the active profile's folder options, then apply per-request
/// query overrides (`?recursive`, `?compare_method`, `?symlink_policy`,
/// `?include_skipped`).
/// Returns `Err` when `?profile=` references an unknown id.
fn resolve_folder_options_for_request(
    paths: &AppPaths,
    params: &[(String, String)],
) -> Result<FolderCompareOptions, String> {
    let profile = resolve_profile_for_request(paths, params)?;
    let mut opts = profile.folder;
    if let Some(v) = query_value(params, "recursive")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.recursive = parsed;
    }
    if let Some(v) = query_value(params, "compare_method") {
        opts.compare_method = match v {
            "full-contents" => linsync_core::CompareMethod::FullContents,
            "quick-contents" => linsync_core::CompareMethod::QuickContents,
            "binary-contents" => linsync_core::CompareMethod::BinaryContents,
            "modified-date" => linsync_core::CompareMethod::ModifiedDate,
            "date-size" => linsync_core::CompareMethod::DateAndSize,
            "size" => linsync_core::CompareMethod::Size,
            "existence" => linsync_core::CompareMethod::Existence,
            "hash-blake3" => linsync_core::CompareMethod::HashBlake3,
            "normalized-text" => linsync_core::CompareMethod::NormalizedText,
            _ => return Err(format!("unknown compare_method '{v}'")),
        };
    }
    if let Some(v) = query_value(params, "symlink_policy") {
        opts.symlink_policy = match v {
            "compare-target" => linsync_core::SymlinkPolicy::CompareTarget,
            "follow" => linsync_core::SymlinkPolicy::Follow,
            "special-file" => linsync_core::SymlinkPolicy::SpecialFile,
            _ => return Err(format!("unknown symlink_policy '{v}'")),
        };
    }
    if let Some(v) = query_value(params, "include_skipped")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        opts.include_skipped = parsed;
    }
    Ok(opts)
}

fn resolve_compare_options_for_request(
    paths: &AppPaths,
    params: &[(String, String)],
) -> Result<GuiCompareOptions, String> {
    let profile = resolve_profile_for_request(paths, params)?;

    let mut text = profile.text;
    apply_text_query_overrides(&mut text, params)?;

    let mut folder = profile.folder;
    if let Some(v) = query_value(params, "recursive")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        folder.recursive = parsed;
    }
    if let Some(v) = query_value(params, "compare_method") {
        folder.compare_method = match v {
            "full-contents" => linsync_core::CompareMethod::FullContents,
            "quick-contents" => linsync_core::CompareMethod::QuickContents,
            "binary-contents" => linsync_core::CompareMethod::BinaryContents,
            "modified-date" => linsync_core::CompareMethod::ModifiedDate,
            "date-size" => linsync_core::CompareMethod::DateAndSize,
            "size" => linsync_core::CompareMethod::Size,
            "existence" => linsync_core::CompareMethod::Existence,
            "hash-blake3" => linsync_core::CompareMethod::HashBlake3,
            "normalized-text" => linsync_core::CompareMethod::NormalizedText,
            _ => folder.compare_method,
        };
    }
    if let Some(v) = query_value(params, "symlink_policy") {
        folder.symlink_policy = match v {
            "compare-target" => linsync_core::SymlinkPolicy::CompareTarget,
            "follow" => linsync_core::SymlinkPolicy::Follow,
            "special-file" => linsync_core::SymlinkPolicy::SpecialFile,
            _ => folder.symlink_policy,
        };
    }
    if let Some(v) = query_value(params, "include_skipped")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        folder.include_skipped = parsed;
    }

    let mut document = profile.document;
    apply_document_query_overrides(&mut document, params)?;

    let mut table = profile.table;
    if let Some(v) = query_value(params, "delimiter") {
        table.delimiter = match v {
            "tab" | "\\t" => '\t',
            s => s.chars().next().unwrap_or(table.delimiter),
        };
    }
    if let Some(v) = query_value(params, "has_header")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        table.has_header = parsed;
    }
    if let Some(v) = query_value(params, "key_columns") {
        table.key_columns = v
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect();
    }
    if let Some(v) = query_value(params, "ignore_columns") {
        table.ignore_columns = v
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect();
    }
    if let Some(v) = query_value(params, "numeric_tolerance")
        && let Ok(n) = v.parse::<f64>()
    {
        table.numeric_tolerance = Some(n);
    }
    if let Some(v) = query_value(params, "ignore_row_order")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        table.ignore_row_order = parsed;
    }

    let mut binary = profile.binary;
    if let Some(v) = query_value(params, "bytes_per_row")
        && let Ok(n) = v.parse::<usize>()
        && n > 0
    {
        binary.bytes_per_row = n;
    }
    if let Some(v) = query_value(params, "compare_content")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        binary.compare_content = parsed;
    }
    if let Some(v) = query_value(params, "compare_metadata")
        && let Some(parsed) = parse_bool_query_param(v)
    {
        binary.compare_metadata = parsed;
    }

    Ok(GuiCompareOptions {
        text,
        folder,
        table,
        binary,
        image: profile.image,
        document,
    })
}

fn apply_document_query_overrides(
    opts: &mut DocumentCompareOptions,
    params: &[(String, String)],
) -> Result<(), String> {
    if let Some(v) = query_value(params, "mode") {
        opts.mode = match v {
            "Document" | "document" => opts.mode,
            "text" => DocumentCompareMode::Text,
            "ocr_text" | "ocr-text" => DocumentCompareMode::OcrText,
            "rendered" => DocumentCompareMode::Rendered,
            _ => opts.mode,
        };
    }
    if let Some(v) = query_value(params, "ocr_language") {
        opts.ocr_language = v.to_owned();
    }
    if let Some(v) = query_value(params, "document_timeout") {
        opts.timeout_secs = v
            .parse::<u64>()
            .map_err(|_| format!("invalid document_timeout '{v}'"))?;
    }
    Ok(())
}

fn compare_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(left) = query_value(&params, "left") else {
        return bridge_error(400, "Bad Request", "missing left path");
    };
    let Some(right) = query_value(&params, "right") else {
        return bridge_error(400, "Bad Request", "missing right path");
    };

    let options = match resolve_compare_options_for_request(paths, &params) {
        Ok(opts) => opts,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let new_tab = query_bool(&params, "new_tab");

    // Archive-as-folder: with no explicit mode (or an explicit "Archive"), if the
    // two inputs are an archive pair with an enabled unpacker, compare them as a
    // folder of their unpacked contents (nested archives recurse one level).
    // Any other explicit mode (Hex, Text, …) overrides this auto-routing.
    let requested_mode = query_value(&params, "mode")
        .map(str::trim)
        .filter(|m| !m.is_empty());
    if matches!(requested_mode, None | Some("Archive"))
        && let Some(plugin) = archive_pair_unpacker(Path::new(left), Path::new(right), paths)
    {
        let tab = archive_tab(left.to_owned(), right.to_owned(), &plugin, &options);
        let context = match state.lock() {
            Ok(mut state) => state.apply_compare(tab, new_tab),
            Err(_) => {
                return bridge_error(500, "Internal Server Error", "session state unavailable");
            }
        };
        record_recent_context(paths, &context);
        return match context_to_json(&context) {
            Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
            Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
        };
    }

    // Built-in archive-as-folder: if both inputs are supported archive formats
    // and no plugin took precedence, extract and compare as folders.
    if matches!(requested_mode, None | Some("Archive"))
        && linsync_core::is_builtin_archive_format(Path::new(left))
        && linsync_core::is_builtin_archive_format(Path::new(right))
    {
        let (tab, dirs) = builtin_archive_tab(
            Path::new(left),
            Path::new(right),
            left.to_owned(),
            right.to_owned(),
            &options,
            paths,
        );
        let Some(tab) = tab else {
            return bridge_error(
                500,
                "Internal Server Error",
                "archive compare produced no tab",
            );
        };
        let context = match state.lock() {
            Ok(mut state) => {
                let context = state.apply_compare(tab, new_tab);
                let tab_id = context.session.active_tab_id;
                if let Some(old) = state.archive_extract_dirs.remove(&tab_id) {
                    let _ = fs::remove_dir_all(old);
                }
                if let Some(dir) = dirs.into_iter().next() {
                    state.archive_extract_dirs.insert(tab_id, dir);
                }
                context
            }
            Err(_) => {
                return bridge_error(500, "Internal Server Error", "session state unavailable");
            }
        };
        record_recent_context(paths, &context);
        return match context_to_json(&context) {
            Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
            Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
        };
    }

    // Optional cancellation: when the QML supplies `?request_id=X`, register a
    // cancel flag so a concurrent `/cancel?id=X` can abort this compare. The
    // flag is registered/removed under the state lock, but the long compare
    // below runs WITHOUT holding the lock, so `/cancel` is never blocked by it.
    let (request_id, progress) =
        register_progress_request(&params, state, "starting", 0, "Starting compare");
    let should_cancel: Box<dyn Fn() -> bool> = if let Some(id) = &request_id {
        let flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut state) = state.lock() {
            state.compare_cancels.insert(id.clone(), Arc::clone(&flag));
        }
        Box::new(move || flag.load(Ordering::Relaxed))
    } else {
        Box::new(|| false)
    };

    let maybe_tab = build_tab_for_paths_with_mode_cancellable_and_artifacts(
        Path::new(left),
        Path::new(right),
        query_value(&params, "mode"),
        &options,
        &*should_cancel,
        progress,
    );

    if let Some(id) = &request_id
        && let Ok(mut state) = state.lock()
    {
        state.compare_cancels.remove(id);
    }
    remove_progress_request(request_id.as_deref(), state);

    let Some((tab, artifact_dirs)) = maybe_tab else {
        // The compare was cancelled — leave the session state untouched.
        return http_response(
            200,
            "OK",
            "application/json",
            br#"{"cancelled":true}"#.to_vec(),
        );
    };

    let context = match state.lock() {
        Ok(mut state) => {
            let context = state.apply_compare(tab, new_tab);
            let tab_id = context.session.active_tab_id;
            if let Some(old) = state.rendered_page_cache_dirs.remove(&tab_id) {
                let _ = fs::remove_dir_all(old);
            }
            if let Some(dir) = artifact_dirs.into_iter().next() {
                state.rendered_page_cache_dirs.insert(tab_id, dir);
            }
            context
        }
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    record_recent_context(paths, &context);
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Handle `/cancel?id=X` — flip the cancel flag for the in-flight `/compare`
/// request that registered `request_id == X`. Returns `{"cancelled":true}` if a
/// matching request was found, `{"cancelled":false}` otherwise (already
/// finished or unknown id). Always 200 so the QML treats it as best-effort.
fn cancel_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id");
    };
    let cancelled = match state.lock() {
        Ok(state) => state
            .compare_cancels
            .get(id)
            .map(|flag| {
                flag.store(true, Ordering::Relaxed);
                true
            })
            .unwrap_or(false),
        Err(_) => false,
    };
    http_response(
        200,
        "OK",
        "application/json",
        format!(r#"{{"cancelled":{cancelled}}}"#).into_bytes(),
    )
}

fn progress_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing id");
    };
    let progress_json = match state.lock() {
        Ok(state) => state
            .compare_progress
            .get(id)
            .map(|p| {
                let prog = p.lock().ok();
                match &prog {
                    Some(prog) => serde_json::json!({
                        "phase": prog.phase,
                        "current": prog.current,
                        "total": prog.total,
                        "message": prog.message,
                    }),
                    None => {
                        serde_json::json!({"phase":"unknown","current":0,"total":0,"message":""})
                    }
                }
            })
            .unwrap_or_else(
                || serde_json::json!({"phase":"none","current":0,"total":0,"message":""}),
            ),
        Err(_) => serde_json::json!({"phase":"error","current":0,"total":0,"message":""}),
    };
    http_response(
        200,
        "OK",
        "application/json",
        serde_json::to_string(&progress_json)
            .unwrap_or_else(|_| r#"{"phase":"error"}"#.to_owned())
            .into_bytes(),
    )
}
/// Handle `/raw-compare?left_text=...&right_text=...&left_name=...&right_name=...&mode=...`
///
/// Compares raw text strings directly without requiring files on disk.
/// Writes temp files for the full pipeline to consume so the UX (tabs,
/// undo, save, etc.) works identically to file-based compares.
fn raw_compare_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let left_text = match query_value(&params, "left_text") {
        Some(v) => percent_decode(v),
        None => return bridge_error(400, "Bad Request", "missing left_text"),
    };
    let right_text = match query_value(&params, "right_text") {
        Some(v) => percent_decode(v),
        None => return bridge_error(400, "Bad Request", "missing right_text"),
    };
    let left_name = query_value(&params, "left_name").unwrap_or("Left");
    let right_name = query_value(&params, "right_name").unwrap_or("Right");
    let new_tab = query_bool(&params, "new_tab");

    let text_options = match resolve_text_options_for_request(paths, &params) {
        Ok(opts) => opts,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };

    // Use linsync-core's compare_text which accepts raw &str
    let result = compare_text(
        left_name,
        &left_text,
        right_name,
        &right_text,
        &text_options,
    );
    let (left_rows, right_rows) = text_rows_for_gui_with_options(&result, &text_options);

    let tab = GuiCompareTab {
        id: 1,
        title: "Text: raw text compare".to_owned(),
        mode: "Text".to_owned(),
        left_path: format!("📄 {left_name}"),
        right_path: format!("📄 {right_name}"),
        base_path: None,
        status: "Text compare complete".to_owned(),
        difference_count: result.summary.differences,
        left_dirty: false,
        right_dirty: false,
        can_undo: false,
        can_redo: false,
        validation: GuiOpenValidation {
            compatible: true,
            path_kind: "RawText".to_owned(),
            message: "Compared pasted text".to_owned(),
        },
        summary: vec![
            summary_item("Diff blocks", result.summary.diff_blocks),
            summary_item("Changed lines", result.summary.changed_lines),
            summary_item("Left-only lines", result.summary.left_only_lines),
            summary_item("Right-only lines", result.summary.right_only_lines),
        ],
        left_rows,
        right_rows,
        total_rows: None,
        diff_row_indexes: Vec::new(),
        search_row_indexes: Vec::new(),
        folder_entries: vec![],
        folder_total: None,
        encoding_metadata: Some(result.encoding_summary()),
        table_headers: None,
        table_cells: None,
        artifacts: Vec::new(),
        rendered_pages: None,
        options: Some(GuiCompareOptions {
            text: text_options.clone(),
            ..GuiCompareOptions::default()
        }),
    };

    let context = match state.lock() {
        Ok(mut state) => state.apply_compare(tab, new_tab),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };

    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn copy_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(row) = query_value(&params, "row").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing row");
    };
    let Some(direction) = query_value(&params, "direction") else {
        return bridge_error(400, "Bad Request", "missing direction");
    };

    let context = match state.lock() {
        Ok(mut state) => match state.copy_row(row, direction) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn copy_all_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(direction) = query_value(&params, "direction") else {
        return bridge_error(400, "Bad Request", "missing direction");
    };

    let context = match state.lock() {
        Ok(mut state) => match state.copy_all(direction) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn undo_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let context = match state.lock() {
        Ok(mut state) => match state.undo() {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn redo_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let context = match state.lock() {
        Ok(mut state) => match state.redo() {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn save_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(side) = query_value(&params, "side") else {
        return bridge_error(400, "Bad Request", "missing side");
    };

    let context = match state.lock() {
        Ok(mut state) => match state.save_side(side) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Validate that `path` is one of the active tab's compare paths (left, right,
/// or base). Prevents arbitrary file access via the inline editor endpoints.
fn path_is_active_tab_path(path: &str, state: &Arc<Mutex<GuiBridgeState>>) -> bool {
    let Ok(s) = state.lock() else {
        return false;
    };
    let Some(tab) = s
        .session
        .tabs
        .iter()
        .find(|t| t.id == s.session.active_tab_id)
    else {
        return false;
    };
    tab.left_path == path || tab.right_path == path || tab.base_path.as_deref() == Some(path)
}

/// Read raw text content from a file path. Used by the GUI inline editor.
fn file_read_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    if !path_is_active_tab_path(path, state) {
        return bridge_error(403, "Forbidden", "path is not an active compare path");
    }
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let body = serde_json::json!({ "ok": true, "content": content }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &format!("read failed: {err}")),
    }
}

/// Write raw text content to a file path. Used by the GUI inline editor.
fn file_write_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    if !path_is_active_tab_path(path, state) {
        return bridge_error(403, "Forbidden", "path is not an active compare path");
    }
    let Some(content) = query_value(&params, "content") else {
        return bridge_error(400, "Bad Request", "missing content");
    };
    match std::fs::write(path, content) {
        Ok(()) => http_response(200, "OK", "application/json", br#"{"ok":true}"#.to_vec()),
        Err(err) => bridge_error(
            500,
            "Internal Server Error",
            &format!("write failed: {err}"),
        ),
    }
}

fn activate_tab_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id").and_then(|value| value.parse::<u64>().ok()) else {
        return bridge_error(400, "Bad Request", "missing tab id");
    };

    let context = match state.lock() {
        Ok(mut state) => match state.activate_tab(id) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn close_tab_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id").and_then(|value| value.parse::<u64>().ok()) else {
        return bridge_error(400, "Bad Request", "missing tab id");
    };

    let context = match state.lock() {
        Ok(mut state) => state.close_tab(id),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn bookmark_set_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(row) = query_value(&params, "row").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing bookmark row");
    };
    let bookmarked = query_value(&params, "bookmarked")
        .and_then(parse_bool_query_param)
        .unwrap_or(true);

    let context = match state.lock() {
        Ok(mut state) => match state.set_bookmark(row, bookmarked) {
            Ok(context) => context,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Build a core `FolderQuery` from `/folder/query` request parameters
/// (`search`, `types`, `offset`, `limit`).
fn folder_query_from_params(params: &[(String, String)]) -> linsync_core::FolderQuery {
    let mut query = linsync_core::FolderQuery::default();
    if let Some(search) = query_value(params, "search")
        && !search.is_empty()
    {
        query.search = Some(search.to_owned());
    }
    if let Some(types) = query_value(params, "types") {
        let mut filter = linsync_core::FolderTypeFilter {
            files: false,
            directories: false,
            symlinks: false,
            special: false,
        };
        for token in types.split(',') {
            match token.trim() {
                "file" | "files" => filter.files = true,
                "dir" | "directory" | "directories" => filter.directories = true,
                "symlink" | "symlinks" | "link" => filter.symlinks = true,
                "special" => filter.special = true,
                _ => {}
            }
        }
        if filter.files || filter.directories || filter.symlinks || filter.special {
            query.types = filter;
        }
    }
    if let Some(state) = query_value(params, "state") {
        use linsync_core::FolderEntryFilter;
        query.state = match state {
            "changed" | "differences" | "diff" => FolderEntryFilter::Differences,
            "left_only" => FolderEntryFilter::LeftOnly,
            "right_only" => FolderEntryFilter::RightOnly,
            "identical" | "equal" => FolderEntryFilter::Identical,
            "different" => FolderEntryFilter::Different,
            "errors" => FolderEntryFilter::Errors,
            // "all" / "" / anything else keeps the default (everything).
            _ => FolderEntryFilter::All,
        };
    }
    if let Some(sort) = query_value(params, "sort") {
        use linsync_core::FolderSortKey;
        query.sort = match sort {
            "name" => FolderSortKey::Name,
            "state" => FolderSortKey::State,
            "type" => FolderSortKey::Type,
            // The GUI's left/right size columns both map to the core's
            // "larger of the two sides" size key.
            "size" | "leftSize" | "rightSize" => FolderSortKey::Size,
            "modified" => FolderSortKey::Modified,
            // "path" / anything else keeps the default (relative path).
            _ => FolderSortKey::Path,
        };
    }
    if let Some(descending) = query_value(params, "descending").and_then(parse_bool_query_param) {
        query.descending = descending;
    }
    if let Some(group_by) = query_value(params, "group_by") {
        // Shared parser with the CLI (core's FromStr); the bridge stays
        // lenient and treats unknown values as "no grouping".
        query.group_by = group_by.parse().unwrap_or_default();
    }
    if let Some(offset) = query_value(params, "offset").and_then(|v| v.parse::<usize>().ok()) {
        query.offset = offset;
    }
    if let Some(limit) = query_value(params, "limit").and_then(|v| v.parse::<usize>().ok()) {
        query.limit = Some(limit);
    }
    query
}

/// `/folder/query?left=&right=&search=&types=&offset=&limit=&state=&sort=&descending=&group_by=` — compare two
/// folders and return the entries filtered/paged through the core `FolderQuery`,
/// so the GUI folder table can search + type-filter + paginate via the core API.
fn folder_query_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let (Some(left), Some(right)) = (query_value(&params, "left"), query_value(&params, "right"))
    else {
        return bridge_error(400, "Bad Request", "missing left or right path");
    };
    let folder_options = match resolve_folder_options_for_request(paths, &params) {
        Ok(opts) => opts,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let result = match compare_folders(Path::new(left), Path::new(right), &folder_options) {
        Ok(result) => result,
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    let folder_query = folder_query_from_params(&params);
    let page = result.query(&folder_query);
    let filtered: Vec<FolderEntryDiff> = page
        .groups
        .iter()
        .flat_map(|group| group.entries.iter().map(|entry| (*entry).clone()))
        .collect();
    let entries = folder_entries_for_gui(&filtered);
    let body = serde_json::json!({
        "entries": entries,
        "totalMatched": page.total_matched,
        "offset": page.offset,
        "returned": page.returned,
        "hasMore": page.has_more,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Report compile-time capabilities so the QML can hide modes the binary can't
/// serve (e.g. webpage rendered/screenshot, which need the `web-engine` build).
///
/// `web_renderer` adds the runtime dimension: which backend rendered/screenshot
/// would actually use on this host — `"qml"` (Qt WebEngine), `"chromium"`
/// (headless Chromium fallback), or `"none"` (web-engine build but no usable
/// renderer binary, or a non-web-engine build).
fn capabilities_bridge_response() -> Vec<u8> {
    #[cfg(feature = "web-engine")]
    let web_renderer = linsync_core::active_renderer_kind();
    #[cfg(not(feature = "web-engine"))]
    let web_renderer = "none";
    let body = serde_json::json!({
        "web_engine": cfg!(feature = "web-engine"),
        "web_renderer": web_renderer,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Return a windowed slice of a text comparison
/// (`?left=&right=&offset=&limit=` + the same option params `/compare` accepts),
/// so the GUI can extend a large diff window-by-window instead of loading every
/// row into the view. The rows are built exactly as `/compare` builds them —
/// the same `left_rows`/`right_rows` split, honoring render mode / ignore flags
/// / syntax / find — so a fetched window appends seamlessly onto the first
/// window the compare response embedded. `totalRows`/`hasMore` drive paging.
fn text_window_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let (Some(left), Some(right)) = (query_value(&params, "left"), query_value(&params, "right"))
    else {
        return bridge_error(400, "Bad Request", "missing left or right path");
    };
    let offset = query_value(&params, "offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    // 0 (or absent) means "to the end".
    let limit = query_value(&params, "limit")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(usize::MAX);

    let options = match resolve_text_options_for_request(paths, &params) {
        Ok(options) => options,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let result = match linsync_core::compare_text_files(Path::new(left), Path::new(right), &options)
    {
        Ok(result) => result,
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    let (total_rows, left_window, right_window) =
        if options.render_mode == linsync_core::TextRenderMode::SideBySide {
            // Windowed core build: expensive per-row work (syntax highlighting,
            // find-match marking) runs only for the requested rows instead of
            // the whole file on every scroll fetch.
            let page = result.view_rows_window(&options, offset, limit);
            let (left, right) = gui_rows_from_view_rows(page.rows);
            (page.total_rows, left, right)
        } else {
            // Rendered (unified/context/normal) modes have no windowed core
            // equivalent; they do no per-row syntax work, so full-build-then-
            // slice stays cheap.
            let (left_rows, right_rows) = text_rows_for_gui_with_options(&result, &options);
            let total_rows = left_rows.len().max(right_rows.len());
            let end = offset.saturating_add(limit).min(total_rows);
            let window = |rows: Vec<GuiLineRow>| -> Vec<GuiLineRow> {
                if offset >= rows.len() {
                    Vec::new()
                } else {
                    rows[offset..end.min(rows.len())].to_vec()
                }
            };
            (total_rows, window(left_rows), window(right_rows))
        };
    let returned = left_window.len().max(right_window.len());
    let body = serde_json::json!({
        "totalRows": total_rows,
        "offset": offset,
        "returned": returned,
        "hasMore": offset + returned < total_rows,
        "left_rows": left_window,
        "right_rows": right_window,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Serve a window of table rows from the active table tab. The canonical tab
/// holds the full `table_cells`; this endpoint slices it without re-running the
/// compare so large tables page efficiently.
fn table_window_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let offset = query_value(&params, "offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query_value(&params, "limit")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(usize::MAX);

    let (total, rows) = match state.lock() {
        Ok(s) => {
            let Some(tab) = s
                .session
                .tabs
                .iter()
                .find(|t| t.id == s.session.active_tab_id)
            else {
                return bridge_error(404, "Not Found", "no active tab");
            };
            if tab.mode != "Table" {
                return bridge_error(400, "Bad Request", "active tab is not a table compare");
            }
            let Some(all_rows) = tab.table_cells.as_ref() else {
                return serde_json::json!({
                    "rows": [],
                    "offset": offset,
                    "limit": limit,
                    "total": 0,
                    "hasMore": false,
                })
                .to_string()
                .into_bytes();
            };
            let total = all_rows.len();
            let end = offset.saturating_add(limit).min(total);
            let window = all_rows
                .get(offset..end)
                .map(<[linsync_core::TableRowDiff]>::to_vec)
                .unwrap_or_default();
            (total, window)
        }
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let body = serde_json::json!({
        "rows": rows,
        "offset": offset,
        "limit": limit,
        "total": total,
        "hasMore": offset + rows.len() < total,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn folder_open_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let key = query_value(&params, "key").unwrap_or("config");
    let target = match key {
        "config" => paths.config_dir.clone(),
        "data" => paths.data_dir.clone(),
        "cache" => paths.cache_dir.clone(),
        "state" => paths.state_dir.clone(),
        "filters" => paths.filters_file(),
        "settings" => paths.settings_file(),
        other => {
            return bridge_error(400, "Bad Request", &format!("unknown folder key '{other}'"));
        }
    };

    if !target.exists()
        && let Some(parent) = target.parent()
        && parent != target
    {
        let _ = fs::create_dir_all(&target);
    }

    match open_with_xdg(&target) {
        Ok(_) => {
            let body =
                serde_json::json!({ "ok": true, "path": target.display().to_string() }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

fn open_with_xdg(target: &Path) -> Result<(), String> {
    let opener = env::var_os("LINSYNC_OPENER")
        .map(PathBuf::from)
        .or_else(|| find_command_in_path("xdg-open"));
    let opener = opener.ok_or_else(|| "could not find xdg-open; set LINSYNC_OPENER".to_owned())?;
    let mut command = Command::new(opener);
    command.arg(target);
    let status = command
        .status()
        .map_err(|err| format!("failed to launch opener: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("opener exited with status {status}"))
    }
}

fn reveal_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    let Some(path_str) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    let path = PathBuf::from(percent_decode(path_str));
    if !path.exists() {
        return bridge_error(
            404,
            "Not Found",
            &format!("path does not exist: {}", path.display()),
        );
    }
    let revealer = env::var_os("LINSYNC_REVEAL").map(PathBuf::from);
    let result = if let Some(ref cmd) = revealer {
        Command::new(cmd).arg(&path).status()
    } else {
        let fm1 = find_command_in_path("filemanager");
        if let Some(fm) = fm1 {
            Command::new(fm).arg(&path).status()
        } else {
            let parent = if path.is_dir() {
                path.clone()
            } else {
                path.parent().map(|p| p.to_owned()).unwrap_or(path.clone())
            };
            Command::new("xdg-open").arg(&parent).status()
        }
    };
    match result {
        Ok(status) if status.success() => {
            let body = serde_json::json!({"ok":true}).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Ok(status) => bridge_error(
            500,
            "Internal Server Error",
            &format!("revealer exited with status {status}"),
        ),
        Err(err) => bridge_error(
            500,
            "Internal Server Error",
            &format!("failed to launch revealer: {err}"),
        ),
    }
}

fn open_external_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    let Some(path_str) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    let path = PathBuf::from(percent_decode(path_str));
    if !path.exists() {
        return bridge_error(
            404,
            "Not Found",
            &format!("path does not exist: {}", path.display()),
        );
    }
    match open_with_xdg(&path) {
        Ok(_) => {
            let body = serde_json::json!({"ok":true}).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

fn copy_clipboard_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    let Some(text) = query_value(&params, "text") else {
        return bridge_error(400, "Bad Request", "missing text");
    };
    let text = percent_decode(text);
    let clipboard_cmd = if env::var_os("WAYLAND_DISPLAY").is_some() {
        find_command_in_path("wl-copy")
    } else {
        find_command_in_path("xclip").filter(|_| env::var_os("DISPLAY").is_some())
    };
    match clipboard_cmd {
        Some(cmd) => match Command::new(&cmd)
            .args(if cmd.file_name().map(|f| f == "xclip").unwrap_or(false) {
                vec!["-selection", "clipboard"]
            } else {
                vec![]
            })
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                match child.wait() {
                    Ok(status) if status.success() => {
                        let body = serde_json::json!({"ok":true}).to_string();
                        http_response(200, "OK", "application/json", body.into_bytes())
                    }
                    Ok(status) => bridge_error(
                        500,
                        "Internal Server Error",
                        &format!("clipboard command exited with {status}"),
                    ),
                    Err(err) => bridge_error(
                        500,
                        "Internal Server Error",
                        &format!("clipboard command wait failed: {err}"),
                    ),
                }
            }
            Err(err) => bridge_error(
                500,
                "Internal Server Error",
                &format!("failed to launch clipboard command: {err}"),
            ),
        },
        None => bridge_error(
            500,
            "Internal Server Error",
            "no clipboard command found (need xclip or wl-copy)",
        ),
    }
}

fn archive_member_edit_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(archive_str) = query_value(&params, "archive") else {
        return bridge_error(400, "Bad Request", "missing archive");
    };
    let Some(member_str) = query_value(&params, "member") else {
        return bridge_error(400, "Bad Request", "missing member");
    };
    let archive = PathBuf::from(percent_decode(archive_str));
    let member = percent_decode(member_str);

    if !archive.exists() {
        return bridge_error(
            404,
            "Not Found",
            &format!("archive does not exist: {}", archive.display()),
        );
    }

    let canonical = match archive.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return bridge_error(500, "Internal Server Error", &e.to_string());
        }
    };

    // Reject a second concurrent edit for the same archive.
    {
        let state_guard = match state.lock() {
            Ok(g) => g,
            Err(_) => return bridge_error(500, "Internal Server Error", "state lock poisoned"),
        };
        if state_guard
            .archive_edit_tokens
            .values()
            .any(|ctx| ctx.archive() == canonical)
        {
            return bridge_error(
                409,
                "Conflict",
                "an edit is already in progress for this archive",
            );
        }
    }

    // Generate the token first so the staging dir is unpredictable and unique.
    let token = match bridge_token() {
        Ok(t) => t,
        Err(e) => {
            return bridge_error(500, "Internal Server Error", &e.to_string());
        }
    };
    let staging_root = paths.cache_dir.join("archive-edits").join(&token);
    let portal_bak = paths
        .state_dir
        .join("archive-edit")
        .join(&token)
        .with_extension("bak");

    let ctx = match linsync_core::extract_member_for_edit(
        &archive,
        &member,
        &staging_root,
        Some(&portal_bak),
    ) {
        Ok(ctx) => ctx,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging_root);
            // Extraction may have written the portal backup (possibly
            // partially) before failing — reclaim it too.
            let _ = fs::remove_file(&portal_bak);
            return bridge_error(archive_write_error_status(&e), "Error", &e.to_string());
        }
    };

    // Re-check under the lock after extraction to close the race window.
    {
        let mut state_guard = match state.lock() {
            Ok(g) => g,
            Err(_) => {
                let _ = fs::remove_dir_all(&staging_root);
                let _ = fs::remove_file(&portal_bak);
                return bridge_error(500, "Internal Server Error", "state lock poisoned");
            }
        };
        if state_guard
            .archive_edit_tokens
            .values()
            .any(|ctx| ctx.archive() == canonical)
        {
            let _ = fs::remove_dir_all(&staging_root);
            let _ = fs::remove_file(&portal_bak);
            return bridge_error(
                409,
                "Conflict",
                "an edit is already in progress for this archive",
            );
        }
        let staged_path = ctx.staged_path().to_path_buf();
        let atomic = ctx.atomic();
        state_guard.archive_edit_tokens.insert(token.clone(), ctx);
        let body = serde_json::json!({
            "ok": true,
            "token": token,
            "staged_path": staged_path,
            "atomic": atomic,
        })
        .to_string();
        http_response(200, "OK", "application/json", body.into_bytes())
    }
}

/// Single source for the `ArchiveWriteError` → HTTP status contract
/// (documented on the error type in `linsync-core::archive_write`). Both the
/// edit and commit endpoints must serve the same status for the same failure.
fn archive_write_error_status(e: &linsync_core::ArchiveWriteError) -> u16 {
    match e {
        linsync_core::ArchiveWriteError::InvalidMemberName { .. }
        | linsync_core::ArchiveWriteError::MemberNameEncoding { .. }
        | linsync_core::ArchiveWriteError::NonRegularMember { .. }
        | linsync_core::ArchiveWriteError::NonRegularStagedFile { .. }
        | linsync_core::ArchiveWriteError::CapsExceeded { .. }
        | linsync_core::ArchiveWriteError::UnsupportedArchive { .. } => 400,
        linsync_core::ArchiveWriteError::ArchiveNotFound { .. }
        | linsync_core::ArchiveWriteError::MemberNotFound { .. } => 404,
        linsync_core::ArchiveWriteError::StaleArchive { .. }
        | linsync_core::ArchiveWriteError::LockContention { .. } => 409,
        _ => 500,
    }
}

fn archive_member_commit_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(token) = query_value(&params, "token") else {
        return bridge_error(400, "Bad Request", "missing token");
    };
    let keep_backup = query_bool(&params, "keep_backup");

    let ctx = {
        let mut state_guard = match state.lock() {
            Ok(g) => g,
            Err(_) => return bridge_error(500, "Internal Server Error", "state lock poisoned"),
        };
        match state_guard.archive_edit_tokens.remove(token) {
            Some(ctx) => ctx,
            None => return bridge_error(400, "Bad Request", "invalid or expired token"),
        }
    };

    let options = linsync_core::CommitOptions { keep_backup };
    match linsync_core::commit_member_edit(&ctx, &options) {
        Ok(outcome) => {
            let _ = fs::remove_dir_all(ctx.staging_root());
            let mut response = serde_json::json!({"ok": true});
            if let Some(bak) = &outcome.bak_path {
                response["bak_path"] = serde_json::json!(bak);
            }
            if let Some(warn) = &outcome.bak_cleanup_warning {
                response["bak_cleanup_warning"] = serde_json::json!(warn);
            }
            http_response(
                200,
                "OK",
                "application/json",
                response.to_string().into_bytes(),
            )
        }
        Err(e) => {
            // Staging holds the user's only copy of their edit — never delete
            // it on failure. Re-register the token so the edit stays owned:
            // the user can retry (meaningful for RenameFailed) or discard,
            // which cleans staging and the portal backup. The only case the
            // token is not re-registered is the rare race where another edit
            // for the same archive started during the unlocked commit; the
            // staged file is then orphaned but its path is reported below
            // (and the startup sweep eventually reclaims it).
            let is_retryable = matches!(e, linsync_core::ArchiveWriteError::RenameFailed { .. });
            let mut token_retained = false;
            if let Ok(mut state_guard) = state.lock() {
                let canonical = ctx.archive();
                if !state_guard
                    .archive_edit_tokens
                    .values()
                    .any(|c| c.archive() == canonical)
                {
                    state_guard
                        .archive_edit_tokens
                        .insert(token.to_owned(), ctx.clone());
                    token_retained = true;
                }
            }
            let mut error_body = serde_json::json!({
                "error": e.to_string(),
                "retryable": is_retryable,
                "staged_path": ctx.staged_path(),
                "token_retained": token_retained,
            });
            if let linsync_core::ArchiveWriteError::PortalReadOnly {
                backup: Some(backup),
                ..
            } = &e
            {
                error_body["backup_path"] = serde_json::json!(backup);
            }
            bridge_error_json(archive_write_error_status(&e), "Error", error_body)
        }
    }
}

fn archive_member_discard_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(token) = query_value(&params, "token") else {
        return bridge_error(400, "Bad Request", "missing token");
    };

    let (staging_root, portal_backup) = {
        let mut state_guard = match state.lock() {
            Ok(g) => g,
            Err(_) => return bridge_error(500, "Internal Server Error", "state lock poisoned"),
        };
        match state_guard.archive_edit_tokens.remove(token) {
            Some(ctx) => (
                ctx.staging_root().to_path_buf(),
                ctx.portal_backup().map(|p| p.to_path_buf()),
            ),
            None => return bridge_error(400, "Bad Request", "invalid or expired token"),
        }
    };

    let _ = fs::remove_dir_all(&staging_root);
    if let Some(bak) = portal_backup {
        let _ = fs::remove_file(&bak);
    }
    let body = serde_json::json!({"ok": true}).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn sessions_recent_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    let mut recent: RecentSessions = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("failed to load recent sessions: {err}"),
            );
        }
    };
    // Hide any leftover internal test-fixture sessions from the Sessions page list
    // (and from being re-opened). Prevents dev/smoke pollution from showing up.
    prune_internal_fixture_sessions(&mut recent);
    let entries: Vec<serde_json::Value> = recent
        .sessions
        .iter()
        .enumerate()
        .map(|(index, file)| {
            serde_json::json!({
                "index": index,
                "title": file.session.title,
                "left": file.session.left.display().to_string(),
                "right": file.session.right.display().to_string(),
                "mode": compare_view_mode_label(file.selected_view),
                "lastResult": file.last_result.as_ref().map(|r| serde_json::json!({
                    "equal": r.equal,
                    "differenceCount": r.difference_count,
                })),
            })
        })
        .collect();
    let body = serde_json::json!({ "sessions": entries }).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn sessions_reopen_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(index) = query_value(&params, "index").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing index");
    };
    let recent_store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    let mut recent = match recent_store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    prune_internal_fixture_sessions(&mut recent);
    let Some(session_file) = recent.sessions.get(index) else {
        return bridge_error(404, "Not Found", "recent session index out of range");
    };

    // The recent-sessions reopen flow has no per-request profile
    // selection. Resolve from the active profile and tolerate a
    // missing/invalid pointer by falling back to defaults; the session's own
    // saved text options still win (build_tab_for_session_file overlays them).
    let base = resolve_compare_options_for_request(paths, &[])
        .unwrap_or_else(|_| GuiCompareOptions::default());
    let multi_tab = restore_multi_tab_context(session_file);
    let single_tab = if multi_tab.is_none() {
        Some(build_tab_for_session_file(session_file, &base))
    } else {
        None
    };
    let context = match state.lock() {
        Ok(mut state) => match multi_tab {
            // A multi-tab workspace snapshot: re-add every saved tab to the
            // live session, then activate the tab that was active when the
            // workspace was recorded (ids are reassigned on insert).
            Some(snapshot) => {
                let snapshot_active_id = snapshot.session.active_tab_id;
                let mut mapped_active_id = None;
                for tab in snapshot.session.tabs {
                    let old_id = tab.id;
                    let inserted = state.apply_compare(tab, true);
                    if old_id == snapshot_active_id {
                        mapped_active_id = Some(inserted.session.active_tab_id);
                    }
                }
                match mapped_active_id {
                    Some(id) => state.activate_tab(id).unwrap_or_else(|_| state.context()),
                    None => state.context(),
                }
            }
            None => {
                state.apply_compare(single_tab.expect("single tab built when no snapshot"), true)
            }
        },
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    record_recent_context(paths, &context);
    match context_to_json(&context) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err),
    }
}

/// `/sessions/delete?index=X` — remove a recent session by its index.
fn sessions_delete_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(index) = query_value(&params, "index").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing index");
    };
    let store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    let mut recent = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    // Prune fixture entries first: the index the Sessions page sends counts
    // within the pruned list (/sessions/recent), not the raw on-disk one.
    prune_internal_fixture_sessions(&mut recent);
    if index >= recent.sessions.len() {
        return bridge_error(404, "Not Found", "session index out of range");
    }
    recent.sessions.remove(index);
    if let Err(err) = store.save(&recent) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({"ok": true, "removed": index}).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// `/sessions/rename?index=X&title=Y` — rename a recent session.
fn sessions_rename_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(index) = query_value(&params, "index").and_then(|value| value.parse::<usize>().ok())
    else {
        return bridge_error(400, "Bad Request", "missing index");
    };
    let Some(title) = query_value(&params, "title") else {
        return bridge_error(400, "Bad Request", "missing title");
    };
    let store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    let mut recent = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    // Prune fixture entries first: the index the Sessions page sends counts
    // within the pruned list (/sessions/recent), not the raw on-disk one.
    prune_internal_fixture_sessions(&mut recent);
    let Some(session) = recent.sessions.get_mut(index) else {
        return bridge_error(404, "Not Found", "session index out of range");
    };
    session.session.title = title.to_owned();
    if let Err(err) = store.save(&recent) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let body = serde_json::json!({"ok": true, "index": index, "title": title}).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Save the currently open tabs as a named project file at `?path=` (with an
/// optional `?name=`). Each persistable tab becomes a `SessionFile` entry.
fn project_save_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
    paths: &AppPaths,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing project path");
    };
    let name = query_value(&params, "name").unwrap_or("Untitled project");

    let (sessions, active_index) = match state.lock() {
        Ok(s) => {
            let sessions: Vec<linsync_core::SessionFile> = s
                .session
                .tabs
                .iter()
                .filter(|tab| tab_has_persistable_paths(tab))
                .map(session_file_from_tab)
                .collect();
            let active_index = s
                .session
                .tabs
                .iter()
                .filter(|tab| tab_has_persistable_paths(tab))
                .position(|tab| tab.id == s.session.active_tab_id);
            (sessions, active_index)
        }
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    if sessions.is_empty() {
        return bridge_error(404, "Not Found", "no comparable tabs to save");
    }

    let mut project = linsync_core::ProjectFile::new(name);
    project.active_session_index = active_index;
    project.sessions = sessions;

    // Store paths relative to the project file's directory when they live under
    // it, so the project travels with its folder; load() resolves them back.
    if let Some(base) = Path::new(path).parent() {
        for session in &mut project.sessions {
            linsync_core::relativize_session_paths_against(&mut session.session, base);
        }
    }

    match linsync_core::ProjectFileStore::new(PathBuf::from(path)).save(&project) {
        Ok(()) => {
            record_recent_project(paths, path);
            let body = serde_json::json!({
                "ok": true,
                "name": name,
                "sessions": project.sessions.len(),
                "path": path,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Record a project file path in the recent-workspaces list (best-effort).
fn record_recent_project(paths: &AppPaths, path: &str) {
    let store = RecentPathStore::new(paths.recent_projects_file(), recent_limit(paths));
    if let Err(err) = store.add(PathBuf::from(path)) {
        tracing::warn!(path, error = %err, "failed to record recent project");
    }
}

/// List recent project files (most-recent first), skipping any that no longer
/// exist on disk.
fn project_recent_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store = RecentPathStore::new(paths.recent_projects_file(), recent_limit(paths));
    let recent = store.load_or_default().unwrap_or_default();
    let projects: Vec<serde_json::Value> = recent
        .paths
        .iter()
        .filter(|p| p.exists())
        .map(|p| {
            serde_json::json!({
                "path": p.display().to_string(),
                "name": p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            })
        })
        .collect();
    let body = serde_json::json!({ "projects": projects }).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Open a project file at `?path=` and return it as a launch-context JSON the
/// QML can apply (`applySessionContextJson`): one tab per saved session.
fn project_open_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing project path");
    };
    let project = match linsync_core::ProjectFileStore::new(PathBuf::from(path)).load() {
        Ok(p) => p,
        Err(err) => return bridge_error(400, "Bad Request", &err.to_string()),
    };
    if project.sessions.is_empty() {
        return bridge_error(404, "Not Found", "project has no comparisons");
    }
    record_recent_project(paths, path);

    let base = resolve_compare_options_for_request(paths, &[])
        .unwrap_or_else(|_| GuiCompareOptions::default());
    let mut tabs: Vec<GuiCompareTab> = Vec::with_capacity(project.sessions.len());
    for (index, session) in project.sessions.iter().enumerate() {
        let mut tab = build_tab_for_session_file(session, &base);
        tab.id = (index as u64) + 1;
        tabs.push(tab);
    }
    let active_tab_id = (project.active_session_index.unwrap_or(0) as u64) + 1;
    let context = GuiLaunchContext::from_tabs(tabs, active_tab_id);

    match serde_json::to_value(&context) {
        Ok(mut value) => {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("ok".to_owned(), serde_json::json!(true));
                obj.insert("name".to_owned(), serde_json::json!(project.name));
            }
            http_response(
                200,
                "OK",
                "application/json",
                value.to_string().into_bytes(),
            )
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn report_bridge_response(
    query: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
    paths: &AppPaths,
) -> Vec<u8> {
    let params = query_params(query);
    let format = query_value(&params, "format").unwrap_or("json");
    let tab = match state.lock() {
        Ok(s) => s
            .session
            .tabs
            .iter()
            .find(|t| t.id == s.session.active_tab_id)
            .cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let Some(tab) = tab else {
        return bridge_error(404, "Not Found", "no active tab");
    };
    match format {
        "summary" => {
            let mut blocks: Vec<serde_json::Value> = Vec::new();
            let mut i = 0;
            while i < tab.left_rows.len().max(tab.right_rows.len()) {
                let left = tab.left_rows.get(i);
                let right = tab.right_rows.get(i);
                let state_str = left
                    .map(|r| r.state.as_str())
                    .or(right.map(|r| r.state.as_str()))
                    .unwrap_or("equal");
                if state_str != "equal" {
                    blocks.push(serde_json::json!({
                        "kind": "difference",
                        "left_start": left.and_then(|r| r.number).unwrap_or(0),
                        "right_start": right.and_then(|r| r.number).unwrap_or(0),
                        "left_len": if left.is_some() { 1 } else { 0 },
                        "right_len": if right.is_some() { 1 } else { 0 },
                    }));
                }
                i += 1;
            }

            let mut summary = serde_json::json!({
                "schema_version": 1,
                "mode": tab.mode.to_lowercase(),
                "left_path": tab.left_path,
                "right_path": tab.right_path,
                "equal": tab.difference_count == 0,
                "differences": tab.difference_count,
                "blocks": blocks,
            });

            if tab.mode == "Folder" {
                let mut identical = 0usize;
                let mut different = 0usize;
                let mut left_only = 0usize;
                let mut right_only = 0usize;
                for entry in &tab.folder_entries {
                    match entry.state.as_str() {
                        "equal" => identical += 1,
                        "changed" => different += 1,
                        "left_only" => left_only += 1,
                        "right_only" => right_only += 1,
                        _ => {}
                    }
                }
                summary["folder_summary"] = serde_json::json!({
                    "identical": identical,
                    "different": different,
                    "left_only": left_only,
                    "right_only": right_only,
                });
            }

            http_response(
                200,
                "OK",
                "application/json",
                summary.to_string().into_bytes(),
            )
        }
        "folder-plan" => {
            if tab.mode != "Folder" {
                return bridge_error(
                    400,
                    "Bad Request",
                    "folder-plan format requires a folder compare tab",
                );
            }
            let mut entries: Vec<serde_json::Value> = Vec::new();
            let mut total = 0usize;
            let mut identical = 0usize;
            let mut different = 0usize;
            let mut left_only = 0usize;
            let mut right_only = 0usize;
            for entry in &tab.folder_entries {
                total += 1;
                match entry.state.as_str() {
                    "equal" => identical += 1,
                    "changed" => different += 1,
                    "left_only" => left_only += 1,
                    "right_only" => right_only += 1,
                    _ => {}
                }
                entries.push(serde_json::json!({
                    "path": entry.path,
                    "state": entry.state,
                    "left_size": entry.left_size,
                    "right_size": entry.right_size,
                }));
            }
            let body = serde_json::json!({
                "schema_version": 1,
                "entries": entries,
                "summary": {
                    "total": total,
                    "identical": identical,
                    "different": different,
                    "left_only": left_only,
                    "right_only": right_only,
                }
            });
            http_response(200, "OK", "application/json", body.to_string().into_bytes())
        }
        "full-json" => {
            let mut artifact_entries: Vec<serde_json::Value> = Vec::new();
            for a in &tab.artifacts {
                artifact_entries.push(serde_json::to_value(a).unwrap_or_default());
            }
            let tab_json = serde_json::to_value(&tab).unwrap_or_default();
            let body = serde_json::json!({
                "schema_version": 1,
                "mode": tab.mode.to_lowercase(),
                "tab": tab_json,
                "artifacts": artifact_entries,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        "unified" => {
            let mut lines = Vec::new();
            lines.push(format!("--- {}", tab.left_path));
            lines.push(format!("+++ {}", tab.right_path));
            for i in 0..tab.left_rows.len().max(tab.right_rows.len()) {
                let left = tab.left_rows.get(i);
                let right = tab.right_rows.get(i);
                let state = left
                    .map(|r| r.state.as_str())
                    .or(right.map(|r| r.state.as_str()))
                    .unwrap_or("equal");
                match state {
                    "equal" => {
                        if let Some(r) = left {
                            lines.push(format!(" {}", r.text));
                        }
                    }
                    "left_only" => {
                        if let Some(r) = left {
                            lines.push(format!("-{}", r.text));
                        }
                    }
                    "right_only" => {
                        if let Some(r) = right {
                            lines.push(format!("+{}", r.text));
                        }
                    }
                    "changed" => {
                        if let Some(r) = left {
                            lines.push(format!("-{}", r.text));
                        }
                        if let Some(r) = right {
                            lines.push(format!("+{}", r.text));
                        }
                    }
                    _ => {
                        if let Some(r) = left.or(right) {
                            lines.push(format!(" {}", r.text));
                        }
                    }
                }
            }
            let report_text = lines.join("\n");
            let saved_path = save_artifact(paths, "report-unified", report_text.as_bytes()).ok();
            let mut artifact_entries: Vec<serde_json::Value> = Vec::new();
            if let Some(ref p) = saved_path {
                artifact_entries.push(serde_json::json!({
                    "type": "report_file",
                    "path": p.to_string_lossy(),
                    "format": "unified"
                }));
            }
            for a in &tab.artifacts {
                artifact_entries.push(serde_json::to_value(a).unwrap_or_default());
            }
            let mut body_map = serde_json::json!({
                "content": report_text,
                "artifacts": artifact_entries,
            });
            if let Some(p) = saved_path {
                body_map["artifact_path"] = serde_json::json!(p.to_string_lossy().as_ref());
            }
            http_response(
                200,
                "OK",
                "application/json",
                body_map.to_string().into_bytes(),
            )
        }
        _ => {
            let mut artifact_entries: Vec<serde_json::Value> = Vec::new();
            for a in &tab.artifacts {
                artifact_entries.push(serde_json::to_value(a).unwrap_or_default());
            }
            let body = serde_json::json!({
                "tab": {
                    "mode": tab.mode,
                    "left_path": tab.left_path,
                    "right_path": tab.right_path,
                    "status": tab.status,
                    "difference_count": tab.difference_count,
                    "summary": tab.summary,
                },
                "artifacts": artifact_entries,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
    }
}

fn artifacts_list_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let tab = match state.lock() {
        Ok(s) => s
            .session
            .tabs
            .iter()
            .find(|t| t.id == s.session.active_tab_id)
            .cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let Some(tab) = tab else {
        return bridge_error(404, "Not Found", "no active tab");
    };
    let body = serde_json::json!({
        "artifacts": tab.artifacts,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn artifacts_cleanup_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let max_age_seconds: u64 = query_value(&params, "max_age_seconds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(86400);
    match cleanup_artifacts(paths, Duration::from_secs(max_age_seconds)) {
        Ok(removed) => {
            let body = serde_json::json!({
                "removed": removed,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn sessions_save_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let title = query_value(&params, "title").unwrap_or("Untitled Session");
    let tab = match state.lock() {
        Ok(s) => s
            .session
            .tabs
            .iter()
            .find(|t| t.id == s.session.active_tab_id)
            .cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let Some(tab) = tab else {
        return bridge_error(404, "Not Found", "no active tab");
    };
    // Refuse rather than save an entry the /sessions/recent responder would
    // filter straight back out (internal test fixtures) or that has no usable
    // paths — a 200 followed by nothing appearing reads as data loss.
    if !tab_has_persistable_paths(&tab) {
        return bridge_error(
            400,
            "Bad Request",
            "active tab's paths cannot be saved as a session",
        );
    }
    let mut session_file = SessionFile::new(CompareSession {
        title: title.to_owned(),
        left: PathBuf::from(&tab.left_path),
        base: None,
        right: PathBuf::from(&tab.right_path),
        options: CompareOptions {
            text: tab_text_options(&tab),
        },
    });
    session_file.selected_view = compare_view_mode(&tab.mode);
    persist_tab_snapshot(&mut session_file, &tab);
    let store = RecentSessionStore::new(paths.recent_sessions_file(), recent_limit(paths));
    let mut recent: RecentSessions = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("failed to load sessions: {err}"),
            );
        }
    };
    recent.sessions.insert(0, session_file);
    if let Err(err) = store.save(&recent) {
        return bridge_error(
            500,
            "Internal Server Error",
            &format!("failed to save session: {err}"),
        );
    }
    let body = serde_json::json!({"ok":true}).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn filters_list_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store = FilterStore::new(paths.filters_file());
    let filters: NamedFilters = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    match serde_json::to_string(&filters) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn filters_save_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(body) = query_value(&params, "body") else {
        return bridge_error(400, "Bad Request", "missing filter body");
    };
    let parsed = match FileFilter::parse(body) {
        Ok(filter) => filter,
        Err(err) => {
            return bridge_error(400, "Bad Request", &format!("filter parse failed: {err}"));
        }
    };
    if parsed.name.is_none() {
        return bridge_error(
            400,
            "Bad Request",
            "filter body must include a 'name:' header",
        );
    }
    let store = FilterStore::new(paths.filters_file());
    match store.upsert(parsed) {
        Ok(filters) => match serde_json::to_string(&filters) {
            Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
            Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
        },
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn filters_delete_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(name) = query_value(&params, "name") else {
        return bridge_error(400, "Bad Request", "missing filter name");
    };
    let store = FilterStore::new(paths.filters_file());
    let mut filters = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    filters.filters.retain(|f| f.name.as_deref() != Some(name));
    match store.save(&filters) {
        Ok(_) => match serde_json::to_string(&filters) {
            Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
            Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
        },
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

fn filters_validate_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    let Some(body) = query_value(&params, "body") else {
        return bridge_error(400, "Bad Request", "missing filter body");
    };
    match FileFilter::parse(body) {
        Ok(filter) => {
            let response = serde_json::json!({
                "ok": true,
                "name": filter.name,
                "rules": filter.rules.len(),
            });
            http_response(
                200,
                "OK",
                "application/json",
                response.to_string().into_bytes(),
            )
        }
        Err(err) => {
            let response = serde_json::json!({
                "ok": false,
                "line": err.line,
                "message": err.message,
                "kind": format!("{:?}", err.kind),
                "migration_hint": err.is_migration_hint(),
            });
            http_response(
                200,
                "OK",
                "application/json",
                response.to_string().into_bytes(),
            )
        }
    }
}

fn filters_migrate_bridge_response(query: &str) -> Vec<u8> {
    let params = query_params(query);
    // Accept either `body` (raw text content) or `path` (file path to read).
    let body_owned: Option<String> = query_value(&params, "body").map(str::to_owned);
    let path_owned: Option<String> = query_value(&params, "path").map(str::to_owned);
    let text = if let Some(body) = body_owned {
        body
    } else if let Some(path) = path_owned {
        match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                return bridge_error(
                    400,
                    "Bad Request",
                    &format!("failed to read file '{path}': {err}"),
                );
            }
        }
    } else {
        return bridge_error(400, "Bad Request", "missing 'body' or 'path' parameter");
    };
    let result = linsync_core::migrate_filter_text(&text);
    let response = serde_json::json!({
        "ok": true,
        "migrated": result.migrated,
        "warnings": result.warnings,
    });
    http_response(
        200,
        "OK",
        "application/json",
        response.to_string().into_bytes(),
    )
}

fn walk_options_bridge_response(paths: &AppPaths) -> Vec<u8> {
    let store = SettingsStore::new(paths.settings_file());
    let settings = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    let body = serde_json::json!({
        "respect_gitignore": settings.respect_gitignore,
        "follow_symlinks": settings.follow_symlinks,
        "max_walk_depth": settings.max_walk_depth,
        "includes": settings.session_includes,
        "excludes": settings.session_excludes,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn walk_options_set_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(key) = query_value(&params, "key") else {
        return bridge_error(400, "Bad Request", "missing walk option key");
    };
    let value = query_value(&params, "value").unwrap_or("");
    let store = SettingsStore::new(paths.settings_file());
    let mut settings = match store.load_or_default() {
        Ok(value) => value,
        Err(err) => {
            return bridge_error(500, "Internal Server Error", &err.to_string());
        }
    };
    match key {
        "respect_gitignore" => match parse_bool_setting(key, value) {
            Ok(b) => settings.respect_gitignore = b,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        "follow_symlinks" => match parse_bool_setting(key, value) {
            Ok(b) => settings.follow_symlinks = b,
            Err(err) => return bridge_error(400, "Bad Request", &err),
        },
        "max_walk_depth" => match value.parse::<u32>() {
            Ok(n) => settings.max_walk_depth = n.min(256),
            Err(_) => {
                return bridge_error(
                    400,
                    "Bad Request",
                    &format!("invalid max_walk_depth: {value}"),
                );
            }
        },
        "includes" => {
            settings.session_includes = split_csv_list(value);
        }
        "excludes" => {
            settings.session_excludes = split_csv_list(value);
        }
        other => {
            return bridge_error(
                400,
                "Bad Request",
                &format!("unknown walk option '{other}'"),
            );
        }
    }
    if let Err(err) = store.save(&settings) {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    walk_options_bridge_response(paths)
}

fn split_csv_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

fn plugins_list_bridge_response(
    paths: &AppPaths,
    plugin_enabled: &Arc<Mutex<HashMap<String, bool>>>,
) -> Vec<u8> {
    // Read through the in-memory lock so list and toggle share the same view.
    let enabled_map = match plugin_enabled.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return bridge_error(500, "Internal Server Error", "plugin state unavailable"),
    };
    let discovery = discover_installed_plugins(paths);
    let user_plugins_dir = paths.user_plugins_dir();
    let trusted_map = linsync_core::load_plugin_trusted_map(paths);
    let plugins: Vec<serde_json::Value> = discovery
        .plugins
        .iter()
        .map(|p| plugin_to_json(p, &enabled_map, &trusted_map, &user_plugins_dir))
        .collect();
    let errors: Vec<serde_json::Value> = discovery
        .errors
        .iter()
        .map(|err| {
            serde_json::json!({
                "path": err.path.display().to_string(),
                "message": err.message,
            })
        })
        .collect();
    let roots: Vec<String> = linsync_core::plugin_discovery_roots(paths)
        .iter()
        .map(|root| root.display().to_string())
        .collect();
    // Surface the sandbox confinement that helpers run under, so the Plugins
    // page can show whether plugin execution is confined or degraded.
    let sandbox = linsync_core::active_sandbox_status();
    // The active profile's prediffer chain + whether it can be edited (user
    // profiles only), so the page can show a per-prediffer "in profile" toggle.
    let active_profile = active_profile_prediffer_info(paths);
    let body = serde_json::json!({
        "plugins": plugins,
        "errors": errors,
        "roots": roots,
        "sandbox": { "label": sandbox.label, "confined": sandbox.confined },
        "active_profile": active_profile,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// Describe the active profile for the Plugins page: its id, whether it is an
/// editable (user) profile, and the prediffer plugin ids it currently routes.
fn active_profile_prediffer_info(paths: &AppPaths) -> serde_json::Value {
    let store =
        ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file());
    let Ok(Some(active_id)) = store.load_active_pointer() else {
        return serde_json::json!({ "id": null, "editable": false, "prediffers": [] });
    };
    let editable = find_builtin(&active_id).is_none();
    let (prediffers, plugin_enablement) = if editable {
        store
            .load(&active_id)
            .map(|p| (p.text.prediffer_plugins, p.plugin_enablement))
            .unwrap_or_default()
    } else {
        find_builtin(&active_id)
            .map(|p| {
                (
                    p.text.prediffer_plugins.clone(),
                    p.plugin_enablement.clone(),
                )
            })
            .unwrap_or_default()
    };
    serde_json::json!({
        "id": active_id.to_string(),
        "editable": editable,
        "prediffers": prediffers,
        "plugin_enablement": plugin_enablement,
    })
}

fn plugins_toggle_bridge_response(
    query: &str,
    paths: &AppPaths,
    plugin_enabled: &Arc<Mutex<HashMap<String, bool>>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    let enabled_str = query_value(&params, "enabled").unwrap_or("true");
    let enabled = matches!(enabled_str, "true" | "1" | "yes");
    // Acquire the lock for the full load-modify-save sequence so concurrent
    // toggles cannot interleave and produce a partial write.
    let mut guard = match plugin_enabled.lock() {
        Ok(g) => g,
        Err(_) => return bridge_error(500, "Internal Server Error", "plugin state unavailable"),
    };
    guard.insert(id.to_owned(), enabled);
    let file = paths.plugins_enabled_file();
    if let Some(parent) = file.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        return bridge_error(500, "Internal Server Error", &err.to_string());
    }
    let text = match serde_json::to_string_pretty(&*guard) {
        Ok(t) => t,
        Err(err) => return bridge_error(500, "Internal Server Error", &err.to_string()),
    };
    match fs::write(&file, text) {
        Ok(()) => {
            let body = serde_json::json!({ "ok": true }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// `/plugins/diagnostic?id=X` — probe a discovered plugin's helper and report a
/// structured health verdict (exit/timeout/stdout/stderr + parsed response) plus
/// the active sandbox confinement. Backs the Plugins page "Diagnose" action.
fn plugins_diagnostic_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    if !linsync_core::is_stable_plugin_id(id) {
        return bridge_error(400, "Bad Request", "invalid plugin id");
    }
    let discovery = discover_installed_plugins(paths);
    let Some(plugin) = discovery.plugins.iter().find(|p| p.manifest.id == id) else {
        return bridge_error(404, "Not Found", "no installed plugin with that id");
    };
    let sandbox = linsync_core::active_sandbox_status();
    let sandbox_json = serde_json::json!({ "label": sandbox.label, "confined": sandbox.confined });
    match linsync_core::probe_plugin(
        &plugin.root,
        &plugin.manifest,
        Vec::new(),
        &linsync_core::PluginExecutionOptions::default(),
    ) {
        Ok(outcome) => {
            let response = outcome.response.as_ref().map(|r| {
                serde_json::json!({
                    "status": format!("{:?}", r.status).to_lowercase(),
                    "diagnostics": r
                        .diagnostics
                        .iter()
                        .map(|d| serde_json::json!({"severity": d.severity, "message": d.message}))
                        .collect::<Vec<_>>(),
                })
            });
            let body = serde_json::json!({
                "id": id,
                "healthy": outcome.is_healthy(),
                "exit_code": outcome.exit_code,
                "timed_out": outcome.timed_out,
                "stdout": outcome.stdout,
                "stderr": outcome.stderr,
                "response": response,
                "sandbox": sandbox_json,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => {
            let body = serde_json::json!({
                "id": id,
                "healthy": false,
                "error": err.to_string(),
                "sandbox": sandbox_json,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
    }
}

/// Record a plugin's trusted state (`?id=&trusted=true|false`). The GUI calls
/// this before enabling a discovered plugin for the first time, so that an
/// enabled plugin is always one the user has authorized to run.
fn plugins_trust_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    if !linsync_core::is_stable_plugin_id(id) {
        return bridge_error(400, "Bad Request", "invalid plugin id");
    }
    // Default to trusting; an explicit `?trusted=false` revokes.
    let trusted = query_value(&params, "trusted")
        .map(|v| v != "false")
        .unwrap_or(true);
    match linsync_core::set_plugin_trusted(paths, id, trusted) {
        Ok(()) => {
            let body = serde_json::json!({ "ok": true, "id": id, "trusted": trusted }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(500, "Internal Server Error", &err.to_string()),
    }
}

/// Install a plugin from a local directory (`?path=`) into the user plugin
/// directory. 409 if an id is already installed, 400 on a bad manifest/path.
fn plugins_install_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    use linsync_core::PluginStoreError;
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing plugin source path");
    };
    match linsync_core::install_plugin(paths, std::path::Path::new(path)) {
        Ok(installed) => {
            let body = serde_json::json!({
                "ok": true,
                "id": installed.manifest.id,
                "name": installed.manifest.name,
                "root": installed.root.to_string_lossy(),
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(PluginStoreError::AlreadyInstalled(id)) => bridge_error(
            409,
            "Conflict",
            &format!("a plugin with id '{id}' is already installed"),
        ),
        Err(e @ (PluginStoreError::InvalidManifest(_) | PluginStoreError::InvalidId(_))) => {
            bridge_error(400, "Bad Request", &e.to_string())
        }
        Err(e) => bridge_error(500, "Internal Server Error", &e.to_string()),
    }
}

/// Remove a user-installed plugin (`?id=`). 404 if not installed in the user
/// directory; system plugin directories are never touched.
fn plugins_remove_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    use linsync_core::PluginStoreError;
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    match linsync_core::remove_plugin(paths, id) {
        Ok(()) => {
            let body = serde_json::json!({ "ok": true, "id": id }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(PluginStoreError::UnknownPlugin(_)) => {
            bridge_error(404, "Not Found", "no installed plugin with that id")
        }
        Err(PluginStoreError::InvalidId(_)) => {
            bridge_error(400, "Bad Request", "invalid plugin id")
        }
        Err(e) => bridge_error(500, "Internal Server Error", &e.to_string()),
    }
}

fn plugins_options_get_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    if !linsync_core::is_stable_plugin_id(id) {
        return bridge_error(400, "Bad Request", "invalid plugin id");
    }

    // Look up the schema from the discovered manifest (empty if plugin not found).
    let discovery = discover_installed_plugins(paths);
    let schema: Vec<serde_json::Value> = discovery
        .plugins
        .iter()
        .find(|p| p.manifest.id == id)
        .map(|p| {
            p.manifest
                .options_schema
                .iter()
                .map(|opt| {
                    serde_json::json!({
                        "key": opt.key,
                        "label": opt.label,
                        "kind": format!("{:?}", opt.kind).to_lowercase(),
                        "default": opt.default,
                        "choices": opt.choices,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let values = linsync_core::load_plugin_options(paths, id);
    let body = serde_json::json!({
        "schema": schema,
        "values": values,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn plugins_options_set_bridge_response(query: &str, paths: &AppPaths) -> Vec<u8> {
    let params = query_params(query);
    let Some(id) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing plugin id");
    };
    if !linsync_core::is_stable_plugin_id(id) {
        return bridge_error(400, "Bad Request", "invalid plugin id");
    }
    let Some(key) = query_value(&params, "key") else {
        return bridge_error(400, "Bad Request", "missing option key");
    };
    let Some(raw) = query_value(&params, "value") else {
        return bridge_error(400, "Bad Request", "missing option value");
    };

    // Parse the value as JSON so `true`/`7`/`"x"` get the right type; fall back
    // to a plain string. The core store validates it against the plugin's
    // manifest schema before persisting.
    let value: serde_json::Value =
        serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_owned()));
    match linsync_core::set_plugin_option(paths, id, key, value) {
        Ok(_) => {
            let body = serde_json::json!({ "ok": true }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => {
            let body = serde_json::json!({ "ok": false, "error": err.to_string() }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
    }
}

fn plugin_to_json(
    plugin: &DiscoveredPlugin,
    enabled_map: &std::collections::HashMap<String, bool>,
    trusted_map: &std::collections::HashMap<String, bool>,
    user_plugins_dir: &Path,
) -> serde_json::Value {
    let manifest = &plugin.manifest;
    // The plugin root is the per-plugin sub-directory; its parent is the
    // containing plugins directory.  Compare that parent to the user plugins
    // directory to distinguish user-installed plugins from system ones.
    let source = plugin
        .root
        .parent()
        .map(|parent| {
            if parent == user_plugins_dir {
                "user"
            } else {
                "system"
            }
        })
        .unwrap_or("user");
    let enabled = *enabled_map.get(&manifest.id).unwrap_or(&true);
    serde_json::json!({
        "id": manifest.id,
        "name": manifest.name,
        "version": manifest.version,
        "license": manifest.license,
        "classes": manifest.classes.iter().map(|class| format!("{class:?}").to_lowercase()).collect::<Vec<_>>(),
        "extensions": manifest.extensions.clone(),
        "mime_types": manifest.mime_types.clone(),
        "deterministic": manifest.deterministic,
        "directory": plugin.root.display().to_string(),
        "source": source,
        "enabled": enabled,
        "trusted": trusted_map.get(&manifest.id).copied().unwrap_or(false),
        "has_options": !manifest.options_schema.is_empty(),
    })
}

fn folder_op_plan_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(kind) = query_value(&params, "kind") else {
        return bridge_error(400, "Bad Request", "missing op kind");
    };
    // Each selected entry arrives as its own `entries=` param (percent-encoded),
    // so paths containing commas survive intact.
    let entries: Vec<PathBuf> = query_values(&params, "entries")
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();

    let active = match state.lock() {
        Ok(state) => state.active_tab().cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(tab) = active else {
        return bridge_error(400, "Bad Request", "no active compare tab");
    };
    if tab.mode != "Folder" {
        return bridge_error(
            400,
            "Bad Request",
            "folder ops require a folder compare tab",
        );
    }

    let folder_options = match resolve_folder_options_for_request(paths, &params) {
        Ok(opts) => opts,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let compare = match compare_folders(
        Path::new(&tab.left_path),
        Path::new(&tab.right_path),
        &folder_options,
    ) {
        Ok(result) => result,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("folder compare failed: {err}"),
            );
        }
    };

    let Some(op_kind) = parse_folder_op_kind(kind, &params) else {
        return bridge_error(400, "Bad Request", "unsupported op kind");
    };
    let delete_side = folder_op_delete_side(&op_kind);
    let mut plan = plan_folder_operation(&compare, op_kind, &entries);
    let left_base = Path::new(&tab.left_path);
    let right_base = Path::new(&tab.right_path);
    if let Err(err) = linsync_core::assess_operation_risks(&mut plan, left_base, right_base) {
        return bridge_error(
            500,
            "Internal Server Error",
            &format!("risk assessment failed: {err}"),
        );
    }
    let permanent_delete = plan.contains_deletes && !use_trash_for_deletes(paths);
    let mut body = folder_plan_to_json(&plan);
    body["permanent_delete"] = serde_json::Value::Bool(permanent_delete);
    if permanent_delete {
        body["permanent_warning"] = serde_json::Value::String(permanent_delete_warning(
            delete_side,
            plan.counts.delete_count,
        ));
    }
    http_response(200, "OK", "application/json", body.to_string().into_bytes())
}

/// True when the user's settings route deletes to the freedesktop trash;
/// false means folder-op deletes are permanent and require confirmation.
fn use_trash_for_deletes(paths: &AppPaths) -> bool {
    SettingsStore::new(paths.settings_file())
        .load_or_default()
        .map(|settings| settings.delete_preference == DeletePreference::MoveToTrash)
        .unwrap_or(true)
}

fn folder_op_delete_side(kind: &FolderOperationKind) -> Option<CompareSide> {
    match kind {
        FolderOperationKind::DeleteLeft => Some(CompareSide::Left),
        FolderOperationKind::DeleteRight => Some(CompareSide::Right),
        _ => None,
    }
}

fn folder_op_execute_bridge_response(
    query: &str,
    paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(kind) = query_value(&params, "kind") else {
        return bridge_error(400, "Bad Request", "missing op kind");
    };
    // Each selected entry arrives as its own `entries=` param (percent-encoded),
    // so paths containing commas survive intact.
    let entries: Vec<PathBuf> = query_values(&params, "entries")
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();

    let active = match state.lock() {
        Ok(state) => state.active_tab().cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(tab) = active else {
        return bridge_error(400, "Bad Request", "no active compare tab");
    };
    if tab.mode != "Folder" {
        return bridge_error(
            400,
            "Bad Request",
            "folder ops require a folder compare tab",
        );
    }

    let folder_options = match resolve_folder_options_for_request(paths, &params) {
        Ok(opts) => opts,
        Err(err) => return bridge_error(400, "Bad Request", &err),
    };
    let compare = match compare_folders(
        Path::new(&tab.left_path),
        Path::new(&tab.right_path),
        &folder_options,
    ) {
        Ok(result) => result,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("folder compare failed: {err}"),
            );
        }
    };

    let Some(op_kind) = parse_folder_op_kind(kind, &params) else {
        return bridge_error(400, "Bad Request", "unsupported op kind");
    };
    let delete_side = folder_op_delete_side(&op_kind);
    let plan = plan_folder_operation(&compare, op_kind, &entries);

    let use_trash = use_trash_for_deletes(paths);
    let confirm_permanent = query_bool(&params, "confirm_permanent");
    if plan.contains_deletes && !use_trash && !confirm_permanent {
        // Refuse before touching the filesystem: permanent deletes are
        // unrecoverable, so the caller must resend with confirm_permanent=1.
        return bridge_error(
            409,
            "Conflict",
            &permanent_delete_warning(delete_side, plan.counts.delete_count),
        );
    }
    let confirmation = if confirm_permanent {
        linsync_core::PermanentDeleteConfirmation::Confirmed
    } else {
        linsync_core::PermanentDeleteConfirmation::NotConfirmed
    };
    let outcomes = execute_folder_operation_plan(&plan, &paths.data_dir, use_trash, confirmation);
    let body = folder_outcomes_to_json(&plan, &outcomes).to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn parse_folder_op_kind(kind: &str, params: &[(String, String)]) -> Option<FolderOperationKind> {
    let new_name = query_value(params, "new_name").map(|name| name.to_owned());
    Some(match kind {
        "copy_left_to_right" => FolderOperationKind::CopyLeftToRight,
        "copy_right_to_left" => FolderOperationKind::CopyRightToLeft,
        "delete_left" => FolderOperationKind::DeleteLeft,
        "delete_right" => FolderOperationKind::DeleteRight,
        "rename_left" => FolderOperationKind::RenameLeft {
            new_name: new_name?,
        },
        "rename_right" => FolderOperationKind::RenameRight {
            new_name: new_name?,
        },
        "create_missing_left" => FolderOperationKind::CreateMissingLeft,
        "create_missing_right" => FolderOperationKind::CreateMissingRight,
        "refresh" => FolderOperationKind::Refresh,
        _ => return None,
    })
}

fn folder_plan_to_json(plan: &linsync_core::FolderOperationPlan) -> serde_json::Value {
    let risk = plan.risk_summary();
    serde_json::json!({
        "operations": plan
            .operations
            .iter()
            .map(|op| serde_json::json!({
                "kind": format!("{:?}", op.kind),
                "relative_path": op.relative_path.display().to_string(),
                "source": op.source.as_ref().map(|p| p.display().to_string()),
                "target": op.target.as_ref().map(|p| p.display().to_string()),
                "overwrites_existing": op.overwrites_existing,
            }))
            .collect::<Vec<_>>(),
        "counts": {
            "copy_count": plan.counts.copy_count,
            "delete_count": plan.counts.delete_count,
            "rename_count": plan.counts.rename_count,
            "create_folder_count": plan.counts.create_folder_count,
            "refresh_count": plan.counts.refresh_count,
            "overwrite_warning_count": plan.counts.overwrite_warning_count,
            "permission_warning_count": plan.counts.permission_warning_count,
            "conflict_warning_count": plan.counts.conflict_warning_count,
        },
        "warnings": plan
            .warnings
            .iter()
            .map(|w| serde_json::json!({
                "relative_path": w.relative_path.display().to_string(),
                "kind": format!("{:?}", w.kind),
                "message": w.message,
            }))
            .collect::<Vec<_>>(),
        "risk_summary": {
            "total_operations": risk.total_operations,
            "overwrite_count": risk.overwrite_count,
            "delete_count": risk.delete_count,
            "high_risk_count": risk.high_risk_count,
        },
    })
}

fn folder_outcomes_to_json(
    plan: &linsync_core::FolderOperationPlan,
    outcomes: &[FolderOperationOutcome],
) -> serde_json::Value {
    let succeeded = outcomes
        .iter()
        .filter(|o| matches!(o.status, FolderOperationStatus::Succeeded))
        .count();
    let failed = outcomes
        .iter()
        .filter(|o| matches!(o.status, FolderOperationStatus::Failed))
        .count();
    serde_json::json!({
        "plan": folder_plan_to_json(plan),
        "outcomes": outcomes
            .iter()
            .map(|outcome| serde_json::json!({
                "kind": format!("{:?}", outcome.operation.kind),
                "relative_path": outcome.operation.relative_path.display().to_string(),
                "status": match outcome.status {
                    FolderOperationStatus::Succeeded => "succeeded",
                    FolderOperationStatus::Skipped => "skipped",
                    FolderOperationStatus::Failed => "failed",
                },
                "message": outcome.message,
            }))
            .collect::<Vec<_>>(),
        "summary": {
            "succeeded": succeeded,
            "failed": failed,
            "total": outcomes.len(),
        },
    })
}

fn image_regions_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let regions = match state.lock() {
        Ok(s) => s.last_image_result.as_ref().map(|r| r.diff_regions.clone()),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(regions) = regions else {
        return bridge_error(404, "Not Found", "no image compare result available");
    };
    let total = regions.len();
    let body = serde_json::json!({
        "regions": regions,
        "total": total,
    });
    let json = serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"error":"serialization error"}"#.to_owned());
    http_response(200, "OK", "application/json", json.into_bytes())
}

fn image_save_overlay_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(path_str) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };
    let destination = PathBuf::from(path_str);
    if destination.as_os_str().is_empty() {
        return bridge_error(400, "Bad Request", "empty path");
    }
    if destination.is_dir() {
        return bridge_error(400, "Bad Request", "path points to a directory");
    }

    let source = match state.lock() {
        Ok(s) => s.last_image_overlay_path.clone(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(source) = source else {
        return bridge_error(404, "Not Found", "no image overlay available");
    };
    if !source.exists() {
        return bridge_error(
            404,
            "Not Found",
            "image overlay artifact is no longer available",
        );
    }

    if let Some(parent) = destination.parent()
        && !parent.as_os_str().is_empty()
        && let Err(err) = fs::create_dir_all(parent)
    {
        return bridge_error(
            500,
            "Internal Server Error",
            &format!("failed to create destination directory: {err}"),
        );
    }

    match fs::copy(&source, &destination) {
        Ok(bytes) => {
            let body = serde_json::json!({
                "ok": true,
                "path": destination.display().to_string(),
                "bytes": bytes,
            })
            .to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => bridge_error(
            500,
            "Internal Server Error",
            &format!("failed to save overlay: {err}"),
        ),
    }
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
}

fn binary_interpret_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let offset = match query_value(&params, "offset").and_then(|v| v.parse::<usize>().ok()) {
        Some(o) => o,
        None => return bridge_error(400, "Bad Request", "missing or invalid offset"),
    };
    let kind_str = match query_value(&params, "kind") {
        Some(k) => k,
        None => return bridge_error(400, "Bad Request", "missing kind"),
    };
    let kind = match parse_typed_value_kind(kind_str) {
        Some(k) => k,
        None => return bridge_error(400, "Bad Request", &format!("unknown kind '{kind_str}'")),
    };

    let tab = match state.lock() {
        Ok(s) => s
            .session
            .tabs
            .iter()
            .find(|t| t.id == s.session.active_tab_id)
            .cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let Some(tab) = tab else {
        return bridge_error(404, "Not Found", "no active tab");
    };
    if tab.mode != "Hex" {
        return bridge_error(400, "Bad Request", "active tab is not a binary compare");
    }

    let left_bytes = match fs::read(&tab.left_path) {
        Ok(b) => b,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("failed to read left file: {err}"),
            );
        }
    };
    let right_bytes = match fs::read(&tab.right_path) {
        Ok(b) => b,
        Err(err) => {
            return bridge_error(
                500,
                "Internal Server Error",
                &format!("failed to read right file: {err}"),
            );
        }
    };

    let result = compare_binary(
        &tab.left_path,
        &left_bytes,
        &tab.right_path,
        &right_bytes,
        &BinaryCompareOptions {
            compare_content: false,
            ..BinaryCompareOptions::default()
        },
    );

    let interpretation = match result.interpret_at(offset, kind) {
        Some(i) => i,
        None => return bridge_error(400, "Bad Request", "offset out of bounds"),
    };

    let body = serde_json::to_string(&interpretation)
        .unwrap_or_else(|_| r#"{"error":"serialization error"}"#.to_owned());
    http_response(200, "OK", "application/json", body.into_bytes())
}

/// `/binary/window?offset=&limit=` — return a slice of hex rows for a binary
/// compare tab, so the GUI can page through large files without loading all
/// rows at once.
fn binary_window_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let offset = query_value(&params, "offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query_value(&params, "limit")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(usize::MAX);

    // Extract only the requested window while holding the lock — cloning the
    // whole tab (every hex row of both sides) per page request would defeat
    // the point of windowing.
    let (total_rows, left_window, right_window) = match state.lock() {
        Ok(s) => {
            let Some(tab) = s
                .session
                .tabs
                .iter()
                .find(|t| t.id == s.session.active_tab_id)
            else {
                return bridge_error(404, "Not Found", "no active tab");
            };
            if tab.mode != "Hex" {
                return bridge_error(400, "Bad Request", "active tab is not a binary compare");
            }
            let total_rows = tab.left_rows.len().max(tab.right_rows.len());
            let end = offset.saturating_add(limit).min(total_rows);
            let window = |rows: &[GuiLineRow]| -> Vec<GuiLineRow> {
                rows.get(offset..end.min(rows.len()))
                    .map(<[GuiLineRow]>::to_vec)
                    .unwrap_or_default()
            };
            (total_rows, window(&tab.left_rows), window(&tab.right_rows))
        }
        Err(_) => return bridge_error(500, "Internal Server Error", "state unavailable"),
    };
    let returned = left_window.len().max(right_window.len());
    let body = serde_json::json!({
        "totalRows": total_rows,
        "offset": offset,
        "returned": returned,
        "hasMore": offset + returned < total_rows,
        "left_rows": left_window,
        "right_rows": right_window,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

fn parse_typed_value_kind(s: &str) -> Option<TypedValueKind> {
    match s {
        "u8" => Some(TypedValueKind::U8),
        "i8" => Some(TypedValueKind::I8),
        "u16_le" => Some(TypedValueKind::U16Le),
        "u16_be" => Some(TypedValueKind::U16Be),
        "i16_le" => Some(TypedValueKind::I16Le),
        "i16_be" => Some(TypedValueKind::I16Be),
        "u32_le" => Some(TypedValueKind::U32Le),
        "u32_be" => Some(TypedValueKind::U32Be),
        "i32_le" => Some(TypedValueKind::I32Le),
        "i32_be" => Some(TypedValueKind::I32Be),
        "u64_le" => Some(TypedValueKind::U64Le),
        "u64_be" => Some(TypedValueKind::U64Be),
        "i64_le" => Some(TypedValueKind::I64Le),
        "i64_be" => Some(TypedValueKind::I64Be),
        "f32_le" => Some(TypedValueKind::F32Le),
        "f32_be" => Some(TypedValueKind::F32Be),
        "f64_le" => Some(TypedValueKind::F64Le),
        "f64_be" => Some(TypedValueKind::F64Be),
        _ => None,
    }
}

fn merge_conflicts_bridge_response(state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let active = match state.lock() {
        Ok(state) => state.active_tab().cloned(),
        Err(_) => return bridge_error(500, "Internal Server Error", "session state unavailable"),
    };
    let Some(tab) = active else {
        return bridge_error(400, "Bad Request", "no active compare tab");
    };
    if tab.mode != "Text" {
        return bridge_error(
            400,
            "Bad Request",
            "conflict navigation requires a text tab",
        );
    }
    let compare = compare_tab_text_rows(&tab);
    let conflicts: Vec<serde_json::Value> = compare
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| matches!(block.kind, DiffBlockKind::Difference))
        .map(|(index, block)| {
            serde_json::json!({
                "index": index,
                "left_start": block.left_start.unwrap_or_default(),
                "left_len": block.left_len,
                "right_start": block.right_start.unwrap_or_default(),
                "right_len": block.right_len,
            })
        })
        .collect();
    let body = serde_json::json!({
        "conflicts": conflicts,
        "total": compare.blocks.len(),
        "differences": compare.summary.diff_blocks,
        "can_save": tab.left_dirty || tab.right_dirty,
    })
    .to_string();
    http_response(200, "OK", "application/json", body.into_bytes())
}

// ── Three-way merge bridge handlers ──────────────────────────────────────────

/// Shared logic: read three files, create a `ThreeWayMergeState`, store it in
/// `state`, and return a JSON summary of the conflicts + current output text.
pub(crate) fn start_three_way_merge_session(
    base_path: &str,
    left_path: &str,
    right_path: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Result<String, String> {
    let base_doc = TextDocument::from_path(std::path::Path::new(base_path))
        .map_err(|err| format!("failed to read base '{}': {err}", base_path))?;
    let left_doc = TextDocument::from_path(std::path::Path::new(left_path))
        .map_err(|err| format!("failed to read left '{}': {err}", left_path))?;
    let right_doc = TextDocument::from_path(std::path::Path::new(right_path))
        .map_err(|err| format!("failed to read right '{}': {err}", right_path))?;

    let session = ThreeWayMergeState::new(base_doc, left_doc, right_doc);
    let conflicts_json = three_way_conflicts_json(&session);
    let output_text = session.output().text();

    match state.lock() {
        Ok(mut s) => {
            s.three_way_session = Some(session);
        }
        Err(_) => return Err("session state unavailable".to_owned()),
    }

    let body = serde_json::json!({
        "ok": true,
        "conflicts": conflicts_json,
        // At start nothing is resolved yet, so every conflict is unresolved.
        "unresolved_count": conflicts_json.len(),
        "output_text": output_text,
    })
    .to_string();
    Ok(body)
}

fn merge3_start_bridge_response(
    query: &str,
    _paths: &AppPaths,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Vec<u8> {
    let params = query_params(query);
    let Some(base) = query_value(&params, "base") else {
        return bridge_error(400, "Bad Request", "missing base path");
    };
    let Some(left) = query_value(&params, "left") else {
        return bridge_error(400, "Bad Request", "missing left path");
    };
    let Some(right) = query_value(&params, "right") else {
        return bridge_error(400, "Bad Request", "missing right path");
    };

    match start_three_way_merge_session(base, left, right, state) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(400, "Bad Request", &err),
    }
}

/// Shared logic: resolve a conflict in the current `ThreeWayMergeState`.
pub(crate) fn resolve_three_way_conflict(
    id: u32,
    choice_str: &str,
    custom_text: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Result<String, String> {
    let choice = match choice_str {
        "left" => MergeChoice::Left,
        "right" => MergeChoice::Right,
        "base" => MergeChoice::Base,
        "custom" => MergeChoice::Custom(custom_text.to_owned()),
        other => return Err(format!("unsupported choice '{other}'")),
    };

    let mut guard = state
        .lock()
        .map_err(|_| "session state unavailable".to_owned())?;
    let session = guard
        .three_way_session
        .as_mut()
        .ok_or_else(|| "no active three-way merge session".to_owned())?;

    session
        .resolve(ConflictId(id), choice)
        .map_err(|err| err.to_string())?;

    let conflicts_json = three_way_conflicts_json(session);
    let output_text = session.output().text();
    // `conflicts` is the stable full list (it never shrinks as conflicts are
    // resolved), so the GUI must use `unresolved_count` for "remaining".
    let body = serde_json::json!({
        "ok": true,
        "conflicts": conflicts_json,
        "unresolved_count": session.unresolved_count(),
        "output_text": output_text,
    })
    .to_string();
    Ok(body)
}

fn merge3_resolve_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(id_str) = query_value(&params, "id") else {
        return bridge_error(400, "Bad Request", "missing conflict id");
    };
    let Ok(id) = id_str.parse::<u32>() else {
        return bridge_error(
            400,
            "Bad Request",
            "conflict id must be a non-negative integer",
        );
    };
    let Some(choice) = query_value(&params, "choice") else {
        return bridge_error(400, "Bad Request", "missing choice");
    };
    let text = query_value(&params, "text").unwrap_or("");

    match resolve_three_way_conflict(id, choice, text, state) {
        Ok(body) => http_response(200, "OK", "application/json", body.into_bytes()),
        Err(err) => bridge_error(400, "Bad Request", &err),
    }
}

/// Shared logic: write the current three-way merge output to a file.
pub(crate) fn save_three_way_merge_output(
    path: &str,
    state: &Arc<Mutex<GuiBridgeState>>,
) -> Result<(), String> {
    let guard = state
        .lock()
        .map_err(|_| "session state unavailable".to_owned())?;
    let session = guard
        .three_way_session
        .as_ref()
        .ok_or_else(|| "no active three-way merge session".to_owned())?;
    session
        .save_to(std::path::Path::new(path))
        .map_err(|err| format!("failed to save merged output: {err}"))
}

fn validate_merge_session(session: &ThreeWayMergeState) -> Result<(), usize> {
    let unresolved = session.unresolved_count();
    if unresolved == 0 {
        Ok(())
    } else {
        Err(unresolved)
    }
}

fn merge3_save_bridge_response(query: &str, state: &Arc<Mutex<GuiBridgeState>>) -> Vec<u8> {
    let params = query_params(query);
    let Some(path) = query_value(&params, "path") else {
        return bridge_error(400, "Bad Request", "missing path");
    };

    {
        let guard = match state.lock() {
            Ok(g) => g,
            Err(_) => {
                return bridge_error(500, "Internal Server Error", "session state unavailable");
            }
        };
        let Some(session) = guard.three_way_session.as_ref() else {
            return bridge_error(400, "Bad Request", "no active three-way merge session");
        };
        if let Err(count) = validate_merge_session(session) {
            return http_response(
                409,
                "Conflict",
                "application/json",
                serde_json::json!({
                    "ok": false,
                    "error": format!("{count} unresolved conflict(s) remain"),
                    "unresolved_count": count,
                })
                .to_string()
                .into_bytes(),
            );
        }
    }

    match save_three_way_merge_output(path, state) {
        Ok(()) => {
            let body = serde_json::json!({ "ok": true }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
        Err(err) => {
            let body = serde_json::json!({ "ok": false, "error": err }).to_string();
            http_response(200, "OK", "application/json", body.into_bytes())
        }
    }
}

fn three_way_conflicts_json(session: &ThreeWayMergeState) -> Vec<serde_json::Value> {
    session
        .conflicts()
        .into_iter()
        .map(|conflict| {
            serde_json::json!({
                "id": conflict.id.0,
                "start_line": conflict.start_line,
                "end_line": conflict.end_line,
                "base_lines": conflict.base_lines,
                "left_lines": conflict.left_lines,
                "right_lines": conflict.right_lines,
            })
        })
        .collect()
}

fn query_params(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

fn query_value<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
}

/// All values for a repeated query key, in order (each already percent-decoded
/// by [`query_params`]). Used for multi-valued params like `entries`, where one
/// param per item avoids an in-band separator that would split values
/// containing that separator (e.g. a path with a comma).
fn query_values<'a>(params: &'a [(String, String)], key: &str) -> Vec<&'a str> {
    params
        .iter()
        .filter(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
        .collect()
}

fn query_bool(params: &[(String, String)], key: &str) -> bool {
    query_value(params, key).is_some_and(|value| {
        value.eq_ignore_ascii_case("1")
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    decoded.push((high << 4) | low);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn bridge_error(status: u16, reason: &str, message: &str) -> Vec<u8> {
    let body = serde_json::json!({ "error": message })
        .to_string()
        .into_bytes();
    http_response(status, reason, "application/json", body)
}

fn bridge_error_json(status: u16, reason: &str, body: serde_json::Value) -> Vec<u8> {
    http_response(
        status,
        reason,
        "application/json",
        body.to_string().into_bytes(),
    )
}

fn http_response(status: u16, reason: &str, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    let body = version_json_response_body(content_type, body);
    // No `Access-Control-Allow-Origin` header: the bridge is intended for the local
    // QML host. Allowing a wildcard origin would let any web page on the user's
    // machine read files via /compare and overwrite them via /copy-all + /save.
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}

fn version_json_response_body(content_type: &str, body: Vec<u8>) -> Vec<u8> {
    if !content_type
        .split(';')
        .next()
        .is_some_and(|ty| ty.trim().eq_ignore_ascii_case("application/json"))
    {
        return body;
    }
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };
    if !value.is_object() {
        return body;
    }
    insert_response_schema_version(&mut value);
    serde_json::to_vec(&value).unwrap_or(body)
}

#[cfg(feature = "cxxqt-app")]
fn use_cxxqt_host() -> bool {
    // Default to the in-process Qt host. It sets the Wayland xdg_toplevel
    // app_id to "com.visorcraft.LinSync" before the window maps, which is
    // what KDE Plasma needs to associate the running window with the
    // pinned launcher (com.visorcraft.LinSync.desktop). The external qml6
    // runner can't do this because it stamps its own app_id
    // ("org.qt-project.qml") onto every window it creates.
    //
    // Set LINSYNC_QML_HOST=external to force the legacy qml6 spawn.
    !matches!(
        env::var("LINSYNC_QML_HOST"),
        Ok(value) if value.eq_ignore_ascii_case("external")
    )
}

#[cfg(feature = "cxxqt-app")]
fn run_cxxqt_host(
    paths: &AppPaths,
    qml_file: &Path,
    launch_context_path: Option<&Path>,
    launch_context: Option<GuiLaunchContext>,
) -> Result<ExitCode, String> {
    use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

    let qml_root = qml_file
        .parent()
        .ok_or_else(|| format!("invalid QML file path '{}'", qml_file.display()))?;

    // Start the HTTP bridge first so Main.qml can read bridgeUrl as soon as
    // Component.onCompleted fires. Main.qml's sessionBridge defaults to null
    // and falls back to the HTTP bridge transport when so — we don't need to
    // register the cxx-qt LinSyncSessionBridge type for this host.
    let bridge = start_bridge_server(paths.clone(), launch_context)?;
    let bridge_info = serde_json::json!({
        "bridge_url": &bridge.base_url,
        "version": env!("CARGO_PKG_VERSION"),
        "context_path": launch_context_path.map(|p| p.display().to_string()),
        "section": env::var("LINSYNC_STARTUP_SECTION").ok().filter(|s| !s.is_empty()),
    });
    let payload = serde_json::to_string(&bridge_info).unwrap();
    let bridge_info_path = write_bridge_info_file(payload.as_bytes());
    if bridge_info_path.is_none() {
        tracing::warn!("bridge info sidecar not written; GUI will run without the HTTP bridge");
    }
    // SAFETY: edition 2024 requires `unsafe` for env::set_var. We are still
    // single-threaded here — QGuiApplication and the bridge worker threads
    // are spun up below.
    unsafe {
        if let Some(ref path) = bridge_info_path {
            env::set_var("LINSYNC_BRIDGE_INFO", path.display().to_string());
        }
        env::set_var("QML_XHR_ALLOW_FILE_READ", "1");
        if env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
            env::set_var("QT_QUICK_CONTROLS_STYLE", "Fusion");
        }
    }

    let mut app = QGuiApplication::new();
    // setDesktopFileName must run before any QWindow is mapped — Qt reads it
    // once in QWaylandWindow::initWindow() to set xdg_toplevel.app_id, which
    // is what KDE Plasma matches against the .desktop file basename for
    // taskbar grouping.
    QGuiApplication::set_desktop_file_name(&QString::from("com.visorcraft.LinSync"));
    app.pin_mut()
        .set_application_name(&QString::from("LinSync"));
    app.pin_mut()
        .set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));
    app.pin_mut()
        .set_organization_name(&QString::from("VisorCraft"));
    app.pin_mut()
        .set_organization_domain(&QString::from("visorcraft.com"));

    // Install a UI translation catalog for the active locale, if one ships
    // alongside the QML (sibling `i18n/` dir holding `linsync_<locale>.qm`).
    // No-op when no catalog matches the locale, so the English source strings
    // remain. Must run before the engine loads QML so qsTr() resolves.
    if let Some(i18n_dir) = qml_root.parent().map(|p| p.join("i18n")) {
        let loaded = crate::cxxqt_session::ffi::linsync_install_translator(&QString::from(
            i18n_dir.to_string_lossy().as_ref(),
        ));
        if loaded {
            tracing::info!(dir = %i18n_dir.display(), "installed UI translation catalog");
        }
    }

    // AppImage builds bundle Breeze but cannot rely on the host's icon theme
    // search paths, so point Qt at the bundled AppDir/share/icons tree.
    icon_theme::set_icon_theme("breeze");

    let mut engine = QQmlApplicationEngine::new();
    engine
        .pin_mut()
        .add_import_path(&QString::from(qml_root.display().to_string()));
    let qml_url = QUrl::from_local_file(&QString::from(qml_file.display().to_string()));
    engine.pin_mut().load(&qml_url);

    let code = app.pin_mut().exec();
    Ok(ExitCode::from(code.clamp(0, u8::MAX as i32) as u8))
}

fn print_help() {
    println!(
        "LinSync GUI\n\nUsage: linsync [--print-qml-path] [--] [PATH...]\n\nEnvironment:\n  LINSYNC_QML_ROOT    Directory containing Main.qml\n  LINSYNC_QML_RUNNER  Qt QML runner command, defaulting to qml6/qml\n  LINSYNC_QML_HOST    Set to external to force the fallback QML runner when cxxqt-app is enabled"
    );
}

fn resolve_qml_file() -> Result<PathBuf, String> {
    qml_file_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "could not find LinSync QML resources; set LINSYNC_QML_ROOT".to_owned())
}

fn qml_file_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(root) = env::var_os("LINSYNC_QML_ROOT") {
        candidates.push(PathBuf::from(root).join("Main.qml"));
    }

    if let Ok(exe) = env::current_exe()
        && let Some(bin_dir) = exe.parent()
    {
        candidates.push(bin_dir.join("../share/linsync/qml/Main.qml"));
        candidates.push(bin_dir.join("../../share/linsync/qml/Main.qml"));
    }

    candidates.push(PathBuf::from("/app/share/linsync/qml/Main.qml"));
    candidates.push(PathBuf::from("/usr/local/share/linsync/qml/Main.qml"));
    candidates.push(PathBuf::from("/usr/share/linsync/qml/Main.qml"));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/Main.qml"));
    candidates
}

fn resolve_window_icon_file(qml_file: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(qml_root) = qml_file.parent() {
        candidates.push(qml_root.join("assets/com.visorcraft.LinSync.png"));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest_dir.join("qml/assets/com.visorcraft.LinSync.png"));
    candidates.push(
        manifest_dir.join("../../packaging/icons/hicolor/512x512/apps/com.visorcraft.LinSync.png"),
    );
    candidates.push(
        manifest_dir.join("../../packaging/icons/hicolor/scalable/apps/com.visorcraft.LinSync.svg"),
    );

    candidates.into_iter().find(|path| path.is_file())
}

fn resolve_qml_runner() -> Option<PathBuf> {
    if let Some(value) = env::var_os("LINSYNC_QML_RUNNER")
        && !value.is_empty()
    {
        return Some(PathBuf::from(value));
    }

    ["qml6", "qml"].into_iter().find_map(find_command_in_path)
}

fn find_command_in_path(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join(command))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
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
        let tab = build_tab_for_paths_with_mode(
            &left,
            &right,
            Some("Table"),
            &GuiCompareOptions::default(),
        );
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
                    &format!(
                        "GET /compare/table/window?offset={offset}&limit={limit} HTTP/1.1\r\n"
                    ),
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
        let tab = build_tab_for_paths_with_mode(
            &left,
            &right,
            Some("Hex"),
            &GuiCompareOptions::default(),
        );
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
        let shared = build_tab_for_paths_with_mode(
            &left,
            &right,
            Some("Text"),
            &GuiCompareOptions::default(),
        );

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
        let source_file =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/WebpageComparePage.qml");
        let qml =
            fs::read_to_string(&source_file).expect("WebpageComparePage.qml should be readable");
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
        let page_file =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qml/WebpageComparePage.qml");
        let page =
            fs::read_to_string(&page_file).expect("WebpageComparePage.qml should be readable");
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

        let settings = fs::read_to_string(qml_root.join("SettingsPage.qml"))
            .expect("SettingsPage should read");
        assert!(settings.contains("page.emit(\"reduceMotion\", checked)"));

        let nav =
            fs::read_to_string(qml_root.join("LinSyncNavItem.qml")).expect("nav item should read");
        assert!(nav.contains("duration: nav.reduceMotion ? 0 : 110"));

        let plugins =
            fs::read_to_string(qml_root.join("PluginsPage.qml")).expect("PluginsPage should read");
        assert!(plugins.contains("duration: page.reduceMotion ? 0 : 120"));
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
        assert!(path_looks_like_internal_test_fixture(Path::new(
            "/work/repos/visorcraft/linsync/tests/fixtures/text/left.txt"
        )));
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
        let same_response = String::from_utf8(bridge_response(&same_request, &paths, &state))
            .expect("utf-8 response");
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
        let response =
            String::from_utf8(bridge_response("GET /session HTTP/1.1\r\n", &paths, &state))
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
        let bridge = start_bridge_server(test_app_paths("bridge-health"), None)
            .expect("bridge should start");
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
        let response =
            String::from_utf8(bridge_response("GET /health HTTP/1.1\r\n", &paths, &state))
                .expect("utf-8 response");
        assert!(
            !response.contains("Access-Control-Allow-Origin"),
            "bridge must not advertise CORS to browser-origin pages: {response}"
        );
    }

    #[test]
    fn bridge_rejects_cross_origin_requests() {
        let bridge =
            start_bridge_server(test_app_paths("origin"), None).expect("bridge should start");
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
            &paths,
            &state,
            Some("secret-token"),
        ))
        .expect("utf-8 response");
        let present = String::from_utf8(bridge_response_with_token(
            "GET /secret-token/health HTTP/1.1\r\n",
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
                b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'~'
                | b'/'
                | b':' => vec![b],
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
}
