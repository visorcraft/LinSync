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

use linsync::{apply_gui_setting, parse_bool_setting, percent_decode};
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
    compare_documents_cancellable, compare_folders, compare_folders_with_progress,
    compare_images_cancellable, compare_table_files, compare_text,
    compare_text_files_with_prediffer, create_save_plan, discover_installed_plugins,
    execute_folder_operation_plan, find_builtin, is_likely_binary, permanent_delete_warning,
    plan_folder_operation, save_artifact, write_encoded_text_with_plan,
};
use serde::{Deserialize, Serialize};

const BRIDGE_VERSION: u32 = 1;
const RESPONSE_SCHEMA_VERSION: u32 = 1;
const GUI_TAB_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
/// Key under `SessionLayout.extra` holding the multi-tab snapshot JSON.
const GUI_TABS_SNAPSHOT_KEY: &str = "gui_tabs_snapshot";

#[cfg(feature = "cxxqt-app")]
mod cxxqt_translator;
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
    confirm_on_close: bool,
    persist_recent_paths: bool,
    max_recent_paths: usize,
    reduce_motion: bool,
    keep_archive_backup: bool,
    live_compare: bool,
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
            confirm_on_close: settings.confirm_on_close,
            persist_recent_paths: settings.persist_recent_paths,
            max_recent_paths: settings.recent_limit,
            reduce_motion: settings.reduce_motion,
            keep_archive_backup: settings.keep_archive_backup,
            live_compare: settings.live_compare,
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
    /// Cached folder compare result for the active folder tab, so `/folder/query`
    /// and folder-op plan/execute can page/sort/filter without re-running the
    /// comparison. Invalidated whenever a new compare is applied.
    folder_compare_cache: Option<FolderCompareCache>,
}

struct FolderCompareCache {
    left: PathBuf,
    right: PathBuf,
    options: linsync_core::FolderCompareOptions,
    result: Arc<linsync_core::FolderCompareResult>,
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

/// RAII guard for a registered `/progress?id=X` entry. On drop it removes the
/// snapshot from `GuiBridgeState::compare_progress`, so an early return or
/// panic cannot leak progress entries.
struct ProgressGuard {
    request_id: Option<String>,
    state: Arc<Mutex<GuiBridgeState>>,
    progress: Option<Arc<Mutex<CompareProgress>>>,
}

impl ProgressGuard {
    fn none(state: Arc<Mutex<GuiBridgeState>>) -> Self {
        Self {
            request_id: None,
            state,
            progress: None,
        }
    }

    fn progress(&self) -> Option<Arc<Mutex<CompareProgress>>> {
        self.progress.clone()
    }
}

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        if let Some(id) = self.request_id.take()
            && let Ok(mut state) = self.state.lock()
        {
            state.compare_progress.remove(&id);
        }
    }
}

fn register_progress_request(
    params: &[(String, String)],
    state: &Arc<Mutex<GuiBridgeState>>,
    phase: &str,
    total: usize,
    message: &str,
) -> ProgressGuard {
    let Some(request_id) = query_value(params, "request_id").map(str::to_owned) else {
        return ProgressGuard::none(Arc::clone(state));
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
    ProgressGuard {
        request_id: Some(request_id),
        state: Arc::clone(state),
        progress: Some(progress),
    }
}

/// A registered cancellable compare request. Dropping it removes both the
/// cancel flag and the progress entry, so panics or early returns cannot leak
/// either map entry.
struct CancellableRequest {
    progress_guard: ProgressGuard,
    should_cancel: Box<dyn Fn() -> bool>,
    cancellation_token: linsync_core::plugin::PluginCancellationToken,
}

impl CancellableRequest {
    /// Polls the cancellation flag. Returns `false` when no `request_id`
    /// was supplied (nothing to cancel).
    fn is_cancelled(&self) -> bool {
        (self.should_cancel)()
    }

    fn progress(&self) -> Option<Arc<Mutex<CompareProgress>>> {
        self.progress_guard.progress()
    }

    fn cancel_checker(&self) -> &dyn Fn() -> bool {
        &*self.should_cancel
    }

    fn cancellation_token(&self) -> linsync_core::plugin::PluginCancellationToken {
        self.cancellation_token.clone()
    }
}

impl Drop for CancellableRequest {
    fn drop(&mut self) {
        if let Some(id) = &self.progress_guard.request_id
            && let Ok(mut state) = self.progress_guard.state.lock()
        {
            state.compare_cancels.remove(id);
        }
    }
}

/// Register both a progress tracker and a cancellation flag for a compare
/// request. The returned guard cleans up both entries when dropped.
fn register_cancellable_request(
    params: &[(String, String)],
    state: &Arc<Mutex<GuiBridgeState>>,
    phase: &str,
    total: usize,
    message: &str,
) -> CancellableRequest {
    let progress_guard = register_progress_request(params, state, phase, total, message);
    let (should_cancel, cancellation_token): (Box<dyn Fn() -> bool>, _) =
        if let Some(id) = &progress_guard.request_id {
            let flag = Arc::new(AtomicBool::new(false));
            let token = linsync_core::plugin::PluginCancellationToken::from_arc(Arc::clone(&flag));
            if let Ok(mut state) = state.lock() {
                state.compare_cancels.insert(id.clone(), Arc::clone(&flag));
            }
            (Box::new(move || flag.load(Ordering::Relaxed)), token)
        } else {
            (
                Box::new(|| false),
                linsync_core::plugin::PluginCancellationToken::new(),
            )
        };
    CancellableRequest {
        progress_guard,
        should_cancel,
        cancellation_token,
    }
}

const GUI_HISTORY_LIMIT: usize = 16;

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
            folder_compare_cache: None,
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
        self.folder_compare_cache = None;
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
            std::thread::spawn(move || {
                let _ = fs::remove_dir_all(dir);
            });
        }
        if let Some(dir) = self.archive_extract_dirs.remove(&tab_id) {
            std::thread::spawn(move || {
                let _ = fs::remove_dir_all(dir);
            });
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
        rederive_syntax_spans(tab);
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
        rederive_syntax_spans(tab);
        tab.status = "Redid last merge action".to_owned();
        self.push_undo_snapshot(active_tab_id, current);
        Ok(self.context())
    }

    /// Phase 1 of a save, taken under the state lock: validate the active tab,
    /// resolve which side(s) to write, and clone their rows/path out so the
    /// actual file I/O can happen without holding the lock. Does no disk I/O
    /// and mutates no dirty/status flags.
    fn prepare_save(&mut self, side: &str) -> Result<PreparedSave, String> {
        let tab_id = self.session.active_tab_id;
        let tab = self
            .session
            .tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id)
            .ok_or_else(|| "no active compare tab".to_owned())?;
        if tab.mode != "Text" {
            return Err("save currently supports text compare tabs only".to_owned());
        }

        let scope = match side {
            "left" => SaveScope::Left,
            "right" => SaveScope::Right,
            "dirty" | "all" => SaveScope::DirtyAll,
            other => return Err(format!("unsupported save side: {other}")),
        };

        // Build the ordered candidate list. Order matters for DirtyAll status
        // messaging ("Saved left and right"); clean sides are skipped so the
        // "already clean" / "no dirty sides" outcomes derive from scope + an
        // empty sides list in `finish_save`, preserving the old semantics.
        let mut sides = Vec::new();
        for (label, dirty, path, rows) in [
            ("left", tab.left_dirty, &tab.left_path, &tab.left_rows),
            ("right", tab.right_dirty, &tab.right_path, &tab.right_rows),
        ] {
            if !dirty {
                continue;
            }
            if path.is_empty() {
                return Err(format!("cannot save {label} side without a path"));
            }
            sides.push(PreparedSaveSide {
                label,
                path: path.to_owned(),
                rows: rows.to_vec(),
            });
        }
        Ok(PreparedSave {
            tab_id,
            scope,
            sides,
        })
    }

    /// Phase 3 of a save, taken under the state lock again: clear the dirty
    /// flag for each side that was written — but only if its on-tab rows still
    /// match what we wrote, so a concurrent edit/copy between prepare and
    /// finish isn't silently marked saved — then set the tab status.
    fn finish_save(
        &mut self,
        prepared: &PreparedSave,
        attempt: SaveAttempt,
    ) -> Result<GuiLaunchContext, String> {
        let tab = self
            .session
            .tabs
            .iter_mut()
            .find(|tab| tab.id == prepared.tab_id)
            .ok_or_else(|| "no active compare tab".to_owned())?;

        for side in &prepared.sides {
            if !attempt.saved.contains(&side.label) {
                continue;
            }
            let still_matches = match side.label {
                "left" => rows_text_eq(&tab.left_rows, &side.rows),
                "right" => rows_text_eq(&tab.right_rows, &side.rows),
                _ => false,
            };
            if still_matches {
                match side.label {
                    "left" => tab.left_dirty = false,
                    "right" => tab.right_dirty = false,
                    _ => {}
                }
            }
        }

        tab.status = match (&attempt.error, attempt.saved.as_slice()) {
            (Some(err), &[]) => err.clone(),
            (_, saved) => compute_save_status(prepared.scope, saved),
        };

        if let Some(err) = attempt.error {
            return Err(err);
        }
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
        strip_syntax_spans(&mut snapshot);
        push_limited_snapshot(self.undo_stacks.entry(tab_id).or_default(), snapshot);
    }

    fn push_redo_snapshot(&mut self, tab_id: u64, mut snapshot: GuiCompareTab) {
        snapshot.can_undo = false;
        snapshot.can_redo = false;
        strip_syntax_spans(&mut snapshot);
        push_limited_snapshot(self.redo_stacks.entry(tab_id).or_default(), snapshot);
    }
}

/// Clear syntax-highlight spans from every row in a snapshot to keep undo/redo
/// memory bounded — each `SyntaxSpan` carries a heap-allocated `class: String`,
/// and with thousands of rows × dozens of spans/row, retaining 32 full-deep
/// clones can consume hundreds of MB. Spans are cheaply re-derived from the
/// row text + syntax mode on undo/redo via `rederive_syntax_spans`.
fn strip_syntax_spans(snapshot: &mut GuiCompareTab) {
    for row in &mut snapshot.left_rows {
        row.syntax_spans.clear();
    }
    for row in &mut snapshot.right_rows {
        row.syntax_spans.clear();
    }
}

/// Re-derive syntax-highlight spans for a tab's rows after restoring from a
/// stripped undo/redo snapshot. Skips quietly if the mode can't be resolved
/// (rows will simply render without highlighting until the next recompare).
fn rederive_syntax_spans(tab: &mut GuiCompareTab) {
    let mode = tab
        .options
        .as_ref()
        .map(|opts| opts.text.syntax_mode)
        .unwrap_or_default();
    // Resolve Auto using the tab's file paths, mirroring core's
    // resolved_syntax_mode — syntax_spans(text, Auto) returns empty spans
    // because Auto must be mapped to a concrete language via extension first.
    let resolved = if matches!(mode, linsync_core::TextSyntaxMode::Auto) {
        linsync_core::syntax_mode_from_path(Path::new(&tab.left_path))
            .or_else(|| linsync_core::syntax_mode_from_path(Path::new(&tab.right_path)))
            .unwrap_or(linsync_core::TextSyntaxMode::Plain)
    } else {
        mode
    };
    if matches!(resolved, linsync_core::TextSyntaxMode::Plain) {
        return;
    }
    for row in &mut tab.left_rows {
        if row.syntax_spans.is_empty() && !row.text.is_empty() {
            row.syntax_spans = linsync_core::syntax_spans(&row.text, resolved);
        }
    }
    for row in &mut tab.right_rows {
        if row.syntax_spans.is_empty() && !row.text.is_empty() {
            row.syntax_spans = linsync_core::syntax_spans(&row.text, resolved);
        }
    }
}

fn push_limited_snapshot(stack: &mut Vec<GuiCompareTab>, snapshot: GuiCompareTab) {
    stack.push(snapshot);
    if stack.len() > GUI_HISTORY_LIMIT {
        stack.remove(0);
    }
}

/// Which side(s) a save request targets. Drives the status-string computation
/// in `finish_save` so it matches the old single-pass implementation exactly.
#[derive(Clone, Copy)]
enum SaveScope {
    Left,
    Right,
    DirtyAll,
}

/// One side's data, cloned out from under the state lock so the file write can
/// happen without holding it.
struct PreparedSaveSide {
    label: &'static str,
    path: String,
    rows: Vec<GuiLineRow>,
}

/// The collected inputs for a save, produced by phase 1 (`prepare_save`) and
/// consumed by phases 2 and 3.
struct PreparedSave {
    tab_id: u64,
    scope: SaveScope,
    sides: Vec<PreparedSaveSide>,
}

/// Result of phase 2: the sides that were actually written (in order), plus the
/// first error encountered (if any). Sides are attempted in order, so a later
/// side isn't tried once an earlier one fails — matching the old behavior.
struct SaveAttempt {
    saved: Vec<&'static str>,
    error: Option<String>,
}

/// Phase 2 (no state lock): write each prepared side to disk in order. Done
/// outside the lock so a slow/networked filesystem can't freeze every other
/// bridge request (`/progress`, `/cancel`, tab activation, ...) for the
/// duration of the write.
fn perform_save(sides: &[PreparedSaveSide]) -> SaveAttempt {
    let mut saved = Vec::new();
    for side in sides {
        match write_save_side(side) {
            Ok(()) => saved.push(side.label),
            Err(err) => {
                return SaveAttempt {
                    saved,
                    error: Some(err),
                };
            }
        }
    }
    SaveAttempt { saved, error: None }
}

fn write_save_side(side: &PreparedSaveSide) -> Result<(), String> {
    let label = side.label;
    let target = PathBuf::from(&side.path);
    let document = TextDocument::from_path(&target)
        .map_err(|err| format!("failed to read {label} side before save: {err}"))?;
    if document.read_only {
        return Err(format!("cannot save read-only {label} side"));
    }
    let contents = rows_to_document_text(&side.rows, &document);
    let plan = create_save_plan(&target, true);
    write_encoded_text_with_plan(&plan, &contents, document.encoding)
        .map_err(|err| format!("failed to save {label} side: {err}"))
}

/// Recompute the tab status after a save, faithful to the old single-pass
/// implementation: explicit single-side saves say "... side with backup" /
/// "... side already clean"; "dirty"/"all" saves list the sides written.
fn compute_save_status(scope: SaveScope, saved: &[&'static str]) -> String {
    match scope {
        SaveScope::Left | SaveScope::Right => {
            let label = match scope {
                SaveScope::Left => "left",
                SaveScope::Right => "right",
                SaveScope::DirtyAll => unreachable!("handled in the DirtyAll arm"),
            };
            if saved.is_empty() {
                format!("{label} side already clean")
            } else {
                format!("Saved {label} side with backup")
            }
        }
        SaveScope::DirtyAll => {
            if saved.is_empty() {
                "No dirty sides to save".to_owned()
            } else {
                format!("Saved {}", saved.join(" and "))
            }
        }
    }
}

/// Cheap content check used by `finish_save` to decide whether clearing the
/// dirty flag is still safe: if the side's row texts changed between prepare
/// and finish (a concurrent copy/edit), leave it dirty so the user is prompted
/// to save again rather than silently losing the new edit.
fn rows_text_eq(a: &[GuiLineRow], b: &[GuiLineRow]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.text == y.text)
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
    let any_needs_windowing = context.session.tabs.iter().any(tab_needs_windowing);
    if !any_needs_windowing {
        return serde_json::to_value(context);
    }

    let tab_values: Result<Vec<serde_json::Value>, _> = context
        .session
        .tabs
        .iter()
        .map(|tab| {
            if tab_needs_windowing(tab) {
                let mut w = tab.clone();
                apply_text_windowing(&mut w);
                apply_folder_windowing(&mut w);
                apply_binary_windowing(&mut w);
                apply_table_windowing(&mut w);
                serde_json::to_value(&w)
            } else {
                serde_json::to_value(tab)
            }
        })
        .collect();

    let mut session_map = serde_json::Map::new();
    session_map.insert(
        "active_tab_id".into(),
        serde_json::to_value(context.session.active_tab_id)?,
    );
    session_map.insert("tabs".into(), serde_json::Value::Array(tab_values?));
    session_map.insert(
        "recent_paths".into(),
        serde_json::to_value(&context.session.recent_paths)?,
    );

    let mut value = serde_json::Map::new();
    value.insert("session".into(), serde_json::Value::Object(session_map));
    if let Some(ref section) = context.startup_section {
        value.insert(
            "startup_section".into(),
            serde_json::Value::String(section.clone()),
        );
    }
    if let Some(ref merge) = context.merge {
        value.insert("merge".into(), serde_json::to_value(merge)?);
    }
    Ok(serde_json::Value::Object(value))
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
                Some(image_tab(
                    left,
                    right,
                    left_path,
                    right_path,
                    options,
                    should_cancel,
                )),
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
                    // Discovered fires per-side (left walk + right walk), so
                    // the unique entry count is roughly half. Use that as the
                    // compare-phase total estimate; .max() self-corrects for
                    // asymmetric trees where one side dominates.
                    p.total = (discovered_total / 2).max(compared_count);
                    p.message = relative_path.display().to_string();
                }
            }
            FolderCompareEvent::Completed { .. } | FolderCompareEvent::Cancelled { .. } => {
                if let Some(p) = &progress
                    && let Ok(mut p) = p.lock()
                {
                    p.phase = "done".to_owned();
                    p.current = compared_count;
                    p.total = compared_count;
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
    should_cancel: &dyn Fn() -> bool,
) -> GuiCompareTab {
    let mut image_options = options.image.clone();
    if image_options.timeout_secs == 0 {
        image_options.timeout_secs = 300;
    }
    match compare_images_cancellable(left, right, &image_options, should_cancel) {
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
            None,
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

mod bridge;
pub(crate) use bridge::*;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
