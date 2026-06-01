pub mod binary;
#[cfg(feature = "document-compare")]
pub mod document;
pub mod filter;
pub mod folder;
#[cfg(feature = "image-compare")]
pub mod image;
pub mod logging;
pub mod merge;
pub mod paths;
pub mod plugin;
pub mod profile;
pub mod storage;
pub mod table;
pub mod text;
pub mod trash;
pub mod webpage;

pub use binary::{
    BinaryCompareOptions, BinaryCompareResult, BinaryFileMetadata, BinaryMetadataCompare,
    BinaryMetadataDifference, ByteDiff, HexParseError, HexRow, SearchMatch, SearchSide,
    TypedInterpretation, TypedValueKind, compare_binary, compare_binary_files, is_likely_binary,
    parse_hex_pattern,
};
#[cfg(feature = "document-compare")]
pub use document::{
    DocumentCompareError, DocumentCompareMode, DocumentCompareOptions, DocumentCompareResult,
    compare_document_files, mime_hint_from_path, select_plugin_id,
};
pub use filter::{
    FileFilter, FilterAction, FilterDecision, FilterEntryContext, FilterFileKind,
    FilterMatchOptions, FilterParseError, FilterParseErrorKind, FilterRule, FilterTarget,
    MigratedFilter, PatternSyntax, migrate_filter_text,
};
pub use folder::{
    CompareMethod, FolderCompareControl, FolderCompareError, FolderCompareEvent,
    FolderCompareOptions, FolderCompareResult, FolderCompareStatus, FolderCompareSummary,
    FolderEntryDiff, FolderEntryFilter, FolderEntryState, FolderEntryType, FolderGrouping,
    FolderOperation, FolderOperationCounts, FolderOperationKind, FolderOperationOutcome,
    FolderOperationPlan, FolderOperationStatus, FolderOperationWarning, FolderOperationWarningKind,
    FolderQuery, FolderQueryGroup, FolderQueryPage, FolderSortKey, FolderTypeFilter, HashAlgorithm,
    SymlinkPolicy, assess_operation_risks, compare_folders, compare_folders_with_progress,
    execute_folder_operation_plan, plan_folder_operation,
};
#[cfg(feature = "image-compare")]
pub use image::{
    ImageCompareError, ImageCompareMode, ImageCompareOptions, ImageCompareResult,
    ImageFormatSupport, compare_images, compare_images_streaming, generate_overlay,
    supported_image_formats,
};
pub use logging::{LoggingError, init_file_logging, install_panic_log_hook};
pub use merge::{
    ConflictId, ConflictMarkerParseError, ConflictRegion, EditableDocument, MergeChoice,
    MergeError, ParsedConflictMarker, ThreeWayConflict, ThreeWayMergeError, ThreeWayMergeResult,
    ThreeWayMergeState, TwoWayMergeState, backup_path, create_save_plan, merge_three_way,
    parse_conflict_markers, restore_backup, write_encoded_text_with_plan, write_text_with_plan,
};
pub use paths::AppPaths;
pub use plugin::{
    CURRENT_PLUGIN_PROTOCOL_VERSION, CURRENT_PLUGIN_SCHEMA_VERSION, DiscoveredPlugin,
    PluginCancellationToken, PluginChunk, PluginClass, PluginDiagnostic, PluginDiscovery,
    PluginDiscoveryError, PluginError, PluginExecutionOptions, PluginExecutionResult,
    PluginInputDescriptor, PluginManifest, PluginOperation, PluginOperationError,
    PluginOperationOutput, PluginOperationRequest, PluginOperationResponse, PluginOperationStatus,
    PluginOption, PluginOptionError, PluginOptionKind, PluginOutputKind, PluginOutputStream,
    PluginProbeOutcome, PluginSandbox, PluginStoreError, PluginTextOperationOptions,
    PluginTextResult, SandboxStatus, UnpackFolderResponse, VirtualNode, active_sandbox_status,
    clear_plugin_option, discover_installed_plugins, discover_plugins, is_stable_plugin_id,
    load_plugin_enabled_map, load_plugin_options, plugin_discovery_roots, probe_plugin,
    resolve_enabled_prediffer, run_plugin_helper, run_prediffer_plugin,
    run_prediffer_plugin_with_options, run_streaming_plugin, run_unpack_folder_plugin,
    run_unpack_text_plugin, run_unpack_text_plugin_with_options, save_plugin_options,
    set_plugin_enabled, set_plugin_option,
};
pub use profile::builtin::{builtin_profile_ids, builtin_profiles, find_builtin};
pub use profile::{
    ActiveProfilePointer, CURRENT_PROFILE_SCHEMA_VERSION, CompareProfile, ProfileId, ProfileStore,
    ProfileStoreError, ProfileValidationError,
};
pub use storage::{
    ArtifactManifest, CompareArtifact, CompareViewMode, FilterStore, NamedFilters, ProjectFile,
    ProjectFileStore, RecentPathStore, RecentPaths, RecentSessionStore, RecentSessions,
    SessionFile, SessionFileStore, SessionLayout, Settings, SettingsStore, StoreError,
    ThemePreference, WindowSize, artifact_dir, cleanup_artifacts, save_artifact,
};
pub use table::{
    TableCellDiff, TableCellState, TableColumnRule, TableCompareOptions, TableCompareResult,
    TableError, TableParseError, TableRowDiff, compare_table_files, compare_tables,
    parse_delimited,
};
pub use text::{
    CompareOptions, CompareSession, CompareSide, CompareSummary, DiffAlgorithm, DiffBlock,
    DiffBlockKind, DiffLine, DiffLineKind, EncodingSummary, InlineDiff, InlineGranularity,
    LineEnding, MergeAction, MergeConflict, MoveDirection, SavePlan, SyntaxSpan, TextBookmark,
    TextCompareOptions, TextCompareResult, TextDocument, TextEncoding, TextFindMatch,
    TextFindOptions, TextInputEncoding, TextRegexRuleSet, TextRenderMode, TextSubstitution,
    TextSyntaxMode, TextViewRow, builtin_text_regex_rule_sets, compare_documents,
    compare_documents_cancellable, compare_text, compare_text_files,
    compare_text_files_cancellable, compare_text_files_with_prediffer, text_regex_rule_set,
};
pub use trash::{
    DeleteBackend, DeleteError, DeleteOutcome, DeletePlan, DeletePreference, DeleteRestoreGuidance,
    PermanentDeleteConfirmation, TrashedEntry, delete_restore_guidance, execute_delete_plan,
    move_to_freedesktop_trash, permanently_delete, plan_delete,
};
pub use webpage::{
    WebpageCompareError, WebpageCompareMode, WebpageCompareOptions, WebpageCompareResult,
    clear_webcompare_cache, compare_webpage_extracted_text, compare_webpage_html_source,
    compare_webpage_resource_tree, webcompare_cache_dir,
};
#[cfg(feature = "web-engine")]
pub use webpage::{WebpageRenderedResult, compare_webpage_rendered, compare_webpage_screenshot};
