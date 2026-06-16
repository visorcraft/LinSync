#[cfg(feature = "sandbox")]
pub mod archive;
pub mod archive_write;
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
pub mod syntax;
pub mod table;
pub mod text;
pub mod trash;
pub mod webpage;

#[cfg(feature = "sandbox")]
pub use archive::{
    ArchiveError, compare_builtin_archives, compare_builtin_archives_with_dirs,
    is_builtin_archive_format,
};
pub use archive_write::{
    ArchiveEditCaps, ArchiveWriteError, CommitOptions, CommitOutcome, MemberEditContext,
    commit_member_edit, extract_member_for_edit, extract_member_for_edit_with_caps,
    validate_member_path, verify_post_repack_listing,
};
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
    compare_virtual_trees, execute_folder_operation_plan, plan_folder_operation,
};
#[cfg(feature = "image-compare")]
pub use image::{
    FrameCompareMode, FrameSummary, ImageCompareError, ImageCompareMode, ImageCompareOptions,
    ImageCompareResult, ImageFormatSupport, compare_images, compare_images_all_frames,
    compare_images_streaming, generate_overlay, supported_image_formats,
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
    ExtractMemberResponse, PluginCancellationToken, PluginChunk, PluginClass, PluginDiagnostic,
    PluginDiscovery, PluginDiscoveryError, PluginError, PluginExecutionOptions,
    PluginExecutionResult, PluginInputDescriptor, PluginManifest, PluginOperation,
    PluginOperationError, PluginOperationOutput, PluginOperationRequest, PluginOperationResponse,
    PluginOperationStatus, PluginOption, PluginOptionError, PluginOptionKind, PluginOutputKind,
    PluginOutputStream, PluginProbeOutcome, PluginSandbox, PluginStoreError,
    PluginTextOperationOptions, PluginTextResult, PredifferConflictPolicy, RenderPagesResponse,
    SandboxStatus, UnpackFolderResponse, VirtualNode, WordPosition, active_sandbox_status,
    clear_plugin_option, compare_archives_with_unpacker, compare_archives_with_unpacker_recursive,
    discover_installed_plugins, discover_plugins, extract_archive_member, install_plugin,
    is_plugin_enabled_for_profile, is_plugin_trusted, is_stable_plugin_id, load_plugin_enabled_map,
    load_plugin_options, load_plugin_trusted_map, plugin_discovery_roots, probe_plugin,
    remove_plugin, resolve_enabled_prediffer, resolve_enabled_prediffers,
    resolve_enabled_virtualizer_for_extension, resolve_prediffer_conflicts, run_plugin_helper,
    run_prediffer_chain, run_prediffer_plugin, run_prediffer_plugin_with_options,
    run_render_pages_plugin, run_streaming_plugin, run_unpack_folder_plugin,
    run_unpack_text_plugin, run_unpack_text_plugin_with_options, save_plugin_options,
    set_plugin_enabled, set_plugin_option, set_plugin_trusted,
};
pub use profile::builtin::{builtin_profile_ids, builtin_profiles, find_builtin};
pub use profile::{
    ActiveProfilePointer, CURRENT_PROFILE_SCHEMA_VERSION, CompareProfile, ProfileId, ProfileStore,
    ProfileStoreError, ProfileValidationError,
};
pub use storage::{
    ArtifactManifest, CompareArtifact, CompareViewMode, FilterStore, NamedFilters, ProjectFile,
    ProjectFileStore, RecentPathStore, RecentPaths, RecentSessionStore, RecentSessions,
    SessionFile, SessionFileStore, SessionLayout, SessionResultSummary, Settings, SettingsStore,
    StoreError, ThemePreference, WindowSize, artifact_dir, cleanup_artifacts,
    relativize_session_paths_against, save_artifact,
};
pub use syntax::{SyntaxSpan, TextSyntaxMode, syntax_mode_from_path, syntax_spans};
pub use table::{
    TableCellDiff, TableCellState, TableColumnRule, TableCompareOptions, TableCompareResult,
    TableError, TableParseError, TableRowDiff, compare_table_files, compare_tables,
    parse_delimited,
};
pub use text::{
    CompareOptions, CompareSession, CompareSide, CompareSummary, DiffAlgorithm, DiffBlock,
    DiffBlockKind, DiffLine, DiffLineKind, EncodingSummary, InlineDiff, InlineGranularity,
    LineEnding, MergeAction, MergeConflict, MoveDirection, SavePlan, TextBookmark,
    TextCompareOptions, TextCompareResult, TextDocument, TextEncoding, TextFindMatch,
    TextFindOptions, TextInputEncoding, TextRegexRuleSet, TextRenderMode, TextSubstitution,
    TextViewPage, TextViewRow, builtin_text_regex_rule_sets, compare_documents,
    compare_documents_cancellable, compare_text, compare_text_files,
    compare_text_files_cancellable, compare_text_files_with_prediffer,
    compare_text_files_with_prediffer_chain, text_regex_rule_set,
};
pub use trash::{
    DeleteBackend, DeleteError, DeleteOutcome, DeletePlan, DeletePreference, DeleteRestoreGuidance,
    PermanentDeleteConfirmation, TrashedEntry, delete_restore_guidance, execute_delete_plan,
    move_to_freedesktop_trash, permanent_delete_warning, permanently_delete, plan_delete,
};
pub use webpage::{
    WebpageCompareError, WebpageCompareMode, WebpageCompareOptions, WebpageCompareResult,
    clear_webcompare_cache, compare_webpage_extracted_text, compare_webpage_html_source,
    compare_webpage_resource_tree, webcompare_cache_dir,
};
#[cfg(feature = "web-engine")]
pub use webpage::{
    WebpageRenderedResult, active_renderer_kind, compare_webpage_rendered,
    compare_webpage_screenshot,
};
