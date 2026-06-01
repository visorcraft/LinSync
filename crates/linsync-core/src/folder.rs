use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufReader, Read};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::binary::is_likely_binary;
use crc32fast::Hasher as Crc32Hasher;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::filter::{
    FileFilter, FilterDecision, FilterEntryContext, FilterFileKind, FilterMatchOptions,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderCompareOptions {
    pub recursive: bool,
    pub compare_method: CompareMethod,
    pub timestamp_tolerance: Duration,
    pub filters: Vec<FileFilter>,
    pub filter_match_options: FilterMatchOptions,
    pub include_skipped: bool,
    pub symlink_policy: SymlinkPolicy,
    pub large_file_threshold: Option<u64>,
    pub large_file_fallback_method: CompareMethod,
    pub hash_algorithm: HashAlgorithm,
    pub compare_permissions: bool,
    pub compare_ownership: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompareMethod {
    FullContents,
    QuickContents,
    BinaryContents,
    ModifiedDate,
    DateAndSize,
    Size,
    Existence,
    HashBlake3,
    NormalizedText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SymlinkPolicy {
    CompareTarget,
    Follow,
    SpecialFile,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HashAlgorithm {
    #[default]
    Blake3,
    Sha256,
    Crc32,
}

impl Default for FolderCompareOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            compare_method: CompareMethod::BinaryContents,
            timestamp_tolerance: Duration::ZERO,
            filters: Vec::new(),
            filter_match_options: FilterMatchOptions::default(),
            include_skipped: true,
            symlink_policy: SymlinkPolicy::CompareTarget,
            large_file_threshold: None,
            large_file_fallback_method: CompareMethod::BinaryContents,
            hash_algorithm: HashAlgorithm::default(),
            compare_permissions: false,
            compare_ownership: false,
        }
    }
}

impl CompareMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullContents => "full-contents",
            Self::QuickContents => "quick-contents",
            Self::BinaryContents => "binary-contents",
            Self::ModifiedDate => "modified-date",
            Self::DateAndSize => "date-size",
            Self::Size => "size",
            Self::Existence => "existence",
            Self::HashBlake3 => "hash-blake3",
            Self::NormalizedText => "normalized-text",
        }
    }
}

#[derive(Debug)]
pub enum FolderCompareError {
    Io(io::Error),
    NotDirectory(PathBuf),
}

impl std::fmt::Display for FolderCompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::NotDirectory(path) => write!(f, "not a directory: {}", path.display()),
        }
    }
}

impl std::error::Error for FolderCompareError {}

impl From<io::Error> for FolderCompareError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderCompareResult {
    pub left_root: PathBuf,
    pub right_root: PathBuf,
    pub entries: Vec<FolderEntryDiff>,
    pub summary: FolderCompareSummary,
}

impl FolderCompareResult {
    pub fn is_equal(&self) -> bool {
        self.entries.iter().all(|entry| {
            matches!(
                entry.state,
                FolderEntryState::Identical | FolderEntryState::Skipped
            )
        })
    }

    pub fn filtered_entries(&self, filter: FolderEntryFilter) -> Vec<&FolderEntryDiff> {
        self.entries
            .iter()
            .filter(|entry| filter.matches(entry.state))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderCompareSummary {
    pub compared_count: usize,
    pub skipped_count: usize,
    pub identical_count: usize,
    pub different_count: usize,
    pub one_sided_count: usize,
    pub left_only_count: usize,
    pub right_only_count: usize,
    pub errors_count: usize,
    pub aborted_count: usize,
    pub method_downgrade_count: usize,
    pub elapsed: Duration,
    pub status: FolderCompareStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FolderCompareStatus {
    #[default]
    Complete,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderEntryDiff {
    pub relative_path: PathBuf,
    pub name: String,
    pub extension: Option<String>,
    pub state: FolderEntryState,
    pub left_size: Option<u64>,
    pub right_size: Option<u64>,
    pub left_modified: Option<SystemTime>,
    pub right_modified: Option<SystemTime>,
    pub entry_type: FolderEntryType,
    pub effective_method: Option<CompareMethod>,
    pub method_note: Option<String>,
    pub is_dir: bool,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_permissions: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_permissions: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FolderEntryType {
    File,
    Directory,
    Symlink,
    Special,
}

impl FolderEntryType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Special => "special",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FolderEntryState {
    Identical,
    Different,
    LeftOnly,
    RightOnly,
    Skipped,
    Error,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FolderEntryFilter {
    #[default]
    All,
    Differences,
    Identical,
    Different,
    LeftOnly,
    RightOnly,
    Errors,
    Skipped,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderCompareEvent {
    Discovered {
        relative_path: PathBuf,
    },
    Compared {
        relative_path: PathBuf,
        state: FolderEntryState,
    },
    Skipped {
        relative_path: PathBuf,
    },
    Error {
        relative_path: PathBuf,
        message: String,
    },
    Completed {
        summary: FolderCompareSummary,
    },
    Cancelled {
        completed: usize,
        aborted: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderCompareControl {
    Continue,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderOperationKind {
    CopyLeftToRight,
    CopyRightToLeft,
    DeleteLeft,
    DeleteRight,
    RenameLeft { new_name: String },
    RenameRight { new_name: String },
    CreateMissingLeft,
    CreateMissingRight,
    Refresh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderOperationPlan {
    pub operations: Vec<FolderOperation>,
    pub counts: FolderOperationCounts,
    pub warnings: Vec<FolderOperationWarning>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderOperationCounts {
    pub copy_count: usize,
    pub delete_count: usize,
    pub rename_count: usize,
    pub create_folder_count: usize,
    pub refresh_count: usize,
    pub overwrite_warning_count: usize,
    pub permission_warning_count: usize,
    pub conflict_warning_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderOperation {
    pub kind: FolderOperationKind,
    pub relative_path: PathBuf,
    pub source: Option<PathBuf>,
    pub target: Option<PathBuf>,
    pub overwrites_existing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FolderOperationWarning {
    pub relative_path: PathBuf,
    pub kind: FolderOperationWarningKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FolderOperationWarningKind {
    Overwrite,
    Permission,
    Conflict,
    InvalidSelection,
    OverwriteExisting,
    DeleteReadOnly,
    CrossDeviceCopy,
    SymlinkTraversal,
    PermissionDenied,
    TargetNewer,
    SourceLarger,
}

impl FolderEntryFilter {
    pub fn matches(self, state: FolderEntryState) -> bool {
        match self {
            Self::All => true,
            Self::Differences => matches!(
                state,
                FolderEntryState::Different
                    | FolderEntryState::LeftOnly
                    | FolderEntryState::RightOnly
            ),
            Self::Identical => state == FolderEntryState::Identical,
            Self::Different => state == FolderEntryState::Different,
            Self::LeftOnly => state == FolderEntryState::LeftOnly,
            Self::RightOnly => state == FolderEntryState::RightOnly,
            Self::Skipped => state == FolderEntryState::Skipped,
            Self::Errors => state == FolderEntryState::Error,
            Self::Aborted => state == FolderEntryState::Aborted,
        }
    }
}

impl FolderEntryState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identical => "identical",
            Self::Different => "different",
            Self::LeftOnly => "left-only",
            Self::RightOnly => "right-only",
            Self::Skipped => "skipped",
            Self::Error => "error",
            Self::Aborted => "aborted",
        }
    }
}

/// Selects which entry *types* a folder-result query keeps. Every flag defaults
/// to `true`, so a default filter is a no-op that retains every entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderTypeFilter {
    pub files: bool,
    pub directories: bool,
    pub symlinks: bool,
    pub special: bool,
}

impl Default for FolderTypeFilter {
    fn default() -> Self {
        Self {
            files: true,
            directories: true,
            symlinks: true,
            special: true,
        }
    }
}

impl FolderTypeFilter {
    pub fn matches(self, entry_type: FolderEntryType) -> bool {
        match entry_type {
            FolderEntryType::File => self.files,
            FolderEntryType::Directory => self.directories,
            FolderEntryType::Symlink => self.symlinks,
            FolderEntryType::Special => self.special,
        }
    }

    /// True when no type is excluded (the filter keeps everything).
    pub fn is_unrestricted(self) -> bool {
        self.files && self.directories && self.symlinks && self.special
    }
}

/// Column a folder-result query sorts on. Ties always break on the relative
/// path so the ordering is fully deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FolderSortKey {
    /// File/dir name, compared case-insensitively.
    Name,
    /// Full relative path (the default).
    #[default]
    Path,
    /// Comparison state.
    State,
    /// Entry type.
    Type,
    /// Larger of the two side sizes.
    Size,
    /// Most recent of the two side modification times.
    Modified,
}

/// How a folder-result query buckets the returned page. Group order follows
/// first appearance in the sorted page, not alphabetical label order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FolderGrouping {
    #[default]
    None,
    State,
    Type,
    Directory,
}

/// A reusable, client-agnostic description of how to view a folder result:
/// filter by state and type, full-text search the relative path, sort, group,
/// and paginate. Clients (CLI, GUI) build one of these and call
/// [`FolderCompareResult::query`] rather than re-implementing the logic.
#[derive(Debug, Clone, Default)]
pub struct FolderQuery {
    pub state: FolderEntryFilter,
    pub types: FolderTypeFilter,
    /// Case-insensitive substring matched against the relative path. An empty
    /// or absent value matches everything.
    pub search: Option<String>,
    pub sort: FolderSortKey,
    pub descending: bool,
    pub group_by: FolderGrouping,
    pub offset: usize,
    /// Maximum entries to return after the offset. `None` means no limit.
    pub limit: Option<usize>,
}

/// One bucket of query results. With [`FolderGrouping::None`] there is a single
/// group whose `label` is empty.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderQueryGroup<'a> {
    pub label: String,
    pub entries: Vec<&'a FolderEntryDiff>,
}

/// The outcome of a [`FolderQuery`]: the matched total (before pagination), the
/// applied offset, the entries on this page (grouped), and whether more follow.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderQueryPage<'a> {
    /// Entries matching the filter + search, before offset/limit are applied.
    pub total_matched: usize,
    /// The offset actually applied (clamped to `total_matched`).
    pub offset: usize,
    /// Entries returned on this page (the sum across `groups`).
    pub returned: usize,
    /// True when entries remain beyond this page.
    pub has_more: bool,
    pub groups: Vec<FolderQueryGroup<'a>>,
}

fn folder_entry_size(entry: &FolderEntryDiff) -> u64 {
    entry
        .left_size
        .unwrap_or(0)
        .max(entry.right_size.unwrap_or(0))
}

fn folder_entry_modified(entry: &FolderEntryDiff) -> Option<SystemTime> {
    match (entry.left_modified, entry.right_modified) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (only, None) | (None, only) => only,
    }
}

fn folder_state_rank(state: FolderEntryState) -> u8 {
    match state {
        FolderEntryState::Different => 0,
        FolderEntryState::LeftOnly => 1,
        FolderEntryState::RightOnly => 2,
        FolderEntryState::Identical => 3,
        FolderEntryState::Skipped => 4,
        FolderEntryState::Error => 5,
        FolderEntryState::Aborted => 6,
    }
}

fn folder_type_rank(entry_type: FolderEntryType) -> u8 {
    match entry_type {
        FolderEntryType::Directory => 0,
        FolderEntryType::File => 1,
        FolderEntryType::Symlink => 2,
        FolderEntryType::Special => 3,
    }
}

fn folder_group_label(entry: &FolderEntryDiff, grouping: FolderGrouping) -> String {
    match grouping {
        FolderGrouping::None => String::new(),
        FolderGrouping::State => entry.state.as_str().to_string(),
        FolderGrouping::Type => entry.entry_type.as_str().to_string(),
        FolderGrouping::Directory => entry
            .relative_path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| ".".to_string()),
    }
}

impl FolderCompareResult {
    /// Apply a [`FolderQuery`] to the entries: filter by state/type, search the
    /// relative path, sort, paginate, then group the resulting page. The
    /// returned page borrows from `self`.
    pub fn query(&self, query: &FolderQuery) -> FolderQueryPage<'_> {
        let needle = query
            .search
            .as_deref()
            .map(str::to_lowercase)
            .filter(|s| !s.is_empty());

        let mut matched: Vec<&FolderEntryDiff> = self
            .entries
            .iter()
            .filter(|entry| query.state.matches(entry.state))
            .filter(|entry| query.types.matches(entry.entry_type))
            .filter(|entry| match &needle {
                None => true,
                Some(needle) => entry
                    .relative_path
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(needle.as_str()),
            })
            .collect();

        matched.sort_by(|a, b| {
            let primary = match query.sort {
                FolderSortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                FolderSortKey::Path => a.relative_path.cmp(&b.relative_path),
                FolderSortKey::State => folder_state_rank(a.state).cmp(&folder_state_rank(b.state)),
                FolderSortKey::Type => {
                    folder_type_rank(a.entry_type).cmp(&folder_type_rank(b.entry_type))
                }
                FolderSortKey::Size => folder_entry_size(a).cmp(&folder_entry_size(b)),
                FolderSortKey::Modified => folder_entry_modified(a).cmp(&folder_entry_modified(b)),
            };
            let ordered = primary.then_with(|| a.relative_path.cmp(&b.relative_path));
            if query.descending {
                ordered.reverse()
            } else {
                ordered
            }
        });

        let total_matched = matched.len();
        let offset = query.offset.min(total_matched);
        let end = match query.limit {
            Some(limit) => offset.saturating_add(limit).min(total_matched),
            None => total_matched,
        };
        let page = &matched[offset..end];
        let has_more = end < total_matched;

        let groups = if matches!(query.group_by, FolderGrouping::None) {
            if page.is_empty() {
                Vec::new()
            } else {
                vec![FolderQueryGroup {
                    label: String::new(),
                    entries: page.to_vec(),
                }]
            }
        } else {
            let mut groups: Vec<FolderQueryGroup<'_>> = Vec::new();
            for &entry in page {
                let label = folder_group_label(entry, query.group_by);
                match groups.iter_mut().find(|group| group.label == label) {
                    Some(group) => group.entries.push(entry),
                    None => groups.push(FolderQueryGroup {
                        label,
                        entries: vec![entry],
                    }),
                }
            }
            groups
        };

        FolderQueryPage {
            total_matched,
            offset,
            returned: page.len(),
            has_more,
            groups,
        }
    }
}

pub fn plan_folder_operation(
    result: &FolderCompareResult,
    kind: FolderOperationKind,
    selected_paths: &[PathBuf],
) -> FolderOperationPlan {
    let selected = selected_paths.iter().collect::<BTreeSet<_>>();
    let mut operations = Vec::new();
    let mut warnings = Vec::new();

    for entry in result
        .entries
        .iter()
        .filter(|entry| selected.is_empty() || selected.contains(&entry.relative_path))
    {
        match &kind {
            FolderOperationKind::CopyLeftToRight => plan_copy(
                result,
                entry,
                kind.clone(),
                &result.left_root,
                &result.right_root,
                &mut operations,
                &mut warnings,
            ),
            FolderOperationKind::CopyRightToLeft => plan_copy(
                result,
                entry,
                kind.clone(),
                &result.right_root,
                &result.left_root,
                &mut operations,
                &mut warnings,
            ),
            FolderOperationKind::DeleteLeft => plan_delete_side(
                result,
                entry,
                kind.clone(),
                &result.left_root,
                true,
                &mut operations,
                &mut warnings,
            ),
            FolderOperationKind::DeleteRight => plan_delete_side(
                result,
                entry,
                kind.clone(),
                &result.right_root,
                false,
                &mut operations,
                &mut warnings,
            ),
            FolderOperationKind::RenameLeft { new_name }
            | FolderOperationKind::RenameRight { new_name } => {
                let root = if matches!(&kind, FolderOperationKind::RenameLeft { .. }) {
                    &result.left_root
                } else {
                    &result.right_root
                };
                plan_rename(
                    entry,
                    kind.clone(),
                    root,
                    new_name,
                    &mut operations,
                    &mut warnings,
                );
            }
            FolderOperationKind::CreateMissingLeft => plan_create_missing(
                entry,
                kind.clone(),
                &result.left_root,
                entry.state == FolderEntryState::RightOnly,
                &mut operations,
                &mut warnings,
            ),
            FolderOperationKind::CreateMissingRight => plan_create_missing(
                entry,
                kind.clone(),
                &result.right_root,
                entry.state == FolderEntryState::LeftOnly,
                &mut operations,
                &mut warnings,
            ),
            FolderOperationKind::Refresh => operations.push(FolderOperation {
                kind: kind.clone(),
                relative_path: entry.relative_path.clone(),
                source: None,
                target: None,
                overwrites_existing: false,
            }),
        }
    }

    let counts = summarize_operation_plan(&operations, &warnings);
    FolderOperationPlan {
        operations,
        counts,
        warnings,
    }
}

fn plan_copy(
    result: &FolderCompareResult,
    entry: &FolderEntryDiff,
    kind: FolderOperationKind,
    source_root: &Path,
    target_root: &Path,
    operations: &mut Vec<FolderOperation>,
    warnings: &mut Vec<FolderOperationWarning>,
) {
    let source = source_root.join(&entry.relative_path);
    if !path_exists_for_operation(&source) {
        warnings.push(invalid_selection_warning(
            entry,
            "copy source does not exist on the selected side",
        ));
        return;
    }
    if !guard_operation_within_root(&source, source_root, entry, warnings) {
        return;
    }

    let target = target_root.join(&entry.relative_path);
    if !guard_operation_within_root(&target, target_root, entry, warnings) {
        return;
    }
    let overwrites_existing = path_exists_for_operation(&target);
    if overwrites_existing {
        warnings.push(FolderOperationWarning {
            relative_path: entry.relative_path.clone(),
            kind: FolderOperationWarningKind::Overwrite,
            message: format!("copy will overwrite {}", target.display()),
        });
        if readonly_path(&target) {
            warnings.push(FolderOperationWarning {
                relative_path: entry.relative_path.clone(),
                kind: FolderOperationWarningKind::Permission,
                message: format!("copy target is read-only: {}", target.display()),
            });
        }
    }

    if entry.state == FolderEntryState::Different {
        warnings.push(FolderOperationWarning {
            relative_path: entry.relative_path.clone(),
            kind: FolderOperationWarningKind::Conflict,
            message: format!(
                "copy will replace a different item between {} and {}",
                result.left_root.display(),
                result.right_root.display()
            ),
        });
    }

    operations.push(FolderOperation {
        kind,
        relative_path: entry.relative_path.clone(),
        source: Some(source),
        target: Some(target),
        overwrites_existing,
    });
}

fn plan_delete_side(
    _result: &FolderCompareResult,
    entry: &FolderEntryDiff,
    kind: FolderOperationKind,
    root: &Path,
    left_side: bool,
    operations: &mut Vec<FolderOperation>,
    warnings: &mut Vec<FolderOperationWarning>,
) {
    let exists_on_side = if left_side {
        entry.left_size.is_some()
    } else {
        entry.right_size.is_some()
    };
    if !exists_on_side {
        warnings.push(invalid_selection_warning(
            entry,
            "delete target does not exist on the selected side",
        ));
        return;
    }

    let target = root.join(&entry.relative_path);
    if !guard_operation_within_root(&target, root, entry, warnings) {
        return;
    }
    if readonly_path(&target) {
        warnings.push(FolderOperationWarning {
            relative_path: entry.relative_path.clone(),
            kind: FolderOperationWarningKind::Permission,
            message: format!("delete target is read-only: {}", target.display()),
        });
    }

    operations.push(FolderOperation {
        kind,
        relative_path: entry.relative_path.clone(),
        source: Some(target),
        target: None,
        overwrites_existing: false,
    });
}

fn plan_rename(
    entry: &FolderEntryDiff,
    kind: FolderOperationKind,
    root: &Path,
    new_name: &str,
    operations: &mut Vec<FolderOperation>,
    warnings: &mut Vec<FolderOperationWarning>,
) {
    if !is_safe_rename_target_name(new_name) {
        warnings.push(invalid_selection_warning(
            entry,
            "rename target must be a non-empty file name",
        ));
        return;
    }

    let source = root.join(&entry.relative_path);
    if !path_exists_for_operation(&source) {
        warnings.push(invalid_selection_warning(
            entry,
            "rename source does not exist on the selected side",
        ));
        return;
    }
    if !guard_operation_within_root(&source, root, entry, warnings) {
        return;
    }

    let target = source
        .parent()
        .map(|parent| parent.join(new_name))
        .unwrap_or_else(|| root.join(new_name));
    if !guard_operation_within_root(&target, root, entry, warnings) {
        return;
    }
    let overwrites_existing = path_exists_for_operation(&target);
    if overwrites_existing {
        warnings.push(FolderOperationWarning {
            relative_path: entry.relative_path.clone(),
            kind: FolderOperationWarningKind::Overwrite,
            message: format!("rename target already exists: {}", target.display()),
        });
    }

    operations.push(FolderOperation {
        kind,
        relative_path: entry.relative_path.clone(),
        source: Some(source),
        target: Some(target),
        overwrites_existing,
    });
}

fn is_safe_rename_target_name(new_name: &str) -> bool {
    let mut components = Path::new(new_name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn plan_create_missing(
    entry: &FolderEntryDiff,
    kind: FolderOperationKind,
    root: &Path,
    valid_state: bool,
    operations: &mut Vec<FolderOperation>,
    warnings: &mut Vec<FolderOperationWarning>,
) {
    if !valid_state || !entry.is_dir {
        warnings.push(invalid_selection_warning(
            entry,
            "create-missing only applies to one-sided folders",
        ));
        return;
    }

    let target = root.join(&entry.relative_path);
    if !guard_operation_within_root(&target, root, entry, warnings) {
        return;
    }
    operations.push(FolderOperation {
        kind,
        relative_path: entry.relative_path.clone(),
        source: None,
        target: Some(target),
        overwrites_existing: false,
    });
}

fn summarize_operation_plan(
    operations: &[FolderOperation],
    warnings: &[FolderOperationWarning],
) -> FolderOperationCounts {
    let mut counts = FolderOperationCounts::default();
    for operation in operations {
        match operation.kind {
            FolderOperationKind::CopyLeftToRight | FolderOperationKind::CopyRightToLeft => {
                counts.copy_count += 1
            }
            FolderOperationKind::DeleteLeft | FolderOperationKind::DeleteRight => {
                counts.delete_count += 1
            }
            FolderOperationKind::RenameLeft { .. } | FolderOperationKind::RenameRight { .. } => {
                counts.rename_count += 1
            }
            FolderOperationKind::CreateMissingLeft | FolderOperationKind::CreateMissingRight => {
                counts.create_folder_count += 1
            }
            FolderOperationKind::Refresh => counts.refresh_count += 1,
        }
    }

    for warning in warnings {
        match warning.kind {
            FolderOperationWarningKind::Overwrite
            | FolderOperationWarningKind::OverwriteExisting => counts.overwrite_warning_count += 1,
            FolderOperationWarningKind::Permission
            | FolderOperationWarningKind::PermissionDenied
            | FolderOperationWarningKind::DeleteReadOnly => counts.permission_warning_count += 1,
            FolderOperationWarningKind::Conflict | FolderOperationWarningKind::TargetNewer => {
                counts.conflict_warning_count += 1
            }
            FolderOperationWarningKind::InvalidSelection
            | FolderOperationWarningKind::CrossDeviceCopy
            | FolderOperationWarningKind::SymlinkTraversal
            | FolderOperationWarningKind::SourceLarger => {}
        }
    }

    counts
}

fn invalid_selection_warning(entry: &FolderEntryDiff, message: &str) -> FolderOperationWarning {
    FolderOperationWarning {
        relative_path: entry.relative_path.clone(),
        kind: FolderOperationWarningKind::InvalidSelection,
        message: message.to_owned(),
    }
}

fn readonly_path(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(false)
}

fn path_exists_for_operation(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Verify that an operation `path` (a copy source/target, delete target,
/// rename source/target, etc.) is genuinely contained within the selected
/// comparison `root` after resolving the symlinks in its ancestry.
///
/// This blocks a symlinked directory component — for example one followed
/// under [`SymlinkPolicy::Follow`] — from redirecting a copy or delete to a
/// location outside the comparison root. Without it, a compare entry whose
/// `relative_path` is `linkdir/file` (where `linkdir` is a symlink to
/// `/home/user/Documents`) would resolve `root.join("linkdir/file")` to a file
/// outside the root and copy/delete it.
///
/// The leaf component is intentionally *not* dereferenced: it need not exist
/// (copy/create targets are created on demand), and a top-level symlink entry
/// is handled safely by [`execute_copy`], which recreates the link rather than
/// following it. Only the deepest existing ancestor is canonicalized, so a
/// symlinked parent that escapes the root is detected while legitimate
/// not-yet-created descendants are still permitted.
fn operation_path_within_root(path: &Path, root: &Path) -> bool {
    let Ok(canon_root) = fs::canonicalize(root) else {
        return false;
    };
    let mut ancestor = path.parent();
    while let Some(dir) = ancestor {
        if dir.as_os_str().is_empty() {
            break;
        }
        match fs::canonicalize(dir) {
            Ok(canon) => return canon == canon_root || canon.starts_with(&canon_root),
            // This ancestor does not exist yet (e.g. a copy target's new parent
            // dirs); walk up to the deepest existing ancestor.
            Err(_) => ancestor = dir.parent(),
        }
    }
    // No existing ancestor resolved — conservatively reject.
    false
}

/// Emit an InvalidSelection warning and return `false` when `path` escapes
/// `root`; otherwise return `true`. Used by the operation planners to refuse
/// copy/delete/rename targets redirected outside the comparison root.
fn guard_operation_within_root(
    path: &Path,
    root: &Path,
    entry: &FolderEntryDiff,
    warnings: &mut Vec<FolderOperationWarning>,
) -> bool {
    if operation_path_within_root(path, root) {
        return true;
    }
    warnings.push(invalid_selection_warning(
        entry,
        "operation path resolves outside the comparison root (symlink escape)",
    ));
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FolderOperationRiskSummary {
    pub total_operations: usize,
    pub overwrite_count: usize,
    pub delete_count: usize,
    pub high_risk_count: usize,
    pub warnings: Vec<FolderOperationWarning>,
}

impl FolderOperationPlan {
    pub fn risk_summary(&self) -> FolderOperationRiskSummary {
        let overwrite_count = self
            .operations
            .iter()
            .filter(|op| op.overwrites_existing)
            .count();
        let delete_count = self
            .operations
            .iter()
            .filter(|op| {
                matches!(
                    op.kind,
                    FolderOperationKind::DeleteLeft | FolderOperationKind::DeleteRight
                )
            })
            .count();
        let high_risk_kinds = [
            FolderOperationWarningKind::OverwriteExisting,
            FolderOperationWarningKind::DeleteReadOnly,
            FolderOperationWarningKind::TargetNewer,
            FolderOperationWarningKind::SourceLarger,
            FolderOperationWarningKind::CrossDeviceCopy,
            FolderOperationWarningKind::SymlinkTraversal,
            FolderOperationWarningKind::PermissionDenied,
        ];
        let high_risk_count = self
            .warnings
            .iter()
            .filter(|w| high_risk_kinds.contains(&w.kind))
            .count();
        FolderOperationRiskSummary {
            total_operations: self.operations.len(),
            overwrite_count,
            delete_count,
            high_risk_count,
            warnings: self.warnings.clone(),
        }
    }
}

pub fn assess_operation_risks(
    plan: &mut FolderOperationPlan,
    _left_base: &Path,
    _right_base: &Path,
) -> io::Result<()> {
    let mut new_warnings = Vec::new();
    for operation in &plan.operations {
        match &operation.kind {
            FolderOperationKind::CopyLeftToRight | FolderOperationKind::CopyRightToLeft => {
                let Some(source) = &operation.source else {
                    continue;
                };
                let Some(target) = &operation.target else {
                    continue;
                };
                let source_meta = fs::metadata(source);
                let target_meta = fs::metadata(target);

                if target_meta.is_ok() {
                    new_warnings.push(FolderOperationWarning {
                        relative_path: operation.relative_path.clone(),
                        kind: FolderOperationWarningKind::OverwriteExisting,
                        message: format!(
                            "target file exists and will be overwritten: {}",
                            target.display()
                        ),
                    });

                    if let (Ok(sm), Ok(tm)) = (&source_meta, &target_meta) {
                        if let (Ok(src_mtime), Ok(tgt_mtime)) = (sm.modified(), tm.modified())
                            && tgt_mtime > src_mtime
                        {
                            new_warnings.push(FolderOperationWarning {
                                relative_path: operation.relative_path.clone(),
                                kind: FolderOperationWarningKind::TargetNewer,
                                message: format!(
                                    "target is newer than source: {}",
                                    operation.relative_path.display()
                                ),
                            });
                        }

                        let source_size = sm.len();
                        let target_size = tm.len();
                        if target_size > 0 && (source_size as f64 / target_size as f64) > 2.0 {
                            new_warnings.push(FolderOperationWarning {
                                relative_path: operation.relative_path.clone(),
                                kind: FolderOperationWarningKind::SourceLarger,
                                message: format!(
                                    "source is significantly larger than target: {}",
                                    operation.relative_path.display()
                                ),
                            });
                        }

                        #[cfg(unix)]
                        if let (Ok(sm), Ok(tm)) = (&source_meta, &target_meta)
                            && sm.dev() != tm.dev()
                        {
                            new_warnings.push(FolderOperationWarning {
                                relative_path: operation.relative_path.clone(),
                                kind: FolderOperationWarningKind::CrossDeviceCopy,
                                message: format!(
                                    "source and target are on different filesystems: {}",
                                    operation.relative_path.display()
                                ),
                            });
                        }
                    }
                }
            }
            FolderOperationKind::DeleteLeft | FolderOperationKind::DeleteRight => {
                if let Some(target) = &operation.source
                    && readonly_path(target)
                {
                    new_warnings.push(FolderOperationWarning {
                        relative_path: operation.relative_path.clone(),
                        kind: FolderOperationWarningKind::DeleteReadOnly,
                        message: format!("deleting a read-only file: {}", target.display()),
                    });
                }
            }
            _ => {}
        }

        if let Some(source) = &operation.source
            && path_contains_symlink_components(source)
        {
            new_warnings.push(FolderOperationWarning {
                relative_path: operation.relative_path.clone(),
                kind: FolderOperationWarningKind::SymlinkTraversal,
                message: format!(
                    "operation involves symlink traversal: {}",
                    operation.relative_path.display()
                ),
            });
        }
    }
    plan.warnings.extend(new_warnings);
    plan.counts = summarize_operation_plan(&plan.operations, &plan.warnings);
    Ok(())
}

fn path_contains_symlink_components(path: &Path) -> bool {
    let mut current = path;
    loop {
        if fs::symlink_metadata(current)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return true;
        }
        match current.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => current = parent,
            _ => return false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderOperationOutcome {
    pub operation: FolderOperation,
    pub status: FolderOperationStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderOperationStatus {
    Succeeded,
    Skipped,
    Failed,
}

pub fn execute_folder_operation_plan(
    plan: &FolderOperationPlan,
    data_home: &Path,
    use_trash_for_deletes: bool,
) -> Vec<FolderOperationOutcome> {
    plan.operations
        .iter()
        .map(|op| execute_folder_operation(op, data_home, use_trash_for_deletes))
        .collect()
}

fn execute_folder_operation(
    op: &FolderOperation,
    data_home: &Path,
    use_trash_for_deletes: bool,
) -> FolderOperationOutcome {
    let status_message = match &op.kind {
        FolderOperationKind::CopyLeftToRight | FolderOperationKind::CopyRightToLeft => {
            execute_copy(op)
        }
        FolderOperationKind::DeleteLeft | FolderOperationKind::DeleteRight => {
            execute_delete(op, data_home, use_trash_for_deletes)
        }
        FolderOperationKind::RenameLeft { .. } | FolderOperationKind::RenameRight { .. } => {
            execute_rename(op)
        }
        FolderOperationKind::CreateMissingLeft | FolderOperationKind::CreateMissingRight => {
            execute_create_missing(op)
        }
        FolderOperationKind::Refresh => Ok("refresh marker".to_owned()),
    };

    match status_message {
        Ok(message) => FolderOperationOutcome {
            operation: op.clone(),
            status: FolderOperationStatus::Succeeded,
            message,
        },
        Err(message) => FolderOperationOutcome {
            operation: op.clone(),
            status: FolderOperationStatus::Failed,
            message,
        },
    }
}

fn execute_copy(op: &FolderOperation) -> Result<String, String> {
    let source = op
        .source
        .as_ref()
        .ok_or_else(|| "copy operation missing source".to_owned())?;
    let target = op
        .target
        .as_ref()
        .ok_or_else(|| "copy operation missing target".to_owned())?;

    let metadata = fs::symlink_metadata(source)
        .map_err(|err| format!("cannot stat source '{}': {err}", source.display()))?;

    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|err| format!("cannot create target parent '{}': {err}", parent.display()))?;
    }

    if metadata.file_type().is_symlink() {
        // Recreate the symlink itself rather than dereferencing it. `fs::copy`
        // would follow the link and copy the *content* of whatever it points
        // at — potentially a file outside the comparison roots (e.g.
        // ~/.ssh/id_rsa) — into the destination root. This mirrors how
        // `copy_dir_recursive` already preserves nested symlinks, and matches
        // the no-follow `CompareTarget` policy semantics (compare the link
        // text, do not follow it).
        #[cfg(unix)]
        {
            let link = fs::read_link(source)
                .map_err(|err| format!("cannot read symlink '{}': {err}", source.display()))?;
            // symlink() fails with EEXIST if the target is present; remove the
            // existing entry (without following it) first.
            if fs::symlink_metadata(target).is_ok() {
                fs::remove_file(target).map_err(|err| {
                    format!("cannot replace existing '{}': {err}", target.display())
                })?;
            }
            std::os::unix::fs::symlink(&link, target)
                .map_err(|err| format!("copy symlink failed: {err}"))?;
            Ok(format!(
                "recreated symlink '{}' -> '{}'",
                target.display(),
                link.display()
            ))
        }
        #[cfg(not(unix))]
        {
            fs::copy(source, target)
                .map_err(|err| format!("copy file failed: {err}"))
                .map(|bytes| format!("copied {bytes} bytes to '{}'", target.display()))
        }
    } else if metadata.is_dir() {
        copy_dir_recursive(source, target)
            .map_err(|err| format!("copy directory failed: {err}"))?;
        Ok(format!("copied directory to '{}'", target.display()))
    } else {
        fs::copy(source, target)
            .map_err(|err| format!("copy file failed: {err}"))
            .map(|bytes| format!("copied {bytes} bytes to '{}'", target.display()))
    }
}

fn copy_dir_recursive(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_type = entry.file_type()?;
        let dest = target.join(entry.file_name());
        if entry_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        } else if entry_type.is_symlink() {
            #[cfg(unix)]
            {
                let link = fs::read_link(entry.path())?;
                std::os::unix::fs::symlink(link, &dest)?;
            }
            #[cfg(not(unix))]
            {
                fs::copy(entry.path(), &dest)?;
            }
        } else {
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

fn execute_delete(
    op: &FolderOperation,
    data_home: &Path,
    use_trash: bool,
) -> Result<String, String> {
    // Delete operations carry the path to remove in `source` (see
    // `plan_delete_side`); `target` is always `None` for a delete.
    let target = op
        .source
        .as_ref()
        .ok_or_else(|| "delete operation missing source path".to_owned())?;
    if use_trash {
        crate::trash::move_to_freedesktop_trash(target, data_home)
            .map(|trashed| format!("moved to trash at '{}'", trashed.trash_file_path.display()))
            .map_err(|err| format!("trash move failed: {err}"))
    } else {
        crate::trash::permanently_delete(target)
            .map(|_| format!("permanently deleted '{}'", target.display()))
            .map_err(|err| format!("permanent delete failed: {err}"))
    }
}

fn execute_rename(op: &FolderOperation) -> Result<String, String> {
    let source = op
        .source
        .as_ref()
        .ok_or_else(|| "rename operation missing source".to_owned())?;
    let target = op
        .target
        .as_ref()
        .ok_or_else(|| "rename operation missing target".to_owned())?;
    fs::rename(source, target)
        .map(|_| format!("renamed to '{}'", target.display()))
        .map_err(|err| format!("rename failed: {err}"))
}

fn execute_create_missing(op: &FolderOperation) -> Result<String, String> {
    let target = op
        .target
        .as_ref()
        .ok_or_else(|| "create operation missing target".to_owned())?;
    fs::create_dir_all(target)
        .map(|_| format!("created folder '{}'", target.display()))
        .map_err(|err| format!("create folder failed: {err}"))
}

fn virtual_node_type(kind: &str) -> FolderEntryType {
    match kind {
        "file" => FolderEntryType::File,
        "dir" | "directory" => FolderEntryType::Directory,
        "symlink" | "link" => FolderEntryType::Symlink,
        _ => FolderEntryType::Special,
    }
}

/// Build a [`FolderEntryDiff`] for a path present on one or both sides of a
/// virtual-tree comparison.
fn virtual_entry_diff(
    rel: &str,
    left: Option<&crate::plugin::VirtualNode>,
    right: Option<&crate::plugin::VirtualNode>,
) -> FolderEntryDiff {
    let relative_path = PathBuf::from(rel);
    let name = relative_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.to_owned());
    let extension = relative_path
        .extension()
        .map(|e| e.to_string_lossy().into_owned());
    let node = left.or(right).expect("at least one side is present");
    let entry_type = virtual_node_type(&node.kind);
    let state = match (left, right) {
        (Some(_), None) => FolderEntryState::LeftOnly,
        (None, Some(_)) => FolderEntryState::RightOnly,
        (Some(l), Some(r)) => {
            // Directories carry no content; presence on both sides is equal.
            let equal = if l.kind != r.kind {
                false
            } else if entry_type == FolderEntryType::Directory {
                true
            } else if let (Some(lh), Some(rh)) = (&l.sha256, &r.sha256) {
                lh == rh
            } else {
                l.size == r.size
            };
            if equal {
                FolderEntryState::Identical
            } else {
                FolderEntryState::Different
            }
        }
        (None, None) => unreachable!("a path is only listed when one side has it"),
    };
    FolderEntryDiff {
        relative_path,
        name,
        extension,
        state,
        left_size: left.and_then(|n| n.size),
        right_size: right.and_then(|n| n.size),
        left_modified: None,
        right_modified: None,
        entry_type,
        effective_method: None,
        method_note: None,
        is_dir: entry_type == FolderEntryType::Directory,
        error: None,
        left_permissions: None,
        right_permissions: None,
        left_owner: None,
        right_owner: None,
        left_group: None,
        right_group: None,
        left_hash: left.and_then(|n| n.sha256.clone()),
        right_hash: right.and_then(|n| n.sha256.clone()),
    }
}

/// Compare two virtual folder trees (e.g. produced by an `unpack_folder`
/// plugin) into a [`FolderCompareResult`], so the standard folder query and
/// rendering paths apply to plugin-virtualized archives. Equality uses the
/// SHA-256 when both sides provide one, else the size; directories match on
/// presence. Entries are sorted by relative path.
pub fn compare_virtual_trees(
    left: &[crate::plugin::VirtualNode],
    right: &[crate::plugin::VirtualNode],
) -> FolderCompareResult {
    let left_map: BTreeMap<&str, &crate::plugin::VirtualNode> =
        left.iter().map(|n| (n.path.as_str(), n)).collect();
    let right_map: BTreeMap<&str, &crate::plugin::VirtualNode> =
        right.iter().map(|n| (n.path.as_str(), n)).collect();
    let mut paths: BTreeSet<&str> = BTreeSet::new();
    paths.extend(left_map.keys().copied());
    paths.extend(right_map.keys().copied());

    let mut entries = Vec::with_capacity(paths.len());
    let mut summary = FolderCompareSummary::default();
    for rel in paths {
        let entry =
            virtual_entry_diff(rel, left_map.get(rel).copied(), right_map.get(rel).copied());
        match entry.state {
            FolderEntryState::Identical => summary.identical_count += 1,
            FolderEntryState::Different => summary.different_count += 1,
            FolderEntryState::LeftOnly => {
                summary.left_only_count += 1;
                summary.one_sided_count += 1;
            }
            FolderEntryState::RightOnly => {
                summary.right_only_count += 1;
                summary.one_sided_count += 1;
            }
            _ => {}
        }
        summary.compared_count += 1;
        entries.push(entry);
    }

    FolderCompareResult {
        left_root: PathBuf::from("<virtual:left>"),
        right_root: PathBuf::from("<virtual:right>"),
        entries,
        summary,
    }
}

pub fn compare_folders(
    left: &Path,
    right: &Path,
    options: &FolderCompareOptions,
) -> Result<FolderCompareResult, FolderCompareError> {
    compare_folders_with_progress(left, right, options, |_| FolderCompareControl::Continue)
}

pub fn compare_folders_with_progress<F>(
    left: &Path,
    right: &Path,
    options: &FolderCompareOptions,
    mut on_event: F,
) -> Result<FolderCompareResult, FolderCompareError>
where
    F: FnMut(FolderCompareEvent) -> FolderCompareControl,
{
    let started = Instant::now();

    if !left.is_dir() {
        return Err(FolderCompareError::NotDirectory(left.to_path_buf()));
    }

    if !right.is_dir() {
        return Err(FolderCompareError::NotDirectory(right.to_path_buf()));
    }

    let left_entries = collect_entries(left, options)?;
    let right_entries = collect_entries(right, options)?;
    let mut all_paths = BTreeSet::new();
    all_paths.extend(left_entries.keys().cloned());
    all_paths.extend(right_entries.keys().cloned());
    let all_paths = all_paths.into_iter().collect::<Vec<_>>();

    let mut entries = Vec::new();
    let mut cancelled = false;
    for (index, relative_path) in all_paths.iter().cloned().enumerate() {
        if on_event(FolderCompareEvent::Discovered {
            relative_path: relative_path.clone(),
        }) == FolderCompareControl::Cancel
        {
            entries.extend(aborted_entries(
                &all_paths[index..],
                &left_entries,
                &right_entries,
            ));
            cancelled = true;
            break;
        }

        let left_meta = left_entries.get(&relative_path);
        let right_meta = right_entries.get(&relative_path);
        let entry = build_folder_entry(relative_path, left, right, left_meta, right_meta, options);
        let event = match entry.state {
            FolderEntryState::Skipped => FolderCompareEvent::Skipped {
                relative_path: entry.relative_path.clone(),
            },
            FolderEntryState::Error => FolderCompareEvent::Error {
                relative_path: entry.relative_path.clone(),
                message: entry
                    .error
                    .clone()
                    .unwrap_or_else(|| "compare error".to_owned()),
            },
            _ => FolderCompareEvent::Compared {
                relative_path: entry.relative_path.clone(),
                state: entry.state,
            },
        };
        entries.push(entry);

        if on_event(event) == FolderCompareControl::Cancel {
            entries.extend(aborted_entries(
                &all_paths[index + 1..],
                &left_entries,
                &right_entries,
            ));
            cancelled = true;
            break;
        }
    }

    let elapsed = started.elapsed();
    let status = if cancelled {
        FolderCompareStatus::Cancelled
    } else {
        FolderCompareStatus::Complete
    };
    let summary = summarize_entries(&entries, elapsed, status);
    if !options.include_skipped {
        entries.retain(|entry| entry.state != FolderEntryState::Skipped);
    }

    if cancelled {
        let completed = entries
            .iter()
            .filter(|entry| entry.state != FolderEntryState::Aborted)
            .count();
        let aborted = entries
            .iter()
            .filter(|entry| entry.state == FolderEntryState::Aborted)
            .count();
        on_event(FolderCompareEvent::Cancelled { completed, aborted });
    }
    on_event(FolderCompareEvent::Completed {
        summary: summary.clone(),
    });

    Ok(FolderCompareResult {
        left_root: left.to_path_buf(),
        right_root: right.to_path_buf(),
        entries,
        summary,
    })
}

fn build_folder_entry(
    relative_path: PathBuf,
    left: &Path,
    right: &Path,
    left_meta: Option<&EntryMeta>,
    right_meta: Option<&EntryMeta>,
    options: &FolderCompareOptions,
) -> FolderEntryDiff {
    let state = match (left_meta, right_meta) {
        (Some(left_meta), Some(right_meta))
            if left_meta.error.is_some() || right_meta.error.is_some() =>
        {
            FolderEntryState::Error
        }
        (Some(left_meta), None) if left_meta.error.is_some() => FolderEntryState::Error,
        (None, Some(right_meta)) if right_meta.error.is_some() => FolderEntryState::Error,
        (Some(left_meta), Some(right_meta)) if left_meta.skipped || right_meta.skipped => {
            FolderEntryState::Skipped
        }
        (Some(left_meta), None) if left_meta.skipped => FolderEntryState::Skipped,
        (None, Some(right_meta)) if right_meta.skipped => FolderEntryState::Skipped,
        (Some(left_meta), Some(right_meta)) => {
            match entries_match(left, right, &relative_path, left_meta, right_meta, options) {
                Ok(true) => FolderEntryState::Identical,
                Ok(false) => FolderEntryState::Different,
                Err(_) => FolderEntryState::Error,
            }
        }
        (Some(_), None) => FolderEntryState::LeftOnly,
        (None, Some(_)) => FolderEntryState::RightOnly,
        (None, None) => FolderEntryState::Aborted,
    };
    let error = match (state, left_meta, right_meta) {
        (FolderEntryState::Error, Some(left_meta), Some(right_meta)) => left_meta
            .error
            .clone()
            .or_else(|| right_meta.error.clone())
            .or_else(|| {
                match entries_match(left, right, &relative_path, left_meta, right_meta, options) {
                    Ok(_) => None,
                    Err(err) => Some(err.to_string()),
                }
            }),
        (FolderEntryState::Error, Some(left_meta), None) => left_meta.error.clone(),
        (FolderEntryState::Error, None, Some(right_meta)) => right_meta.error.clone(),
        _ => None,
    };
    let (effective_method, method_note) = entry_effective_method(left_meta, right_meta, options);

    let is_file = left_meta
        .or(right_meta)
        .is_some_and(|meta| meta.kind == EntryKind::File);

    let (left_permissions, right_permissions) = if options.compare_permissions && is_file {
        let lp = left_meta.and_then(|_| {
            fs::symlink_metadata(left.join(&relative_path))
                .ok()
                .map(|m| extract_permissions(&m))
        });
        let rp = right_meta.and_then(|_| {
            fs::symlink_metadata(right.join(&relative_path))
                .ok()
                .map(|m| extract_permissions(&m))
        });
        (lp, rp)
    } else {
        (None, None)
    };

    let (left_owner, right_owner, left_group, right_group) = if options.compare_ownership && is_file
    {
        let lo = left_meta.and_then(|_| {
            fs::symlink_metadata(left.join(&relative_path))
                .ok()
                .and_then(|m| extract_owner(&m))
        });
        let ro = right_meta.and_then(|_| {
            fs::symlink_metadata(right.join(&relative_path))
                .ok()
                .and_then(|m| extract_owner(&m))
        });
        let lg = left_meta.and_then(|_| {
            fs::symlink_metadata(left.join(&relative_path))
                .ok()
                .and_then(|m| extract_group(&m))
        });
        let rg = right_meta.and_then(|_| {
            fs::symlink_metadata(right.join(&relative_path))
                .ok()
                .and_then(|m| extract_group(&m))
        });
        (lo, ro, lg, rg)
    } else {
        (None, None, None, None)
    };

    let (left_hash, right_hash) = if is_file
        && left_meta.is_some()
        && right_meta.is_some()
        && matches!(
            state,
            FolderEntryState::Identical | FolderEntryState::Different
        ) {
        // Only the hash-based compare method reads file contents to hash
        // them; populating the display hash for any other method (regardless
        // of the configured algorithm) would force a full read of every file
        // pair and double I/O on large trees.
        let needs_hash = options.compare_method == CompareMethod::HashBlake3;
        if needs_hash {
            let lh = compute_file_hash(&left.join(&relative_path), options.hash_algorithm).ok();
            let rh = compute_file_hash(&right.join(&relative_path), options.hash_algorithm).ok();
            (lh, rh)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    FolderEntryDiff {
        name: folder_entry_name(&relative_path),
        extension: folder_entry_extension(&relative_path),
        entry_type: folder_entry_type(left_meta, right_meta),
        relative_path,
        state,
        left_size: left_meta.map(|meta| meta.size),
        right_size: right_meta.map(|meta| meta.size),
        left_modified: left_meta.and_then(|meta| meta.modified),
        right_modified: right_meta.and_then(|meta| meta.modified),
        effective_method,
        method_note,
        is_dir: left_meta.or(right_meta).is_some_and(|meta| meta.is_dir),
        error,
        left_permissions,
        right_permissions,
        left_owner,
        right_owner,
        left_group,
        right_group,
        left_hash,
        right_hash,
    }
}

fn aborted_entries(
    paths: &[PathBuf],
    left_entries: &BTreeMap<PathBuf, EntryMeta>,
    right_entries: &BTreeMap<PathBuf, EntryMeta>,
) -> Vec<FolderEntryDiff> {
    paths
        .iter()
        .map(|relative_path| {
            let left_meta = left_entries.get(relative_path);
            let right_meta = right_entries.get(relative_path);
            FolderEntryDiff {
                name: folder_entry_name(relative_path),
                extension: folder_entry_extension(relative_path),
                relative_path: relative_path.clone(),
                state: FolderEntryState::Aborted,
                left_size: left_meta.map(|meta| meta.size),
                right_size: right_meta.map(|meta| meta.size),
                left_modified: left_meta.and_then(|meta| meta.modified),
                right_modified: right_meta.and_then(|meta| meta.modified),
                entry_type: folder_entry_type(left_meta, right_meta),
                effective_method: None,
                method_note: None,
                is_dir: left_meta.or(right_meta).is_some_and(|meta| meta.is_dir),
                error: Some("folder comparison cancelled before visiting this item".to_owned()),
                left_permissions: None,
                right_permissions: None,
                left_owner: None,
                right_owner: None,
                left_group: None,
                right_group: None,
                left_hash: None,
                right_hash: None,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryMeta {
    size: u64,
    is_dir: bool,
    kind: EntryKind,
    link_target: Option<PathBuf>,
    modified: Option<SystemTime>,
    skipped: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Directory,
    Symlink,
    Special,
}

fn folder_entry_name(relative_path: &Path) -> String {
    relative_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| relative_path.display().to_string())
}

fn folder_entry_extension(relative_path: &Path) -> Option<String> {
    relative_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(ToOwned::to_owned)
}

fn folder_entry_type(
    left_meta: Option<&EntryMeta>,
    right_meta: Option<&EntryMeta>,
) -> FolderEntryType {
    match left_meta.or(right_meta).map(|meta| meta.kind) {
        Some(EntryKind::File) => FolderEntryType::File,
        Some(EntryKind::Directory) => FolderEntryType::Directory,
        Some(EntryKind::Symlink) => FolderEntryType::Symlink,
        Some(EntryKind::Special) | None => FolderEntryType::Special,
    }
}

fn entries_match(
    left_root: &Path,
    right_root: &Path,
    relative_path: &Path,
    left: &EntryMeta,
    right: &EntryMeta,
    options: &FolderCompareOptions,
) -> Result<bool, FolderCompareError> {
    if left.kind != right.kind {
        return Ok(false);
    }

    if left.kind == EntryKind::Symlink {
        return Ok(left.link_target == right.link_target);
    }

    if left.is_dir || right.is_dir {
        return Ok(left.is_dir == right.is_dir);
    }

    let same_size = left.size == right.size;
    let same_date =
        modified_times_match(left.modified, right.modified, options.timestamp_tolerance);

    let (effective_method, _) = effective_compare_method(left, right, options);

    let contents_match = match effective_method {
        CompareMethod::Existence => true,
        CompareMethod::Size => same_size,
        CompareMethod::ModifiedDate => same_date,
        CompareMethod::DateAndSize => same_size && same_date,
        CompareMethod::BinaryContents => {
            same_size
                && binary_files_equal_until_first_difference(
                    &left_root.join(relative_path),
                    &right_root.join(relative_path),
                )?
        }
        CompareMethod::QuickContents | CompareMethod::FullContents => {
            same_size
                && fs::read(left_root.join(relative_path))?
                    == fs::read(right_root.join(relative_path))?
        }
        CompareMethod::HashBlake3 => {
            same_size
                && compute_file_hash(&left_root.join(relative_path), options.hash_algorithm)?
                    == compute_file_hash(&right_root.join(relative_path), options.hash_algorithm)?
        }
        CompareMethod::NormalizedText => {
            normalized_text_content(&left_root.join(relative_path))?
                == normalized_text_content(&right_root.join(relative_path))?
        }
    };

    if !contents_match {
        return Ok(false);
    }

    if options.compare_permissions
        && !file_permissions_match(
            &left_root.join(relative_path),
            &right_root.join(relative_path),
        )?
    {
        return Ok(false);
    }

    if options.compare_ownership
        && !file_ownership_match(
            &left_root.join(relative_path),
            &right_root.join(relative_path),
        )?
    {
        return Ok(false);
    }

    Ok(true)
}

fn compute_file_hash(path: &Path, algorithm: HashAlgorithm) -> io::Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    match algorithm {
        HashAlgorithm::Blake3 => {
            let mut hasher = blake3::Hasher::new();
            io::copy(&mut reader, &mut hasher)?;
            Ok(hasher.finalize().to_hex().to_string())
        }
        HashAlgorithm::Sha256 => {
            let mut hasher = Sha256::new();
            io::copy(&mut reader, &mut hasher)?;
            Ok(format!("{:x}", hasher.finalize()))
        }
        HashAlgorithm::Crc32 => {
            let mut hasher = Crc32Hasher::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            Ok(format!("{:08x}", hasher.finalize()))
        }
    }
}

fn file_permissions_match(left: &Path, right: &Path) -> io::Result<bool> {
    let left_meta = fs::symlink_metadata(left)?;
    let right_meta = fs::symlink_metadata(right)?;
    Ok(extract_permissions(&left_meta) == extract_permissions(&right_meta))
}

#[cfg(unix)]
fn file_ownership_match(left: &Path, right: &Path) -> io::Result<bool> {
    let left_meta = fs::symlink_metadata(left)?;
    let right_meta = fs::symlink_metadata(right)?;
    Ok(left_meta.uid() == right_meta.uid() && left_meta.gid() == right_meta.gid())
}

#[cfg(not(unix))]
fn file_ownership_match(_left: &Path, _right: &Path) -> io::Result<bool> {
    Ok(true)
}

#[cfg(unix)]
fn extract_permissions(meta: &std::fs::Metadata) -> u32 {
    meta.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn extract_permissions(_meta: &std::fs::Metadata) -> u32 {
    0
}

#[cfg(unix)]
fn extract_owner(meta: &std::fs::Metadata) -> Option<String> {
    let uid = meta.uid();
    match nix_get_username(uid) {
        Some(name) => Some(name),
        None => Some(uid.to_string()),
    }
}

#[cfg(not(unix))]
fn extract_owner(_meta: &std::fs::Metadata) -> Option<String> {
    None
}

#[cfg(unix)]
fn extract_group(meta: &std::fs::Metadata) -> Option<String> {
    let gid = meta.gid();
    match nix_get_groupname(gid) {
        Some(name) => Some(name),
        None => Some(gid.to_string()),
    }
}

#[cfg(not(unix))]
fn extract_group(_meta: &std::fs::Metadata) -> Option<String> {
    None
}

#[cfg(unix)]
fn nix_get_username(uid: u32) -> Option<String> {
    use std::process::Command;
    let output = Command::new("getent")
        .arg("passwd")
        .arg(uid.to_string())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let mut parts = line.split(':');
    parts.next().map(|name| name.to_owned())
}

#[cfg(unix)]
fn nix_get_groupname(gid: u32) -> Option<String> {
    use std::process::Command;
    let output = Command::new("getent")
        .arg("group")
        .arg(gid.to_string())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let mut parts = line.split(':');
    parts.next().map(|name| name.to_owned())
}

fn binary_files_equal_until_first_difference(left: &Path, right: &Path) -> io::Result<bool> {
    readers_equal_until_first_difference(fs::File::open(left)?, fs::File::open(right)?)
}

fn readers_equal_until_first_difference(left: impl Read, right: impl Read) -> io::Result<bool> {
    // `Read::read` is allowed to return fewer bytes than requested even when
    // more data is available (e.g. on FUSE/NFS, signalled reads, or large
    // pipes). Compare against fixed-size chunks via BufReader so we don't
    // mis-report equal files as different on short reads.
    let mut left = io::BufReader::with_capacity(BUFFER, left);
    let mut right = io::BufReader::with_capacity(BUFFER, right);

    loop {
        let left_chunk = io::BufRead::fill_buf(&mut left)?;
        let right_chunk = io::BufRead::fill_buf(&mut right)?;
        if left_chunk.is_empty() && right_chunk.is_empty() {
            return Ok(true);
        }
        if left_chunk.is_empty() || right_chunk.is_empty() {
            return Ok(false);
        }
        let take = left_chunk.len().min(right_chunk.len());
        if left_chunk[..take] != right_chunk[..take] {
            return Ok(false);
        }
        io::BufRead::consume(&mut left, take);
        io::BufRead::consume(&mut right, take);
    }
}

const BUFFER: usize = 8192;

fn normalized_text_content(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    Ok(text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n"))
}

fn entry_effective_method(
    left: Option<&EntryMeta>,
    right: Option<&EntryMeta>,
    options: &FolderCompareOptions,
) -> (Option<CompareMethod>, Option<String>) {
    let (Some(left), Some(right)) = (left, right) else {
        return (None, None);
    };

    if left.error.is_some() || right.error.is_some() || left.skipped || right.skipped {
        return (None, None);
    }

    if left.kind != EntryKind::File || right.kind != EntryKind::File {
        return (None, None);
    }

    let (method, note) = effective_compare_method(left, right, options);
    (Some(method), note)
}

fn effective_compare_method(
    left: &EntryMeta,
    right: &EntryMeta,
    options: &FolderCompareOptions,
) -> (CompareMethod, Option<String>) {
    let selected = options.compare_method;
    if !matches!(
        selected,
        CompareMethod::FullContents | CompareMethod::QuickContents
    ) {
        return (selected, None);
    }

    let Some(threshold) = options.large_file_threshold else {
        return (selected, None);
    };

    let largest_side = left.size.max(right.size);
    if largest_side <= threshold {
        return (selected, None);
    }

    let fallback = match options.large_file_fallback_method {
        CompareMethod::QuickContents | CompareMethod::BinaryContents => {
            options.large_file_fallback_method
        }
        _ => CompareMethod::BinaryContents,
    };

    if fallback == selected {
        return (selected, None);
    }

    (
        fallback,
        Some(format!(
            "method downgraded from {} to {} because largest side is {largest_side} bytes and threshold is {threshold} bytes",
            selected.as_str(),
            fallback.as_str()
        )),
    )
}

fn modified_times_match(
    left: Option<SystemTime>,
    right: Option<SystemTime>,
    tolerance: Duration,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            let delta = left
                .duration_since(right)
                .or_else(|_| right.duration_since(left))
                .unwrap_or_default();
            delta <= tolerance
        }
        _ => left == right,
    }
}

fn collect_entries(
    root: &Path,
    options: &FolderCompareOptions,
) -> io::Result<BTreeMap<PathBuf, EntryMeta>> {
    let mut entries = BTreeMap::new();
    let mut visited_dirs = BTreeSet::new();
    let metadata = fs::metadata(root)?;
    if metadata.is_dir() {
        visited_dirs.insert(directory_identity(root, &metadata)?);
    }
    collect_entries_inner(root, root, options, &mut entries, &mut visited_dirs)?;
    Ok(entries)
}

fn collect_entries_inner(
    root: &Path,
    current: &Path,
    options: &FolderCompareOptions,
    entries: &mut BTreeMap<PathBuf, EntryMeta>,
    visited_dirs: &mut BTreeSet<DirectoryIdentity>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let Ok(relative_path) = path.strip_prefix(root) else {
            continue;
        };

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(err) => {
                insert_error_entry(entries, relative_path, err.to_string());
                continue;
            }
        };

        if file_type.is_symlink() {
            collect_symlink_entry(root, relative_path, &path, options, entries, visited_dirs)?;
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(err) => {
                insert_error_entry(entries, relative_path, err.to_string());
                continue;
            }
        };

        collect_metadata_entry(
            relative_path,
            &path,
            &metadata,
            options,
            entries,
            visited_dirs,
        )?;

        if options.recursive
            && metadata.is_dir()
            && entries
                .get(relative_path)
                .is_some_and(|entry| !entry.skipped && entry.error.is_none())
        {
            let identity = directory_identity(&path, &metadata)?;
            visited_dirs.insert(identity.clone());
            collect_entries_inner(root, &path, options, entries, visited_dirs)?;
            visited_dirs.remove(&identity);
        }
    }

    Ok(())
}

fn collect_symlink_entry(
    root: &Path,
    relative_path: &Path,
    path: &Path,
    options: &FolderCompareOptions,
    entries: &mut BTreeMap<PathBuf, EntryMeta>,
    visited_dirs: &mut BTreeSet<DirectoryIdentity>,
) -> io::Result<()> {
    match options.symlink_policy {
        SymlinkPolicy::CompareTarget => match fs::read_link(path) {
            Ok(target) => {
                let context = filter_context(relative_path, false, Some(0), None, None, None);
                let skipped = is_filtered(&context, options);
                entries.insert(
                    relative_path.to_path_buf(),
                    EntryMeta {
                        size: 0,
                        is_dir: false,
                        kind: EntryKind::Symlink,
                        link_target: Some(target),
                        modified: None,
                        skipped,
                        error: None,
                    },
                );
            }
            Err(err) => insert_error_entry(entries, relative_path, err.to_string()),
        },
        SymlinkPolicy::SpecialFile => {
            let context = filter_context(relative_path, false, Some(0), None, None, None);
            let skipped = is_filtered(&context, options);
            entries.insert(
                relative_path.to_path_buf(),
                EntryMeta {
                    size: 0,
                    is_dir: false,
                    kind: EntryKind::Symlink,
                    link_target: None,
                    modified: None,
                    skipped,
                    error: None,
                },
            );
        }
        SymlinkPolicy::Follow => {
            let metadata = match fs::metadata(path) {
                Ok(metadata) => metadata,
                Err(err) => {
                    insert_error_entry(entries, relative_path, err.to_string());
                    return Ok(());
                }
            };
            collect_metadata_entry(
                relative_path,
                path,
                &metadata,
                options,
                entries,
                visited_dirs,
            )?;
            if options.recursive
                && metadata.is_dir()
                && entries
                    .get(relative_path)
                    .is_some_and(|entry| !entry.skipped && entry.error.is_none())
            {
                let identity = directory_identity(path, &metadata)?;
                visited_dirs.insert(identity.clone());
                collect_entries_inner(root, path, options, entries, visited_dirs)?;
                visited_dirs.remove(&identity);
            }
        }
    }

    Ok(())
}

fn collect_metadata_entry(
    relative_path: &Path,
    path: &Path,
    metadata: &fs::Metadata,
    options: &FolderCompareOptions,
    entries: &mut BTreeMap<PathBuf, EntryMeta>,
    visited_dirs: &BTreeSet<DirectoryIdentity>,
) -> io::Result<()> {
    let kind = if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.is_file() {
        EntryKind::File
    } else {
        EntryKind::Special
    };
    let is_dir = kind == EntryKind::Directory;
    let file_kind = if kind == EntryKind::File && filters_require_file_kind(options) {
        classify_filter_file_kind(path).ok()
    } else {
        None
    };
    let context = filter_context(
        relative_path,
        is_dir,
        Some(metadata.len()),
        metadata.modified().ok(),
        file_kind,
        Some(path),
    );
    let skipped = is_filtered(&context, options);
    let error = if kind == EntryKind::Special {
        Some("unsupported special file".to_owned())
    } else if is_dir && options.recursive && !skipped {
        let identity = directory_identity(path, metadata)?;
        if visited_dirs.contains(&identity) {
            Some("recursive directory loop detected".to_owned())
        } else {
            None
        }
    } else {
        None
    };

    entries.insert(
        relative_path.to_path_buf(),
        EntryMeta {
            size: metadata.len(),
            is_dir,
            kind,
            link_target: None,
            modified: metadata.modified().ok(),
            skipped,
            error,
        },
    );

    Ok(())
}

fn insert_error_entry(
    entries: &mut BTreeMap<PathBuf, EntryMeta>,
    relative_path: &Path,
    error: String,
) {
    entries.insert(
        relative_path.to_path_buf(),
        EntryMeta {
            size: 0,
            is_dir: false,
            kind: EntryKind::Special,
            link_target: None,
            modified: None,
            skipped: false,
            error: Some(error),
        },
    );
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DirectoryIdentity {
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(not(unix))]
    Canonical(PathBuf),
}

fn directory_identity(_path: &Path, metadata: &fs::Metadata) -> io::Result<DirectoryIdentity> {
    #[cfg(unix)]
    {
        Ok(DirectoryIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    #[cfg(not(unix))]
    {
        Ok(DirectoryIdentity::Canonical(_path.canonicalize()?))
    }
}

fn is_filtered(context: &FilterEntryContext<'_>, options: &FolderCompareOptions) -> bool {
    let mut has_applicable_include_rule = false;
    let mut included = false;

    for filter in &options.filters {
        has_applicable_include_rule |= filter.has_include_rule_for(context.is_dir);
        match filter.decision_for_entry_with_options(context, &options.filter_match_options) {
            FilterDecision::Exclude => return true,
            FilterDecision::Include => included = true,
            FilterDecision::Neutral => {}
        }
    }

    has_applicable_include_rule && !included
}

fn filter_context<'a>(
    relative_path: &'a Path,
    is_dir: bool,
    size: Option<u64>,
    modified: Option<SystemTime>,
    file_kind: Option<FilterFileKind>,
    resolved_path: Option<&'a Path>,
) -> FilterEntryContext<'a> {
    FilterEntryContext {
        // Rules match against the *relative* path; `resolved_path` carries the
        // real filesystem path so a directory `size` expression can recurse.
        path: relative_path,
        is_dir,
        size,
        modified,
        file_kind,
        resolved_path,
    }
}

fn filters_require_file_kind(options: &FolderCompareOptions) -> bool {
    options.filters.iter().any(FileFilter::requires_file_kind)
}

fn classify_filter_file_kind(path: &Path) -> io::Result<FilterFileKind> {
    let mut file = fs::File::open(path)?;
    let mut sample = vec![0; 4096];
    let read = file.read(&mut sample)?;
    sample.truncate(read);

    Ok(if is_likely_binary(&sample) {
        FilterFileKind::Binary
    } else {
        FilterFileKind::Text
    })
}

fn summarize_entries(
    entries: &[FolderEntryDiff],
    elapsed: Duration,
    status: FolderCompareStatus,
) -> FolderCompareSummary {
    let mut identical_count = 0;
    let mut different_count = 0;
    let mut left_only_count = 0;
    let mut right_only_count = 0;
    let mut skipped_count = 0;
    let mut errors_count = 0;
    let mut aborted_count = 0;
    let mut method_downgrade_count = 0;

    for entry in entries {
        match entry.state {
            FolderEntryState::Identical => identical_count += 1,
            FolderEntryState::Different => different_count += 1,
            FolderEntryState::LeftOnly => left_only_count += 1,
            FolderEntryState::RightOnly => right_only_count += 1,
            FolderEntryState::Skipped => skipped_count += 1,
            FolderEntryState::Error => errors_count += 1,
            FolderEntryState::Aborted => aborted_count += 1,
        }
        if entry.method_note.is_some() {
            method_downgrade_count += 1;
        }
    }

    FolderCompareSummary {
        compared_count: entries.len() - skipped_count - aborted_count,
        skipped_count,
        identical_count,
        different_count,
        one_sided_count: left_only_count + right_only_count,
        left_only_count,
        right_only_count,
        errors_count,
        aborted_count,
        method_downgrade_count,
        elapsed,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn compares_folder_entries() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("same.txt"), "same").unwrap();
        fs::write(right.join("same.txt"), "same").unwrap();
        fs::write(left.join("left.txt"), "left").unwrap();
        fs::write(right.join("right.txt"), "right").unwrap();
        fs::write(left.join("different.txt"), "left").unwrap();
        fs::write(right.join("different.txt"), "right side").unwrap();

        let result = compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();
        let states: BTreeMap<_, _> = result
            .entries
            .iter()
            .map(|entry| (entry.relative_path.as_path(), entry.state))
            .collect();

        assert_eq!(states[Path::new("same.txt")], FolderEntryState::Identical);
        assert_eq!(states[Path::new("left.txt")], FolderEntryState::LeftOnly);
        assert_eq!(states[Path::new("right.txt")], FolderEntryState::RightOnly);
        assert_eq!(
            states[Path::new("different.txt")],
            FolderEntryState::Different
        );
        assert_eq!(result.summary.compared_count, 4);
        assert_eq!(result.summary.identical_count, 1);
        assert_eq!(result.summary.different_count, 1);
        assert_eq!(result.summary.one_sided_count, 2);
        assert_eq!(result.summary.left_only_count, 1);
        assert_eq!(result.summary.right_only_count, 1);
        assert_eq!(result.summary.skipped_count, 0);
        assert_eq!(result.summary.errors_count, 0);
        assert_eq!(result.summary.status, FolderCompareStatus::Complete);
        let different = result
            .entries
            .iter()
            .find(|entry| entry.relative_path == Path::new("different.txt"))
            .unwrap();
        assert_eq!(different.name, "different.txt");
        assert_eq!(different.extension.as_deref(), Some("txt"));
        assert_eq!(different.entry_type, FolderEntryType::File);
        assert!(different.left_modified.is_some());
        assert!(different.right_modified.is_some());
        assert_eq!(
            result
                .filtered_entries(FolderEntryFilter::Differences)
                .into_iter()
                .map(|entry| entry.relative_path.as_path())
                .collect::<Vec<_>>(),
            vec![
                Path::new("different.txt"),
                Path::new("left.txt"),
                Path::new("right.txt")
            ]
        );
        assert_eq!(
            result
                .filtered_entries(FolderEntryFilter::Identical)
                .into_iter()
                .map(|entry| entry.relative_path.as_path())
                .collect::<Vec<_>>(),
            vec![Path::new("same.txt")]
        );
        assert!(
            result
                .filtered_entries(FolderEntryFilter::Errors)
                .is_empty()
        );
        assert!(
            result
                .filtered_entries(FolderEntryFilter::Skipped)
                .is_empty()
        );
    }

    #[test]
    fn binary_contents_detect_same_size_changes() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("same-size.txt"), "abcd").unwrap();
        fs::write(right.join("same-size.txt"), "wxyz").unwrap();

        let size_result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                recursive: true,
                compare_method: CompareMethod::Size,
                timestamp_tolerance: Duration::ZERO,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();
        let binary_result =
            compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();

        assert_eq!(size_result.entries[0].state, FolderEntryState::Identical);
        assert_eq!(binary_result.entries[0].state, FolderEntryState::Different);
    }

    #[test]
    fn binary_content_compare_can_stop_after_first_difference() {
        let left = CountingReader::new(b"abc".to_vec());
        let right = CountingReader::new(b"xbc".to_vec());

        assert!(!readers_equal_until_first_difference(left.clone(), right.clone()).unwrap());
        assert_eq!(left.bytes_read(), 1);
        assert_eq!(right.bytes_read(), 1);
    }

    #[test]
    fn large_file_threshold_records_effective_method_downgrade() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("large.txt"), "abcd").unwrap();
        fs::write(right.join("large.txt"), "abce").unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                compare_method: CompareMethod::FullContents,
                large_file_threshold: Some(3),
                large_file_fallback_method: CompareMethod::BinaryContents,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        let entry = &result.entries[0];
        assert_eq!(entry.state, FolderEntryState::Different);
        assert_eq!(entry.effective_method, Some(CompareMethod::BinaryContents));
        assert!(
            entry
                .method_note
                .as_deref()
                .is_some_and(|note| note.contains("method downgraded from full-contents"))
        );
        assert_eq!(result.summary.method_downgrade_count, 1);
    }

    #[derive(Clone)]
    struct CountingReader {
        data: std::sync::Arc<Vec<u8>>,
        offset: std::sync::Arc<std::sync::Mutex<usize>>,
        bytes_read: std::sync::Arc<std::sync::Mutex<usize>>,
    }

    impl CountingReader {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data: std::sync::Arc::new(data),
                offset: std::sync::Arc::new(std::sync::Mutex::new(0)),
                bytes_read: std::sync::Arc::new(std::sync::Mutex::new(0)),
            }
        }

        fn bytes_read(&self) -> usize {
            *self.bytes_read.lock().unwrap()
        }
    }

    impl Read for CountingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let mut offset = self.offset.lock().unwrap();
            if *offset >= self.data.len() {
                return Ok(0);
            }

            let end = (*offset + 1).min(self.data.len());
            let chunk = &self.data[*offset..end];
            buffer[..chunk.len()].copy_from_slice(chunk);
            *offset = end;
            *self.bytes_read.lock().unwrap() += chunk.len();
            Ok(chunk.len())
        }
    }

    #[test]
    fn hash_method_compares_file_content_hashes() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("same-size.txt"), "abcd").unwrap();
        fs::write(right.join("same-size.txt"), "wxyz").unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                compare_method: CompareMethod::HashBlake3,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.entries[0].state, FolderEntryState::Different);
        assert_eq!(
            result.entries[0].effective_method,
            Some(CompareMethod::HashBlake3)
        );
        // The hash-based method populates the display hashes.
        assert!(result.entries[0].left_hash.is_some());
        assert!(result.entries[0].right_hash.is_some());
    }

    #[test]
    fn display_hash_skipped_for_non_hash_method_with_non_blake3_algorithm() {
        // Selecting a non-Blake3 algorithm must not force per-file hashing
        // when the compare method does not actually hash content — doing so
        // doubled I/O on large trees.
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("same.txt"), "abcd").unwrap();
        fs::write(right.join("same.txt"), "abcd").unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                compare_method: CompareMethod::Size,
                hash_algorithm: HashAlgorithm::Sha256,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.entries[0].state, FolderEntryState::Identical);
        assert!(result.entries[0].left_hash.is_none());
        assert!(result.entries[0].right_hash.is_none());
    }

    #[test]
    fn normalized_text_method_ignores_line_endings_and_trailing_space() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("notes.txt"), "alpha  \r\nbeta\r\n").unwrap();
        fs::write(right.join("notes.txt"), "alpha\nbeta\n").unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                compare_method: CompareMethod::NormalizedText,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.entries[0].state, FolderEntryState::Identical);
        assert_eq!(
            result.entries[0].effective_method,
            Some(CompareMethod::NormalizedText)
        );
    }

    #[test]
    fn folder_operation_plan_records_copy_warnings_and_counts() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("different.txt"), "left").unwrap();
        fs::write(right.join("different.txt"), "right").unwrap();
        let mut permissions = fs::metadata(right.join("different.txt"))
            .unwrap()
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(right.join("different.txt"), permissions).unwrap();

        let result = compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();
        let plan = plan_folder_operation(
            &result,
            FolderOperationKind::CopyLeftToRight,
            &[PathBuf::from("different.txt")],
        );

        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.counts.copy_count, 1);
        assert_eq!(plan.counts.overwrite_warning_count, 1);
        assert_eq!(plan.counts.permission_warning_count, 1);
        assert_eq!(plan.counts.conflict_warning_count, 1);
        assert!(plan.operations[0].overwrites_existing);
        assert!(
            plan.operations[0]
                .source
                .as_ref()
                .unwrap()
                .ends_with("different.txt")
        );
        assert!(
            plan.operations[0]
                .target
                .as_ref()
                .unwrap()
                .ends_with("different.txt")
        );
    }

    #[test]
    fn folder_operation_plan_handles_rename_create_refresh_and_invalid_selection() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(left.join("only-dir")).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("rename.txt"), "left").unwrap();

        let result = compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();
        let rename = plan_folder_operation(
            &result,
            FolderOperationKind::RenameLeft {
                new_name: "renamed.txt".to_owned(),
            },
            &[PathBuf::from("rename.txt")],
        );
        let create = plan_folder_operation(
            &result,
            FolderOperationKind::CreateMissingRight,
            &[PathBuf::from("only-dir")],
        );
        let refresh = plan_folder_operation(
            &result,
            FolderOperationKind::Refresh,
            &[PathBuf::from("rename.txt"), PathBuf::from("only-dir")],
        );
        let invalid_create = plan_folder_operation(
            &result,
            FolderOperationKind::CreateMissingRight,
            &[PathBuf::from("rename.txt")],
        );
        let traversal_rename = plan_folder_operation(
            &result,
            FolderOperationKind::RenameLeft {
                new_name: "..".to_owned(),
            },
            &[PathBuf::from("rename.txt")],
        );

        assert_eq!(rename.counts.rename_count, 1);
        assert!(
            rename.operations[0]
                .target
                .as_ref()
                .unwrap()
                .ends_with("renamed.txt")
        );
        assert_eq!(create.counts.create_folder_count, 1);
        assert_eq!(refresh.counts.refresh_count, 2);
        assert!(invalid_create.operations.is_empty());
        assert_eq!(
            invalid_create.warnings[0].kind,
            FolderOperationWarningKind::InvalidSelection
        );
        assert!(traversal_rename.operations.is_empty());
        assert_eq!(
            traversal_rename.warnings[0].kind,
            FolderOperationWarningKind::InvalidSelection
        );
    }

    #[test]
    fn operation_path_within_root_detects_symlinked_parent_escape() {
        let fixture = TempFixture::new();
        let root = fixture.path.join("root");
        let outside = fixture.path.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("victim.txt"), "x").unwrap();
        // root/linkdir -> ../outside (a symlinked directory).
        std::os::unix::fs::symlink(&outside, root.join("linkdir")).unwrap();

        // A path traversing the symlinked parent escapes the root → rejected.
        assert!(!operation_path_within_root(
            &root.join("linkdir/victim.txt"),
            &root
        ));
        // A genuine (even not-yet-existing) in-root path is allowed.
        fs::create_dir_all(root.join("real")).unwrap();
        assert!(operation_path_within_root(
            &root.join("real/new.txt"),
            &root
        ));
        // A top-level symlink entry's parent IS the root → allowed; execute_copy
        // recreates the link rather than dereferencing it.
        std::os::unix::fs::symlink(outside.join("victim.txt"), root.join("toplink")).unwrap();
        assert!(operation_path_within_root(&root.join("toplink"), &root));
    }

    #[test]
    fn copy_of_symlink_recreates_link_instead_of_dereferencing() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();

        // A secret file OUTSIDE both comparison roots.
        let secret = fixture.path.join("secret.txt");
        fs::write(&secret, "TOP SECRET").unwrap();
        // left/link -> secret.txt (a symlink the user might copy to the right).
        std::os::unix::fs::symlink(&secret, left.join("link")).unwrap();

        let result = compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();
        let plan = plan_folder_operation(
            &result,
            FolderOperationKind::CopyLeftToRight,
            &[PathBuf::from("link")],
        );
        assert_eq!(plan.operations.len(), 1, "symlink entry should be copyable");

        let outcomes = execute_folder_operation_plan(&plan, fixture.path.as_path(), false);
        assert_eq!(
            outcomes[0].status,
            FolderOperationStatus::Succeeded,
            "{:?}",
            outcomes[0]
        );

        // The copied object must be a symlink, NOT a regular file containing the
        // dereferenced secret content (which would exfiltrate a file outside the
        // roots into the destination root).
        let copied = right.join("link");
        let meta = fs::symlink_metadata(&copied).unwrap();
        assert!(
            meta.file_type().is_symlink(),
            "copy must recreate the symlink, not dereference it"
        );
        assert_eq!(fs::read_link(&copied).unwrap(), secret);
    }

    #[test]
    fn follow_policy_refuses_operations_escaping_comparison_root() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();

        // A directory OUTSIDE both roots, with a victim file.
        let outside = fixture.path.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        fs::write(&victim, "do not touch").unwrap();
        // left/linkdir -> outside (a symlinked directory).
        std::os::unix::fs::symlink(&outside, left.join("linkdir")).unwrap();

        let opts = FolderCompareOptions {
            symlink_policy: SymlinkPolicy::Follow,
            ..FolderCompareOptions::default()
        };
        let result = compare_folders(&left, &right, &opts).unwrap();

        // Sanity: Follow surfaces the nested entry rooted under `left`, even
        // though its bytes live outside the root — this is the attack surface.
        let escaping: Vec<PathBuf> = result
            .entries
            .iter()
            .map(|e| e.relative_path.clone())
            .filter(|p| p.ends_with("victim.txt"))
            .collect();
        assert!(
            !escaping.is_empty(),
            "Follow policy should surface the nested entry"
        );

        // Both delete and copy of that entry must be refused by the containment
        // guard (no operations, an InvalidSelection warning).
        for kind in [
            FolderOperationKind::DeleteLeft,
            FolderOperationKind::CopyLeftToRight,
        ] {
            let plan = plan_folder_operation(&result, kind.clone(), &escaping);
            assert!(
                plan.operations.is_empty(),
                "{kind:?} escaping the root must be refused; got {:?}",
                plan.operations
            );
            assert!(
                plan.warnings
                    .iter()
                    .any(|w| w.kind == FolderOperationWarningKind::InvalidSelection),
                "{kind:?} should emit an InvalidSelection warning; got {:?}",
                plan.warnings
            );
            // Even if executed, the external file is untouched.
            let _ = execute_folder_operation_plan(&plan, fixture.path.as_path(), false);
        }
        assert!(victim.exists(), "victim file outside the root must survive");
        assert_eq!(fs::read_to_string(&victim).unwrap(), "do not touch");
    }

    #[test]
    fn progress_callback_can_cancel_and_preserve_aborted_rows() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("a.txt"), "same").unwrap();
        fs::write(right.join("a.txt"), "same").unwrap();
        fs::write(left.join("b.txt"), "left").unwrap();
        fs::write(right.join("b.txt"), "right").unwrap();
        fs::write(left.join("c.txt"), "left").unwrap();
        fs::write(right.join("c.txt"), "right").unwrap();
        let mut compared_events = 0;
        let mut saw_cancelled_event = false;

        let result = compare_folders_with_progress(
            &left,
            &right,
            &FolderCompareOptions::default(),
            |event| match event {
                FolderCompareEvent::Compared { .. } => {
                    compared_events += 1;
                    if compared_events == 1 {
                        FolderCompareControl::Cancel
                    } else {
                        FolderCompareControl::Continue
                    }
                }
                FolderCompareEvent::Cancelled { completed, aborted } => {
                    saw_cancelled_event = true;
                    assert_eq!(completed, 1);
                    assert_eq!(aborted, 2);
                    FolderCompareControl::Continue
                }
                _ => FolderCompareControl::Continue,
            },
        )
        .unwrap();

        assert!(saw_cancelled_event);
        assert_eq!(result.summary.status, FolderCompareStatus::Cancelled);
        assert_eq!(result.summary.compared_count, 1);
        assert_eq!(result.summary.aborted_count, 2);
        assert_eq!(result.filtered_entries(FolderEntryFilter::Aborted).len(), 2);
        assert!(!result.is_equal());
    }

    #[test]
    fn progress_callback_reports_skipped_error_and_completed_events() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("skip.log"), "left").unwrap();
        fs::write(right.join("skip.log"), "right").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("missing", left.join("broken")).unwrap();
            std::os::unix::fs::symlink("missing", right.join("broken")).unwrap();
        }

        let mut saw_discovered = false;
        let mut saw_skipped = false;
        let mut saw_error = false;
        let mut saw_completed = false;
        let result = compare_folders_with_progress(
            &left,
            &right,
            &FolderCompareOptions {
                filters: vec![FileFilter::parse("f!:skip").unwrap()],
                symlink_policy: SymlinkPolicy::Follow,
                ..FolderCompareOptions::default()
            },
            |event| {
                match event {
                    FolderCompareEvent::Discovered { .. } => saw_discovered = true,
                    FolderCompareEvent::Skipped { .. } => saw_skipped = true,
                    FolderCompareEvent::Error { .. } => saw_error = true,
                    FolderCompareEvent::Completed { .. } => saw_completed = true,
                    _ => {}
                }
                FolderCompareControl::Continue
            },
        )
        .unwrap();

        assert!(saw_discovered);
        assert!(saw_skipped);
        #[cfg(unix)]
        assert!(saw_error);
        assert!(saw_completed);
        assert_eq!(result.summary.status, FolderCompareStatus::Complete);
    }

    #[test]
    fn timestamp_tolerance_matches_close_modified_times() {
        let left = UNIX_EPOCH + Duration::from_secs(100);
        let right = UNIX_EPOCH + Duration::from_secs(103);

        assert!(modified_times_match(
            Some(left),
            Some(right),
            Duration::from_secs(3)
        ));
        assert!(!modified_times_match(
            Some(left),
            Some(right),
            Duration::from_secs(2)
        ));
    }

    #[test]
    fn modified_date_compare_uses_timestamp_tolerance() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        let left_file = left.join("same.txt");
        let right_file = right.join("same.txt");
        fs::write(&left_file, "same").unwrap();
        fs::write(&right_file, "same").unwrap();
        set_mtime(&left_file, 100);
        set_mtime(&right_file, 103);

        let strict = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                recursive: true,
                compare_method: CompareMethod::DateAndSize,
                timestamp_tolerance: Duration::ZERO,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();
        let tolerant = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                recursive: true,
                compare_method: CompareMethod::DateAndSize,
                timestamp_tolerance: Duration::from_secs(3),
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        assert_eq!(strict.entries[0].state, FolderEntryState::Different);
        assert_eq!(tolerant.entries[0].state, FolderEntryState::Identical);
    }

    #[test]
    fn filters_mark_entries_skipped_before_content_compare() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(left.join("target")).unwrap();
        fs::create_dir_all(right.join("target")).unwrap();
        fs::write(left.join("target/generated.txt"), "left").unwrap();
        fs::write(right.join("target/generated.txt"), "right").unwrap();
        fs::write(left.join("same.txt"), "same").unwrap();
        fs::write(right.join("same.txt"), "same").unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                filters: vec![FileFilter::parse("wd!:target").unwrap()],
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        let states: BTreeMap<_, _> = result
            .entries
            .iter()
            .map(|entry| (entry.relative_path.as_path(), entry.state))
            .collect();
        assert_eq!(states[Path::new("target")], FolderEntryState::Skipped);
        assert!(!states.contains_key(Path::new("target/generated.txt")));
        assert_eq!(states[Path::new("same.txt")], FolderEntryState::Identical);
        assert_eq!(result.summary.compared_count, 1);
        assert_eq!(result.summary.skipped_count, 1);
        assert!(result.is_equal());
        assert_eq!(
            result.filtered_entries(FolderEntryFilter::Skipped)[0].relative_path,
            PathBuf::from("target")
        );
    }

    #[test]
    fn include_filters_skip_non_matching_files() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(left.join("src")).unwrap();
        fs::create_dir_all(right.join("src")).unwrap();
        fs::write(left.join("src/main.rs"), "same").unwrap();
        fs::write(right.join("src/main.rs"), "same").unwrap();
        fs::write(left.join("notes.txt"), "left").unwrap();
        fs::write(right.join("notes.txt"), "right").unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                filters: vec![FileFilter::parse("wf:*.rs").unwrap()],
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        let states: BTreeMap<_, _> = result
            .entries
            .iter()
            .map(|entry| (entry.relative_path.as_path(), entry.state))
            .collect();
        assert_eq!(states[Path::new("src")], FolderEntryState::Identical);
        assert_eq!(
            states[Path::new("src/main.rs")],
            FolderEntryState::Identical
        );
        assert_eq!(states[Path::new("notes.txt")], FolderEntryState::Skipped);
        assert_eq!(result.summary.compared_count, 2);
        assert_eq!(result.summary.skipped_count, 1);
    }

    #[test]
    fn metadata_filters_skip_by_kind_size_and_timestamp() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("small.txt"), "same").unwrap();
        fs::write(right.join("small.txt"), "same").unwrap();
        fs::write(left.join("large.txt"), "left content").unwrap();
        fs::write(right.join("large.txt"), "right content").unwrap();
        fs::write(left.join("data.bin"), b"\0left").unwrap();
        fs::write(right.join("data.bin"), b"\0right").unwrap();

        let text_only = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                filters: vec![FileFilter::parse("fe:type == text").unwrap()],
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();
        let size_only = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                filters: vec![FileFilter::parse("fe:size >= 10B").unwrap()],
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();
        let timestamp_excluded = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                filters: vec![FileFilter::parse("fe!:modified_ms >= 0").unwrap()],
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        let text_states: BTreeMap<_, _> = text_only
            .entries
            .iter()
            .map(|entry| (entry.relative_path.as_path(), entry.state))
            .collect();
        assert_eq!(
            text_states[Path::new("data.bin")],
            FolderEntryState::Skipped
        );
        assert_eq!(
            text_states[Path::new("large.txt")],
            FolderEntryState::Different
        );

        let size_states: BTreeMap<_, _> = size_only
            .entries
            .iter()
            .map(|entry| (entry.relative_path.as_path(), entry.state))
            .collect();
        assert_eq!(
            size_states[Path::new("small.txt")],
            FolderEntryState::Skipped
        );
        assert_eq!(
            size_states[Path::new("large.txt")],
            FolderEntryState::Different
        );

        assert!(
            timestamp_excluded
                .entries
                .iter()
                .all(|entry| entry.state == FolderEntryState::Skipped)
        );
        assert_eq!(timestamp_excluded.summary.compared_count, 0);
        assert_eq!(timestamp_excluded.summary.skipped_count, 3);
    }

    #[test]
    fn built_in_pseudo_filesystem_filters_match_relative_names() {
        let filter = FileFilter::generated_directories();

        for name in ["proc", "sys", "dev", "run"] {
            let context = filter_context(Path::new(name), true, None, None, None, None);
            assert!(is_filtered(
                &context,
                &FolderCompareOptions {
                    filters: vec![filter.clone()],
                    ..FolderCompareOptions::default()
                }
            ));
        }
    }

    #[test]
    fn can_hide_skipped_folder_entries() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("generated.txt"), "left").unwrap();
        fs::write(right.join("generated.txt"), "right").unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                filters: vec![FileFilter::parse("f!:generated").unwrap()],
                include_skipped: false,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        assert!(result.entries.is_empty());
        assert_eq!(result.summary.compared_count, 0);
        assert_eq!(result.summary.skipped_count, 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_target_policy_compares_link_text_without_following() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        std::os::unix::fs::symlink("left-target.txt", left.join("link")).unwrap();
        std::os::unix::fs::symlink("right-target.txt", right.join("link")).unwrap();

        let result = compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();

        assert_eq!(result.entries[0].state, FolderEntryState::Different);
        assert_eq!(result.summary.different_count, 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_follow_policy_compares_targets_and_detects_loops() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        let outside_left = fixture.path.join("outside-left.txt");
        let outside_right = fixture.path.join("outside-right.txt");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(&outside_left, "same").unwrap();
        fs::write(&outside_right, "same").unwrap();
        std::os::unix::fs::symlink("../outside-left.txt", left.join("link")).unwrap();
        std::os::unix::fs::symlink("../outside-right.txt", right.join("link")).unwrap();
        std::os::unix::fs::symlink(".", left.join("loop")).unwrap();
        std::os::unix::fs::symlink(".", right.join("loop")).unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                symlink_policy: SymlinkPolicy::Follow,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();
        let states: BTreeMap<_, _> = result
            .entries
            .iter()
            .map(|entry| (entry.relative_path.as_path(), entry.state))
            .collect();

        assert_eq!(states[Path::new("link")], FolderEntryState::Identical);
        assert_eq!(states[Path::new("loop")], FolderEntryState::Error);
        assert_eq!(result.summary.identical_count, 1);
        assert_eq!(result.summary.errors_count, 1);
        assert!(
            result
                .entries
                .iter()
                .find(|entry| entry.relative_path == Path::new("loop"))
                .and_then(|entry| entry.error.as_deref())
                .is_some_and(|error| error.contains("recursive directory loop"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_special_policy_treats_links_as_special_entries() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        std::os::unix::fs::symlink("left-target.txt", left.join("link")).unwrap();
        std::os::unix::fs::symlink("right-target.txt", right.join("link")).unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                symlink_policy: SymlinkPolicy::SpecialFile,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.entries[0].state, FolderEntryState::Identical);
        assert!(result.is_equal());
    }

    #[cfg(unix)]
    #[test]
    fn followed_broken_symlinks_are_reported_as_error_rows() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        std::os::unix::fs::symlink("missing", left.join("broken")).unwrap();
        std::os::unix::fs::symlink("missing", right.join("broken")).unwrap();

        let result = compare_folders(
            &left,
            &right,
            &FolderCompareOptions {
                symlink_policy: SymlinkPolicy::Follow,
                ..FolderCompareOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.entries[0].state, FolderEntryState::Error);
        assert!(result.entries[0].error.is_some());
        assert_eq!(result.summary.errors_count, 1);
        assert_eq!(result.filtered_entries(FolderEntryFilter::Errors).len(), 1);
        assert!(!result.is_equal());
    }

    #[cfg(unix)]
    #[test]
    fn special_files_are_reported_as_error_rows() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        make_fifo(&left.join("pipe"));
        make_fifo(&right.join("pipe"));

        let result = compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();

        assert_eq!(result.entries[0].state, FolderEntryState::Error);
        assert_eq!(result.summary.errors_count, 1);
        assert!(
            result.entries[0]
                .error
                .as_deref()
                .is_some_and(|error| error.contains("unsupported special file"))
        );
    }

    #[cfg(unix)]
    fn make_fifo(path: &Path) {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let result = unsafe { libc::mkfifo(path.as_ptr(), 0o600) };
        assert_eq!(result, 0, "{}", io::Error::last_os_error());
    }

    fn set_mtime(path: &Path, seconds: i64) {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let times = [
            libc::timespec {
                tv_sec: seconds,
                tv_nsec: 0,
            },
            libc::timespec {
                tv_sec: seconds,
                tv_nsec: 0,
            },
        ];
        let result = unsafe { libc::utimensat(libc::AT_FDCWD, path.as_ptr(), times.as_ptr(), 0) };
        assert_eq!(result, 0, "{}", io::Error::last_os_error());
    }

    struct TempFixture {
        path: PathBuf,
    }

    impl TempFixture {
        fn new() -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("linsync-test-{suffix}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn hash_algorithm_default_is_blake3() {
        assert_eq!(HashAlgorithm::default(), HashAlgorithm::Blake3);
    }

    #[test]
    fn compute_file_hash_blake3() {
        let dir = std::env::temp_dir().join("linsync-hash-test-blake3");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        fs::write(&path, b"hello world").unwrap();
        let hash = compute_file_hash(&path, HashAlgorithm::Blake3).unwrap();
        assert!(!hash.is_empty());
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compute_file_hash_sha256() {
        let dir = std::env::temp_dir().join("linsync-hash-test-sha256");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        fs::write(&path, b"hello world").unwrap();
        let hash = compute_file_hash(&path, HashAlgorithm::Sha256).unwrap();
        assert!(!hash.is_empty());
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compute_file_hash_crc32() {
        let dir = std::env::temp_dir().join("linsync-hash-test-crc32");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        fs::write(&path, b"hello world").unwrap();
        let hash = compute_file_hash(&path, HashAlgorithm::Crc32).unwrap();
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 8);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_entry_diff_serializes_with_metadata() {
        let entry = FolderEntryDiff {
            relative_path: PathBuf::from("test.txt"),
            name: "test.txt".to_owned(),
            extension: Some("txt".to_owned()),
            state: FolderEntryState::Different,
            left_size: Some(100),
            right_size: Some(200),
            left_modified: None,
            right_modified: None,
            entry_type: FolderEntryType::File,
            effective_method: Some(CompareMethod::HashBlake3),
            method_note: None,
            is_dir: false,
            error: None,
            left_permissions: Some(0o644),
            right_permissions: Some(0o755),
            left_owner: Some("alice".to_owned()),
            right_owner: Some("bob".to_owned()),
            left_group: Some("users".to_owned()),
            right_group: Some("admin".to_owned()),
            left_hash: Some("abc123".to_owned()),
            right_hash: Some("def456".to_owned()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: FolderEntryDiff = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.relative_path, entry.relative_path);
        assert_eq!(deserialized.left_permissions, entry.left_permissions);
        assert_eq!(deserialized.right_permissions, entry.right_permissions);
        assert_eq!(deserialized.left_owner, entry.left_owner);
        assert_eq!(deserialized.right_owner, entry.right_owner);
        assert_eq!(deserialized.left_group, entry.left_group);
        assert_eq!(deserialized.right_group, entry.right_group);
        assert_eq!(deserialized.left_hash, entry.left_hash);
        assert_eq!(deserialized.right_hash, entry.right_hash);
    }

    #[test]
    fn risk_summary_empty_plan() {
        let plan = FolderOperationPlan {
            operations: Vec::new(),
            counts: FolderOperationCounts::default(),
            warnings: Vec::new(),
        };
        let summary = plan.risk_summary();
        assert_eq!(summary.total_operations, 0);
        assert_eq!(summary.overwrite_count, 0);
        assert_eq!(summary.delete_count, 0);
        assert_eq!(summary.high_risk_count, 0);
        assert!(summary.warnings.is_empty());
    }

    #[test]
    fn risk_summary_counts_overwrites() {
        let plan = FolderOperationPlan {
            operations: vec![
                FolderOperation {
                    kind: FolderOperationKind::CopyLeftToRight,
                    relative_path: PathBuf::from("a.txt"),
                    source: Some(PathBuf::from("/left/a.txt")),
                    target: Some(PathBuf::from("/right/a.txt")),
                    overwrites_existing: true,
                },
                FolderOperation {
                    kind: FolderOperationKind::CopyLeftToRight,
                    relative_path: PathBuf::from("b.txt"),
                    source: Some(PathBuf::from("/left/b.txt")),
                    target: Some(PathBuf::from("/right/b.txt")),
                    overwrites_existing: false,
                },
            ],
            counts: FolderOperationCounts::default(),
            warnings: vec![FolderOperationWarning {
                relative_path: PathBuf::from("a.txt"),
                kind: FolderOperationWarningKind::OverwriteExisting,
                message: "target exists".to_owned(),
            }],
        };
        let summary = plan.risk_summary();
        assert_eq!(summary.total_operations, 2);
        assert_eq!(summary.overwrite_count, 1);
        assert_eq!(summary.delete_count, 0);
        assert_eq!(summary.high_risk_count, 1);
        assert_eq!(summary.warnings.len(), 1);
    }

    #[test]
    fn risk_summary_counts_deletes() {
        let plan = FolderOperationPlan {
            operations: vec![
                FolderOperation {
                    kind: FolderOperationKind::DeleteLeft,
                    relative_path: PathBuf::from("a.txt"),
                    source: Some(PathBuf::from("/left/a.txt")),
                    target: None,
                    overwrites_existing: false,
                },
                FolderOperation {
                    kind: FolderOperationKind::DeleteRight,
                    relative_path: PathBuf::from("b.txt"),
                    source: Some(PathBuf::from("/right/b.txt")),
                    target: None,
                    overwrites_existing: false,
                },
                FolderOperation {
                    kind: FolderOperationKind::CopyLeftToRight,
                    relative_path: PathBuf::from("c.txt"),
                    source: Some(PathBuf::from("/left/c.txt")),
                    target: Some(PathBuf::from("/right/c.txt")),
                    overwrites_existing: false,
                },
            ],
            counts: FolderOperationCounts::default(),
            warnings: Vec::new(),
        };
        let summary = plan.risk_summary();
        assert_eq!(summary.total_operations, 3);
        assert_eq!(summary.overwrite_count, 0);
        assert_eq!(summary.delete_count, 2);
        assert_eq!(summary.high_risk_count, 0);
    }

    #[cfg(unix)]
    #[test]
    fn assess_operation_risks_detects_readonly() {
        let fixture = TempFixture::new();
        let left = fixture.path.join("left");
        let right = fixture.path.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("file.txt"), "left").unwrap();
        fs::write(right.join("file.txt"), "right").unwrap();
        let mut perms = fs::metadata(right.join("file.txt")).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(right.join("file.txt"), perms).unwrap();

        let result = compare_folders(&left, &right, &FolderCompareOptions::default()).unwrap();
        let mut plan = plan_folder_operation(
            &result,
            FolderOperationKind::DeleteRight,
            &[PathBuf::from("file.txt")],
        );
        assess_operation_risks(&mut plan, &left, &right).unwrap();

        assert!(
            plan.warnings
                .iter()
                .any(|w| w.kind == FolderOperationWarningKind::DeleteReadOnly),
            "expected DeleteReadOnly warning, got: {:?}",
            plan.warnings
        );
    }

    fn query_entry(
        rel: &str,
        state: FolderEntryState,
        entry_type: FolderEntryType,
        size: Option<u64>,
        modified_secs: Option<u64>,
    ) -> FolderEntryDiff {
        let relative_path = PathBuf::from(rel);
        let name = relative_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let extension = relative_path
            .extension()
            .map(|e| e.to_string_lossy().into_owned());
        let modified = modified_secs.map(|s| UNIX_EPOCH + std::time::Duration::from_secs(s));
        FolderEntryDiff {
            relative_path,
            name,
            extension,
            state,
            left_size: size,
            right_size: size,
            left_modified: modified,
            right_modified: modified,
            entry_type,
            effective_method: None,
            method_note: None,
            is_dir: entry_type == FolderEntryType::Directory,
            error: None,
            left_permissions: None,
            right_permissions: None,
            left_owner: None,
            right_owner: None,
            left_group: None,
            right_group: None,
            left_hash: None,
            right_hash: None,
        }
    }

    fn query_result(entries: Vec<FolderEntryDiff>) -> FolderCompareResult {
        FolderCompareResult {
            left_root: PathBuf::from("/left"),
            right_root: PathBuf::from("/right"),
            entries,
            summary: FolderCompareSummary::default(),
        }
    }

    fn flat_paths<'a>(page: &FolderQueryPage<'a>) -> Vec<String> {
        page.groups
            .iter()
            .flat_map(|group| group.entries.iter())
            .map(|entry| entry.relative_path.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn query_filters_by_type_and_state() {
        let result = query_result(vec![
            query_entry(
                "a.txt",
                FolderEntryState::Different,
                FolderEntryType::File,
                Some(10),
                None,
            ),
            query_entry(
                "dir",
                FolderEntryState::Different,
                FolderEntryType::Directory,
                None,
                None,
            ),
            query_entry(
                "link",
                FolderEntryState::LeftOnly,
                FolderEntryType::Symlink,
                None,
                None,
            ),
            query_entry(
                "same.txt",
                FolderEntryState::Identical,
                FolderEntryType::File,
                Some(5),
                None,
            ),
        ]);

        let page = result.query(&FolderQuery {
            state: FolderEntryFilter::Differences,
            types: FolderTypeFilter {
                directories: false,
                symlinks: false,
                ..FolderTypeFilter::default()
            },
            ..FolderQuery::default()
        });

        // Differences keeps Different + LeftOnly; type filter then drops the
        // directory and the symlink, leaving only the changed file.
        assert_eq!(flat_paths(&page), vec!["a.txt"]);
        assert_eq!(page.total_matched, 1);
        assert!(!page.has_more);
    }

    #[test]
    fn query_search_is_case_insensitive_over_relative_path() {
        let result = query_result(vec![
            query_entry(
                "src/Main.rs",
                FolderEntryState::Different,
                FolderEntryType::File,
                None,
                None,
            ),
            query_entry(
                "docs/readme.md",
                FolderEntryState::Different,
                FolderEntryType::File,
                None,
                None,
            ),
            query_entry(
                "src/lib.rs",
                FolderEntryState::Different,
                FolderEntryType::File,
                None,
                None,
            ),
        ]);

        let page = result.query(&FolderQuery {
            search: Some("SRC/".to_string()),
            ..FolderQuery::default()
        });

        assert_eq!(flat_paths(&page), vec!["src/Main.rs", "src/lib.rs"]);
        assert_eq!(page.total_matched, 2);

        // An empty search matches everything (treated as no filter).
        let all = result.query(&FolderQuery {
            search: Some(String::new()),
            ..FolderQuery::default()
        });
        assert_eq!(all.total_matched, 3);
    }

    #[test]
    fn query_sorts_by_size_descending_with_path_tiebreak() {
        let result = query_result(vec![
            query_entry(
                "b.bin",
                FolderEntryState::Different,
                FolderEntryType::File,
                Some(100),
                None,
            ),
            query_entry(
                "a.bin",
                FolderEntryState::Different,
                FolderEntryType::File,
                Some(100),
                None,
            ),
            query_entry(
                "small.bin",
                FolderEntryState::Different,
                FolderEntryType::File,
                Some(1),
                None,
            ),
        ]);

        let page = result.query(&FolderQuery {
            sort: FolderSortKey::Size,
            descending: true,
            ..FolderQuery::default()
        });

        // Larger first; equal sizes break on path (reversed under descending).
        assert_eq!(flat_paths(&page), vec!["b.bin", "a.bin", "small.bin"]);
    }

    #[test]
    fn query_paginates_with_offset_and_limit() {
        let entries = (0..5)
            .map(|i| {
                query_entry(
                    &format!("file{i}.txt"),
                    FolderEntryState::Different,
                    FolderEntryType::File,
                    None,
                    None,
                )
            })
            .collect();
        let result = query_result(entries);

        let page = result.query(&FolderQuery {
            offset: 1,
            limit: Some(2),
            ..FolderQuery::default()
        });

        assert_eq!(flat_paths(&page), vec!["file1.txt", "file2.txt"]);
        assert_eq!(page.total_matched, 5);
        assert_eq!(page.offset, 1);
        assert_eq!(page.returned, 2);
        assert!(page.has_more);

        // Offset past the end clamps and yields an empty, terminal page.
        let beyond = result.query(&FolderQuery {
            offset: 99,
            limit: Some(2),
            ..FolderQuery::default()
        });
        assert_eq!(beyond.offset, 5);
        assert_eq!(beyond.returned, 0);
        assert!(!beyond.has_more);
        assert!(beyond.groups.is_empty());
    }

    #[test]
    fn query_groups_by_type_in_first_seen_order() {
        let result = query_result(vec![
            query_entry(
                "dir",
                FolderEntryState::Different,
                FolderEntryType::Directory,
                None,
                None,
            ),
            query_entry(
                "a.txt",
                FolderEntryState::Different,
                FolderEntryType::File,
                None,
                None,
            ),
            query_entry(
                "b.txt",
                FolderEntryState::Different,
                FolderEntryType::File,
                None,
                None,
            ),
        ]);

        // Sort by type so directories sort ahead of files, then group by type.
        let page = result.query(&FolderQuery {
            sort: FolderSortKey::Type,
            group_by: FolderGrouping::Type,
            ..FolderQuery::default()
        });

        assert_eq!(page.groups.len(), 2);
        assert_eq!(page.groups[0].label, "directory");
        assert_eq!(page.groups[0].entries.len(), 1);
        assert_eq!(page.groups[1].label, "file");
        assert_eq!(page.groups[1].entries.len(), 2);
    }

    #[test]
    fn compare_virtual_trees_classifies_by_hash_and_presence() {
        use crate::plugin::VirtualNode;
        let vn = |path: &str, kind: &str, sha: Option<&str>| VirtualNode {
            path: path.to_string(),
            kind: kind.to_string(),
            size: None,
            sha256: sha.map(|s| s.to_string()),
        };
        let left = vec![
            vn("same.txt", "file", Some("aaa")),
            vn("changed.txt", "file", Some("bbb")),
            vn("only-left.txt", "file", Some("ccc")),
            vn("dir", "dir", None),
        ];
        let right = vec![
            vn("same.txt", "file", Some("aaa")),
            vn("changed.txt", "file", Some("zzz")),
            vn("only-right.txt", "file", Some("ddd")),
            vn("dir", "dir", None),
        ];
        let result = compare_virtual_trees(&left, &right);
        let state = |p: &str| {
            result
                .entries
                .iter()
                .find(|e| e.relative_path == Path::new(p))
                .unwrap_or_else(|| panic!("missing {p}"))
                .state
        };
        assert_eq!(state("same.txt"), FolderEntryState::Identical);
        assert_eq!(state("changed.txt"), FolderEntryState::Different);
        assert_eq!(state("only-left.txt"), FolderEntryState::LeftOnly);
        assert_eq!(state("only-right.txt"), FolderEntryState::RightOnly);
        assert_eq!(
            state("dir"),
            FolderEntryState::Identical,
            "dirs match on presence"
        );
        assert_eq!(result.summary.identical_count, 2);
        assert_eq!(result.summary.different_count, 1);
        assert_eq!(result.summary.left_only_count, 1);
        assert_eq!(result.summary.right_only_count, 1);
        assert_eq!(result.summary.one_sided_count, 2);

        // The standard folder query API works on the virtualized result.
        let page = result.query(&FolderQuery {
            state: FolderEntryFilter::Differences,
            ..FolderQuery::default()
        });
        assert_eq!(page.total_matched, 3, "Different + LeftOnly + RightOnly");
    }
}
