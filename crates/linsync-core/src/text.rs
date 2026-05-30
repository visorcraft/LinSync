use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use regex::{Regex, RegexSet, RegexSetBuilder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareSide {
    Left,
    Base,
    Right,
    Result,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompareOptions {
    pub text: TextCompareOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompareSession {
    pub title: String,
    pub left: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<PathBuf>,
    pub right: PathBuf,
    #[serde(default)]
    pub options: CompareOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextCompareOptions {
    pub ignore_case: bool,
    pub ignore_whitespace: bool,
    pub ignore_eol: bool,
    pub ignore_blank_lines: bool,
    pub ignore_line_patterns: Vec<String>,
    pub substitutions: Vec<TextSubstitution>,
    pub detect_moves: bool,
    #[serde(default = "default_min_move_lines")]
    pub min_move_lines: usize,
}

fn default_min_move_lines() -> usize {
    3
}

impl Default for TextCompareOptions {
    fn default() -> Self {
        Self {
            ignore_case: false,
            ignore_whitespace: false,
            ignore_eol: false,
            ignore_blank_lines: false,
            ignore_line_patterns: Vec::new(),
            substitutions: Vec::new(),
            detect_moves: false,
            min_move_lines: default_min_move_lines(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSubstitution {
    pub pattern: String,
    pub replacement: String,
}

impl TextCompareOptions {
    pub fn validate_regex_options(&self) -> Result<(), regex::Error> {
        RegexSetBuilder::new(&self.ignore_line_patterns)
            .case_insensitive(self.ignore_case)
            .build()
            .map(|_| ())?;

        for substitution in &self.substitutions {
            Regex::new(&substitution.pattern)?;
        }

        Ok(())
    }

    pub fn validate_ignore_line_patterns(&self) -> Result<(), regex::Error> {
        self.validate_regex_options()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDocument {
    pub name: String,
    pub path: Option<PathBuf>,
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
    pub has_bom: bool,
    pub had_replacement_characters: bool,
    pub read_only: bool,
    pub byte_len: usize,
    pub lines: Vec<TextLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    LossyUtf8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    None,
    Lf,
    Crlf,
    Cr,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLine {
    pub number: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub text: String,
    pub newline: Option<LineEnding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextCompareResult {
    pub left_name: String,
    pub right_name: String,
    pub left_document: TextDocument,
    pub right_document: TextDocument,
    pub lines: Vec<DiffLine>,
    pub blocks: Vec<DiffBlock>,
    pub summary: CompareSummary,
}

impl TextCompareResult {
    pub fn is_equal(&self) -> bool {
        self.summary.differences == 0
    }

    pub fn difference_count(&self) -> usize {
        self.summary.differences
    }

    pub fn to_unified_diff(&self, context: usize) -> String {
        unified_diff(self, context)
    }

    pub fn to_context_diff(&self, context: usize) -> String {
        context_diff(self, context)
    }

    pub fn to_normal_diff(&self) -> String {
        normal_diff(self)
    }

    pub fn to_html_report(&self) -> String {
        html_report(self, None)
    }

    pub fn to_html_report_with_context(&self, context: Option<usize>) -> String {
        html_report(self, context)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareSummary {
    pub equal: bool,
    pub differences: usize,
    pub equal_lines: usize,
    pub changed_lines: usize,
    pub left_only_lines: usize,
    pub right_only_lines: usize,
    pub diff_blocks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffBlock {
    pub kind: DiffBlockKind,
    pub left_start: Option<usize>,
    pub right_start: Option<usize>,
    pub left_len: usize,
    pub right_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffBlockKind {
    Equal,
    Difference,
    Moved {
        partner_block: usize,
        direction: MoveDirection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveDirection {
    /// Content moved from left to right (deleted on left, added on right).
    LeftToRight,
    /// Content moved from right to left (added on left, deleted on right).
    RightToLeft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub left_line: Option<usize>,
    pub right_line: Option<usize>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub inline: Vec<InlineDiff>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Equal,
    Changed,
    LeftOnly,
    RightOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineDiff {
    pub left_start: usize,
    pub left_end: usize,
    pub right_start: usize,
    pub right_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeAction {
    CopyLeftToRight { block_index: usize },
    CopyRightToLeft { block_index: usize },
    ChooseLeft { block_index: usize },
    ChooseRight { block_index: usize },
    MarkResolved { conflict_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeConflict {
    pub index: usize,
    pub left_start: usize,
    pub base_start: usize,
    pub right_start: usize,
    pub left_len: usize,
    pub base_len: usize,
    pub right_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePlan {
    pub target: PathBuf,
    pub temporary: PathBuf,
    pub create_backup: bool,
    pub preserve_permissions: bool,
}

pub fn compare_text_files(
    left: &Path,
    right: &Path,
    options: &TextCompareOptions,
) -> io::Result<TextCompareResult> {
    let left_document = TextDocument::from_path(left)?;
    let right_document = TextDocument::from_path(right)?;
    Ok(compare_documents(left_document, right_document, options))
}

/// Cancellable variant of [`compare_text_files`]. `should_cancel` is polled
/// before and during the (O(n·m)) LCS construction; returning `true` aborts the
/// compare and yields `Ok(None)`. Used by the GUI bridge to honour the Stop
/// button on large-file text compares.
pub fn compare_text_files_cancellable(
    left: &Path,
    right: &Path,
    options: &TextCompareOptions,
    should_cancel: &dyn Fn() -> bool,
) -> io::Result<Option<TextCompareResult>> {
    let left_document = TextDocument::from_path(left)?;
    let right_document = TextDocument::from_path(right)?;
    Ok(compare_documents_cancellable(
        left_document,
        right_document,
        options,
        should_cancel,
    ))
}

pub fn compare_text(
    left_name: &str,
    left: &str,
    right_name: &str,
    right: &str,
    options: &TextCompareOptions,
) -> TextCompareResult {
    compare_documents(
        TextDocument::from_text(left_name, left),
        TextDocument::from_text(right_name, right),
        options,
    )
}

pub fn compare_documents(
    left_document: TextDocument,
    right_document: TextDocument,
    options: &TextCompareOptions,
) -> TextCompareResult {
    compare_documents_cancellable(left_document, right_document, options, &|| false)
        .expect("a non-cancelling compare always produces a result")
}

/// Cancellable variant of [`compare_documents`]. Returns `None` when
/// `should_cancel` reports `true` (checked up-front and once per row of the LCS
/// table); otherwise behaves exactly like [`compare_documents`].
pub fn compare_documents_cancellable(
    left_document: TextDocument,
    right_document: TextDocument,
    options: &TextCompareOptions,
    should_cancel: &dyn Fn() -> bool,
) -> Option<TextCompareResult> {
    if should_cancel() {
        return None;
    }
    let left_lines = comparable_lines(&left_document, options);
    let right_lines = comparable_lines(&right_document, options);
    let lcs = lcs_table_cancellable(&left_lines, &right_lines, should_cancel)?;
    let raw_lines = raw_diff_lines(
        &left_document,
        &right_document,
        &left_lines,
        &right_lines,
        &lcs,
    );
    let lines = pair_changed_lines(raw_lines);
    let mut blocks = diff_blocks(&lines);
    if options.detect_moves {
        detect_moved_blocks(&lines, &mut blocks, options);
    }
    let summary = compare_summary(&lines, &blocks);

    Some(TextCompareResult {
        left_name: left_document.name.clone(),
        right_name: right_document.name.clone(),
        left_document,
        right_document,
        lines,
        blocks,
        summary,
    })
}

impl TextDocument {
    pub fn from_path(path: &Path) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        let read_only = fs::metadata(path)
            .map(|metadata| metadata.permissions().readonly())
            .unwrap_or(false);
        Ok(Self::from_bytes(
            path.display().to_string(),
            Some(path.to_path_buf()),
            &bytes,
            read_only,
        ))
    }

    pub fn from_text(name: &str, text: &str) -> Self {
        Self::from_bytes(name.to_owned(), None, text.as_bytes(), false)
    }

    pub fn from_bytes(name: String, path: Option<PathBuf>, bytes: &[u8], read_only: bool) -> Self {
        let decoded = decode_text(bytes);
        let lines = split_lines(&decoded.text);
        let line_ending = detect_line_ending(&lines);

        Self {
            name,
            path,
            encoding: decoded.encoding,
            line_ending,
            has_bom: decoded.has_bom,
            had_replacement_characters: decoded.had_replacement_characters,
            read_only,
            byte_len: bytes.len(),
            lines,
        }
    }
}

struct DecodedText {
    text: String,
    encoding: TextEncoding,
    has_bom: bool,
    had_replacement_characters: bool,
}

fn decode_text(bytes: &[u8]) -> DecodedText {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (text, had_replacement_characters) = decode_utf8_lossy(&bytes[3..]);
        return DecodedText {
            text,
            encoding: TextEncoding::Utf8Bom,
            has_bom: true,
            had_replacement_characters,
        };
    }

    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16(&bytes[2..], true);
    }

    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16(&bytes[2..], false);
    }

    let (text, had_replacement_characters) = decode_utf8_lossy(bytes);
    DecodedText {
        text,
        encoding: if had_replacement_characters {
            TextEncoding::LossyUtf8
        } else {
            TextEncoding::Utf8
        },
        has_bom: false,
        had_replacement_characters,
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> DecodedText {
    let mut words = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let word = if little_endian {
            u16::from_le_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_be_bytes([chunk[0], chunk[1]])
        };
        words.push(word);
    }

    let text = String::from_utf16_lossy(&words);
    DecodedText {
        text: text.clone(),
        encoding: if little_endian {
            TextEncoding::Utf16Le
        } else {
            TextEncoding::Utf16Be
        },
        has_bom: true,
        had_replacement_characters: text.contains(char::REPLACEMENT_CHARACTER),
    }
}

fn decode_utf8_lossy(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_owned(), false),
        Err(_) => {
            let text = String::from_utf8_lossy(bytes).into_owned();
            (text, true)
        }
    }
}

fn split_lines(text: &str) -> Vec<TextLine> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut line_start = 0;
    let mut cursor = 0;
    let mut number = 1;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\r' if cursor + 1 < bytes.len() && bytes[cursor + 1] == b'\n' => {
                push_line(
                    &mut lines,
                    number,
                    line_start,
                    cursor,
                    &text[line_start..cursor],
                    Some(LineEnding::Crlf),
                );
                cursor += 2;
                line_start = cursor;
                number += 1;
            }
            b'\r' => {
                push_line(
                    &mut lines,
                    number,
                    line_start,
                    cursor,
                    &text[line_start..cursor],
                    Some(LineEnding::Cr),
                );
                cursor += 1;
                line_start = cursor;
                number += 1;
            }
            b'\n' => {
                push_line(
                    &mut lines,
                    number,
                    line_start,
                    cursor,
                    &text[line_start..cursor],
                    Some(LineEnding::Lf),
                );
                cursor += 1;
                line_start = cursor;
                number += 1;
            }
            _ => cursor += 1,
        }
    }

    if line_start < bytes.len() {
        push_line(
            &mut lines,
            number,
            line_start,
            bytes.len(),
            &text[line_start..],
            None,
        );
    }

    lines
}

fn push_line(
    lines: &mut Vec<TextLine>,
    number: usize,
    byte_start: usize,
    byte_end: usize,
    text: &str,
    newline: Option<LineEnding>,
) {
    lines.push(TextLine {
        number,
        byte_start,
        byte_end,
        text: text.to_owned(),
        newline,
    });
}

fn detect_line_ending(lines: &[TextLine]) -> LineEnding {
    let mut detected = None;

    for newline in lines.iter().filter_map(|line| line.newline) {
        match detected {
            None => detected = Some(newline),
            Some(existing) if existing == newline => {}
            Some(_) => return LineEnding::Mixed,
        }
    }

    detected.unwrap_or(LineEnding::None)
}

fn comparable_lines(document: &TextDocument, options: &TextCompareOptions) -> Vec<ComparableLine> {
    let normalization = NormalizationPlan::new(options);

    document
        .lines
        .iter()
        .filter(|line| {
            !normalization
                .ignore_line_patterns
                .as_ref()
                .is_some_and(|patterns| patterns.is_match(&line.text))
        })
        .filter_map(|line| {
            let text = normalization.normalize_line(&line.text);
            if options.ignore_blank_lines && text.trim().is_empty() {
                return None;
            }

            Some(ComparableLine {
                number: line.number,
                text,
            })
        })
        .collect()
}

struct NormalizationPlan<'a> {
    options: &'a TextCompareOptions,
    ignore_line_patterns: Option<RegexSet>,
    substitutions: Vec<(Regex, &'a str)>,
}

impl<'a> NormalizationPlan<'a> {
    fn new(options: &'a TextCompareOptions) -> Self {
        let ignore_line_patterns = RegexSetBuilder::new(&options.ignore_line_patterns)
            .case_insensitive(options.ignore_case)
            .build()
            .ok();
        let substitutions = options
            .substitutions
            .iter()
            .filter_map(|substitution| {
                Regex::new(&substitution.pattern)
                    .ok()
                    .map(|regex| (regex, substitution.replacement.as_str()))
            })
            .collect();

        Self {
            options,
            ignore_line_patterns,
            substitutions,
        }
    }

    fn normalize_line(&self, line: &str) -> String {
        let mut normalized = line.to_owned();
        for (regex, replacement) in &self.substitutions {
            normalized = regex.replace_all(&normalized, *replacement).into_owned();
        }

        if self.options.ignore_whitespace {
            normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
        }

        if self.options.ignore_case {
            normalized = normalized.to_lowercase();
        }

        normalized
    }
}

#[derive(Debug, Clone)]
struct ComparableLine {
    number: usize,
    text: String,
}

fn raw_diff_lines(
    left_document: &TextDocument,
    right_document: &TextDocument,
    left_lines: &[ComparableLine],
    right_lines: &[ComparableLine],
    lcs: &[Vec<usize>],
) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut i = 0;
    let mut j = 0;

    while i < left_lines.len() && j < right_lines.len() {
        if left_lines[i].text == right_lines[j].text {
            lines.push(equal_line(
                left_document,
                right_document,
                left_lines[i].number,
                right_lines[j].number,
            ));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            lines.push(left_only_line(left_document, left_lines[i].number));
            i += 1;
        } else {
            lines.push(right_only_line(right_document, right_lines[j].number));
            j += 1;
        }
    }

    while i < left_lines.len() {
        lines.push(left_only_line(left_document, left_lines[i].number));
        i += 1;
    }

    while j < right_lines.len() {
        lines.push(right_only_line(right_document, right_lines[j].number));
        j += 1;
    }

    lines
}

fn equal_line(
    left_document: &TextDocument,
    right_document: &TextDocument,
    left_number: usize,
    right_number: usize,
) -> DiffLine {
    let left = &left_document.lines[left_number - 1];
    let right = &right_document.lines[right_number - 1];
    DiffLine {
        kind: DiffLineKind::Equal,
        left_line: Some(left_number),
        right_line: Some(right_number),
        left: Some(left.text.clone()),
        right: Some(right.text.clone()),
        inline: Vec::new(),
    }
}

fn left_only_line(document: &TextDocument, line_number: usize) -> DiffLine {
    let line = &document.lines[line_number - 1];
    DiffLine {
        kind: DiffLineKind::LeftOnly,
        left_line: Some(line_number),
        right_line: None,
        left: Some(line.text.clone()),
        right: None,
        inline: Vec::new(),
    }
}

fn right_only_line(document: &TextDocument, line_number: usize) -> DiffLine {
    let line = &document.lines[line_number - 1];
    DiffLine {
        kind: DiffLineKind::RightOnly,
        left_line: None,
        right_line: Some(line_number),
        left: None,
        right: Some(line.text.clone()),
        inline: Vec::new(),
    }
}

fn pair_changed_lines(raw_lines: Vec<DiffLine>) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut index = 0;

    while index < raw_lines.len() {
        let current = &raw_lines[index];
        let next = match raw_lines.get(index + 1) {
            Some(next)
                if matches!(current.kind, DiffLineKind::LeftOnly)
                    && matches!(next.kind, DiffLineKind::RightOnly) =>
            {
                Some(next)
            }
            _ => None,
        };

        if let Some(next) = next {
            let left = current.left.clone().unwrap_or_default();
            let right = next.right.clone().unwrap_or_default();
            lines.push(DiffLine {
                kind: DiffLineKind::Changed,
                left_line: current.left_line,
                right_line: next.right_line,
                inline: inline_diff(&left, &right),
                left: Some(left),
                right: Some(right),
            });
            index += 2;
        } else {
            lines.push(current.clone());
            index += 1;
        }
    }

    lines
}

fn inline_diff(left: &str, right: &str) -> Vec<InlineDiff> {
    if left == right {
        return Vec::new();
    }

    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    let mut prefix = 0;
    while prefix < left_chars.len()
        && prefix < right_chars.len()
        && left_chars[prefix] == right_chars[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix + prefix < left_chars.len()
        && suffix + prefix < right_chars.len()
        && left_chars[left_chars.len() - 1 - suffix] == right_chars[right_chars.len() - 1 - suffix]
    {
        suffix += 1;
    }

    vec![InlineDiff {
        left_start: prefix,
        left_end: left_chars.len().saturating_sub(suffix),
        right_start: prefix,
        right_end: right_chars.len().saturating_sub(suffix),
    }]
}

fn lcs_table_cancellable(
    left: &[ComparableLine],
    right: &[ComparableLine],
    should_cancel: &dyn Fn() -> bool,
) -> Option<Vec<Vec<usize>>> {
    let mut table = vec![vec![0; right.len() + 1]; left.len() + 1];

    for i in (0..left.len()).rev() {
        if should_cancel() {
            return None;
        }
        for j in (0..right.len()).rev() {
            table[i][j] = if left[i].text == right[j].text {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    Some(table)
}

fn diff_blocks(lines: &[DiffLine]) -> Vec<DiffBlock> {
    let mut blocks = Vec::new();
    let Some(first) = lines.first() else {
        return blocks;
    };

    let mut current_kind = block_kind(first.kind);
    let mut left_start = first.left_line;
    let mut right_start = first.right_line;
    let mut left_len = usize::from(first.left_line.is_some());
    let mut right_len = usize::from(first.right_line.is_some());

    for line in lines.iter().skip(1) {
        let kind = block_kind(line.kind);
        if kind == current_kind {
            left_len += usize::from(line.left_line.is_some());
            right_len += usize::from(line.right_line.is_some());
        } else {
            blocks.push(DiffBlock {
                kind: current_kind,
                left_start,
                right_start,
                left_len,
                right_len,
            });
            current_kind = kind;
            left_start = line.left_line;
            right_start = line.right_line;
            left_len = usize::from(line.left_line.is_some());
            right_len = usize::from(line.right_line.is_some());
        }
    }

    blocks.push(DiffBlock {
        kind: current_kind,
        left_start,
        right_start,
        left_len,
        right_len,
    });

    blocks
}

fn block_kind(kind: DiffLineKind) -> DiffBlockKind {
    match kind {
        DiffLineKind::Equal => DiffBlockKind::Equal,
        DiffLineKind::Changed | DiffLineKind::LeftOnly | DiffLineKind::RightOnly => {
            DiffBlockKind::Difference
        }
    }
}

/// After the primary diff blocks are computed, scan `Difference` blocks for
/// Delete-only ↔ Add-only pairs whose content is identical under the current
/// normalization options.  Matching pairs are re-tagged as `Moved`.
///
/// Algorithm is O(n) in the number of blocks using a HashMap on content key.
fn detect_moved_blocks(lines: &[DiffLine], blocks: &mut [DiffBlock], options: &TextCompareOptions) {
    let normalization = NormalizationPlan::new(options);
    let min = options.min_move_lines;

    // Collect content keys for left-only (Delete) and right-only (Add) blocks
    // that are large enough.

    // key → vec of block indices that are left-only
    let mut delete_map: HashMap<String, Vec<usize>> = HashMap::new();
    // key → vec of block indices that are right-only
    let mut add_map: HashMap<String, Vec<usize>> = HashMap::new();

    for (block_idx, block) in blocks.iter().enumerate() {
        if !matches!(block.kind, DiffBlockKind::Difference) {
            continue;
        }

        // Determine if this block is purely left-only (delete) or purely
        // right-only (add).  Mixed blocks (Changed lines) are skipped.
        let block_lines = block_diff_lines(lines, block);

        let all_left_only = block_lines
            .iter()
            .all(|l| matches!(l.kind, DiffLineKind::LeftOnly));
        let all_right_only = block_lines
            .iter()
            .all(|l| matches!(l.kind, DiffLineKind::RightOnly));

        if !all_left_only && !all_right_only {
            continue;
        }

        let line_count = if all_left_only {
            block.left_len
        } else {
            block.right_len
        };
        if line_count < min {
            continue;
        }

        // Build a canonical content key for the block by joining normalized line
        // texts.
        let key = block_lines
            .iter()
            .map(|l| {
                let raw = if all_left_only {
                    l.left.as_deref().unwrap_or("")
                } else {
                    l.right.as_deref().unwrap_or("")
                };
                normalization.normalize_line(raw)
            })
            .collect::<Vec<_>>()
            .join("\n");

        if all_left_only {
            delete_map.entry(key).or_default().push(block_idx);
        } else {
            add_map.entry(key).or_default().push(block_idx);
        }
    }

    // For each key present in both maps, pair them up one-to-one.
    for (key, delete_indices) in &delete_map {
        let Some(add_indices) = add_map.get(key) else {
            continue;
        };

        for (del_idx, add_idx) in delete_indices.iter().zip(add_indices.iter()) {
            blocks[*del_idx].kind = DiffBlockKind::Moved {
                partner_block: *add_idx,
                direction: MoveDirection::LeftToRight,
            };
            blocks[*add_idx].kind = DiffBlockKind::Moved {
                partner_block: *del_idx,
                direction: MoveDirection::RightToLeft,
            };
        }
    }
}

/// Return the slice of `DiffLine`s that belong to `block` by matching line
/// numbers stored in the block against the line list.
fn block_diff_lines<'a>(lines: &'a [DiffLine], block: &DiffBlock) -> Vec<&'a DiffLine> {
    lines
        .iter()
        .filter(|line| match line.kind {
            DiffLineKind::LeftOnly => block.left_start.is_some_and(|s| {
                line.left_line
                    .is_some_and(|n| n >= s && n < s + block.left_len)
            }),
            DiffLineKind::RightOnly => block.right_start.is_some_and(|s| {
                line.right_line
                    .is_some_and(|n| n >= s && n < s + block.right_len)
            }),
            DiffLineKind::Changed => {
                // Changed lines are mixed; include if left or right side falls in range.
                let left_in = block.left_start.is_some_and(|s| {
                    line.left_line
                        .is_some_and(|n| n >= s && n < s + block.left_len)
                });
                let right_in = block.right_start.is_some_and(|s| {
                    line.right_line
                        .is_some_and(|n| n >= s && n < s + block.right_len)
                });
                left_in || right_in
            }
            DiffLineKind::Equal => false,
        })
        .collect()
}

fn compare_summary(lines: &[DiffLine], blocks: &[DiffBlock]) -> CompareSummary {
    let mut equal_lines = 0;
    let mut changed_lines = 0;
    let mut left_only_lines = 0;
    let mut right_only_lines = 0;

    for line in lines {
        match line.kind {
            DiffLineKind::Equal => equal_lines += 1,
            DiffLineKind::Changed => changed_lines += 1,
            DiffLineKind::LeftOnly => left_only_lines += 1,
            DiffLineKind::RightOnly => right_only_lines += 1,
        }
    }

    let diff_blocks = blocks
        .iter()
        .filter(|block| {
            matches!(
                block.kind,
                DiffBlockKind::Difference | DiffBlockKind::Moved { .. }
            )
        })
        .count();
    let differences = changed_lines + left_only_lines + right_only_lines;

    CompareSummary {
        equal: differences == 0,
        differences,
        equal_lines,
        changed_lines,
        left_only_lines,
        right_only_lines,
        diff_blocks,
    }
}

fn unified_diff(result: &TextCompareResult, context: usize) -> String {
    let mut output = String::new();
    output.push_str(&format!("--- {}\n", result.left_name));
    output.push_str(&format!("+++ {}\n", result.right_name));

    for range in diff_hunks(&result.lines, context) {
        let stats = range_stats(&result.lines, range.start, range.end);
        output.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            stats.left_start, stats.left_len, stats.right_start, stats.right_len
        ));

        for line in &result.lines[range.start..range.end] {
            match line.kind {
                DiffLineKind::Equal => {
                    output.push(' ');
                    output.push_str(line.left.as_deref().unwrap_or(""));
                    output.push('\n');
                }
                DiffLineKind::Changed => {
                    output.push('-');
                    output.push_str(line.left.as_deref().unwrap_or(""));
                    output.push('\n');
                    output.push('+');
                    output.push_str(line.right.as_deref().unwrap_or(""));
                    output.push('\n');
                }
                DiffLineKind::LeftOnly => {
                    output.push('-');
                    output.push_str(line.left.as_deref().unwrap_or(""));
                    output.push('\n');
                }
                DiffLineKind::RightOnly => {
                    output.push('+');
                    output.push_str(line.right.as_deref().unwrap_or(""));
                    output.push('\n');
                }
            }
        }
    }

    output
}

fn context_diff(result: &TextCompareResult, context: usize) -> String {
    let mut output = String::new();
    output.push_str(&format!("*** {}\n", result.left_name));
    output.push_str(&format!("--- {}\n", result.right_name));

    for range in diff_hunks(&result.lines, context) {
        let stats = range_stats(&result.lines, range.start, range.end);
        output.push_str("***************\n");
        output.push_str(&format!(
            "*** {} ****\n",
            context_range(stats.left_start, stats.left_len)
        ));
        for line in &result.lines[range.start..range.end] {
            match line.kind {
                DiffLineKind::Equal => {
                    output.push_str("  ");
                    output.push_str(line.left.as_deref().unwrap_or(""));
                    output.push('\n');
                }
                DiffLineKind::Changed => {
                    output.push_str("! ");
                    output.push_str(line.left.as_deref().unwrap_or(""));
                    output.push('\n');
                }
                DiffLineKind::LeftOnly => {
                    output.push_str("- ");
                    output.push_str(line.left.as_deref().unwrap_or(""));
                    output.push('\n');
                }
                DiffLineKind::RightOnly => {}
            }
        }
        output.push_str(&format!(
            "--- {} ----\n",
            context_range(stats.right_start, stats.right_len)
        ));
        for line in &result.lines[range.start..range.end] {
            match line.kind {
                DiffLineKind::Equal => {
                    output.push_str("  ");
                    output.push_str(line.right.as_deref().unwrap_or(""));
                    output.push('\n');
                }
                DiffLineKind::Changed => {
                    output.push_str("! ");
                    output.push_str(line.right.as_deref().unwrap_or(""));
                    output.push('\n');
                }
                DiffLineKind::RightOnly => {
                    output.push_str("+ ");
                    output.push_str(line.right.as_deref().unwrap_or(""));
                    output.push('\n');
                }
                DiffLineKind::LeftOnly => {}
            }
        }
    }

    output
}

fn normal_diff(result: &TextCompareResult) -> String {
    let mut output = String::new();
    let mut start = 0;

    while start < result.lines.len() {
        while start < result.lines.len() && result.lines[start].kind == DiffLineKind::Equal {
            start += 1;
        }
        if start >= result.lines.len() {
            break;
        }

        let mut end = start + 1;
        while end < result.lines.len() && result.lines[end].kind != DiffLineKind::Equal {
            end += 1;
        }

        let stats = range_stats(&result.lines, start, end);
        let left_lines = old_lines(&result.lines[start..end]);
        let right_lines = new_lines(&result.lines[start..end]);
        if stats.left_len == 0 {
            output.push_str(&format!(
                "{}a{}\n",
                stats.left_start,
                normal_range(stats.right_start, stats.right_len)
            ));
        } else if stats.right_len == 0 {
            output.push_str(&format!(
                "{}d{}\n",
                normal_range(stats.left_start, stats.left_len),
                stats.right_start
            ));
        } else {
            output.push_str(&format!(
                "{}c{}\n",
                normal_range(stats.left_start, stats.left_len),
                normal_range(stats.right_start, stats.right_len)
            ));
        }

        for line in &left_lines {
            output.push_str("< ");
            output.push_str(line);
            output.push('\n');
        }
        if !left_lines.is_empty() && !right_lines.is_empty() {
            output.push_str("---\n");
        }
        for line in &right_lines {
            output.push_str("> ");
            output.push_str(line);
            output.push('\n');
        }

        start = end;
    }

    output
}

#[derive(Debug, Clone, Copy)]
struct DiffHunkRange {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy)]
struct DiffRangeStats {
    left_start: usize,
    left_len: usize,
    right_start: usize,
    right_len: usize,
}

fn diff_hunks(lines: &[DiffLine], context: usize) -> Vec<DiffHunkRange> {
    let mut ranges: Vec<DiffHunkRange> = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        if line.kind == DiffLineKind::Equal {
            continue;
        }

        let start = index.saturating_sub(context);
        let end = (index + context + 1).min(lines.len());
        match ranges.last_mut() {
            Some(range) if start <= range.end => range.end = range.end.max(end),
            _ => ranges.push(DiffHunkRange { start, end }),
        }
    }

    ranges
}

fn range_stats(lines: &[DiffLine], start: usize, end: usize) -> DiffRangeStats {
    let left_len = lines[start..end].iter().map(left_line_count).sum();
    let right_len = lines[start..end].iter().map(right_line_count).sum();
    let left_start = if left_len == 0 {
        lines[..start].iter().map(left_line_count).sum()
    } else {
        lines[start..end]
            .iter()
            .find_map(|line| line.left_line)
            .unwrap_or(1)
    };
    let right_start = if right_len == 0 {
        lines[..start].iter().map(right_line_count).sum()
    } else {
        lines[start..end]
            .iter()
            .find_map(|line| line.right_line)
            .unwrap_or(1)
    };

    DiffRangeStats {
        left_start,
        left_len,
        right_start,
        right_len,
    }
}

fn left_line_count(line: &DiffLine) -> usize {
    match line.kind {
        DiffLineKind::Equal | DiffLineKind::Changed | DiffLineKind::LeftOnly => 1,
        DiffLineKind::RightOnly => 0,
    }
}

fn right_line_count(line: &DiffLine) -> usize {
    match line.kind {
        DiffLineKind::Equal | DiffLineKind::Changed | DiffLineKind::RightOnly => 1,
        DiffLineKind::LeftOnly => 0,
    }
}

fn old_lines(lines: &[DiffLine]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| match line.kind {
            DiffLineKind::Changed | DiffLineKind::LeftOnly => line.left.clone(),
            DiffLineKind::Equal | DiffLineKind::RightOnly => None,
        })
        .collect()
}

fn new_lines(lines: &[DiffLine]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| match line.kind {
            DiffLineKind::Changed | DiffLineKind::RightOnly => line.right.clone(),
            DiffLineKind::Equal | DiffLineKind::LeftOnly => None,
        })
        .collect()
}

fn context_range(start: usize, len: usize) -> String {
    match len {
        0 => format!("{start},0"),
        1 => start.to_string(),
        _ => format!("{start},{}", start + len - 1),
    }
}

fn normal_range(start: usize, len: usize) -> String {
    if len <= 1 {
        start.to_string()
    } else {
        format!("{start},{}", start + len - 1)
    }
}

fn html_report(result: &TextCompareResult, context: Option<usize>) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">\n");
    output.push_str("<title>LinSync Compare Report</title>\n");
    output.push_str(
        "<style>body{font-family:sans-serif}table{border-collapse:collapse;width:100%}\
td,th{border:1px solid #bbb;padding:0.25rem 0.4rem;font-family:monospace;white-space:pre-wrap}\
.eq{background:#fff}.chg{background:#fff4c2}.del{background:#ffd9d9}.add{background:#daf5d7}</style>\n",
    );
    output.push_str("</head><body>\n");
    output.push_str(&format!(
        "<h1>{} vs {}</h1>\n<p>{} differing lines in {} blocks.</p>\n",
        escape_html(&result.left_name),
        escape_html(&result.right_name),
        result.summary.differences,
        result.summary.diff_blocks
    ));
    output.push_str("<table><thead><tr><th>Left</th><th>Right</th><th>Left Text</th><th>Right Text</th></tr></thead><tbody>\n");
    for (index, line) in result.lines.iter().enumerate() {
        if let Some(context) = context
            && line.kind == DiffLineKind::Equal
            && !line_is_within_context(&result.lines, index, context)
        {
            continue;
        }
        let class = match line.kind {
            DiffLineKind::Equal => "eq",
            DiffLineKind::Changed => "chg",
            DiffLineKind::LeftOnly => "del",
            DiffLineKind::RightOnly => "add",
        };
        output.push_str(&format!(
            "<tr class=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            class,
            line.left_line
                .map(|number| number.to_string())
                .unwrap_or_default(),
            line.right_line
                .map(|number| number.to_string())
                .unwrap_or_default(),
            escape_html(line.left.as_deref().unwrap_or("")),
            escape_html(line.right.as_deref().unwrap_or(""))
        ));
    }
    output.push_str("</tbody></table>\n</body></html>\n");
    output
}

fn line_is_within_context(lines: &[DiffLine], index: usize, context: usize) -> bool {
    let start = index.saturating_sub(context);
    let end = (index + context + 1).min(lines.len());
    lines[start..end]
        .iter()
        .any(|line| line.kind != DiffLineKind::Equal)
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_documents_cancellable_aborts_when_flagged() {
        let opts = TextCompareOptions::default();
        // An always-true cancel flag aborts and returns None (no partial diff).
        let left = TextDocument::from_text("l", "a\nb\nc\n");
        let right = TextDocument::from_text("r", "a\nx\nc\n");
        assert!(
            compare_documents_cancellable(left, right, &opts, &|| true).is_none(),
            "always-cancel must return None"
        );
        // A never-cancel flag yields a normal result.
        let left = TextDocument::from_text("l", "a\nb\nc\n");
        let right = TextDocument::from_text("r", "a\nx\nc\n");
        let got = compare_documents_cancellable(left, right, &opts, &|| false);
        assert!(got.is_some(), "never-cancel must yield a result");
        assert!(!got.unwrap().lines.is_empty());
    }

    #[test]
    fn compares_equal_text() {
        let result = compare_text(
            "left",
            "alpha\nbeta\n",
            "right",
            "alpha\nbeta\n",
            &TextCompareOptions::default(),
        );

        assert!(result.is_equal());
        assert_eq!(result.difference_count(), 0);
        assert_eq!(result.summary.equal_lines, 2);
    }

    #[test]
    fn reports_insertions_and_deletions() {
        let result = compare_text(
            "left",
            "alpha\nbeta\n",
            "right",
            "alpha\ngamma\nbeta\n",
            &TextCompareOptions::default(),
        );

        assert!(!result.is_equal());
        assert_eq!(result.difference_count(), 1);
        assert_eq!(result.lines[1].kind, DiffLineKind::RightOnly);
        assert_eq!(result.summary.diff_blocks, 1);
    }

    #[test]
    fn reports_changed_lines_with_inline_spans() {
        let result = compare_text(
            "left",
            "alpha\nbeta\n",
            "right",
            "alpha\nbetamax\n",
            &TextCompareOptions::default(),
        );

        assert_eq!(result.difference_count(), 1);
        assert_eq!(result.lines[1].kind, DiffLineKind::Changed);
        assert_eq!(
            result.lines[1].inline,
            vec![InlineDiff {
                left_start: 4,
                left_end: 4,
                right_start: 4,
                right_end: 7,
            }]
        );
    }

    #[test]
    fn supports_case_and_whitespace_ignore() {
        let result = compare_text(
            "left",
            "Alpha   beta\n",
            "right",
            "alpha beta\n",
            &TextCompareOptions {
                ignore_case: true,
                ignore_whitespace: true,
                ..TextCompareOptions::default()
            },
        );

        assert!(result.is_equal());
    }

    #[test]
    fn supports_blank_line_eol_and_regex_line_ignore() {
        let result = compare_text(
            "left",
            "alpha\r\nGenerated: 123\r\n\r\nomega\r\n",
            "right",
            "alpha\nGenerated: 456\nomega\n",
            &TextCompareOptions {
                ignore_eol: true,
                ignore_blank_lines: true,
                ignore_line_patterns: vec![r"^Generated: \d+$".to_owned()],
                ..TextCompareOptions::default()
            },
        );

        assert!(result.is_equal());
    }

    #[test]
    fn substitution_filters_compare_normalized_text_but_preserve_display() {
        let result = compare_text(
            "left",
            "id=123 path=/tmp/left\n",
            "right",
            "id=999 path=/tmp/right\n",
            &TextCompareOptions {
                substitutions: vec![
                    TextSubstitution {
                        pattern: r"id=\d+".to_owned(),
                        replacement: "id=<id>".to_owned(),
                    },
                    TextSubstitution {
                        pattern: r"path=/tmp/\w+".to_owned(),
                        replacement: "path=<path>".to_owned(),
                    },
                ],
                ..TextCompareOptions::default()
            },
        );

        assert!(result.is_equal());
        assert_eq!(
            result.lines[0].left.as_deref(),
            Some("id=123 path=/tmp/left")
        );
        assert_eq!(
            result.lines[0].right.as_deref(),
            Some("id=999 path=/tmp/right")
        );
    }

    #[test]
    fn substitution_filters_run_before_blank_line_filtering() {
        let result = compare_text(
            "left",
            "Generated: 123\nstable\n",
            "right",
            "stable\n",
            &TextCompareOptions {
                ignore_blank_lines: true,
                substitutions: vec![TextSubstitution {
                    pattern: r"^Generated: \d+$".to_owned(),
                    replacement: String::new(),
                }],
                ..TextCompareOptions::default()
            },
        );

        assert!(result.is_equal());
    }

    #[test]
    fn validates_regex_line_ignore_patterns() {
        let valid = TextCompareOptions {
            ignore_line_patterns: vec![r"^Generated: \d+$".to_owned()],
            substitutions: vec![TextSubstitution {
                pattern: r"id=\d+".to_owned(),
                replacement: "id=<id>".to_owned(),
            }],
            ..TextCompareOptions::default()
        };
        let invalid = TextCompareOptions {
            substitutions: vec![TextSubstitution {
                pattern: "[unterminated".to_owned(),
                replacement: String::new(),
            }],
            ..TextCompareOptions::default()
        };

        assert!(valid.validate_regex_options().is_ok());
        assert!(invalid.validate_regex_options().is_err());
    }

    #[test]
    fn detects_line_endings_and_no_newline_at_eof() {
        let document = TextDocument::from_text("left", "alpha\r\nbeta");
        assert_eq!(document.line_ending, LineEnding::Crlf);
        assert_eq!(document.lines.len(), 2);
        assert_eq!(document.lines[1].newline, None);
    }

    #[test]
    fn decodes_utf8_bom() {
        let document = TextDocument::from_bytes(
            "bom".to_owned(),
            None,
            &[0xEF, 0xBB, 0xBF, b'a', b'\n'],
            false,
        );
        assert_eq!(document.encoding, TextEncoding::Utf8Bom);
        assert!(document.has_bom);
        assert_eq!(document.lines[0].text, "a");
    }

    #[test]
    fn creates_unified_diff() {
        let result = compare_text(
            "left",
            "same before\nalpha\nbeta\nshared\nsame after\n",
            "right",
            "same before\nalpha\ngamma\nshared\nsame after\n",
            &TextCompareOptions::default(),
        );

        let patch = result.to_unified_diff(1);
        assert!(patch.contains("--- left"));
        assert!(patch.contains("+++ right"));
        assert!(patch.contains("@@ -2,3 +2,3 @@"));
        assert!(patch.contains("-beta"));
        assert!(patch.contains("+gamma"));
        assert!(!patch.contains("same before"));
        assert!(!patch.contains("same after"));
    }

    #[test]
    fn creates_context_and_normal_diff_formats() {
        let result = compare_text(
            "left",
            "same before\nalpha\nbeta\nshared\nsame after\n",
            "right",
            "same before\nalpha\ngamma\nshared\nsame after\n",
            &TextCompareOptions::default(),
        );

        let context = result.to_context_diff(1);
        assert!(context.contains("*** left"));
        assert!(context.contains("--- right"));
        assert!(context.contains("*** 2,4 ****"));
        assert!(context.contains("! beta"));
        assert!(context.contains("! gamma"));
        assert!(!context.contains("same before"));
        assert!(!context.contains("same after"));

        let normal = result.to_normal_diff();
        assert_eq!(normal, "3c3\n< beta\n---\n> gamma\n");
    }

    #[test]
    fn html_report_can_limit_equal_context() {
        let result = compare_text(
            "left",
            "same before\nfar before\nleft\nsame after\nfar after\n",
            "right",
            "same before\nfar before\nright\nsame after\nfar after\n",
            &TextCompareOptions::default(),
        );

        let html = result.to_html_report_with_context(Some(1));

        assert!(html.contains("far before"));
        assert!(html.contains("same after"));
        assert!(html.contains("left"));
        assert!(html.contains("right"));
        assert!(!html.contains("same before"));
        assert!(!html.contains("far after"));
    }

    #[test]
    fn detects_moved_block() {
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
        let moves: Vec<_> = result
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, DiffBlockKind::Moved { .. }))
            .collect();
        assert_eq!(
            moves.len(),
            2,
            "expected two moved blocks (the two sections)"
        );
    }

    #[test]
    fn detect_moves_disabled_by_default() {
        let left = "section A\nsection B\n";
        let right = "section B\nsection A\n";
        let opts = TextCompareOptions::default(); // detect_moves false
        let result = compare_documents(
            TextDocument::from_text("left", left),
            TextDocument::from_text("right", right),
            &opts,
        );
        assert!(
            result
                .blocks
                .iter()
                .all(|b| !matches!(b.kind, DiffBlockKind::Moved { .. }))
        );
    }

    #[test]
    fn moved_blocks_require_minimum_lines() {
        // Single-line moves shouldn't be detected as moves — too noisy
        let left = "alpha\nbravo\n";
        let right = "bravo\nalpha\n";
        let opts = TextCompareOptions {
            detect_moves: true,
            min_move_lines: 2,
            ..TextCompareOptions::default()
        };
        let result = compare_documents(
            TextDocument::from_text("left", left),
            TextDocument::from_text("right", right),
            &opts,
        );
        // With min_move_lines=2 and only single-line "moves", nothing should be flagged as Moved
        assert!(
            result
                .blocks
                .iter()
                .all(|b| !matches!(b.kind, DiffBlockKind::Moved { .. }))
        );
    }
}
