use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::text::{
    DiffBlockKind, LineEnding, MergeAction, MergeConflict, SavePlan, TextCompareOptions,
    TextCompareResult, TextDocument, TextEncoding, compare_documents,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditableDocument {
    pub original: TextDocument,
    pub lines: Vec<String>,
    pub dirty: bool,
}

impl EditableDocument {
    pub fn from_document(document: TextDocument) -> Self {
        let lines = document
            .lines
            .iter()
            .map(|line| line.text.clone())
            .collect();
        Self {
            original: document,
            lines,
            dirty: false,
        }
    }

    pub fn text(&self) -> String {
        let ending = self.original.line_ending.as_str();
        let mut text = self.lines.join(ending);
        if self
            .original
            .lines
            .last()
            .and_then(|line| line.newline)
            .is_some()
        {
            text.push_str(ending);
        }
        text
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoWayMergeState {
    pub left: EditableDocument,
    pub right: EditableDocument,
    pub compare: TextCompareResult,
}

impl TwoWayMergeState {
    pub fn new(compare: TextCompareResult) -> Self {
        Self {
            left: EditableDocument::from_document(compare.left_document.clone()),
            right: EditableDocument::from_document(compare.right_document.clone()),
            compare,
        }
    }

    pub fn recompute(&mut self, options: &TextCompareOptions) {
        let left = TextDocument::from_text(&self.compare.left_name, &self.left.text());
        let right = TextDocument::from_text(&self.compare.right_name, &self.right.text());
        self.compare = compare_documents(left, right, options);
    }

    pub fn apply(&mut self, action: MergeAction) -> Result<(), MergeError> {
        match action {
            MergeAction::CopyLeftToRight { block_index } => self.copy_left_to_right(block_index),
            MergeAction::CopyRightToLeft { block_index } => self.copy_right_to_left(block_index),
            MergeAction::ChooseLeft { block_index } => self.copy_left_to_right(block_index),
            MergeAction::ChooseRight { block_index } => self.copy_right_to_left(block_index),
            MergeAction::MarkResolved { .. } => Ok(()),
        }
    }

    pub fn copy_left_to_right(&mut self, block_index: usize) -> Result<(), MergeError> {
        let ranges = block_ranges(&self.compare, block_index)?;
        let replacement = self.left.lines[ranges.left.clone()].to_vec();
        self.right.lines.splice(ranges.right, replacement);
        self.right.dirty = true;
        Ok(())
    }

    pub fn copy_right_to_left(&mut self, block_index: usize) -> Result<(), MergeError> {
        let ranges = block_ranges(&self.compare, block_index)?;
        let replacement = self.right.lines[ranges.right.clone()].to_vec();
        self.left.lines.splice(ranges.left, replacement);
        self.left.dirty = true;
        Ok(())
    }
}

#[derive(Debug)]
pub enum MergeError {
    InvalidBlock(usize),
    UnsupportedEncoding(TextEncoding),
    BackupMissing(PathBuf),
    Io(io::Error),
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBlock(index) => write!(f, "invalid diff block index: {index}"),
            Self::UnsupportedEncoding(encoding) => {
                write!(f, "cannot safely save text decoded as {encoding:?}")
            }
            Self::BackupMissing(path) => {
                write!(f, "backup file does not exist: {}", path.display())
            }
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for MergeError {}

impl From<io::Error> for MergeError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// A stable identifier for a conflict within a [`ThreeWayMergeState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConflictId(pub u32);

/// Which version to choose when resolving a [`ThreeWayMergeState`] conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeChoice {
    Left,
    Right,
    Base,
    Custom(String),
}

/// A single conflict exposed by [`ThreeWayMergeState::conflicts`].
#[derive(Debug, Clone)]
pub struct ThreeWayConflict {
    pub id: ConflictId,
    pub start_line: usize,
    pub end_line: usize,
    pub base_lines: Vec<String>,
    pub left_lines: Vec<String>,
    pub right_lines: Vec<String>,
}

/// An error returned by [`ThreeWayMergeState::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeWayMergeError {
    /// The supplied [`ConflictId`] does not correspond to any conflict in this state.
    UnknownConflict(ConflictId),
}

impl std::fmt::Display for ThreeWayMergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownConflict(id) => write!(f, "unknown conflict id: {}", id.0),
        }
    }
}

impl std::error::Error for ThreeWayMergeError {}

/// Interactive three-way merge state with explicit per-conflict resolution.
///
/// Create with [`ThreeWayMergeState::new`], inspect conflicts with
/// [`conflicts`][Self::conflicts], resolve them with [`resolve`][Self::resolve],
/// then obtain the merged text via [`output`][Self::output] or write it with
/// [`save_to`][Self::save_to].
pub struct ThreeWayMergeState {
    pub base: EditableDocument,
    pub left: EditableDocument,
    pub right: EditableDocument,
    output: EditableDocument,
    resolutions: HashMap<ConflictId, MergeChoice>,
    /// Stable ordered list of conflict IDs derived from the initial merge.
    conflict_ids: Vec<ConflictId>,
    /// Line ending to use when writing the merged output, detected from the
    /// inputs so a CRLF/CR file is not silently rewritten with LF on save.
    output_newline: &'static str,
}

impl ThreeWayMergeState {
    /// Build a new state from three documents. An initial merge is performed
    /// immediately; unresolved conflicts appear as conflict-marker text in the
    /// output until they are resolved.
    pub fn new(base: TextDocument, left: TextDocument, right: TextDocument) -> Self {
        let base_text = base
            .lines
            .iter()
            .map(|l| {
                let mut s = l.text.clone();
                if l.newline.is_some() {
                    s.push('\n');
                }
                s
            })
            .collect::<String>();
        let left_text = left
            .lines
            .iter()
            .map(|l| {
                let mut s = l.text.clone();
                if l.newline.is_some() {
                    s.push('\n');
                }
                s
            })
            .collect::<String>();
        let right_text = right
            .lines
            .iter()
            .map(|l| {
                let mut s = l.text.clone();
                if l.newline.is_some() {
                    s.push('\n');
                }
                s
            })
            .collect::<String>();

        let merge_result = merge_three_way(&base_text, &left_text, &right_text);
        // Assign a stable ConflictId per conflict marker found in the result.
        let conflict_count = merge_result.conflicts.len() as u32;
        let conflict_ids: Vec<ConflictId> = (0..conflict_count).map(ConflictId).collect();

        // Preserve the inputs' line ending (prefer left, then base) on save.
        // Read it from the documents' detected `line_ending` — the flattened
        // `*_text` above always uses '\n', so it cannot be the source.
        let output_newline = newline_for(left.line_ending)
            .or_else(|| newline_for(base.line_ending))
            .unwrap_or("\n");

        let base_doc = EditableDocument::from_document(base);
        let left_doc = EditableDocument::from_document(left);
        let right_doc = EditableDocument::from_document(right);
        let output_doc = Self::build_output_doc(
            &merge_result,
            &conflict_ids,
            &HashMap::new(),
            output_newline,
        );

        Self {
            base: base_doc,
            left: left_doc,
            right: right_doc,
            output: output_doc,
            resolutions: HashMap::new(),
            conflict_ids,
            output_newline,
        }
    }

    /// Returns the list of conflicts that still need (or have) a resolution.
    /// The list is stable across calls.
    pub fn conflicts(&self) -> Vec<ThreeWayConflict> {
        // Re-run the merge to reflect any edits to the inputs, then build the
        // conflict list from the structured regions. We deliberately do NOT
        // re-parse the rendered marker text: file content that itself contains
        // lines like `=======` or `<<<<<<<` would otherwise corrupt parsing and
        // mis-segment the conflict (silent data loss on resolve/save).
        let base_text = editable_to_string(&self.base);
        let left_text = editable_to_string(&self.left);
        let right_text = editable_to_string(&self.right);
        let merge_result = merge_three_way(&base_text, &left_text, &right_text);

        // `merge_three_way` emits at most one whole-file conflict, rendered as
        // the entire `result_lines`, so each region spans line 1..=N.
        let end_line = merge_result.result_lines.len();
        merge_result
            .conflict_regions
            .into_iter()
            .zip(self.conflict_ids.iter().copied())
            .map(|(region, id)| ThreeWayConflict {
                id,
                start_line: 1,
                end_line,
                base_lines: region.base_lines,
                left_lines: region.left_lines,
                right_lines: region.right_lines,
            })
            .collect()
    }

    /// Choose a resolution for the conflict identified by `id`.
    ///
    /// Returns [`ThreeWayMergeError::UnknownConflict`] if `id` is not a valid
    /// conflict ID for this state.
    pub fn resolve(
        &mut self,
        id: ConflictId,
        choice: MergeChoice,
    ) -> Result<(), ThreeWayMergeError> {
        if !self.conflict_ids.contains(&id) {
            return Err(ThreeWayMergeError::UnknownConflict(id));
        }
        self.resolutions.insert(id, choice);
        self.rebuild_output();
        Ok(())
    }

    /// The current merged output document.
    pub fn output(&self) -> &EditableDocument {
        &self.output
    }

    pub fn unresolved_count(&self) -> usize {
        self.conflict_ids
            .iter()
            .filter(|id| !self.resolutions.contains_key(id))
            .count()
    }

    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        // Route through the module's safe writer: atomic temp+rename,
        // permission preservation, and O_NOFOLLOW (instead of a bare
        // fs::write that truncates the target in place and follows symlinks).
        let plan = create_save_plan(path, false);
        write_text_with_plan(&plan, &self.output.text())
            .map_err(|err| std::io::Error::other(err.to_string()))
    }

    fn rebuild_output(&mut self) {
        let base_text = editable_to_string(&self.base);
        let left_text = editable_to_string(&self.left);
        let right_text = editable_to_string(&self.right);
        let merge_result = merge_three_way(&base_text, &left_text, &right_text);
        self.output = Self::build_output_doc(
            &merge_result,
            &self.conflict_ids,
            &self.resolutions,
            self.output_newline,
        );
    }

    /// Build an `EditableDocument` from a merge result, substituting resolved
    /// conflicts with the chosen lines.
    ///
    /// Resolution is driven by the merge's structured [`ConflictRegion`]s, never
    /// by re-parsing the rendered marker text — so file content that contains
    /// marker-like lines cannot corrupt the output.
    fn build_output_doc(
        merge_result: &ThreeWayMergeResult,
        conflict_ids: &[ConflictId],
        resolutions: &HashMap<ConflictId, MergeChoice>,
        newline: &str,
    ) -> EditableDocument {
        if !merge_result.has_conflicts() {
            // Clean merge – use result_lines directly.
            let text = join_lines(&merge_result.result_lines, newline, true);
            return EditableDocument::from_document(TextDocument::from_text("output", &text));
        }

        // `merge_three_way` emits a single whole-file conflict whose rendered
        // form is the entire `result_lines`. If it is resolved, emit the chosen
        // side's structured lines; otherwise keep the conflict-marker text.
        let chosen = conflict_ids.first().and_then(|id| resolutions.get(id));
        let output_lines: Vec<String> = match (merge_result.conflict_regions.first(), chosen) {
            (Some(region), Some(choice)) => match choice {
                MergeChoice::Left => region.left_lines.clone(),
                MergeChoice::Right => region.right_lines.clone(),
                MergeChoice::Base => region.base_lines.clone(),
                MergeChoice::Custom(text) => text.lines().map(str::to_owned).collect(),
            },
            _ => merge_result.result_lines.clone(),
        };

        let text = join_lines(&output_lines, newline, true);
        EditableDocument::from_document(TextDocument::from_text("output", &text))
    }
}

/// Map a detected [`LineEnding`] to the newline string to write, or `None` when
/// it is indeterminate (no newline / mixed) so a fallback can be chosen.
fn newline_for(line_ending: LineEnding) -> Option<&'static str> {
    match line_ending {
        LineEnding::Lf => Some("\n"),
        LineEnding::Crlf => Some("\r\n"),
        LineEnding::Cr => Some("\r"),
        LineEnding::None | LineEnding::Mixed => None,
    }
}

fn editable_to_string(doc: &EditableDocument) -> String {
    doc.text()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreeWayMergeResult {
    pub result_lines: Vec<String>,
    pub conflicts: Vec<MergeConflict>,
    /// Structured content of each conflict, parallel to `conflicts`. Lets
    /// consumers resolve a conflict from real line vectors instead of
    /// re-parsing marker text out of `result_lines` (which silently corrupts
    /// when the file content itself contains marker-like lines).
    pub conflict_regions: Vec<ConflictRegion>,
}

/// The left/base/right line content of a single conflict, kept structured so
/// resolution never has to re-parse rendered conflict-marker text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRegion {
    pub left_lines: Vec<String>,
    pub base_lines: Vec<String>,
    pub right_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedConflictMarker {
    pub index: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub left_label: String,
    pub base_label: Option<String>,
    pub right_label: String,
    pub left_lines: Vec<String>,
    pub base_lines: Vec<String>,
    pub right_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictMarkerParseError {
    Unterminated { start_line: usize },
}

impl std::fmt::Display for ConflictMarkerParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unterminated { start_line } => {
                write!(
                    f,
                    "unterminated conflict marker starting at line {start_line}"
                )
            }
        }
    }
}

impl std::error::Error for ConflictMarkerParseError {}

impl ThreeWayMergeResult {
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }

    pub fn text(&self) -> String {
        join_lines(&self.result_lines, "\n", true)
    }

    pub fn conflict_marker_text(
        &self,
        left_label: &str,
        base_label: &str,
        right_label: &str,
    ) -> String {
        let mut output = String::new();
        for line in &self.result_lines {
            output.push_str(line);
            output.push('\n');
        }

        if self.has_conflicts() {
            output.push_str(&format!(
                "# Conflicts are marked with <<<<<<< {left_label}, ||||||| {base_label}, =======, >>>>>>> {right_label}\n"
            ));
        }

        output
    }
}

pub fn merge_three_way(base: &str, left: &str, right: &str) -> ThreeWayMergeResult {
    let base_lines = plain_lines(base);
    let left_lines = plain_lines(left);
    let right_lines = plain_lines(right);

    if left_lines == right_lines {
        return ThreeWayMergeResult {
            result_lines: left_lines,
            conflicts: Vec::new(),
            conflict_regions: Vec::new(),
        };
    }

    if left_lines == base_lines {
        return ThreeWayMergeResult {
            result_lines: right_lines,
            conflicts: Vec::new(),
            conflict_regions: Vec::new(),
        };
    }

    if right_lines == base_lines {
        return ThreeWayMergeResult {
            result_lines: left_lines,
            conflicts: Vec::new(),
            conflict_regions: Vec::new(),
        };
    }

    if let Some(merged) = merge_append_only(&base_lines, &left_lines, &right_lines) {
        return ThreeWayMergeResult {
            result_lines: merged,
            conflicts: Vec::new(),
            conflict_regions: Vec::new(),
        };
    }

    ThreeWayMergeResult {
        result_lines: conflict_marker_lines(&left_lines, &base_lines, &right_lines),
        conflicts: vec![MergeConflict {
            index: 0,
            left_start: 1,
            base_start: 1,
            right_start: 1,
            left_len: left_lines.len(),
            base_len: base_lines.len(),
            right_len: right_lines.len(),
        }],
        conflict_regions: vec![ConflictRegion {
            left_lines,
            base_lines,
            right_lines,
        }],
    }
}

pub fn parse_conflict_markers(
    text: &str,
) -> Result<Vec<ParsedConflictMarker>, ConflictMarkerParseError> {
    let mut conflicts = Vec::new();
    let mut current = None;

    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        match current.as_mut() {
            None => {
                if let Some(left_label) = marker_label(line, "<<<<<<<") {
                    current = Some(ConflictMarkerBuilder {
                        start_line: line_number,
                        left_label,
                        base_label: None,
                        section: ConflictMarkerSection::Left,
                        left_lines: Vec::new(),
                        base_lines: Vec::new(),
                        right_lines: Vec::new(),
                    });
                }
            }
            Some(builder) => match builder.section {
                ConflictMarkerSection::Left => {
                    if let Some(base_label) = marker_label(line, "|||||||") {
                        builder.base_label = Some(base_label);
                        builder.section = ConflictMarkerSection::Base;
                    } else if line == "=======" {
                        builder.section = ConflictMarkerSection::Right;
                    } else {
                        builder.left_lines.push(line.to_owned());
                    }
                }
                ConflictMarkerSection::Base => {
                    if line == "=======" {
                        builder.section = ConflictMarkerSection::Right;
                    } else {
                        builder.base_lines.push(line.to_owned());
                    }
                }
                ConflictMarkerSection::Right => {
                    if let Some(right_label) = marker_label(line, ">>>>>>>") {
                        let builder = current.take().expect("current conflict");
                        conflicts.push(ParsedConflictMarker {
                            index: conflicts.len(),
                            start_line: builder.start_line,
                            end_line: line_number,
                            left_label: builder.left_label,
                            base_label: builder.base_label,
                            right_label,
                            left_lines: builder.left_lines,
                            base_lines: builder.base_lines,
                            right_lines: builder.right_lines,
                        });
                    } else {
                        builder.right_lines.push(line.to_owned());
                    }
                }
            },
        }
    }

    if let Some(builder) = current {
        return Err(ConflictMarkerParseError::Unterminated {
            start_line: builder.start_line,
        });
    }

    Ok(conflicts)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConflictMarkerSection {
    Left,
    Base,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConflictMarkerBuilder {
    start_line: usize,
    left_label: String,
    base_label: Option<String>,
    section: ConflictMarkerSection,
    left_lines: Vec<String>,
    base_lines: Vec<String>,
    right_lines: Vec<String>,
}

fn marker_label(line: &str, marker: &str) -> Option<String> {
    line.strip_prefix(marker)
        .map(|label| label.trim_start().to_owned())
}

fn merge_append_only(base: &[String], left: &[String], right: &[String]) -> Option<Vec<String>> {
    if base.is_empty() {
        // Every slice "starts with" an empty base, so without this guard two
        // unrelated edits over an empty base would be silently concatenated as
        // a clean merge. Fall through to the conflict path instead.
        return None;
    }
    if !left.starts_with(base) || !right.starts_with(base) {
        return None;
    }

    let mut merged = base.to_vec();
    merged.extend_from_slice(&left[base.len()..]);
    merged.extend_from_slice(&right[base.len()..]);
    Some(merged)
}

fn conflict_marker_lines(left: &[String], base: &[String], right: &[String]) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("<<<<<<< LEFT".to_owned());
    lines.extend(left.iter().cloned());
    lines.push("||||||| BASE".to_owned());
    lines.extend(base.iter().cloned());
    lines.push("=======".to_owned());
    lines.extend(right.iter().cloned());
    lines.push(">>>>>>> RIGHT".to_owned());
    lines
}

fn plain_lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}

fn join_lines(lines: &[String], ending: &str, trailing_newline: bool) -> String {
    let mut text = lines.join(ending);
    if trailing_newline {
        text.push_str(ending);
    }
    text
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockRanges {
    left: Range<usize>,
    right: Range<usize>,
}

fn block_ranges(result: &TextCompareResult, block_index: usize) -> Result<BlockRanges, MergeError> {
    let mut current_block = None;
    let mut current_kind = None;
    let mut left_seen = 0;
    let mut right_seen = 0;
    let mut left_start = 0;
    let mut right_start = 0;
    let mut left_len = 0;
    let mut right_len = 0;

    for line in &result.lines {
        let kind = match line.kind {
            crate::text::DiffLineKind::Equal => DiffBlockKind::Equal,
            _ => DiffBlockKind::Difference,
        };

        if current_kind != Some(kind) {
            if current_block == Some(block_index) {
                return Ok(BlockRanges {
                    left: left_start..left_start + left_len,
                    right: right_start..right_start + right_len,
                });
            }

            current_block = Some(current_block.map_or(0, |index| index + 1));
            current_kind = Some(kind);
            left_start = left_seen;
            right_start = right_seen;
            left_len = 0;
            right_len = 0;
        }

        if line.left_line.is_some() {
            left_len += 1;
            left_seen += 1;
        }
        if line.right_line.is_some() {
            right_len += 1;
            right_seen += 1;
        }
    }

    if current_block == Some(block_index) {
        Ok(BlockRanges {
            left: left_start..left_start + left_len,
            right: right_start..right_start + right_len,
        })
    } else {
        Err(MergeError::InvalidBlock(block_index))
    }
}

pub fn create_save_plan(target: &Path, create_backup: bool) -> SavePlan {
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("linsync-output");
    let temporary = target.with_file_name(format!(".{file_name}.linsync-tmp"));

    SavePlan {
        target: target.to_path_buf(),
        temporary,
        create_backup,
        preserve_permissions: true,
    }
}

pub fn write_text_with_plan(plan: &SavePlan, contents: &str) -> Result<(), MergeError> {
    write_text_bytes_with_plan(plan, contents.as_bytes())
}

pub fn write_encoded_text_with_plan(
    plan: &SavePlan,
    contents: &str,
    encoding: TextEncoding,
) -> Result<(), MergeError> {
    let bytes = encoded_text_bytes(contents, encoding)?;
    write_text_bytes_with_plan(plan, &bytes)
}

fn write_text_bytes_with_plan(plan: &SavePlan, contents: &[u8]) -> Result<(), MergeError> {
    let target_permissions = if plan.preserve_permissions && plan.target.exists() {
        Some(fs::metadata(&plan.target)?.permissions())
    } else {
        None
    };

    write_temporary_file(&plan.temporary, contents, target_permissions.as_ref())?;

    if plan.create_backup && plan.target.exists() {
        copy_into_new_file(
            &plan.target,
            &backup_path(&plan.target),
            target_permissions.as_ref(),
        )?;
    }

    fs::rename(&plan.temporary, &plan.target)?;
    Ok(())
}

fn write_temporary_file(
    temporary: &Path,
    contents: &[u8],
    target_permissions: Option<&fs::Permissions>,
) -> io::Result<()> {
    let mut file = create_new_file(temporary, target_permissions)?;
    file.write_all(contents)?;
    file.sync_all()
}

fn copy_into_new_file(
    source: &Path,
    destination: &Path,
    target_permissions: Option<&fs::Permissions>,
) -> io::Result<()> {
    let mut source = open_source_no_follow(source)?;
    let mut destination = create_new_file(destination, target_permissions)?;
    io::copy(&mut source, &mut destination)?;
    destination.sync_all()
}

fn open_source_no_follow(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NOFOLLOW fails the open if the final path component is a symlink,
        // closing a TOCTOU where another local user with directory write swaps
        // the source for a symlink between metadata read and copy.
        options.custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

fn create_new_file(
    path: &Path,
    target_permissions: Option<&fs::Permissions>,
) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(file_mode(target_permissions));
    }

    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(file_mode(target_permissions)))?;
    }
    #[cfg(not(unix))]
    let _ = target_permissions;
    Ok(file)
}

#[cfg(unix)]
fn file_mode(target_permissions: Option<&fs::Permissions>) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    target_permissions
        .map(|permissions| permissions.mode() & 0o777)
        .unwrap_or(0o600)
}

fn encoded_text_bytes(contents: &str, encoding: TextEncoding) -> Result<Vec<u8>, MergeError> {
    match encoding {
        TextEncoding::Utf8 => Ok(contents.as_bytes().to_vec()),
        TextEncoding::Utf8Bom => {
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(contents.as_bytes());
            Ok(bytes)
        }
        TextEncoding::Utf16Le => {
            let mut bytes = vec![0xFF, 0xFE];
            for unit in contents.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            Ok(bytes)
        }
        TextEncoding::Utf16Be => {
            let mut bytes = vec![0xFE, 0xFF];
            for unit in contents.encode_utf16() {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
            Ok(bytes)
        }
        TextEncoding::LossyUtf8 => Err(MergeError::UnsupportedEncoding(encoding)),
    }
}

pub fn restore_backup(target: &Path) -> Result<(), MergeError> {
    let backup = backup_path(target);
    if !backup.exists() {
        return Err(MergeError::BackupMissing(backup));
    }

    let plan = create_save_plan(target, false);
    let target_permissions = if target.exists() {
        Some(fs::metadata(target)?.permissions())
    } else {
        None
    };
    copy_into_new_file(&backup, &plan.temporary, target_permissions.as_ref())?;
    fs::rename(&plan.temporary, target)?;
    Ok(())
}

pub fn backup_path(target: &Path) -> PathBuf {
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("linsync-output");
    target.with_file_name(format!("{file_name}.bak"))
}

impl crate::text::LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
            Self::Lf | Self::Mixed | Self::None => "\n",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{MergeAction, TextCompareOptions, compare_text};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn resolving_conflict_with_marker_like_content_is_not_corrupted() {
        // Left content itself contains a line that looks like a conflict
        // separator. Resolving to Left must reproduce the left content exactly,
        // not a mis-parsed fragment.
        let base = TextDocument::from_text("base", "x\n");
        let left = TextDocument::from_text("left", "L1\n=======\nL2\n");
        let right = TextDocument::from_text("right", "R1\n");
        let mut state = ThreeWayMergeState::new(base, left, right);

        let conflicts = state.conflicts();
        assert_eq!(conflicts.len(), 1, "expected one whole-file conflict");
        assert_eq!(conflicts[0].left_lines, vec!["L1", "=======", "L2"]);

        state
            .resolve(conflicts[0].id, MergeChoice::Left)
            .expect("resolve left");
        assert_eq!(state.output().text(), "L1\n=======\nL2\n");
    }

    #[test]
    fn merge_output_preserves_crlf_line_endings() {
        // A clean merge of CRLF inputs must not be rewritten with LF.
        let base = TextDocument::from_text("base", "a\r\n");
        let left = TextDocument::from_text("left", "a\r\nb\r\n");
        let right = TextDocument::from_text("right", "a\r\n");
        let state = ThreeWayMergeState::new(base, left, right);
        assert_eq!(state.output().text(), "a\r\nb\r\n");
    }

    #[test]
    fn empty_base_with_divergent_sides_is_a_conflict_not_a_concat() {
        // With an empty base, the append-only heuristic must not silently
        // concatenate two unrelated edits into a "clean" merge.
        let result = merge_three_way("", "apple\n", "banana\n");
        assert!(
            result.has_conflicts(),
            "empty base + divergent sides must conflict, got {:?}",
            result.result_lines
        );
        // A genuine shared prefix still merges append-only.
        let appended = merge_three_way("base\n", "base\nleft\n", "base\nright\n");
        assert!(!appended.has_conflicts());
        assert_eq!(appended.result_lines, vec!["base", "left", "right"]);
    }

    #[test]
    fn copies_left_block_to_right_and_tracks_dirty_state() {
        let compare = compare_text(
            "left",
            "alpha\nbeta\nshared\n",
            "right",
            "alpha\ngamma\nshared\n",
            &TextCompareOptions::default(),
        );
        let mut state = TwoWayMergeState::new(compare);

        state
            .apply(MergeAction::CopyLeftToRight { block_index: 1 })
            .unwrap();

        assert!(state.right.dirty);
        assert_eq!(state.right.text(), "alpha\nbeta\nshared\n");
        assert!(!state.left.dirty);
    }

    #[test]
    fn creates_save_plan_and_backup() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("file.txt");
        fs::write(&target, "old").unwrap();
        let plan = create_save_plan(&target, true);

        write_text_with_plan(&plan, "new").unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(fs::read_to_string(backup_path(&target)).unwrap(), "old");
        assert!(!plan.temporary.exists());
    }

    #[cfg(unix)]
    #[test]
    fn save_plan_preserves_existing_target_permissions() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("private.txt");
        fs::write(&target, "old").unwrap();
        let mut permissions = fs::metadata(&target).unwrap().permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&target, permissions).unwrap();

        write_text_with_plan(&create_save_plan(&target, false), "new").unwrap();

        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn save_plan_creates_new_files_owner_only() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("new-private.txt");

        write_text_with_plan(&create_save_plan(&target, false), "new").unwrap();

        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn save_plan_refuses_preexisting_temporary_path() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("file.txt");
        let victim = fixture.path.join("victim.txt");
        fs::write(&target, "old").unwrap();
        fs::write(&victim, "victim").unwrap();
        let plan = create_save_plan(&target, false);
        std::os::unix::fs::symlink(&victim, &plan.temporary).unwrap();

        let err = write_text_with_plan(&plan, "new").unwrap_err();

        assert!(matches!(err, MergeError::Io(_)));
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(fs::read_to_string(&victim).unwrap(), "victim");
    }

    #[cfg(unix)]
    #[test]
    fn save_plan_refuses_preexisting_backup_path() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("file.txt");
        let victim = fixture.path.join("victim.txt");
        fs::write(&target, "old").unwrap();
        fs::write(&victim, "victim").unwrap();
        std::os::unix::fs::symlink(&victim, backup_path(&target)).unwrap();

        let err = write_text_with_plan(&create_save_plan(&target, true), "new").unwrap_err();

        assert!(matches!(err, MergeError::Io(_)));
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(fs::read_to_string(&victim).unwrap(), "victim");
    }

    #[cfg(unix)]
    #[test]
    fn save_does_not_follow_symlinked_target_into_backup() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("file.txt");
        let victim = fixture.path.join("victim.txt");
        fs::write(&victim, "victim secret").unwrap();
        std::os::unix::fs::symlink(&victim, &target).unwrap();

        let err = write_text_with_plan(&create_save_plan(&target, true), "new").unwrap_err();

        assert!(matches!(err, MergeError::Io(_)));
        assert!(!backup_path(&target).exists());
        assert_eq!(fs::read_to_string(&victim).unwrap(), "victim secret");
    }

    #[cfg(unix)]
    #[test]
    fn restore_refuses_symlinked_backup() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("file.txt");
        let victim = fixture.path.join("victim.txt");
        fs::write(&target, "current").unwrap();
        fs::write(&victim, "victim secret").unwrap();
        std::os::unix::fs::symlink(&victim, backup_path(&target)).unwrap();

        let err = restore_backup(&target).unwrap_err();

        assert!(matches!(err, MergeError::Io(_)));
        assert_eq!(fs::read_to_string(&target).unwrap(), "current");
        assert_eq!(fs::read_to_string(&victim).unwrap(), "victim secret");
    }

    #[test]
    fn restores_from_backup_without_consuming_backup() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("file.txt");
        fs::write(&target, "new").unwrap();
        fs::write(backup_path(&target), "old").unwrap();

        restore_backup(&target).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        assert_eq!(fs::read_to_string(backup_path(&target)).unwrap(), "old");
    }

    #[test]
    fn restore_reports_missing_backup() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("file.txt");
        fs::write(&target, "new").unwrap();

        let err = restore_backup(&target).unwrap_err();

        assert!(matches!(err, MergeError::BackupMissing(_)));
    }

    #[test]
    fn encoded_save_plan_preserves_utf8_bom_and_utf16_bom() {
        let fixture = TempFixture::new();
        let utf8 = fixture.path.join("utf8.txt");
        let utf16 = fixture.path.join("utf16.txt");

        write_encoded_text_with_plan(
            &create_save_plan(&utf8, false),
            "hello\n",
            TextEncoding::Utf8Bom,
        )
        .unwrap();
        write_encoded_text_with_plan(
            &create_save_plan(&utf16, false),
            "hello\n",
            TextEncoding::Utf16Le,
        )
        .unwrap();

        assert!(fs::read(&utf8).unwrap().starts_with(&[0xEF, 0xBB, 0xBF]));
        assert!(fs::read(&utf16).unwrap().starts_with(&[0xFF, 0xFE]));
    }

    #[test]
    fn encoded_save_rejects_lossy_input() {
        let fixture = TempFixture::new();
        let target = fixture.path.join("lossy.txt");

        let err = write_encoded_text_with_plan(
            &create_save_plan(&target, false),
            "hello\n",
            TextEncoding::LossyUtf8,
        )
        .unwrap_err();

        assert!(matches!(err, MergeError::UnsupportedEncoding(_)));
    }

    #[test]
    fn three_way_merge_resolves_clean_conflict_with_explicit_choice() {
        let base = TextDocument::from_text("base", "a\nb\nc\n");
        let left = TextDocument::from_text("left", "a\nb_left\nc\n");
        let right = TextDocument::from_text("right", "a\nb_right\nc\n");
        let mut state = ThreeWayMergeState::new(base, left, right);

        let conflicts = state.conflicts();
        assert_eq!(conflicts.len(), 1, "single conflict expected");

        let conflict_ids: Vec<_> = conflicts.iter().map(|c| c.id).collect();
        state.resolve(conflict_ids[0], MergeChoice::Left).unwrap();
        assert_eq!(state.output().text(), "a\nb_left\nc\n");
    }

    #[test]
    fn three_way_merge_save_writes_output_text() {
        let fixture = TempFixture::new();
        let path = fixture.path.join("merged.txt");
        let base = TextDocument::from_text("base", "a\nb\nc\n");
        let left = TextDocument::from_text("left", "a\nB\nc\n");
        let right = TextDocument::from_text("right", "a\nB\nc\n"); // same change on both sides
        let state = ThreeWayMergeState::new(base, left, right);
        state.save_to(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\nB\nc\n");
    }

    #[test]
    fn three_way_merge_resolve_returns_error_for_unknown_id() {
        let base = TextDocument::from_text("base", "a\n");
        let left = TextDocument::from_text("left", "b\n");
        let right = TextDocument::from_text("right", "c\n");
        let mut state = ThreeWayMergeState::new(base, left, right);
        let fake_id = ConflictId(999);
        let result = state.resolve(fake_id, MergeChoice::Left);
        assert!(matches!(result, Err(ThreeWayMergeError::UnknownConflict(id)) if id == fake_id));
    }

    #[test]
    fn three_way_uses_left_when_right_unchanged() {
        let result = merge_three_way("alpha\n", "alpha\nleft\n", "alpha\n");

        assert!(!result.has_conflicts());
        assert_eq!(result.text(), "alpha\nleft\n");
    }

    #[test]
    fn three_way_keeps_both_append_only_changes() {
        let result = merge_three_way("alpha\n", "alpha\nleft\n", "alpha\nright\n");

        assert!(!result.has_conflicts());
        assert_eq!(result.text(), "alpha\nleft\nright\n");
    }

    #[test]
    fn three_way_accepts_same_change_from_both_sides() {
        let result = merge_three_way("alpha\n", "alpha\nsame\n", "alpha\nsame\n");

        assert!(!result.has_conflicts());
        assert_eq!(result.text(), "alpha\nsame\n");
    }

    #[test]
    fn three_way_marks_overlapping_conflicts() {
        let result = merge_three_way("value = 1\n", "value = 2\n", "value = 3\n");

        assert!(result.has_conflicts());
        assert_eq!(result.conflicts.len(), 1);
        assert!(result.text().contains("<<<<<<< LEFT"));
        assert!(result.text().contains(">>>>>>> RIGHT"));
    }

    #[test]
    fn parses_git_conflict_markers_with_base_section() {
        let conflicts = parse_conflict_markers(
            "before\n<<<<<<< HEAD\nleft\n||||||| base\nbase\n=======\nright\n>>>>>>> feature\nafter\n",
        )
        .unwrap();

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].index, 0);
        assert_eq!(conflicts[0].start_line, 2);
        assert_eq!(conflicts[0].end_line, 8);
        assert_eq!(conflicts[0].left_label, "HEAD");
        assert_eq!(conflicts[0].base_label.as_deref(), Some("base"));
        assert_eq!(conflicts[0].right_label, "feature");
        assert_eq!(conflicts[0].left_lines, vec!["left".to_owned()]);
        assert_eq!(conflicts[0].base_lines, vec!["base".to_owned()]);
        assert_eq!(conflicts[0].right_lines, vec!["right".to_owned()]);
    }

    #[test]
    fn rejects_unterminated_conflict_markers() {
        let err = parse_conflict_markers("<<<<<<< HEAD\nleft\n=======\nright\n").unwrap_err();

        assert!(matches!(
            err,
            ConflictMarkerParseError::Unterminated { start_line: 1 }
        ));
    }

    struct TempFixture {
        path: PathBuf,
    }

    impl TempFixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "linsync-merge-test-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
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
}
