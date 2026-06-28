use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use regex::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

pub use crate::syntax::{SyntaxSpan, TextSyntaxMode};
use crate::syntax::{escape_html, syntax_highlight_html, syntax_mode_from_path, syntax_spans};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareSide {
    Left,
    Base,
    Right,
    Result,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffAlgorithm {
    #[default]
    Lcs,
    Patience,
    Myers,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InlineGranularity {
    #[default]
    Char,
    Word,
    Grapheme,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextInputEncoding {
    #[default]
    Auto,
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    LossyUtf8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextRenderMode {
    #[default]
    SideBySide,
    Unified,
    Context,
    Normal,
    Html,
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
    pub regex_rule_sets: Vec<String>,
    pub ignore_line_patterns: Vec<String>,
    pub substitutions: Vec<TextSubstitution>,
    pub detect_moves: bool,
    #[serde(default = "default_min_move_lines")]
    pub min_move_lines: usize,
    #[serde(default)]
    pub diff_algorithm: DiffAlgorithm,
    #[serde(default)]
    pub inline_granularity: InlineGranularity,
    #[serde(default)]
    pub encoding: TextInputEncoding,
    #[serde(default)]
    pub render_mode: TextRenderMode,
    #[serde(default)]
    pub syntax_mode: TextSyntaxMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_lines: Option<usize>,
    #[serde(default)]
    pub show_only_changes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub find: Option<TextFindOptions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bookmarks: Vec<TextBookmark>,
    /// Ids of prediffer plugins to apply before diffing, when this options set
    /// comes from a profile. Clients resolve each id to an enabled, installed
    /// prediffer (see `linsync_core::resolve_enabled_prediffer`) and run it to
    /// normalize each side. Empty = no prediffer routing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prediffer_plugins: Vec<String>,
    /// How overlapping prediffers in [`prediffer_plugins`] are resolved when the
    /// chain runs (see [`crate::plugin::PredifferConflictPolicy`]). Defaults to
    /// `Chain` (run all, today's behavior), so existing profiles are unchanged.
    ///
    /// [`prediffer_plugins`]: Self::prediffer_plugins
    #[serde(default)]
    pub prediffer_conflict_policy: crate::plugin::PredifferConflictPolicy,
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
            regex_rule_sets: Vec::new(),
            ignore_line_patterns: Vec::new(),
            substitutions: Vec::new(),
            detect_moves: false,
            min_move_lines: default_min_move_lines(),
            diff_algorithm: DiffAlgorithm::default(),
            inline_granularity: InlineGranularity::default(),
            encoding: TextInputEncoding::default(),
            render_mode: TextRenderMode::default(),
            syntax_mode: TextSyntaxMode::default(),
            context_lines: None,
            show_only_changes: false,
            find: None,
            bookmarks: Vec::new(),
            prediffer_plugins: Vec::new(),
            prediffer_conflict_policy: crate::plugin::PredifferConflictPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSubstitution {
    pub pattern: String,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRegexRuleSet {
    pub id: String,
    pub name: String,
    pub description: String,
    pub ignore_line_patterns: Vec<String>,
    pub substitutions: Vec<TextSubstitution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFindOptions {
    pub pattern: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFindMatch {
    pub side: CompareSide,
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextBookmark {
    pub side: CompareSide,
    pub line: usize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// A page of diff view rows produced by
/// [`TextCompareResult::view_rows_window`]: the rows in the requested window,
/// the clamped `offset` actually used, and the total row count for pagination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextViewPage {
    pub total_rows: usize,
    pub offset: usize,
    pub rows: Vec<TextViewRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextViewRow {
    pub index: usize,
    pub left_line: Option<usize>,
    pub right_line: Option<usize>,
    pub left: String,
    pub right: String,
    pub state: String,
    pub block_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folded_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left_syntax: Vec<SyntaxSpan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub right_syntax: Vec<SyntaxSpan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub find_matches: Vec<TextFindMatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bookmarks: Vec<TextBookmark>,
}

pub fn builtin_text_regex_rule_sets() -> Vec<TextRegexRuleSet> {
    vec![
        TextRegexRuleSet {
            id: "generated".to_owned(),
            name: "Generated headers".to_owned(),
            description: "Ignore common generated-file banner lines.".to_owned(),
            ignore_line_patterns: vec![
                r"(?i)^\s*(//|#|;|--)?\s*(generated|auto-generated|autogenerated)\b.*$"
                    .to_owned(),
                r"(?i)^\s*(//|#|;|--)?\s*do not edit\b.*$".to_owned(),
            ],
            substitutions: Vec::new(),
        },
        TextRegexRuleSet {
            id: "volatile".to_owned(),
            name: "Volatile values".to_owned(),
            description: "Normalize UUIDs, ISO timestamps, and hex addresses.".to_owned(),
            ignore_line_patterns: Vec::new(),
            substitutions: vec![
                TextSubstitution {
                    pattern:
                        r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b"
                            .to_owned(),
                    replacement: "<uuid>".to_owned(),
                },
                TextSubstitution {
                    pattern: r"\b\d{4}-\d{2}-\d{2}[T ][0-9:.+-Zz]+\b".to_owned(),
                    replacement: "<timestamp>".to_owned(),
                },
                TextSubstitution {
                    pattern: r"\b0x[0-9a-fA-F]+\b".to_owned(),
                    replacement: "<hex>".to_owned(),
                },
            ],
        },
        TextRegexRuleSet {
            id: "comments".to_owned(),
            name: "Comment-only lines".to_owned(),
            description: "Ignore lines that only contain a line comment.".to_owned(),
            ignore_line_patterns: vec![r"^\s*(//|#|;|--).*$".to_owned()],
            substitutions: Vec::new(),
        },
        TextRegexRuleSet {
            id: "whitespace".to_owned(),
            name: "Whitespace noise".to_owned(),
            description: "Normalize repeated horizontal whitespace.".to_owned(),
            ignore_line_patterns: Vec::new(),
            substitutions: vec![TextSubstitution {
                pattern: r"[ \t]+".to_owned(),
                replacement: " ".to_owned(),
            }],
        },
    ]
}

pub fn text_regex_rule_set(id: &str) -> Option<TextRegexRuleSet> {
    builtin_text_regex_rule_sets()
        .into_iter()
        .find(|rule_set| rule_set.id == id)
}

impl TextCompareOptions {
    pub fn validate_regex_options(&self) -> Result<(), regex::Error> {
        let mut ignore_patterns = self.ignore_line_patterns.clone();
        let mut substitutions = self.substitutions.clone();
        for id in &self.regex_rule_sets {
            if let Some(rule_set) = text_regex_rule_set(id) {
                ignore_patterns.extend(rule_set.ignore_line_patterns);
                substitutions.extend(rule_set.substitutions);
            };
        }

        RegexSetBuilder::new(&ignore_patterns)
            .case_insensitive(self.ignore_case)
            .build()
            .map(|_| ())?;

        for substitution in &substitutions {
            Regex::new(&substitution.pattern)?;
        }

        if let Some(find) = &self.find
            && find.regex
        {
            Regex::new(&find.pattern)?;
        }

        Ok(())
    }

    pub fn validate_rule_sets(&self) -> Result<(), String> {
        for id in &self.regex_rule_sets {
            if text_regex_rule_set(id).is_none() {
                let known = builtin_text_regex_rule_sets()
                    .into_iter()
                    .map(|rule_set| rule_set.id)
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "unknown text regex rule set '{id}'; expected one of: {known}"
                ));
            }
        }
        Ok(())
    }

    pub fn validate_ignore_line_patterns(&self) -> Result<(), regex::Error> {
        self.validate_regex_options()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDocument {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
    pub has_bom: bool,
    #[serde(default)]
    pub had_replacement_characters: bool,
    pub read_only: bool,
    pub byte_len: usize,
    pub lines: Vec<TextLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    LossyUtf8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEnding {
    None,
    Lf,
    Crlf,
    Cr,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextLine {
    pub number: usize,
    #[serde(skip)]
    pub byte_start: usize,
    #[serde(skip)]
    pub byte_end: usize,
    pub text: String,
    pub newline: Option<LineEnding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextCompareResult {
    pub left_name: String,
    pub right_name: String,
    pub left_document: TextDocument,
    pub right_document: TextDocument,
    pub lines: Vec<DiffLine>,
    pub blocks: Vec<DiffBlock>,
    pub summary: CompareSummary,
    /// Sandbox confinement that applied when a plugin (e.g. a prediffer chain)
    /// participated in producing this result. `None` for a pure built-in
    /// comparison where no helper process ran. Omitted from JSON when `None`,
    /// so existing result round-trips are unaffected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<crate::plugin::SandboxStatus>,
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
        html_report(self, None, TextSyntaxMode::Plain)
    }

    pub fn to_html_report_with_context(&self, context: Option<usize>) -> String {
        html_report(self, context, TextSyntaxMode::Plain)
    }

    pub fn encoding_summary(&self) -> EncodingSummary {
        EncodingSummary {
            left_encoding: self.left_document.encoding,
            right_encoding: self.right_document.encoding,
            left_line_ending: self.left_document.line_ending,
            right_line_ending: self.right_document.line_ending,
            left_has_bom: self.left_document.has_bom,
            right_has_bom: self.right_document.has_bom,
            encoding_differs: self.left_document.encoding != self.right_document.encoding,
            line_ending_differs: self.left_document.line_ending != self.right_document.line_ending,
            bom_differs: self.left_document.has_bom != self.right_document.has_bom,
        }
    }

    pub fn render_text(&self, options: &TextCompareOptions) -> String {
        let context = options.context_lines.unwrap_or(3);
        match options.render_mode {
            TextRenderMode::SideBySide => side_by_side_text(self, options),
            TextRenderMode::Unified => self.to_unified_diff(context),
            TextRenderMode::Context => self.to_context_diff(context),
            TextRenderMode::Normal => self.to_normal_diff(),
            TextRenderMode::Html => {
                self.to_html_report_with_options(options.context_lines, options.syntax_mode)
            }
        }
    }

    pub fn to_html_report_with_options(
        &self,
        context: Option<usize>,
        syntax_mode: TextSyntaxMode,
    ) -> String {
        html_report(self, context, syntax_mode)
    }

    pub fn view_rows(&self, options: &TextCompareOptions) -> Vec<TextViewRow> {
        let visible = visible_line_ranges(&self.lines, options);
        let find_matches = options
            .find
            .as_ref()
            .and_then(|find| self.find_matches(find).ok())
            .unwrap_or_default();
        let syntax_mode = resolved_syntax_mode(
            options.syntax_mode,
            self.left_document.path.as_deref(),
            self.right_document.path.as_deref(),
        );
        let mut rows = Vec::new();
        let mut previous_end = 0;

        for range in visible {
            if range.start > previous_end && !options.show_only_changes {
                rows.push(fold_row(rows.len(), range.start - previous_end));
            }
            for (offset, line) in self.lines[range.start..range.end].iter().enumerate() {
                let source_index = range.start + offset;
                rows.push(view_row_for_line(
                    rows.len(),
                    source_index,
                    line,
                    self,
                    syntax_mode,
                    &find_matches,
                    &options.bookmarks,
                ));
            }
            previous_end = range.end;
        }

        if previous_end < self.lines.len() && !options.show_only_changes {
            rows.push(fold_row(rows.len(), self.lines.len() - previous_end));
        }

        if rows.is_empty() && self.lines.is_empty() {
            return rows;
        }

        rows
    }

    /// Like [`view_rows`](Self::view_rows) but materializes only the rows in the
    /// window `[offset, offset + limit)`, reporting the total row count so a UI
    /// (or any client) can paginate a large diff without building every row.
    ///
    /// The expensive per-row work (syntax highlighting, find-match marking) runs
    /// only for the windowed rows; rows outside it are enumerated cheaply for
    /// counting and indexing. The returned rows are byte-for-byte identical to
    /// the matching slice of `view_rows`, including each row's display `index`.
    pub fn view_rows_window(
        &self,
        options: &TextCompareOptions,
        offset: usize,
        limit: usize,
    ) -> TextViewPage {
        // A cheap structural slot for each display row: either a folded gap of
        // `n` equal lines, or a real diff line at `source_index`.
        enum Slot {
            Fold(usize),
            Line(usize),
        }

        let visible = visible_line_ranges(&self.lines, options);
        let mut slots: Vec<Slot> = Vec::new();
        let mut previous_end = 0;
        for range in visible {
            if range.start > previous_end && !options.show_only_changes {
                slots.push(Slot::Fold(range.start - previous_end));
            }
            for source_index in range.start..range.end {
                slots.push(Slot::Line(source_index));
            }
            previous_end = range.end;
        }
        if previous_end < self.lines.len() && !options.show_only_changes {
            slots.push(Slot::Fold(self.lines.len() - previous_end));
        }

        let total_rows = slots.len();
        let start = offset.min(total_rows);
        let end = start.saturating_add(limit).min(total_rows);

        if start >= end {
            return TextViewPage {
                total_rows,
                offset: start,
                rows: Vec::new(),
            };
        }

        // Find matches and syntax mode are needed identically to `view_rows`,
        // but only when the window actually yields rows.
        let find_matches = options
            .find
            .as_ref()
            .and_then(|find| self.find_matches(find).ok())
            .unwrap_or_default();
        let syntax_mode = resolved_syntax_mode(
            options.syntax_mode,
            self.left_document.path.as_deref(),
            self.right_document.path.as_deref(),
        );

        let mut rows = Vec::with_capacity(end - start);
        for (display_index, slot) in slots.iter().enumerate().take(end).skip(start) {
            match slot {
                Slot::Fold(count) => rows.push(fold_row(display_index, *count)),
                Slot::Line(source_index) => rows.push(view_row_for_line(
                    display_index,
                    *source_index,
                    &self.lines[*source_index],
                    self,
                    syntax_mode,
                    &find_matches,
                    &options.bookmarks,
                )),
            }
        }

        TextViewPage {
            total_rows,
            offset: start,
            rows,
        }
    }

    pub fn find_matches(&self, find: &TextFindOptions) -> Result<Vec<TextFindMatch>, regex::Error> {
        if find.pattern.is_empty() {
            return Ok(Vec::new());
        }

        let pattern = if find.regex {
            find.pattern.clone()
        } else {
            regex::escape(&find.pattern)
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!find.case_sensitive)
            .build()?;

        let mut matches = Vec::new();
        for line in &self.lines {
            if let (Some(number), Some(text)) = (line.left_line, line.left.as_deref()) {
                collect_find_matches(CompareSide::Left, number, text, &regex, &mut matches);
            }
            if let (Some(number), Some(text)) = (line.right_line, line.right.as_deref()) {
                collect_find_matches(CompareSide::Right, number, text, &regex, &mut matches);
            }
        }
        Ok(matches)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodingSummary {
    pub left_encoding: TextEncoding,
    pub right_encoding: TextEncoding,
    pub left_line_ending: LineEnding,
    pub right_line_ending: LineEnding,
    pub left_has_bom: bool,
    pub right_has_bom: bool,
    pub encoding_differs: bool,
    pub line_ending_differs: bool,
    pub bom_differs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompareSummary {
    pub equal: bool,
    pub differences: usize,
    pub equal_lines: usize,
    pub changed_lines: usize,
    pub left_only_lines: usize,
    pub right_only_lines: usize,
    pub diff_blocks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffBlock {
    pub kind: DiffBlockKind,
    pub left_start: Option<usize>,
    pub right_start: Option<usize>,
    pub left_len: usize,
    pub right_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffBlockKind {
    Equal,
    Difference,
    Moved {
        partner_block: usize,
        direction: MoveDirection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveDirection {
    /// Content moved from left to right (deleted on left, added on right).
    LeftToRight,
    /// Content moved from right to left (added on left, deleted on right).
    RightToLeft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub left_line: Option<usize>,
    pub right_line: Option<usize>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub inline: Vec<InlineDiff>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Equal,
    Changed,
    LeftOnly,
    RightOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    let left_document = TextDocument::from_path_with_encoding(left, options.encoding)?;
    let right_document = TextDocument::from_path_with_encoding(right, options.encoding)?;
    Ok(compare_documents(left_document, right_document, options))
}

/// Apply a prediffer plugin to both sides before comparing.
///
/// If `prediffer_plugin_dir` is `Some` and the plugin is a valid prediffer,
/// each side's file is preprocessed through the plugin before comparison.
/// If the plugin fails or returns empty output, the original file content is used.
pub fn compare_text_files_with_prediffer(
    left: &Path,
    right: &Path,
    options: &TextCompareOptions,
    prediffer_plugin_dir: Option<&Path>,
    prediffer_manifest: Option<&crate::plugin::PluginManifest>,
    execution_options: &crate::plugin::PluginExecutionOptions,
) -> io::Result<TextCompareResult> {
    let left_document = match apply_prediffer_to_side(
        left,
        "left",
        prediffer_plugin_dir,
        prediffer_manifest,
        execution_options,
    ) {
        Some(doc) => doc,
        None => TextDocument::from_path_with_encoding(left, options.encoding)?,
    };
    let right_document = match apply_prediffer_to_side(
        right,
        "right",
        prediffer_plugin_dir,
        prediffer_manifest,
        execution_options,
    ) {
        Some(doc) => doc,
        None => TextDocument::from_path_with_encoding(right, options.encoding)?,
    };
    Ok(compare_documents(left_document, right_document, options))
}

/// Compare two text files after running an ordered **chain** of prediffers over
/// each side (each stage's output feeds the next). When the chain is empty or a
/// side's chain falls back (a stage failed or produced empty text), that side
/// is read from disk unchanged. See [`crate::plugin::run_prediffer_chain`].
pub fn compare_text_files_with_prediffer_chain(
    left: &Path,
    right: &Path,
    options: &TextCompareOptions,
    prediffers: &[crate::plugin::DiscoveredPlugin],
    execution_options: &crate::plugin::PluginExecutionOptions,
) -> io::Result<TextCompareResult> {
    // Drop overlapping prediffers per the configured conflict policy before
    // running the chain. `Chain` (the default) keeps every stage, so this is a
    // no-op for existing configurations.
    let resolved =
        crate::plugin::resolve_prediffer_conflicts(prediffers, options.prediffer_conflict_policy);
    let chain = resolved.as_slice();
    let left_document =
        match crate::plugin::run_prediffer_chain(chain, "left", left, execution_options) {
            Some(text) => TextDocument::from_text(&left.display().to_string(), &text),
            None => TextDocument::from_path_with_encoding(left, options.encoding)?,
        };
    let right_document =
        match crate::plugin::run_prediffer_chain(chain, "right", right, execution_options) {
            Some(text) => TextDocument::from_text(&right.display().to_string(), &text),
            None => TextDocument::from_path_with_encoding(right, options.encoding)?,
        };
    let mut result = compare_documents(left_document, right_document, options);
    // Record the confinement helper processes ran under when a prediffer
    // actually participated, so clients can surface it on the result.
    if !chain.is_empty() {
        result.sandbox = Some(crate::plugin::active_sandbox_status());
    }
    Ok(result)
}

fn apply_prediffer_to_side(
    path: &Path,
    role: &str,
    plugin_dir: Option<&Path>,
    manifest: Option<&crate::plugin::PluginManifest>,
    execution_options: &crate::plugin::PluginExecutionOptions,
) -> Option<TextDocument> {
    let (dir, man) = (plugin_dir?, manifest?);
    let input = crate::plugin::PluginInputDescriptor::for_file(role, path);
    let result = crate::plugin::run_prediffer_plugin(dir, man, input, execution_options).ok()?;
    if result.text.is_empty() {
        return None;
    }
    Some(TextDocument::from_text(
        &path.display().to_string(),
        &result.text,
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
    let n = left_lines.len();
    let m = right_lines.len();
    let raw_lines = match options.diff_algorithm {
        DiffAlgorithm::Lcs => {
            if n > LCS_FULL_TABLE_THRESHOLD || m > LCS_FULL_TABLE_THRESHOLD {
                hirschberg_diff(
                    &left_document,
                    &right_document,
                    &left_lines,
                    &right_lines,
                    should_cancel,
                )?
            } else {
                let lcs = lcs_table_cancellable(&left_lines, &right_lines, should_cancel)?;
                raw_diff_lines(
                    &left_document,
                    &right_document,
                    &left_lines,
                    &right_lines,
                    &lcs,
                )
            }
        }
        // Myers stores a full trace of V vectors — O((n+m)²) memory — which
        // can OOM / hang on large inputs (e.g. 20k+20k lines → ~25 GB). Fall
        // back to linear-space Hirschberg above the threshold, matching the
        // LCS path's guard. The result is still a valid minimal edit script.
        DiffAlgorithm::Myers if n > LCS_FULL_TABLE_THRESHOLD || m > LCS_FULL_TABLE_THRESHOLD => {
            hirschberg_diff(
                &left_document,
                &right_document,
                &left_lines,
                &right_lines,
                should_cancel,
            )?
        }
        DiffAlgorithm::Myers => myers_diff(
            &left_document,
            &right_document,
            &left_lines,
            &right_lines,
            should_cancel,
        )?,
        // Patience recurses into LCS for inter-anchor gaps; the top-level
        // guard keeps individual gaps under the LCS threshold but the
        // anchor-finding scan itself is O(n×m) (see patience_diff). For very
        // large inputs, fall back to Hirschberg to avoid both the scan cost
        // and the memory pressure of many recursive LCS sub-problems.
        DiffAlgorithm::Patience if n > LCS_FULL_TABLE_THRESHOLD || m > LCS_FULL_TABLE_THRESHOLD => {
            hirschberg_diff(
                &left_document,
                &right_document,
                &left_lines,
                &right_lines,
                should_cancel,
            )?
        }
        DiffAlgorithm::Patience => patience_diff(
            &left_document,
            &right_document,
            &left_lines,
            &right_lines,
            should_cancel,
        )?,
    };
    let lines = pair_changed_lines(raw_lines, options.inline_granularity);
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
        sandbox: None,
    })
}

impl TextDocument {
    pub fn from_path(path: &Path) -> io::Result<Self> {
        Self::from_path_with_encoding(path, TextInputEncoding::Auto)
    }

    pub fn from_path_with_encoding(path: &Path, encoding: TextInputEncoding) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        let read_only = fs::metadata(path)
            .map(|metadata| metadata.permissions().readonly())
            .unwrap_or(false);
        Ok(Self::from_bytes_with_encoding(
            path.display().to_string(),
            Some(path.to_path_buf()),
            &bytes,
            read_only,
            encoding,
        ))
    }

    pub fn from_text(name: &str, text: &str) -> Self {
        Self::from_bytes(name.to_owned(), None, text.as_bytes(), false)
    }

    pub fn from_bytes(name: String, path: Option<PathBuf>, bytes: &[u8], read_only: bool) -> Self {
        Self::from_bytes_with_encoding(name, path, bytes, read_only, TextInputEncoding::Auto)
    }

    pub fn from_bytes_with_encoding(
        name: String,
        path: Option<PathBuf>,
        bytes: &[u8],
        read_only: bool,
        encoding: TextInputEncoding,
    ) -> Self {
        let decoded = decode_text_with_encoding(bytes, encoding);
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

fn decode_text_with_encoding(bytes: &[u8], encoding: TextInputEncoding) -> DecodedText {
    match encoding {
        TextInputEncoding::Auto => decode_text(bytes),
        TextInputEncoding::Utf8 => {
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
        TextInputEncoding::Utf8Bom => {
            let has_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
            let body = if has_bom { &bytes[3..] } else { bytes };
            let (text, had_replacement_characters) = decode_utf8_lossy(body);
            DecodedText {
                text,
                encoding: TextEncoding::Utf8Bom,
                has_bom,
                had_replacement_characters,
            }
        }
        TextInputEncoding::Utf16Le => {
            let has_bom = bytes.starts_with(&[0xFF, 0xFE]);
            let body = if has_bom { &bytes[2..] } else { bytes };
            let mut decoded = decode_utf16(body, true);
            decoded.has_bom = has_bom;
            decoded
        }
        TextInputEncoding::Utf16Be => {
            let has_bom = bytes.starts_with(&[0xFE, 0xFF]);
            let body = if has_bom { &bytes[2..] } else { bytes };
            let mut decoded = decode_utf16(body, false);
            decoded.has_bom = has_bom;
            decoded
        }
        TextInputEncoding::LossyUtf8 => {
            let text = String::from_utf8_lossy(bytes).into_owned();
            DecodedText {
                had_replacement_characters: text.contains(char::REPLACEMENT_CHARACTER),
                text,
                encoding: TextEncoding::LossyUtf8,
                has_bom: false,
            }
        }
    }
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
    if bytes.len() % 2 == 1 {
        // A trailing odd byte is a truncated UTF-16 code unit; surface it as a
        // replacement character instead of silently discarding the byte.
        words.push(0xFFFD);
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
    substitutions: Vec<(Regex, String)>,
}

impl<'a> NormalizationPlan<'a> {
    fn new(options: &'a TextCompareOptions) -> Self {
        let mut ignore_line_patterns_raw = options.ignore_line_patterns.clone();
        let mut substitutions_raw = options.substitutions.clone();
        for id in &options.regex_rule_sets {
            if let Some(rule_set) = text_regex_rule_set(id) {
                ignore_line_patterns_raw.extend(rule_set.ignore_line_patterns);
                substitutions_raw.extend(rule_set.substitutions);
            }
        }

        let ignore_line_patterns = RegexSetBuilder::new(&ignore_line_patterns_raw)
            .case_insensitive(options.ignore_case)
            .build()
            .ok();
        let substitutions = substitutions_raw
            .iter()
            .filter_map(|substitution| {
                Regex::new(&substitution.pattern)
                    .ok()
                    .map(|regex| (regex, substitution.replacement.clone()))
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
            normalized = regex
                .replace_all(&normalized, replacement.as_str())
                .into_owned();
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

fn pair_changed_lines(raw_lines: Vec<DiffLine>, granularity: InlineGranularity) -> Vec<DiffLine> {
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
            let inline = match granularity {
                InlineGranularity::Char => inline_diff(&left, &right),
                InlineGranularity::Word => inline_diff_word(&left, &right),
                InlineGranularity::Grapheme => inline_diff_grapheme(&left, &right),
            };
            lines.push(DiffLine {
                kind: DiffLineKind::Changed,
                left_line: current.left_line,
                right_line: next.right_line,
                inline,
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

struct Token {
    char_start: usize,
    char_end: usize,
    text: String,
}

fn tokenize_words(s: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        if chars[i].is_alphanumeric() {
            while i < chars.len() && chars[i].is_alphanumeric() {
                i += 1;
            }
        } else {
            i += 1;
        }
        tokens.push(Token {
            char_start: start,
            char_end: i,
            text: chars[start..i].iter().collect(),
        });
    }
    tokens
}

fn token_lcs<'a>(left: &'a [Token], right: &'a [Token]) -> Vec<(usize, usize)> {
    let n = left.len();
    let m = right.len();
    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if left[i].text == right[j].text {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut matches = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if left[i].text == right[j].text {
            matches.push((i, j));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    matches
}

fn inline_diff_word(left: &str, right: &str) -> Vec<InlineDiff> {
    if left == right {
        return Vec::new();
    }
    let left_tokens = tokenize_words(left);
    let right_tokens = tokenize_words(right);
    let left_char_len = left.chars().count();
    let right_char_len = right.chars().count();
    if left_tokens.is_empty() && right_tokens.is_empty() {
        return vec![InlineDiff {
            left_start: 0,
            left_end: left_char_len,
            right_start: 0,
            right_end: right_char_len,
        }];
    }
    let matches = token_lcs(&left_tokens, &right_tokens);
    let mut spans = Vec::new();
    let mut li = 0;
    let mut ri = 0;
    for (mi, mj) in &matches {
        let left_changed =
            (li..*mi).any(|k| left_tokens[k].text.chars().any(|c| c.is_alphanumeric()));
        let right_changed =
            (ri..*mj).any(|k| right_tokens[k].text.chars().any(|c| c.is_alphanumeric()));
        if left_changed || right_changed {
            let ls = if li < left_tokens.len() {
                left_tokens[li].char_start
            } else {
                left_char_len
            };
            let le = if *mi < left_tokens.len() {
                left_tokens[*mi].char_start
            } else if *mi > 0 {
                left_tokens[*mi - 1].char_end
            } else {
                0
            };
            let rs = if ri < right_tokens.len() {
                right_tokens[ri].char_start
            } else {
                right_char_len
            };
            let re = if *mj < right_tokens.len() {
                right_tokens[*mj].char_start
            } else if *mj > 0 {
                right_tokens[*mj - 1].char_end
            } else {
                0
            };
            if le > ls || re > rs {
                spans.push(InlineDiff {
                    left_start: ls,
                    left_end: le,
                    right_start: rs,
                    right_end: re,
                });
            }
        }
        li = *mi + 1;
        ri = *mj + 1;
    }
    let trailing_left_changed =
        (li..left_tokens.len()).any(|k| left_tokens[k].text.chars().any(|c| c.is_alphanumeric()));
    let trailing_right_changed =
        (ri..right_tokens.len()).any(|k| right_tokens[k].text.chars().any(|c| c.is_alphanumeric()));
    if trailing_left_changed || trailing_right_changed {
        let ls = if li < left_tokens.len() {
            left_tokens[li].char_start
        } else {
            left_char_len
        };
        let rs = if ri < right_tokens.len() {
            right_tokens[ri].char_start
        } else {
            right_char_len
        };
        if left_char_len > ls || right_char_len > rs {
            spans.push(InlineDiff {
                left_start: ls,
                left_end: left_char_len,
                right_start: rs,
                right_end: right_char_len,
            });
        }
    }
    if spans.is_empty() {
        spans.push(InlineDiff {
            left_start: 0,
            left_end: left_char_len,
            right_start: 0,
            right_end: right_char_len,
        });
    }
    spans
}

fn grapheme_lcs<'a>(left: &'a [&str], right: &'a [&str]) -> Vec<(usize, usize)> {
    let n = left.len();
    let m = right.len();
    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if left[i] == right[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut matches = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if left[i] == right[j] {
            matches.push((i, j));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    matches
}

fn inline_diff_grapheme(left: &str, right: &str) -> Vec<InlineDiff> {
    if left == right {
        return Vec::new();
    }
    let left_graphemes: Vec<&str> = UnicodeSegmentation::graphemes(left, true).collect();
    let right_graphemes: Vec<&str> = UnicodeSegmentation::graphemes(right, true).collect();
    let left_char_offsets: Vec<usize> = {
        let mut offsets = Vec::with_capacity(left_graphemes.len());
        let mut offset = 0;
        for g in &left_graphemes {
            offsets.push(offset);
            offset += g.chars().count();
        }
        offsets
    };
    let right_char_offsets: Vec<usize> = {
        let mut offsets = Vec::with_capacity(right_graphemes.len());
        let mut offset = 0;
        for g in &right_graphemes {
            offsets.push(offset);
            offset += g.chars().count();
        }
        offsets
    };
    let left_char_len = left.chars().count();
    let right_char_len = right.chars().count();
    if left_graphemes.is_empty() && right_graphemes.is_empty() {
        return vec![InlineDiff {
            left_start: 0,
            left_end: 0,
            right_start: 0,
            right_end: 0,
        }];
    }
    let matches = grapheme_lcs(&left_graphemes, &right_graphemes);
    let mut spans = Vec::new();
    let mut li = 0;
    let mut ri = 0;
    for (mi, mj) in &matches {
        if *mi > li || *mj > ri {
            let ls = left_char_offsets[li];
            let le = left_char_offsets[*mi];
            let rs = right_char_offsets[ri];
            let re = right_char_offsets[*mj];
            spans.push(InlineDiff {
                left_start: ls,
                left_end: le,
                right_start: rs,
                right_end: re,
            });
        }
        li = *mi + 1;
        ri = *mj + 1;
    }
    if li < left_graphemes.len() || ri < right_graphemes.len() {
        let ls = if li < left_graphemes.len() {
            left_char_offsets[li]
        } else {
            left_char_len
        };
        let rs = if ri < right_graphemes.len() {
            right_char_offsets[ri]
        } else {
            right_char_len
        };
        spans.push(InlineDiff {
            left_start: ls,
            left_end: left_char_len,
            right_start: rs,
            right_end: right_char_len,
        });
    }
    if spans.is_empty() {
        vec![InlineDiff {
            left_start: 0,
            left_end: left_char_len,
            right_start: 0,
            right_end: right_char_len,
        }]
    } else {
        spans
    }
}

const LCS_FULL_TABLE_THRESHOLD: usize = 4000;

fn lcs_table_cancellable(
    left: &[ComparableLine],
    right: &[ComparableLine],
    should_cancel: &dyn Fn() -> bool,
) -> Option<Vec<Vec<usize>>> {
    let n = left.len();
    let m = right.len();
    if n > LCS_FULL_TABLE_THRESHOLD || m > LCS_FULL_TABLE_THRESHOLD {
        return Some(Vec::new());
    }
    let mut table = vec![vec![0; m + 1]; n + 1];

    for i in (0..n).rev() {
        if should_cancel() {
            return None;
        }
        for j in (0..m).rev() {
            table[i][j] = if left[i].text == right[j].text {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    Some(table)
}

/// Compute the LCS length row for positions left[0..n] vs right using only two
/// rows. Returns the forward row (lengths at each column position).
fn lcs_length_row(
    left: &[ComparableLine],
    right: &[ComparableLine],
    should_cancel: &dyn Fn() -> bool,
) -> Option<Vec<usize>> {
    let m = right.len();
    let mut prev = vec![0usize; m + 1];
    let mut curr = vec![0usize; m + 1];
    for l in left {
        if should_cancel() {
            return None;
        }
        prev.copy_from_slice(&curr);
        for (j, r) in right.iter().enumerate() {
            curr[j + 1] = if l.text == r.text {
                prev[j] + 1
            } else {
                prev[j + 1].max(curr[j])
            };
        }
    }
    Some(curr)
}

/// Compute the reverse LCS length row (from the end backwards).
fn lcs_length_row_rev(
    left: &[ComparableLine],
    right: &[ComparableLine],
    should_cancel: &dyn Fn() -> bool,
) -> Option<Vec<usize>> {
    let m = right.len();
    let mut prev = vec![0usize; m + 1];
    let mut curr = vec![0usize; m + 1];
    for l in left.iter().rev() {
        if should_cancel() {
            return None;
        }
        prev.copy_from_slice(&curr);
        for j in (0..m).rev() {
            curr[j] = if l.text == right[j].text {
                prev[j + 1] + 1
            } else {
                prev[j].max(curr[j + 1])
            };
        }
    }
    Some(curr)
}

/// Hirschberg's algorithm: produce raw diff lines using O(min(n,m)) space.
fn hirschberg_diff(
    left_document: &TextDocument,
    right_document: &TextDocument,
    left: &[ComparableLine],
    right: &[ComparableLine],
    should_cancel: &dyn Fn() -> bool,
) -> Option<Vec<DiffLine>> {
    let n = left.len();
    let m = right.len();
    let mut matches = Vec::new();
    hirschberg_recursive(left, right, 0, n, 0, m, should_cancel, &mut matches)?;
    Some(matches_to_diff_lines(
        left_document,
        right_document,
        left,
        right,
        &matches,
    ))
}

#[allow(clippy::too_many_arguments)]
fn hirschberg_recursive(
    left: &[ComparableLine],
    right: &[ComparableLine],
    li: usize,
    ln: usize,
    ri: usize,
    rn: usize,
    should_cancel: &dyn Fn() -> bool,
    matches: &mut Vec<(usize, usize)>,
) -> Option<()> {
    let n = ln - li;
    let m = rn - ri;
    if n == 0 && m == 0 {
        return Some(());
    }
    if n == 0 {
        return Some(());
    }
    if m == 0 {
        return Some(());
    }
    if n == 1 {
        for (j, r) in right[ri..rn].iter().enumerate() {
            if left[li].text == r.text {
                matches.push((li, ri + j));
                break;
            }
        }
        return Some(());
    }
    if should_cancel() {
        return None;
    }

    let mid = li + n / 2;
    let l_top = &left[li..mid];
    let l_bot = &left[mid..ln];
    let r_slice = &right[ri..rn];

    let forward = lcs_length_row(l_top, r_slice, should_cancel)?;
    let backward = lcs_length_row_rev(l_bot, r_slice, should_cancel)?;

    let mut best_k = 0;
    let mut best_sum = 0;
    for k in 0..=m {
        let sum = forward[k] + backward[m - k];
        if sum > best_sum {
            best_sum = sum;
            best_k = k;
        }
    }

    hirschberg_recursive(
        left,
        right,
        li,
        mid,
        ri,
        ri + best_k,
        should_cancel,
        matches,
    )?;
    hirschberg_recursive(
        left,
        right,
        mid,
        ln,
        ri + best_k,
        rn,
        should_cancel,
        matches,
    )?;
    Some(())
}

fn matches_to_diff_lines(
    left_document: &TextDocument,
    right_document: &TextDocument,
    left: &[ComparableLine],
    right: &[ComparableLine],
    matches: &[(usize, usize)],
) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut li = 0;
    let mut ri = 0;
    let mut sorted = matches.to_vec();
    sorted.sort_by_key(|(i, j)| (*i, *j));

    for (mi, mj) in &sorted {
        while li < *mi {
            lines.push(left_only_line(left_document, left[li].number));
            li += 1;
        }
        while ri < *mj {
            lines.push(right_only_line(right_document, right[ri].number));
            ri += 1;
        }
        lines.push(equal_line(
            left_document,
            right_document,
            left[*mi].number,
            right[*mj].number,
        ));
        li = *mi + 1;
        ri = *mj + 1;
    }
    while li < left.len() {
        lines.push(left_only_line(left_document, left[li].number));
        li += 1;
    }
    while ri < right.len() {
        lines.push(right_only_line(right_document, right[ri].number));
        ri += 1;
    }
    lines
}

fn patience_diff(
    left_document: &TextDocument,
    right_document: &TextDocument,
    left: &[ComparableLine],
    right: &[ComparableLine],
    should_cancel: &dyn Fn() -> bool,
) -> Option<Vec<DiffLine>> {
    if should_cancel() {
        return None;
    }
    let mut left_counts: HashMap<String, usize> = HashMap::new();
    for l in left {
        *left_counts.entry(l.text.clone()).or_insert(0) += 1;
    }
    let mut right_counts: HashMap<String, usize> = HashMap::new();
    for r in right {
        *right_counts.entry(r.text.clone()).or_insert(0) += 1;
    }

    let mut unique_pairs: Vec<(usize, usize)> = Vec::new();
    // Build a map from right-line text → first index so the unique-line
    // lookup is O(1) instead of O(m) per left line (which makes the whole
    // anchor scan O(n×m) for files where most lines are unique).
    let mut right_first_index: HashMap<&str, usize> = HashMap::new();
    for (ri, r) in right.iter().enumerate() {
        right_first_index.entry(r.text.as_str()).or_insert(ri);
    }
    for (li, l) in left.iter().enumerate() {
        if left_counts[&l.text] == 1
            && right_counts.get(&l.text) == Some(&1)
            && let Some(&ri) = right_first_index.get(l.text.as_str())
        {
            unique_pairs.push((li, ri));
        }
    }

    // `unique_pairs` is collected in left-side order. Patience diff then keeps
    // the longest increasing sequence of right-side positions; sorting by right
    // first can select crossing anchors and later produce reversed gap slices.
    let lis = longest_increasing_subsequence(&unique_pairs);
    let mut anchors: Vec<(usize, usize)> = lis.iter().map(|&(li, ri)| (li, ri)).collect();
    anchors.sort_by_key(|(li, _)| *li);

    let mut lines = Vec::new();
    let mut prev_li = 0;
    let mut prev_ri = 0;

    for (li, ri) in &anchors {
        if should_cancel() {
            return None;
        }
        let gap_left = &left[prev_li..*li];
        let gap_right = &right[prev_ri..*ri];
        if !gap_left.is_empty() || !gap_right.is_empty() {
            let gap_diff = lcs_gap_diff(
                left_document,
                right_document,
                gap_left,
                gap_right,
                prev_li,
                prev_ri,
                should_cancel,
            )?;
            lines.extend(gap_diff);
        }
        lines.push(equal_line(
            left_document,
            right_document,
            left[*li].number,
            right[*ri].number,
        ));
        prev_li = *li + 1;
        prev_ri = *ri + 1;
    }

    let gap_left = &left[prev_li..];
    let gap_right = &right[prev_ri..];
    if !gap_left.is_empty() || !gap_right.is_empty() {
        let gap_diff = lcs_gap_diff(
            left_document,
            right_document,
            gap_left,
            gap_right,
            prev_li,
            prev_ri,
            should_cancel,
        )?;
        lines.extend(gap_diff);
    }

    Some(lines)
}

fn longest_increasing_subsequence(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    if pairs.is_empty() {
        return Vec::new();
    }
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; pairs.len()];
    let mut head: Vec<usize> = Vec::new();

    for (i, &(_li, ri)) in pairs.iter().enumerate() {
        let pos = tails
            .binary_search_by(|&idx| pairs[idx].1.cmp(&ri))
            .unwrap_or_else(|x| x);
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
        prev[i] = if pos > 0 { Some(tails[pos - 1]) } else { None };
        if head.len() <= pos {
            head.push(i);
        } else {
            head[pos] = i;
        }
    }

    let mut result = Vec::new();
    let mut current = tails.last().copied();
    while let Some(idx) = current {
        result.push(pairs[idx]);
        current = prev[idx];
    }
    result.reverse();
    result
}

fn lcs_gap_diff(
    left_document: &TextDocument,
    right_document: &TextDocument,
    gap_left: &[ComparableLine],
    gap_right: &[ComparableLine],
    _left_offset: usize,
    _right_offset: usize,
    should_cancel: &dyn Fn() -> bool,
) -> Option<Vec<DiffLine>> {
    if gap_left.is_empty() {
        return Some(
            gap_right
                .iter()
                .map(|r| right_only_line(right_document, r.number))
                .collect(),
        );
    }
    if gap_right.is_empty() {
        return Some(
            gap_left
                .iter()
                .map(|l| left_only_line(left_document, l.number))
                .collect(),
        );
    }
    let n = gap_left.len();
    let m = gap_right.len();
    if n > LCS_FULL_TABLE_THRESHOLD || m > LCS_FULL_TABLE_THRESHOLD {
        return hirschberg_diff(
            left_document,
            right_document,
            gap_left,
            gap_right,
            should_cancel,
        );
    }
    let lcs = lcs_table_cancellable(gap_left, gap_right, should_cancel)?;
    Some(raw_diff_lines(
        left_document,
        right_document,
        gap_left,
        gap_right,
        &lcs,
    ))
}

fn myers_diff(
    left_document: &TextDocument,
    right_document: &TextDocument,
    left: &[ComparableLine],
    right: &[ComparableLine],
    should_cancel: &dyn Fn() -> bool,
) -> Option<Vec<DiffLine>> {
    if should_cancel() {
        return None;
    }
    let n = left.len();
    let m = right.len();
    if n == 0 && m == 0 {
        return Some(Vec::new());
    }
    if n == 0 {
        return Some(
            right
                .iter()
                .map(|r| right_only_line(right_document, r.number))
                .collect(),
        );
    }
    if m == 0 {
        return Some(
            left.iter()
                .map(|l| left_only_line(left_document, l.number))
                .collect(),
        );
    }

    let max = n + m;
    let offset = max as i64;
    let mut v = vec![-1i64; 2 * max + 1];
    v[(offset + 1) as usize] = 0;
    let mut trace: Vec<Vec<i64>> = Vec::new();

    for d in 0..=max {
        if should_cancel() {
            return None;
        }
        for k in (-(d as i64)..=d as i64).step_by(2) {
            let idx = (k + offset) as usize;
            let mut x = if k == -(d as i64) || (k != d as i64 && v[idx - 1] < v[idx + 1]) {
                v[idx + 1]
            } else {
                v[idx - 1] + 1
            };
            let mut y = x - k;
            while x < n as i64 && y < m as i64 && left[x as usize].text == right[y as usize].text {
                x += 1;
                y += 1;
            }
            v[idx] = x;
            if x >= n as i64 && y >= m as i64 {
                trace.push(v.clone());
                let edits = myers_backtrack(&trace, max, n, m);
                return Some(myers_edits_to_diff_lines(
                    left_document,
                    right_document,
                    left,
                    right,
                    &edits,
                ));
            }
        }
        trace.push(v.clone());
    }
    Some(Vec::new())
}

fn myers_backtrack(trace: &[Vec<i64>], max: usize, n: usize, m: usize) -> Vec<MyersEdit> {
    let mut edits = Vec::new();
    let mut x = n as i64;
    let mut y = m as i64;
    let offset = max as i64;

    for d_idx in (1..trace.len()).rev() {
        let previous = &trace[d_idx - 1];
        let d = d_idx as i64;
        let k = x - y;
        let prev_k = if k == -(d)
            || (k != d && previous[(k - 1 + offset) as usize] < previous[(k + 1 + offset) as usize])
        {
            k + 1
        } else {
            k - 1
        };
        let prev_x = previous[(prev_k + offset) as usize];
        let prev_y = prev_x - prev_k;

        while x > prev_x && y > prev_y {
            edits.push(MyersEdit::Keep);
            x -= 1;
            y -= 1;
        }

        if x == prev_x {
            edits.push(MyersEdit::Insert);
            y -= 1;
        } else {
            edits.push(MyersEdit::Delete);
            x -= 1;
        }
    }

    while x > 0 && y > 0 {
        edits.push(MyersEdit::Keep);
        x -= 1;
        y -= 1;
    }

    edits.reverse();
    edits
}

#[derive(Clone, Copy)]
enum MyersEdit {
    Keep,
    Delete,
    Insert,
}

fn myers_edits_to_diff_lines(
    left_document: &TextDocument,
    right_document: &TextDocument,
    left: &[ComparableLine],
    right: &[ComparableLine],
    edits: &[MyersEdit],
) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut li: usize = 0;
    let mut ri: usize = 0;
    let mut pending_deletes: Vec<usize> = Vec::new();
    let mut pending_inserts: Vec<usize> = Vec::new();

    let flush_deletes = |deletes: &[usize], lines: &mut Vec<DiffLine>, li_base: usize| {
        for &di in deletes {
            lines.push(left_only_line(left_document, left[li_base + di].number));
        }
    };
    let flush_inserts = |inserts: &[usize], lines: &mut Vec<DiffLine>, ri_base: usize| {
        for &ii in inserts {
            lines.push(right_only_line(right_document, right[ri_base + ii].number));
        }
    };

    let mut del_offset: usize = 0;
    let mut ins_offset: usize = 0;

    for &edit in edits {
        match edit {
            MyersEdit::Keep => {
                flush_deletes(&pending_deletes, &mut lines, del_offset);
                flush_inserts(&pending_inserts, &mut lines, ins_offset);
                pending_deletes.clear();
                pending_inserts.clear();
                lines.push(equal_line(
                    left_document,
                    right_document,
                    left[li].number,
                    right[ri].number,
                ));
                li += 1;
                ri += 1;
                del_offset = li;
                ins_offset = ri;
            }
            MyersEdit::Delete => {
                pending_deletes.push(li - del_offset);
                li += 1;
            }
            MyersEdit::Insert => {
                pending_inserts.push(ri - ins_offset);
                ri += 1;
            }
        }
    }
    flush_deletes(&pending_deletes, &mut lines, del_offset);
    flush_inserts(&pending_inserts, &mut lines, ins_offset);

    lines
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

    // `diff_blocks` builds one block per maximal run of same-kind lines, so the
    // blocks line up 1:1, in order, with these positional runs. Slicing by
    // position is robust to non-contiguous line numbers (blank-line / regex
    // filtering leaves gaps that a line-number-range lookup would mis-handle).
    let ranges = block_line_ranges(lines);

    for (block_idx, block) in blocks.iter().enumerate() {
        if !matches!(block.kind, DiffBlockKind::Difference) {
            continue;
        }

        // Determine if this block is purely left-only (delete) or purely
        // right-only (add).  Mixed blocks (Changed lines) are skipped.
        let block_lines: Vec<&DiffLine> = lines[ranges[block_idx].clone()].iter().collect();

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

/// Partition `lines` into the contiguous index runs that [`diff_blocks`] groups
/// into blocks — one range per block, in the same order. Recovering a block's
/// lines by position (rather than by line-number range) is correct even when
/// blank-line / regex filtering leaves gaps in the document line numbers.
fn block_line_ranges(lines: &[DiffLine]) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let kind = block_kind(lines[i].kind);
        let start = i;
        i += 1;
        while i < lines.len() && block_kind(lines[i].kind) == kind {
            i += 1;
        }
        ranges.push(start..i);
    }
    ranges
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

fn visible_line_ranges(lines: &[DiffLine], options: &TextCompareOptions) -> Vec<DiffHunkRange> {
    if options.show_only_changes {
        return lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                (line.kind != DiffLineKind::Equal).then_some(DiffHunkRange {
                    start: index,
                    end: index + 1,
                })
            })
            .collect();
    }

    match options.context_lines {
        Some(context) => diff_hunks(lines, context),
        None => {
            if lines.is_empty() {
                Vec::new()
            } else {
                vec![DiffHunkRange {
                    start: 0,
                    end: lines.len(),
                }]
            }
        }
    }
}

fn fold_row(index: usize, count: usize) -> TextViewRow {
    TextViewRow {
        index,
        left_line: None,
        right_line: None,
        left: format!("... {count} unchanged line(s) folded ..."),
        right: format!("... {count} unchanged line(s) folded ..."),
        state: "folded".to_owned(),
        block_kind: "folded".to_owned(),
        folded_count: Some(count),
        left_syntax: Vec::new(),
        right_syntax: Vec::new(),
        find_matches: Vec::new(),
        bookmarks: Vec::new(),
    }
}

fn view_row_for_line(
    index: usize,
    source_index: usize,
    line: &DiffLine,
    result: &TextCompareResult,
    syntax_mode: TextSyntaxMode,
    find_matches: &[TextFindMatch],
    bookmarks: &[TextBookmark],
) -> TextViewRow {
    let state = gui_state_for_line(line.kind).to_owned();
    let block_kind = block_kind_for_source_index(source_index, &result.blocks).to_owned();
    let left_text = line.left.clone().unwrap_or_default();
    let right_text = line.right.clone().unwrap_or_default();
    let left_syntax = if left_text.is_empty() {
        Vec::new()
    } else {
        syntax_spans(&left_text, syntax_mode)
    };
    let right_syntax = if right_text.is_empty() {
        Vec::new()
    } else {
        syntax_spans(&right_text, syntax_mode)
    };
    let row_find_matches = find_matches
        .iter()
        .filter(|m| {
            (m.side == CompareSide::Left && line.left_line == Some(m.line))
                || (m.side == CompareSide::Right && line.right_line == Some(m.line))
        })
        .cloned()
        .collect();
    let row_bookmarks = bookmarks
        .iter()
        .filter(|b| {
            (b.side == CompareSide::Left && line.left_line == Some(b.line))
                || (b.side == CompareSide::Right && line.right_line == Some(b.line))
        })
        .cloned()
        .collect();

    TextViewRow {
        index,
        left_line: line.left_line,
        right_line: line.right_line,
        left: left_text,
        right: right_text,
        state,
        block_kind,
        folded_count: None,
        left_syntax,
        right_syntax,
        find_matches: row_find_matches,
        bookmarks: row_bookmarks,
    }
}

fn gui_state_for_line(kind: DiffLineKind) -> &'static str {
    match kind {
        DiffLineKind::Equal => "equal",
        DiffLineKind::Changed => "changed",
        DiffLineKind::LeftOnly => "left_only",
        DiffLineKind::RightOnly => "right_only",
    }
}

fn block_kind_for_source_index(index: usize, blocks: &[DiffBlock]) -> &'static str {
    let mut cursor = 0usize;
    for block in blocks {
        let span = block.left_len.max(block.right_len).max(1);
        if index >= cursor && index < cursor + span {
            return match block.kind {
                DiffBlockKind::Equal => "equal",
                DiffBlockKind::Difference => "difference",
                DiffBlockKind::Moved { .. } => "moved",
            };
        }
        cursor += span;
    }
    "equal"
}

fn side_by_side_text(result: &TextCompareResult, options: &TextCompareOptions) -> String {
    let mut output = String::new();
    for row in result.view_rows(options) {
        if row.folded_count.is_some() {
            output.push_str(&row.left);
            output.push('\n');
            continue;
        }
        let left_no = row
            .left_line
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_owned());
        let right_no = row
            .right_line
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_owned());
        output.push_str(&format!(
            "{left_no:>6} | {right_no:>6} | {:<12} | {} || {}\n",
            row.state, row.left, row.right
        ));
    }
    output
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

fn html_report(
    result: &TextCompareResult,
    context: Option<usize>,
    syntax_mode: TextSyntaxMode,
) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">\n");
    output.push_str("<title>LinSync Compare Report</title>\n");
    output.push_str(
        "<style>body{font-family:sans-serif}table{border-collapse:collapse;width:100%}\
td,th{border:1px solid #bbb;padding:0.25rem 0.4rem;font-family:monospace;white-space:pre-wrap}\
.eq{background:#fff}.chg{background:#fff4c2}.del{background:#ffd9d9}.add{background:#daf5d7}\
.syn-keyword{color:#7a3e9d;font-weight:600}.syn-string{color:#0b6b3a}.syn-number{color:#8a4b08}\
.syn-comment{color:#69717a;font-style:italic}.syn-key{color:#005f9e}.syn-tag{color:#8a2b58}</style>\n",
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
    let resolved_syntax = resolved_syntax_mode(
        syntax_mode,
        result.left_document.path.as_deref(),
        result.right_document.path.as_deref(),
    );
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
            syntax_highlight_html(line.left.as_deref().unwrap_or(""), resolved_syntax),
            syntax_highlight_html(line.right.as_deref().unwrap_or(""), resolved_syntax)
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

fn collect_find_matches(
    side: CompareSide,
    line: usize,
    text: &str,
    regex: &Regex,
    matches: &mut Vec<TextFindMatch>,
) {
    for m in regex.find_iter(text) {
        matches.push(TextFindMatch {
            side,
            line,
            start: byte_to_char_index(text, m.start()),
            end: byte_to_char_index(text, m.end()),
            text: m.as_str().to_owned(),
        });
    }
}

fn byte_to_char_index(text: &str, byte_index: usize) -> usize {
    text[..byte_index.min(text.len())].chars().count()
}

fn resolved_syntax_mode(
    requested: TextSyntaxMode,
    left: Option<&Path>,
    right: Option<&Path>,
) -> TextSyntaxMode {
    if requested != TextSyntaxMode::Auto {
        return requested;
    }
    left.and_then(syntax_mode_from_path)
        .or_else(|| right.and_then(syntax_mode_from_path))
        .unwrap_or(TextSyntaxMode::Plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

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
    fn large_lcs_compare_can_cancel_after_work_starts() {
        let opts = TextCompareOptions::default();
        let left_text = (0..=LCS_FULL_TABLE_THRESHOLD)
            .map(|line| format!("left-{line}\n"))
            .collect::<String>();
        let right_text = (0..=LCS_FULL_TABLE_THRESHOLD)
            .map(|line| format!("right-{line}\n"))
            .collect::<String>();
        let left = TextDocument::from_text("l", &left_text);
        let right = TextDocument::from_text("r", &right_text);
        let polls = Cell::new(0usize);

        let got = compare_documents_cancellable(left, right, &opts, &|| {
            let next = polls.get() + 1;
            polls.set(next);
            next >= 6
        });

        assert!(got.is_none(), "mid-compare cancellation must abort");
        assert!(
            polls.get() >= 6,
            "cancellation should happen after the large-input diff starts"
        );
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
    fn named_regex_rule_sets_normalize_volatile_values() {
        let result = compare_text(
            "left",
            "id=9f3cf7aa-1d98-4a1a-a80d-d91f442ec4a7 at 2026-05-30T10:00:00Z\n",
            "right",
            "id=11111111-2222-4333-8444-555555555555 at 2026-05-31T11:12:13Z\n",
            &TextCompareOptions {
                regex_rule_sets: vec!["volatile".to_owned()],
                ..TextCompareOptions::default()
            },
        );

        assert!(result.is_equal());
    }

    #[test]
    fn context_folding_returns_fold_rows() {
        let result = compare_text(
            "left",
            "a\nb\nc\nd\ne\n",
            "right",
            "a\nb\nX\nd\ne\n",
            &TextCompareOptions::default(),
        );
        let rows = result.view_rows(&TextCompareOptions {
            context_lines: Some(0),
            ..TextCompareOptions::default()
        });

        assert!(rows.iter().any(|row| row.folded_count == Some(2)));
        assert!(rows.iter().any(|row| row.state == "changed"));
    }

    #[test]
    fn text_compare_result_json_round_trip_preserves_report() {
        // A saved result must re-render identically (report --from-json) without
        // recomparing. Internal byte offsets (`#[serde(skip)]`) are not part of
        // the contract, so fidelity is checked at the rendered-report level.
        let result = compare_text(
            "left.txt",
            "alpha\nbeta\ngamma\n",
            "right.txt",
            "alpha\nBETA\ngamma\ndelta\n",
            &TextCompareOptions::default(),
        );
        let json = serde_json::to_string(&result).expect("serialize");
        let back: TextCompareResult = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(
            back.summary, result.summary,
            "summary survives the round-trip"
        );
        assert_eq!(back.lines.len(), result.lines.len());
        assert_eq!(
            back.to_html_report_with_context(None),
            result.to_html_report_with_context(None),
            "a re-rendered report must match the original"
        );
    }

    #[test]
    fn view_rows_window_matches_full_view_slice() {
        // 40 lines with a single change in the middle, context 2, so the
        // full view has folds around the change.
        let left = (1..=40)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut right_lines: Vec<String> = (1..=40).map(|n| format!("line {n}")).collect();
        right_lines[19] = "line 20 CHANGED".to_string();
        let right = right_lines.join("\n");
        let opts = TextCompareOptions {
            context_lines: Some(2),
            ..TextCompareOptions::default()
        };
        let result = compare_text("L", &left, "R", &right, &opts);
        let full = result.view_rows(&opts);
        let total = full.len();
        assert!(total >= 3, "fixture should yield several rows, got {total}");

        // A full-width window reproduces the entire view exactly.
        let all = result.view_rows_window(&opts, 0, total);
        assert_eq!(all.total_rows, total);
        assert_eq!(all.offset, 0);
        assert_eq!(all.rows, full, "full window must equal view_rows");

        // An interior window equals the matching slice, indices included.
        let win = result.view_rows_window(&opts, 1, 2);
        let hi = (1 + 2).min(total);
        assert_eq!(win.total_rows, total);
        assert_eq!(win.offset, 1);
        assert_eq!(win.rows, full[1..hi]);

        // Offset past the end clamps to an empty, terminal page.
        let beyond = result.view_rows_window(&opts, total + 10, 5);
        assert_eq!(beyond.offset, total);
        assert!(beyond.rows.is_empty());
    }

    #[test]
    fn regex_find_reports_side_line_and_char_spans() {
        let result = compare_text(
            "left",
            "alpha 123\n",
            "right",
            "alpha 456\n",
            &TextCompareOptions::default(),
        );
        let matches = result
            .find_matches(&TextFindOptions {
                pattern: r"\d+".to_owned(),
                regex: true,
                case_sensitive: true,
            })
            .unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].side, CompareSide::Left);
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[0].start, 6);
        assert_eq!(matches[0].end, 9);
    }

    #[test]
    fn literal_find_is_unicode_safe_when_case_insensitive() {
        let result = compare_text(
            "left",
            "İstanbul and foo.bar\n",
            "right",
            "istanbul and fooXbar\n",
            &TextCompareOptions::default(),
        );
        let matches = result
            .find_matches(&TextFindOptions {
                pattern: "foo.bar".to_owned(),
                regex: false,
                case_sensitive: false,
            })
            .unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].side, CompareSide::Left);
        assert_eq!(matches[0].start, 13);
        assert_eq!(matches[0].end, 20);
        assert_eq!(matches[0].text, "foo.bar");
    }

    #[test]
    fn forced_utf16le_decodes_without_bom() {
        let bytes = [b'a', 0, b'\n', 0];
        let document = TextDocument::from_bytes_with_encoding(
            "utf16".to_owned(),
            None,
            &bytes,
            false,
            TextInputEncoding::Utf16Le,
        );

        assert_eq!(document.encoding, TextEncoding::Utf16Le);
        assert_eq!(document.lines[0].text, "a");
    }

    #[test]
    fn html_render_can_include_syntax_spans() {
        let result = compare_text(
            "left.rs",
            "fn main() {}\n",
            "right.rs",
            "fn main() { return; }\n",
            &TextCompareOptions::default(),
        );
        let html = result.to_html_report_with_options(None, TextSyntaxMode::Rust);

        assert!(html.contains("syn-keyword"));
        assert!(html.contains("return"));
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
    #[test]
    fn encoding_summary_detects_differences() {
        let left_bytes = b"alpha\r\nbeta";
        let right_bytes = b"alpha\nbeta";
        let left_document = TextDocument::from_bytes("left".to_owned(), None, left_bytes, false);
        let right_document = TextDocument::from_bytes("right".to_owned(), None, right_bytes, false);
        let result = compare_documents(
            left_document,
            right_document,
            &TextCompareOptions::default(),
        );
        let summary = result.encoding_summary();
        assert_eq!(summary.left_encoding, TextEncoding::Utf8);
        assert_eq!(summary.right_encoding, TextEncoding::Utf8);
        assert!(!summary.encoding_differs);
        assert_eq!(summary.left_line_ending, LineEnding::Crlf);
        assert_eq!(summary.right_line_ending, LineEnding::Lf);
        assert!(summary.line_ending_differs);
        assert!(!summary.bom_differs);
    }

    #[test]
    fn encoding_summary_detects_bom_difference() {
        let left_bytes = &[0xEF, 0xBB, 0xBF, b'a', b'\n'];
        let right_bytes = b"a\n";
        let left_document = TextDocument::from_bytes("left".to_owned(), None, left_bytes, false);
        let right_document = TextDocument::from_bytes("right".to_owned(), None, right_bytes, false);
        let result = compare_documents(
            left_document,
            right_document,
            &TextCompareOptions::default(),
        );
        let summary = result.encoding_summary();
        assert!(summary.encoding_differs);
        assert!(summary.bom_differs);
        assert!(summary.left_has_bom);
        assert!(!summary.right_has_bom);
        assert!(!summary.line_ending_differs);
    }

    #[test]
    fn text_document_serialization_roundtrip() {
        let doc = TextDocument::from_bytes("test".to_owned(), None, b"hello\r\nworld", false);
        let json = serde_json::to_string(&doc).unwrap();
        let back: TextDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "test");
        assert_eq!(back.encoding, TextEncoding::Utf8);
        assert_eq!(back.line_ending, LineEnding::Crlf);
        assert!(!back.has_bom);
        assert!(!back.read_only);
        assert_eq!(back.byte_len, 12);
        assert_eq!(back.lines.len(), 2);
        assert_eq!(back.lines[0].text, "hello");
        assert_eq!(back.lines[0].newline, Some(LineEnding::Crlf));
        assert_eq!(back.lines[1].text, "world");
        assert_eq!(back.lines[1].newline, None);
        assert_eq!(back.path, None);
    }

    #[test]
    fn text_line_skips_byte_fields_in_json() {
        let doc = TextDocument::from_text("x", "abc\ndef");
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("byte_start"));
        assert!(!json.contains("byte_end"));
        let back: TextDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(back.lines[0].byte_start, 0);
        assert_eq!(back.lines[0].byte_end, 0);
    }

    #[test]
    fn line_ending_serialization() {
        assert_eq!(serde_json::to_string(&LineEnding::Lf).unwrap(), "\"lf\"");
        assert_eq!(
            serde_json::to_string(&LineEnding::Crlf).unwrap(),
            "\"crlf\""
        );
        assert_eq!(serde_json::to_string(&LineEnding::Cr).unwrap(), "\"cr\"");
        assert_eq!(
            serde_json::to_string(&LineEnding::None).unwrap(),
            "\"none\""
        );
        assert_eq!(
            serde_json::to_string(&LineEnding::Mixed).unwrap(),
            "\"mixed\""
        );
        let le: LineEnding = serde_json::from_str("\"crlf\"").unwrap();
        assert_eq!(le, LineEnding::Crlf);
    }

    #[test]
    fn text_encoding_serialization() {
        assert_eq!(
            serde_json::to_string(&TextEncoding::Utf8).unwrap(),
            "\"utf8\""
        );
        assert_eq!(
            serde_json::to_string(&TextEncoding::Utf8Bom).unwrap(),
            "\"utf8_bom\""
        );
        assert_eq!(
            serde_json::to_string(&TextEncoding::Utf16Le).unwrap(),
            "\"utf16_le\""
        );
        assert_eq!(
            serde_json::to_string(&TextEncoding::Utf16Be).unwrap(),
            "\"utf16_be\""
        );
        assert_eq!(
            serde_json::to_string(&TextEncoding::LossyUtf8).unwrap(),
            "\"lossy_utf8\""
        );
        let enc: TextEncoding = serde_json::from_str("\"utf16_le\"").unwrap();
        assert_eq!(enc, TextEncoding::Utf16Le);
    }

    #[test]
    fn inline_diff_word_detects_word_changes() {
        let spans = inline_diff_word("hello world", "hello earth");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].left_start, 6);
        assert_eq!(spans[0].left_end, 11);
        assert_eq!(spans[0].right_start, 6);
        assert_eq!(spans[0].right_end, 11);
        assert_eq!(
            &"hello world"[spans[0].left_start..spans[0].left_end],
            "world"
        );
        assert_eq!(
            &"hello earth"[spans[0].right_start..spans[0].right_end],
            "earth"
        );
    }

    #[test]
    fn inline_diff_word_multiple_changes() {
        let spans = inline_diff_word("a b c", "a x c");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].left_start, 2);
        assert_eq!(spans[0].left_end, 3);
        assert_eq!(spans[0].right_start, 2);
        assert_eq!(spans[0].right_end, 3);
    }

    #[test]
    fn inline_diff_grapheme_handles_unicode() {
        let left = "cafe\u{0301}";
        let right = "caf\u{00E9}";
        let spans = inline_diff_grapheme(left, right);
        assert!(!spans.is_empty());
    }

    #[test]
    fn inline_granularity_default_is_char() {
        assert_eq!(InlineGranularity::default(), InlineGranularity::Char);
    }

    #[test]
    fn inline_diff_char_still_works() {
        let opts = TextCompareOptions {
            inline_granularity: InlineGranularity::Char,
            ..TextCompareOptions::default()
        };
        let result = compare_text("left", "alpha\nbeta\n", "right", "alpha\nbetamax\n", &opts);
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
    fn selectable_diff_algorithms_preserve_basic_results() {
        let left = "alpha\nbeta\ngamma\ndelta\n";
        let right = "alpha\nbeta changed\ngamma\nepsilon\n";
        for algorithm in [
            DiffAlgorithm::Lcs,
            DiffAlgorithm::Patience,
            DiffAlgorithm::Myers,
        ] {
            let opts = TextCompareOptions {
                diff_algorithm: algorithm,
                ..TextCompareOptions::default()
            };
            let result = compare_text("left", left, "right", right, &opts);
            assert_eq!(
                result.difference_count(),
                2,
                "algorithm {algorithm:?} should report two changed lines"
            );
            assert_eq!(result.lines.first().unwrap().kind, DiffLineKind::Equal);
            assert!(
                result
                    .lines
                    .iter()
                    .any(|line| line.kind == DiffLineKind::Changed),
                "algorithm {algorithm:?} should pair changed lines"
            );
        }
    }

    /// Assert the structural invariants every diff must satisfy: each side
    /// reconstructs from the lines that carry it, line numbers are strictly
    /// increasing, and `Equal` lines carry identical text on both sides.
    /// Returns the number of `Equal` lines so callers can cross-check
    /// minimality against a reference algorithm.
    fn assert_diff_invariants(left: &str, right: &str, result: &TextCompareResult) -> usize {
        let expected_left: Vec<&str> = if left.is_empty() {
            Vec::new()
        } else {
            left.trim_end_matches('\n').split('\n').collect()
        };
        let expected_right: Vec<&str> = if right.is_empty() {
            Vec::new()
        } else {
            right.trim_end_matches('\n').split('\n').collect()
        };

        let recon_left: Vec<&str> = result
            .lines
            .iter()
            .filter(|l| l.left_line.is_some())
            .map(|l| l.left.as_deref().unwrap_or_default())
            .collect();
        let recon_right: Vec<&str> = result
            .lines
            .iter()
            .filter(|l| l.right_line.is_some())
            .map(|l| l.right.as_deref().unwrap_or_default())
            .collect();
        assert_eq!(
            recon_left, expected_left,
            "left side must reconstruct in order"
        );
        assert_eq!(
            recon_right, expected_right,
            "right side must reconstruct in order"
        );

        let mut last_left = 0;
        let mut last_right = 0;
        let mut equal = 0;
        for line in &result.lines {
            if let Some(ll) = line.left_line {
                assert!(
                    ll > last_left,
                    "left_line must strictly increase: {:#?}",
                    result.lines
                );
                last_left = ll;
            }
            if let Some(rl) = line.right_line {
                assert!(
                    rl > last_right,
                    "right_line must strictly increase: {:#?}",
                    result.lines
                );
                last_right = rl;
            }
            if line.kind == DiffLineKind::Equal {
                assert_eq!(line.left, line.right, "Equal line must match on both sides");
                equal += 1;
            }
        }
        equal
    }

    #[test]
    fn myers_diff_handles_repeated_line_ambiguity() {
        let opts = TextCompareOptions {
            diff_algorithm: DiffAlgorithm::Myers,
            ..TextCompareOptions::default()
        };
        // Repeated-line inputs where Myers may anchor either duplicate. The
        // result must stay a valid, minimal, in-order diff regardless of which
        // duplicate is chosen as the equal anchor. `a\nb` vs `a\na` is the
        // case a previous post-processing pass corrupted into reversed line
        // numbers (left rendered line 2 above line 1).
        let cases = [
            ("b\nb\n", "a\nb\n"),
            ("a\nb\n", "b\nb\n"),
            ("a\nb\n", "a\na\n"),
            ("c\nb\nc\n", "a\nc\n"),
            ("c\nc\nc\n", "b\nc\nc\n"),
            ("p\na\nb\n", "p\np\n"),
        ];
        for (left, right) in cases {
            let result = compare_text("left", left, "right", right, &opts);
            let equal = assert_diff_invariants(left, right, &result);

            // Myers is an optimal-distance algorithm: it must keep exactly as
            // many lines equal as the LCS reference (i.e. produce a minimal
            // edit script), just possibly anchored at a different duplicate.
            let reference = compare_text(
                "left",
                left,
                "right",
                right,
                &TextCompareOptions {
                    diff_algorithm: DiffAlgorithm::Lcs,
                    ..TextCompareOptions::default()
                },
            );
            let reference_equal = reference
                .lines
                .iter()
                .filter(|l| l.kind == DiffLineKind::Equal)
                .count();
            assert_eq!(
                equal, reference_equal,
                "Myers must be minimal for {left:?} vs {right:?}"
            );
        }
    }

    #[test]
    fn patience_diff_handles_crossed_unique_lines() {
        let opts = TextCompareOptions {
            diff_algorithm: DiffAlgorithm::Patience,
            ..TextCompareOptions::default()
        };
        let result = compare_text("left", "a\nc\n", "right", "c\na\n", &opts);

        assert_eq!(result.difference_count(), 2);
        assert!(
            result
                .lines
                .iter()
                .any(|line| line.kind == DiffLineKind::Equal)
        );
    }

    #[test]
    fn inline_granularity_serialization() {
        assert_eq!(
            serde_json::to_string(&InlineGranularity::Char).unwrap(),
            "\"char\""
        );
        assert_eq!(
            serde_json::to_string(&InlineGranularity::Word).unwrap(),
            "\"word\""
        );
        assert_eq!(
            serde_json::to_string(&InlineGranularity::Grapheme).unwrap(),
            "\"grapheme\""
        );
        let g: InlineGranularity = serde_json::from_str("\"word\"").unwrap();
        assert_eq!(g, InlineGranularity::Word);
    }
}
