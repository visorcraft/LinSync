use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFilter {
    pub name: Option<String>,
    pub rules: Vec<FilterRule>,
}

impl FileFilter {
    pub fn generated_directories() -> Self {
        Self {
            name: Some("Generated directories".to_owned()),
            rules: [
                ".git",
                "node_modules",
                "target",
                "build",
                "dist",
                ".cache",
                "vendor",
                "bin",
                "obj",
                "/proc",
                "proc",
                "/sys",
                "sys",
                "/dev",
                "dev",
                "/run",
                "run",
            ]
            .into_iter()
            .map(|pattern| FilterRule {
                target: FilterTarget::Directory,
                action: FilterAction::Exclude,
                syntax: PatternSyntax::Wildcard,
                pattern: pattern.to_owned(),
            })
            .collect(),
        }
    }

    pub fn parse(input: &str) -> Result<Self, FilterParseError> {
        let mut name = None;
        let mut rules = Vec::new();

        for (index, raw_line) in input.lines().enumerate() {
            let line_number = index + 1;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }

            if let Some(value) = line.strip_prefix("name:") {
                name = Some(value.trim().to_owned());
                continue;
            }

            rules.push(parse_rule(line, line_number)?);
        }

        Ok(Self { name, rules })
    }

    pub fn decision_for_path(&self, path: &Path, is_dir: bool) -> FilterDecision {
        self.decision_for_path_with_options(path, is_dir, &FilterMatchOptions::default())
    }

    pub fn decision_for_path_with_options(
        &self,
        path: &Path,
        is_dir: bool,
        options: &FilterMatchOptions,
    ) -> FilterDecision {
        self.decision_for_entry_with_options(
            &FilterEntryContext {
                path,
                is_dir,
                size: None,
                modified: None,
                file_kind: None,
                resolved_path: None,
            },
            options,
        )
    }

    pub fn decision_for_entry_with_options(
        &self,
        context: &FilterEntryContext<'_>,
        options: &FilterMatchOptions,
    ) -> FilterDecision {
        let mut decision = FilterDecision::Neutral;

        for rule in &self.rules {
            if rule.target.matches(context.is_dir)
                && rule.matches_entry_with_options(context, options)
            {
                decision = match rule.action {
                    FilterAction::Include => FilterDecision::Include,
                    FilterAction::Exclude => FilterDecision::Exclude,
                };
            }
        }

        decision
    }

    pub fn has_include_rule_for(&self, is_dir: bool) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.action == FilterAction::Include && rule.target.matches(is_dir))
    }

    pub fn requires_file_kind(&self) -> bool {
        self.rules.iter().any(FilterRule::requires_file_kind)
    }

    /// Evaluate the filter against a real filesystem path that is a file.
    ///
    /// Returns `true` when the file should be kept:
    /// - `Include` decision → kept.
    /// - `Exclude` decision → dropped.
    /// - `Neutral` decision → kept **unless** the filter contains at least one
    ///   Include rule for files, in which case "no rule matched" means the file
    ///   was not whitelisted and is therefore dropped.
    pub fn matches_file(&self, path: &Path) -> bool {
        let context = FilterEntryContext {
            path,
            is_dir: false,
            size: None,
            modified: None,
            file_kind: None,
            resolved_path: None,
        };
        let decision =
            self.decision_for_entry_with_options(&context, &FilterMatchOptions::default());
        match decision {
            FilterDecision::Include => true,
            FilterDecision::Exclude => false,
            FilterDecision::Neutral => !self.has_include_rule_for(false),
        }
    }

    /// Evaluate the filter against a real filesystem path that is a directory.
    ///
    /// Returns `true` when the directory should be kept:
    /// - `Include` decision → kept.
    /// - `Exclude` decision → dropped.
    /// - `Neutral` decision → kept **unless** the filter contains at least one
    ///   Include rule for directories.
    pub fn matches_dir(&self, path: &Path) -> bool {
        let context = FilterEntryContext {
            path,
            is_dir: true,
            size: None,
            modified: None,
            file_kind: None,
            resolved_path: None,
        };
        let decision =
            self.decision_for_entry_with_options(&context, &FilterMatchOptions::default());
        match decision {
            FilterDecision::Include => true,
            FilterDecision::Exclude => false,
            FilterDecision::Neutral => !self.has_include_rule_for(true),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterRule {
    pub target: FilterTarget,
    pub action: FilterAction,
    pub syntax: PatternSyntax,
    pub pattern: String,
}

impl FilterRule {
    pub fn matches(&self, path: &Path) -> bool {
        self.matches_with_options(path, &FilterMatchOptions::default())
    }

    pub fn matches_with_options(&self, path: &Path, options: &FilterMatchOptions) -> bool {
        let value = path.to_string_lossy();
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_else(|| value.clone());
        match self.syntax {
            PatternSyntax::Wildcard => {
                let pattern = if options.case_sensitive {
                    self.pattern.clone()
                } else {
                    self.pattern.to_lowercase()
                };
                let value = if options.case_sensitive {
                    value.into_owned()
                } else {
                    value.to_lowercase()
                };
                let file_name = if options.case_sensitive {
                    file_name.into_owned()
                } else {
                    file_name.to_lowercase()
                };
                wildcard_match(&pattern, &value) || wildcard_match(&pattern, &file_name)
            }
            PatternSyntax::Regex => regex_match(&self.pattern, &value, options.case_sensitive),
            PatternSyntax::Expression => false,
        }
    }

    pub fn matches_entry_with_options(
        &self,
        context: &FilterEntryContext<'_>,
        options: &FilterMatchOptions,
    ) -> bool {
        match self.syntax {
            PatternSyntax::Wildcard | PatternSyntax::Regex => {
                self.matches_with_options(context.path, options)
            }
            PatternSyntax::Expression => expression_match(&self.pattern, context, options),
        }
    }

    pub fn requires_file_kind(&self) -> bool {
        if self.syntax != PatternSyntax::Expression {
            return false;
        }

        parse_filter_expression(&self.pattern)
            .is_ok_and(|expression| expression.attribute == ExpressionAttribute::FileKind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterEntryContext<'a> {
    pub path: &'a Path,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub file_kind: Option<FilterFileKind>,
    /// A filesystem-resolvable path for `path` when `path` itself is relative
    /// (the folder walk matches rules against the relative path but needs a
    /// real path to stat / recursively size a directory). `None` falls back to
    /// `path`.
    pub resolved_path: Option<&'a Path>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterFileKind {
    Text,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterMatchOptions {
    pub case_sensitive: bool,
}

impl Default for FilterMatchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterTarget {
    File,
    Directory,
    /// Matches both files and directories (used by `e:`/`e!:` any-expression rules).
    Any,
}

impl FilterTarget {
    fn matches(self, is_dir: bool) -> bool {
        match self {
            FilterTarget::File => !is_dir,
            FilterTarget::Directory => is_dir,
            FilterTarget::Any => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterAction {
    Include,
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternSyntax {
    Wildcard,
    #[serde(alias = "regex_like")]
    Regex,
    Expression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterDecision {
    Include,
    Exclude,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterParseError {
    pub line: usize,
    pub message: String,
    #[doc = "Programmatic kind for diagnostics; surfaces migration guidance for legacy/Windows-only prefixes."]
    pub kind: FilterParseErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterParseErrorKind {
    InvalidSyntax,
    EmptyPattern,
    UnknownPrefix,
    UnsupportedLegacyExpression,
    UnsupportedWindowsMetadata,
    InvalidRegex,
    InvalidExpression,
}

impl FilterParseError {
    pub fn is_migration_hint(&self) -> bool {
        matches!(
            self.kind,
            FilterParseErrorKind::UnsupportedLegacyExpression
                | FilterParseErrorKind::UnsupportedWindowsMetadata
        )
    }
}

impl std::fmt::Display for FilterParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for FilterParseError {}

fn parse_rule(line: &str, line_number: usize) -> Result<FilterRule, FilterParseError> {
    let (prefix, pattern) = line.split_once(':').ok_or_else(|| FilterParseError {
        line: line_number,
        message: "expected a rule prefix such as f:, f!:, d:, d!:, wf:, or wd:".to_owned(),
        kind: FilterParseErrorKind::InvalidSyntax,
    })?;
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err(FilterParseError {
            line: line_number,
            message: "filter pattern cannot be empty".to_owned(),
            kind: FilterParseErrorKind::EmptyPattern,
        });
    }

    let (target, action, syntax) = match prefix.trim() {
        "f" => (
            FilterTarget::File,
            FilterAction::Include,
            PatternSyntax::Regex,
        ),
        "f!" => (
            FilterTarget::File,
            FilterAction::Exclude,
            PatternSyntax::Regex,
        ),
        "d" => (
            FilterTarget::Directory,
            FilterAction::Include,
            PatternSyntax::Regex,
        ),
        "d!" => (
            FilterTarget::Directory,
            FilterAction::Exclude,
            PatternSyntax::Regex,
        ),
        "wf" => (
            FilterTarget::File,
            FilterAction::Include,
            PatternSyntax::Wildcard,
        ),
        "wf!" => (
            FilterTarget::File,
            FilterAction::Exclude,
            PatternSyntax::Wildcard,
        ),
        "wd" => (
            FilterTarget::Directory,
            FilterAction::Include,
            PatternSyntax::Wildcard,
        ),
        "wd!" => (
            FilterTarget::Directory,
            FilterAction::Exclude,
            PatternSyntax::Wildcard,
        ),
        "fe" => (
            FilterTarget::File,
            FilterAction::Include,
            PatternSyntax::Expression,
        ),
        "fe!" => (
            FilterTarget::File,
            FilterAction::Exclude,
            PatternSyntax::Expression,
        ),
        "de" => (
            FilterTarget::Directory,
            FilterAction::Include,
            PatternSyntax::Expression,
        ),
        "de!" => (
            FilterTarget::Directory,
            FilterAction::Exclude,
            PatternSyntax::Expression,
        ),
        "e" => (
            FilterTarget::Any,
            FilterAction::Include,
            PatternSyntax::Expression,
        ),
        "e!" => (
            FilterTarget::Any,
            FilterAction::Exclude,
            PatternSyntax::Expression,
        ),
        other => {
            if let Some((message, kind)) = unsupported_legacy_prefix(other) {
                return Err(FilterParseError {
                    line: line_number,
                    message,
                    kind,
                });
            }
            return Err(FilterParseError {
                line: line_number,
                message: format!("unknown filter rule prefix '{other}'"),
                kind: FilterParseErrorKind::UnknownPrefix,
            });
        }
    };

    if syntax == PatternSyntax::Regex {
        RegexBuilder::new(pattern)
            .build()
            .map_err(|err| FilterParseError {
                line: line_number,
                message: format!("invalid regex pattern: {err}"),
                kind: FilterParseErrorKind::InvalidRegex,
            })?;
    } else if syntax == PatternSyntax::Expression {
        parse_filter_expression(pattern).map_err(|message| FilterParseError {
            line: line_number,
            message,
            kind: FilterParseErrorKind::InvalidExpression,
        })?;
    }

    Ok(FilterRule {
        target,
        action,
        syntax,
        pattern: pattern.to_owned(),
    })
}

fn unsupported_legacy_prefix(prefix: &str) -> Option<(String, FilterParseErrorKind)> {
    let prefix = prefix.trim();
    if matches!(
        prefix,
        "attr"
            | "attr!"
            | "dos"
            | "dos!"
            | "ctime"
            | "ctime!"
            | "version"
            | "version!"
            | "shell"
            | "shell!"
    ) {
        return Some((
            format!(
                "Windows-only metadata prefix '{prefix}:' is not portable to Linux; see docs/linux-metadata-mapping.md for supported replacements"
            ),
            FilterParseErrorKind::UnsupportedWindowsMetadata,
        ));
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpressionAttribute {
    RelativePath,
    Name,
    Basename,
    Extension,
    FileKind,
    Size,
    ModifiedMs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpressionOperator {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExpressionValue {
    FileKind(FilterFileKind),
    Number(u128),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilterExpression {
    attribute: ExpressionAttribute,
    operator: ExpressionOperator,
    value: ExpressionValue,
}

fn parse_filter_expression(input: &str) -> Result<FilterExpression, String> {
    let (attribute, operator, value) = split_expression(input)?;
    let attribute = parse_expression_attribute(attribute)?;
    let operator = parse_expression_operator(operator);
    let value = parse_expression_value(attribute, operator, value)?;

    Ok(FilterExpression {
        attribute,
        operator,
        value,
    })
}

fn split_expression(input: &str) -> Result<(&str, &str, &str), String> {
    for operator in [">=", "<=", "==", "!=", ">", "<"] {
        if let Some((attribute, value)) = input.split_once(operator) {
            let attribute = attribute.trim();
            let value = value.trim();
            if attribute.is_empty() || value.is_empty() {
                break;
            }
            return Ok((attribute, operator, value));
        }
    }

    Err("filter expression must use ATTRIBUTE OPERATOR VALUE, for example 'size >= 1024' or 'type == text'".to_owned())
}

fn parse_expression_attribute(value: &str) -> Result<ExpressionAttribute, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "path" | "relative_path" | "relative-path" => Ok(ExpressionAttribute::RelativePath),
        "name" | "filename" | "file_name" | "file-name" => Ok(ExpressionAttribute::Name),
        "basename" | "base_name" | "base-name" => Ok(ExpressionAttribute::Basename),
        "extension" | "ext" => Ok(ExpressionAttribute::Extension),
        "type" | "file_type" | "kind" => Ok(ExpressionAttribute::FileKind),
        "size" | "size_bytes" => Ok(ExpressionAttribute::Size),
        "modified_ms" | "mtime_ms" | "timestamp_ms" | "mtime" => {
            Ok(ExpressionAttribute::ModifiedMs)
        }
        other => Err(format!(
            "unsupported filter expression attribute '{other}'; expected path, name, basename, extension, type, size, modified_ms, or mtime"
        )),
    }
}

fn parse_expression_operator(value: &str) -> ExpressionOperator {
    match value {
        "==" => ExpressionOperator::Eq,
        "!=" => ExpressionOperator::Ne,
        ">" => ExpressionOperator::Gt,
        ">=" => ExpressionOperator::Ge,
        "<" => ExpressionOperator::Lt,
        "<=" => ExpressionOperator::Le,
        _ => unreachable!("split_expression only returns known operators"),
    }
}

fn parse_expression_value(
    attribute: ExpressionAttribute,
    operator: ExpressionOperator,
    value: &str,
) -> Result<ExpressionValue, String> {
    match attribute {
        ExpressionAttribute::RelativePath
        | ExpressionAttribute::Name
        | ExpressionAttribute::Basename
        | ExpressionAttribute::Extension => {
            if !matches!(operator, ExpressionOperator::Eq | ExpressionOperator::Ne) {
                return Err(
                    "path, name, basename, and extension filter expressions only support == and !="
                        .to_owned(),
                );
            }
            Ok(ExpressionValue::Text(parse_text_value(value)))
        }
        ExpressionAttribute::FileKind => {
            if !matches!(operator, ExpressionOperator::Eq | ExpressionOperator::Ne) {
                return Err("type filter expressions only support == and !=".to_owned());
            }
            match value.trim().to_ascii_lowercase().as_str() {
                "text" => Ok(ExpressionValue::FileKind(FilterFileKind::Text)),
                "binary" => Ok(ExpressionValue::FileKind(FilterFileKind::Binary)),
                other => Err(format!(
                    "unsupported file type '{other}'; expected text or binary"
                )),
            }
        }
        ExpressionAttribute::Size => parse_byte_count(value).map(ExpressionValue::Number),
        ExpressionAttribute::ModifiedMs => parse_modified_ms(value),
    }
}

fn parse_text_value(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
        .to_owned()
}

fn parse_byte_count(input: &str) -> Result<u128, String> {
    let trimmed = input.trim();
    let digits_len = trimmed
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(trimmed.len());
    if digits_len == 0 {
        return Err("size filter values must start with a byte count".to_owned());
    }

    let number = trimmed[..digits_len]
        .parse::<u128>()
        .map_err(|_| "size filter byte count is too large".to_owned())?;
    let suffix = trimmed[digits_len..].trim().to_ascii_lowercase();
    let multiplier = match suffix.as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        other => {
            return Err(format!(
                "unsupported size suffix '{other}'; expected B, KB, MB, or GB"
            ));
        }
    };

    number
        .checked_mul(multiplier)
        .ok_or_else(|| "size filter byte count is too large".to_owned())
}

/// Parse a `modified_ms` / `mtime` value from either a raw integer (Unix epoch ms)
/// or an ISO-8601 date string like `'2020-01-01'` or `2020-01-01`.
fn parse_modified_ms(value: &str) -> Result<ExpressionValue, String> {
    let trimmed = value.trim();
    // Strip optional surrounding quotes.
    let inner = trimmed
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
        .unwrap_or(trimmed);

    // Try plain integer first.
    if let Ok(n) = inner.parse::<u128>() {
        return Ok(ExpressionValue::Number(n));
    }

    // Try YYYY-MM-DD.
    if let Some(ms) = parse_date_ms(inner) {
        return Ok(ExpressionValue::Number(ms));
    }

    Err("modified_ms / mtime filter values must be Unix epoch milliseconds or an ISO date like '2020-01-01'".to_owned())
}

/// Parse `YYYY-MM-DD` to Unix epoch milliseconds (midnight UTC).
fn parse_date_ms(s: &str) -> Option<u128> {
    let parts: Vec<&str> = s.splitn(3, '-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i64 = parts[0].parse().ok()?;
    let month: u8 = parts[1].parse().ok()?;
    let day: u8 = parts[2].parse().ok()?;

    if !(1..=12).contains(&month) {
        return None;
    }

    // Validate day-of-month based on month and leap year.
    let is_leap = year % 400 == 0 || (year % 4 == 0 && year % 100 != 0);
    let max_day = match month {
        2 => {
            if is_leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=max_day).contains(&day) {
        return None;
    }

    // Days since Unix epoch (1970-01-01) using a simple proleptic Gregorian calculation.
    let days = days_since_epoch(year, month, day)?;
    Some(days as u128 * 86_400_000)
}

/// Number of days from 1970-01-01 to the given date (must be >= 1970-01-01).
fn days_since_epoch(year: i64, month: u8, day: u8) -> Option<u64> {
    // Julian Day Number helper — works for the Gregorian calendar. Computed in
    // i128 so an extreme but i64-parseable `year` cannot overflow `365 * y2`
    // (which panics in debug builds and wraps to a garbage epoch in release).
    fn jdn(y: i64, m: u8, d: u8) -> i128 {
        let y = i128::from(y);
        let m = i128::from(m);
        let d = i128::from(d);
        let a = (14 - m) / 12;
        let y2 = y + 4800 - a;
        let m2 = m + 12 * a - 3;
        d + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045
    }
    // JDN of 1970-01-01.
    const EPOCH_JDN: i128 = 2_440_588;
    let diff = jdn(year, month, day) - EPOCH_JDN;
    if diff < 0 {
        None
    } else {
        u64::try_from(diff).ok()
    }
}

/// Recursively sum the sizes of all files under `dir`.  Symlinks are not
/// followed; errors on individual entries are silently skipped.
/// Maximum recursion depth for the `size` filter expression's recursive
/// directory walk. Prevents runaway walks on extremely deep trees.
const DIR_SIZE_MAX_DEPTH: usize = 64;

fn dir_size_recursive(dir: &std::path::Path, depth: usize) -> u64 {
    if depth > DIR_SIZE_MAX_DEPTH {
        return 0;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in read_dir.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            total = total.saturating_add(dir_size_recursive(&path, depth + 1));
        } else if file_type.is_file()
            && let Ok(meta) = entry.metadata()
        {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

fn expression_match(
    pattern: &str,
    context: &FilterEntryContext<'_>,
    options: &FilterMatchOptions,
) -> bool {
    let Ok(expression) = parse_filter_expression(pattern) else {
        return false;
    };

    match (expression.attribute, expression.value) {
        (
            ExpressionAttribute::RelativePath
            | ExpressionAttribute::Name
            | ExpressionAttribute::Basename
            | ExpressionAttribute::Extension,
            ExpressionValue::Text(expected),
        ) => expression_text_value(context, expression.attribute)
            .is_some_and(|actual| compare_text(actual, &expected, expression.operator, options)),
        (ExpressionAttribute::FileKind, ExpressionValue::FileKind(expected)) => context
            .file_kind
            .is_some_and(|actual| compare_file_kind(actual, expected, expression.operator)),
        (ExpressionAttribute::Size, ExpressionValue::Number(expected)) => {
            // A directory's `size` expression should reflect the recursive
            // content size, never the directory inode size (~4096 bytes) that
            // the folder walk records in `context.size`. So for directories the
            // recursive walk always wins over the cached inode size.
            let resolvable = context.resolved_path.unwrap_or(context.path);
            let actual = if context.is_dir {
                dir_size_recursive(resolvable, 0)
            } else if let Some(s) = context.size {
                s
            } else {
                match std::fs::metadata(resolvable) {
                    Ok(m) => m.len(),
                    Err(_) => return false,
                }
            };
            compare_number(u128::from(actual), expected, expression.operator)
        }
        (ExpressionAttribute::ModifiedMs, ExpressionValue::Number(expected)) => {
            let actual_ms = if let Some(ms) = context.modified.and_then(system_time_millis) {
                ms
            } else {
                match std::fs::metadata(context.path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(system_time_millis)
                {
                    Some(ms) => ms,
                    None => return false,
                }
            };
            compare_number(actual_ms, expected, expression.operator)
        }
        _ => false,
    }
}

fn expression_text_value(
    context: &FilterEntryContext<'_>,
    attribute: ExpressionAttribute,
) -> Option<String> {
    match attribute {
        ExpressionAttribute::RelativePath => Some(context.path.to_string_lossy().into_owned()),
        ExpressionAttribute::Name => context
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
        ExpressionAttribute::Basename => context
            .path
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned()),
        ExpressionAttribute::Extension => context
            .path
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned()),
        ExpressionAttribute::FileKind
        | ExpressionAttribute::Size
        | ExpressionAttribute::ModifiedMs => None,
    }
}

fn compare_text(
    actual: String,
    expected: &str,
    operator: ExpressionOperator,
    options: &FilterMatchOptions,
) -> bool {
    let actual = if options.case_sensitive {
        actual
    } else {
        actual.to_lowercase()
    };
    let expected = if options.case_sensitive {
        expected.to_owned()
    } else {
        expected.to_lowercase()
    };

    match operator {
        ExpressionOperator::Eq => actual == expected,
        ExpressionOperator::Ne => actual != expected,
        ExpressionOperator::Gt
        | ExpressionOperator::Ge
        | ExpressionOperator::Lt
        | ExpressionOperator::Le => false,
    }
}

fn compare_file_kind(
    actual: FilterFileKind,
    expected: FilterFileKind,
    operator: ExpressionOperator,
) -> bool {
    match operator {
        ExpressionOperator::Eq => actual == expected,
        ExpressionOperator::Ne => actual != expected,
        ExpressionOperator::Gt
        | ExpressionOperator::Ge
        | ExpressionOperator::Lt
        | ExpressionOperator::Le => false,
    }
}

fn compare_number(actual: u128, expected: u128, operator: ExpressionOperator) -> bool {
    match operator {
        ExpressionOperator::Eq => actual == expected,
        ExpressionOperator::Ne => actual != expected,
        ExpressionOperator::Gt => actual > expected,
        ExpressionOperator::Ge => actual >= expected,
        ExpressionOperator::Lt => actual < expected,
        ExpressionOperator::Le => actual <= expected,
    }
}

fn system_time_millis(time: SystemTime) -> Option<u128> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

// Thread-local cache of compiled filter regexes, keyed by (pattern,
// case_sensitive). The folder walk calls `regex_match` once per directory
// entry × per regex filter rule — without this cache, a 100k-entry tree with
// one regex filter recompiles the regex ~100k times (each compile is
// µs–ms). The cache is capped so dynamically-generated or adversarial
// rule sets cannot grow memory without bound.
const FILTER_REGEX_CACHE_MAX_SIZE: usize = 128;

thread_local! {
    static FILTER_REGEX_CACHE:
        RefCell<HashMap<(String, bool), Option<Regex>>> =
        RefCell::new(HashMap::new());
}

fn regex_match(pattern: &str, value: &str, case_sensitive: bool) -> bool {
    // Fast path: check the thread-local cache.
    let cached = FILTER_REGEX_CACHE.with(|cache| {
        let cache = cache.borrow();
        cache
            .get(&(pattern.to_owned(), case_sensitive))
            .map(|regex| regex.as_ref().map(|r| r.is_match(value)).unwrap_or(false))
    });
    if let Some(result) = cached {
        return result;
    }
    // Slow path: compile, cache, then match.
    let compiled = RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .ok();
    let result = compiled.as_ref().is_some_and(|regex| regex.is_match(value));
    FILTER_REGEX_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.insert((pattern.to_owned(), case_sensitive), compiled);
        // Hard cap: if the cache has grown past the limit, drop everything.
        // A real LRU would be nicer, but rule sets are normally tiny and a
        // full reset is simple and safe.
        if cache.len() > FILTER_REGEX_CACHE_MAX_SIZE {
            cache.clear();
        }
    });
    result
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    // Match on Unicode scalar values, not bytes, so `?` matches exactly one
    // character and literals compare per-character for multibyte UTF-8 names.
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();
    wildcard_match_inner(&pattern, &value)
}

fn wildcard_match_inner(pattern: &[char], value: &[char]) -> bool {
    let mut pattern_index = 0;
    let mut value_index = 0;
    let mut star_index = None;
    let mut match_index = 0;

    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star_index = Some(pattern_index);
            match_index = value_index;
            pattern_index += 1;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            match_index += 1;
            value_index = match_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

// ── Legacy .flt migration ────────────────────────────────────────────────────

/// The output of [`migrate_filter_text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratedFilter {
    /// The migrated filter text, ready to be written to disk or displayed to
    /// the user.  Lines that could be translated are rewritten; unsupported or
    /// unrecognised prefixes are commented out with an explanatory tag.
    pub migrated: String,
    /// Human-readable warnings for lines that were commented out or
    /// transformed in a lossy way.
    pub warnings: Vec<String>,
}

/// Translate a legacy WinMerge/ExamDiff `.flt` file into LinSync filter
/// syntax.
///
/// - Supported LinSync prefixes (`f:`, `d:`, `wf:`, …) are passed through
///   unchanged.
/// - `ctime:` expressions are rewritten as `e: mtime…` (closest Linux
///   equivalent).
/// - Windows-only prefixes (`attr:`, `dos:`, `shell:`, `version:`) are
///   commented out with `# UNSUPPORTED:`.
/// - Lines that cannot be classified at all are commented out with
///   `# UNRECOGNIZED:` and added to [`MigratedFilter::warnings`].
/// - Blank lines and existing comments (`#`) are preserved verbatim.
pub fn migrate_filter_text(text: &str) -> MigratedFilter {
    let mut out = String::new();
    let mut warnings = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        match classify_legacy_line(trimmed) {
            LegacyLineClass::Supported => {
                out.push_str(line);
                out.push('\n');
            }
            LegacyLineClass::Unsupported(reason) => {
                let warning = format!("unsupported line (commented out): {trimmed}  -- {reason}");
                warnings.push(warning);
                out.push_str("# UNSUPPORTED: ");
                out.push_str(trimmed);
                out.push_str("  -- ");
                out.push_str(reason);
                out.push('\n');
            }
            LegacyLineClass::MigratedToMtime(new_line) => {
                out.push_str(&new_line);
                out.push_str("  # migrated from ctime\n");
            }
            LegacyLineClass::Unrecognized => {
                let warning = format!("unrecognized line (commented out): {trimmed}");
                warnings.push(warning.clone());
                out.push_str("# UNRECOGNIZED: ");
                out.push_str(trimmed);
                out.push('\n');
            }
        }
    }

    MigratedFilter {
        migrated: out,
        warnings,
    }
}

enum LegacyLineClass {
    Supported,
    Unsupported(&'static str),
    MigratedToMtime(String),
    Unrecognized,
}

fn classify_legacy_line(line: &str) -> LegacyLineClass {
    // Supported prefixes: f: f!: d: d!: wf: wf!: wd: wd!: fe: fe!: de: de!: e: e!:
    // Also treat the `name:` header as supported (it is a LinSync directive, not a rule).
    const SUPPORTED: &[&str] = &[
        "name:", "f:", "f!:", "d:", "d!:", "wf:", "wf!:", "wd:", "wd!:", "fe:", "fe!:", "de:",
        "de!:", "e:", "e!:",
    ];
    for p in SUPPORTED {
        if line.starts_with(p) {
            return LegacyLineClass::Supported;
        }
    }
    if line.starts_with("attr:") || line.starts_with("attr!:") {
        return LegacyLineClass::Unsupported("Linux has no equivalent attribute");
    }
    if line.starts_with("dos:") || line.starts_with("dos!:") {
        return LegacyLineClass::Unsupported("DOS metadata not applicable to Linux");
    }
    if line.starts_with("shell:") || line.starts_with("shell!:") {
        return LegacyLineClass::Unsupported("Windows shell extensions not applicable to Linux");
    }
    if line.starts_with("version:") || line.starts_with("version!:") {
        return LegacyLineClass::Unsupported("Windows version metadata not applicable to Linux");
    }
    if let Some(rest) = line.strip_prefix("ctime:") {
        // ctime: <op> '<date>' → e: mtime <op> '<date>'
        return LegacyLineClass::MigratedToMtime(format!("e: mtime{rest}"));
    }
    LegacyLineClass::Unrecognized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Minimal temp-directory helper that cleans up on drop.
    struct TempDir {
        path: std::path::PathBuf,
    }

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    impl TempDir {
        fn new() -> Self {
            let seq = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let pid = std::process::id();
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("linsync-filter-test-{pid}-{ts}-{seq}"));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn context(
        path: &Path,
        size: Option<u64>,
        modified_ms: Option<u64>,
        file_kind: Option<FilterFileKind>,
    ) -> FilterEntryContext<'_> {
        FilterEntryContext {
            path,
            is_dir: false,
            size,
            modified: modified_ms
                .map(|millis| std::time::UNIX_EPOCH + std::time::Duration::from_millis(millis)),
            file_kind,
            resolved_path: None,
        }
    }

    #[test]
    fn parses_filter_rules_and_applies_last_match() {
        let filter = FileFilter::parse(
            r#"
name: Source
wf:*.rs
wf!:target/*
wd!:target
"#,
        )
        .unwrap();

        assert_eq!(filter.name.as_deref(), Some("Source"));
        assert_eq!(
            filter.decision_for_path(Path::new("src/main.rs"), false),
            FilterDecision::Include
        );
        assert_eq!(
            filter.decision_for_path(Path::new("target/main.rs"), false),
            FilterDecision::Exclude
        );
        assert_eq!(
            filter.decision_for_path(Path::new("target"), true),
            FilterDecision::Exclude
        );
    }

    #[test]
    fn supports_include_and_exclude_file_and_directory_patterns() {
        let filter = FileFilter::parse(
            r#"
wf:*.rs
wf!:generated.rs
wd:src
wd!:target
"#,
        )
        .unwrap();

        assert_eq!(
            filter.decision_for_path(Path::new("src/main.rs"), false),
            FilterDecision::Include
        );
        assert_eq!(
            filter.decision_for_path(Path::new("generated.rs"), false),
            FilterDecision::Exclude
        );
        assert_eq!(
            filter.decision_for_path(Path::new("src"), true),
            FilterDecision::Include
        );
        assert_eq!(
            filter.decision_for_path(Path::new("target"), true),
            FilterDecision::Exclude
        );
        assert_eq!(
            filter.decision_for_path(Path::new("target"), false),
            FilterDecision::Neutral
        );
    }

    #[test]
    fn built_in_generated_filter_excludes_common_output_directories() {
        let filter = FileFilter::generated_directories();

        for path in [
            ".git",
            "node_modules",
            "target",
            "build",
            "dist",
            ".cache",
            "vendor",
            "bin",
            "obj",
            "/proc",
            "proc",
            "/sys",
            "sys",
            "/dev",
            "dev",
            "/run",
            "run",
        ] {
            assert_eq!(
                filter.decision_for_path(Path::new(path), true),
                FilterDecision::Exclude,
                "{path} should be excluded"
            );
        }

        assert_eq!(
            filter.decision_for_path(Path::new("src"), true),
            FilterDecision::Neutral
        );
    }

    #[test]
    fn supports_case_sensitive_and_case_insensitive_matching() {
        let filter = FileFilter::parse(
            r#"
wf:*.RS
f!:Generated
"#,
        )
        .unwrap();

        assert_eq!(
            filter.decision_for_path(Path::new("src/main.rs"), false),
            FilterDecision::Neutral
        );
        assert_eq!(
            filter.decision_for_path_with_options(
                Path::new("src/main.rs"),
                false,
                &FilterMatchOptions {
                    case_sensitive: false,
                },
            ),
            FilterDecision::Include
        );
        assert_eq!(
            filter.decision_for_path_with_options(
                Path::new("generated.rs"),
                false,
                &FilterMatchOptions {
                    case_sensitive: false,
                },
            ),
            FilterDecision::Exclude
        );
    }

    #[test]
    fn supports_validated_regex_file_and_directory_rules() {
        let filter = FileFilter::parse(
            r#"
f:^src/.+\.rs$
f!:generated-\d+\.rs$
d:^target$
d!:^target/tmp$
"#,
        )
        .unwrap();

        assert_eq!(
            filter.decision_for_path(Path::new("src/main.rs"), false),
            FilterDecision::Include
        );
        assert_eq!(
            filter.decision_for_path(Path::new("generated-123.rs"), false),
            FilterDecision::Exclude
        );
        assert_eq!(
            filter.decision_for_path(Path::new("target"), true),
            FilterDecision::Include
        );
        assert_eq!(
            filter.decision_for_path(Path::new("target/tmp"), true),
            FilterDecision::Exclude
        );
    }

    #[test]
    fn supports_file_kind_size_and_timestamp_expressions() {
        let text_filter = FileFilter::parse("fe:type == text").unwrap();
        let binary_filter = FileFilter::parse("fe!:type == binary").unwrap();
        let size_filter = FileFilter::parse("fe:size >= 1KB").unwrap();
        let timestamp_filter = FileFilter::parse("fe:modified_ms < 2000").unwrap();

        assert_eq!(
            text_filter.decision_for_entry_with_options(
                &context(
                    Path::new("notes.txt"),
                    Some(12),
                    Some(1500),
                    Some(FilterFileKind::Text),
                ),
                &FilterMatchOptions::default(),
            ),
            FilterDecision::Include
        );
        assert_eq!(
            binary_filter.decision_for_entry_with_options(
                &context(
                    Path::new("image.bin"),
                    Some(12),
                    Some(1500),
                    Some(FilterFileKind::Binary),
                ),
                &FilterMatchOptions::default(),
            ),
            FilterDecision::Exclude
        );
        assert_eq!(
            size_filter.decision_for_entry_with_options(
                &context(Path::new("large.txt"), Some(1024), Some(1500), None),
                &FilterMatchOptions::default(),
            ),
            FilterDecision::Include
        );
        assert_eq!(
            timestamp_filter.decision_for_entry_with_options(
                &context(Path::new("old.txt"), Some(12), Some(1500), None),
                &FilterMatchOptions::default(),
            ),
            FilterDecision::Include
        );
    }

    #[test]
    fn rejects_invalid_file_expressions() {
        let err = FileFilter::parse("fe:type > text").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("only support == and !="));

        let err = FileFilter::parse("fe:size > soon").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("byte count"));

        let err = FileFilter::parse("fe:owner == me").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(
            err.message
                .contains("unsupported filter expression attribute")
        );
    }

    #[test]
    fn rejects_invalid_regex_patterns() {
        let err = FileFilter::parse("f:[unterminated").unwrap_err();

        assert_eq!(err.line, 1);
        assert!(err.message.contains("invalid regex pattern"));
    }

    #[test]
    fn accepts_legacy_regex_like_filter_syntax_name() {
        let rule: FilterRule = serde_json::from_str(
            r#"{"target":"file","action":"include","syntax":"regex_like","pattern":"\\.rs$"}"#,
        )
        .unwrap();

        assert_eq!(rule.syntax, PatternSyntax::Regex);
    }

    #[test]
    fn rejects_unknown_prefixes() {
        let err = FileFilter::parse("x:*.rs").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("unknown"));
    }

    #[test]
    fn de_prefix_parses_size_and_date_expression() {
        let f = FileFilter::parse("de: size > 1024 AND mtime < '2026-01-01'").unwrap_err();
        // AND-chained expressions are not yet supported by the single-expression parser;
        // the parse error should be InvalidExpression, not UnsupportedLegacyExpression.
        assert_eq!(f.kind, FilterParseErrorKind::InvalidExpression);

        let f = FileFilter::parse("de: size > 1024").unwrap();
        assert_eq!(f.rules[0].target, FilterTarget::Directory);
        assert_eq!(f.rules[0].action, FilterAction::Include);
        assert_eq!(f.rules[0].syntax, PatternSyntax::Expression);
    }

    #[test]
    fn de_negation_parses() {
        let f = FileFilter::parse("de!: size > 1048576").unwrap();
        assert_eq!(f.rules[0].target, FilterTarget::Directory);
        assert_eq!(f.rules[0].action, FilterAction::Exclude);
        assert_eq!(f.rules[0].syntax, PatternSyntax::Expression);
    }

    #[test]
    fn e_prefix_parses() {
        let f = FileFilter::parse("e: size > 0").unwrap();
        assert_eq!(f.rules[0].target, FilterTarget::Any);
        assert_eq!(f.rules[0].action, FilterAction::Include);
        assert_eq!(f.rules[0].syntax, PatternSyntax::Expression);
    }

    #[test]
    fn e_negation_parses() {
        let f = FileFilter::parse("e!: size > 0").unwrap();
        assert_eq!(f.rules[0].target, FilterTarget::Any);
        assert_eq!(f.rules[0].action, FilterAction::Exclude);
        assert_eq!(f.rules[0].syntax, PatternSyntax::Expression);
    }

    #[test]
    fn de_prefix_rejects_unknown_attribute() {
        let err = FileFilter::parse("de: chocolate > 1").unwrap_err();
        assert_eq!(err.kind, FilterParseErrorKind::InvalidExpression);
    }

    #[test]
    fn windows_only_metadata_prefixes_still_rejected() {
        for prefix in [
            "attr", "attr!", "dos", "dos!", "ctime", "ctime!", "version", "version!", "shell",
            "shell!",
        ] {
            let input = format!("{prefix}: foo");
            let err = FileFilter::parse(&input).unwrap_err();
            assert_eq!(
                err.kind,
                FilterParseErrorKind::UnsupportedWindowsMetadata,
                "{prefix}: should return UnsupportedWindowsMetadata"
            );
        }
    }

    #[test]
    fn explains_windows_only_metadata_prefixes() {
        let err = FileFilter::parse("attr: archive").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("Windows-only metadata"));
        assert!(err.message.contains("docs/linux-metadata-mapping.md"));
    }

    #[test]
    fn wildcard_supports_star_and_question_mark() {
        assert!(wildcard_match("*.rs", "main.rs"));
        assert!(wildcard_match("file-?.txt", "file-a.txt"));
        assert!(!wildcard_match("file-?.txt", "file-ab.txt"));
    }

    #[test]
    fn wildcard_matches_per_character_for_non_ascii_names() {
        // `?` must match exactly one Unicode scalar value, not one UTF-8 byte.
        // "café" and "naïve" each contain a multibyte char (é = 2 bytes,
        // ï = 2 bytes) that a byte-wise `?` would mishandle.
        assert!(wildcard_match("caf?", "café"));
        assert!(wildcard_match("na?ve", "naïve"));
        // A single `?` must not match the two bytes of a multibyte char.
        assert!(!wildcard_match("caf?", "cafée"));

        // Literal comparison is per-character, so a multibyte literal pattern
        // matches its identical multibyte value and rejects a different one.
        assert!(wildcard_match("café.txt", "café.txt"));
        assert!(!wildcard_match("café.txt", "cafe.txt"));

        // `*` semantics are unchanged across multibyte content.
        assert!(wildcard_match("*é.txt", "résumé.txt"));
        assert!(wildcard_match("caf*", "café"));
    }

    #[test]
    fn de_excludes_large_dirs() {
        // de!: size > 10 MB — directory whose recursive file content exceeds the
        // threshold should be excluded; one that stays under should be kept.
        let f = FileFilter::parse("de!: size > 10485760").unwrap();

        let small_dir = TempDir::new();
        // Write a 1-byte sentinel so the directory is not completely empty.
        std::fs::write(small_dir.path().join("tiny"), b"x").unwrap();

        let big_dir = TempDir::new();
        std::fs::write(big_dir.path().join("blob"), vec![0u8; 11 * 1024 * 1024]).unwrap();

        assert!(f.matches_dir(small_dir.path()), "small dir should be kept");
        assert!(!f.matches_dir(big_dir.path()), "big dir should be excluded");
    }

    #[test]
    fn dir_size_expression_uses_recursive_content_not_inode_size() {
        // The folder walk records the directory inode size (~4096 bytes) in
        // `context.size`.  A `size` expression on a directory must ignore that
        // and use the recursive content size instead.
        let f = FileFilter::parse("de!: size > 10485760").unwrap();
        let options = FilterMatchOptions::default();

        let big_dir = TempDir::new();
        std::fs::write(big_dir.path().join("blob"), vec![0u8; 11 * 1024 * 1024]).unwrap();

        // Simulate the production context: a populated inode size that, on its
        // own, would stay under the 10 MB threshold and wrongly keep the dir.
        let inode_sized_context = FilterEntryContext {
            path: big_dir.path(),
            is_dir: true,
            size: Some(4096),
            modified: None,
            file_kind: None,
            resolved_path: None,
        };
        assert_eq!(
            f.decision_for_entry_with_options(&inode_sized_context, &options),
            FilterDecision::Exclude,
            "directory should be excluded based on 11 MB of recursive content, \
             not the 4096-byte inode size"
        );

        let small_dir = TempDir::new();
        std::fs::write(small_dir.path().join("tiny"), b"x").unwrap();
        let small_context = FilterEntryContext {
            path: small_dir.path(),
            is_dir: true,
            size: Some(4096),
            modified: None,
            file_kind: None,
            resolved_path: None,
        };
        assert_eq!(
            f.decision_for_entry_with_options(&small_context, &options),
            FilterDecision::Neutral,
            "small directory should not match the exclude rule"
        );
    }

    #[test]
    fn fe_excludes_recent_files() {
        // Exclude files modified after 2020-01-01.  Any file created right now
        // during the test run is well past that date.
        let f = FileFilter::parse("fe!: mtime > '2020-01-01'").unwrap();

        let dir = TempDir::new();
        let path = dir.path().join("recent.txt");
        std::fs::write(&path, "hi").unwrap();

        assert!(!f.matches_file(&path), "recent file should be excluded");
    }

    #[test]
    fn e_keeps_files_and_dirs_matching_any_expression() {
        // e: size > 0  — keep entries whose size is non-zero.
        let f = FileFilter::parse("e: size > 0").unwrap();

        let dir = TempDir::new();
        let empty = dir.path().join("empty.txt");
        std::fs::write(&empty, "").unwrap();
        let nonempty = dir.path().join("hello.txt");
        std::fs::write(&nonempty, "hi").unwrap();

        // The filter has one Include rule.  A file that matches it gets an
        // Include decision (kept).  A file that does not match gets Neutral;
        // because the filter has at least one Include rule for the target,
        // Neutral means "not whitelisted" and matches_file returns false.
        assert!(!f.matches_file(&empty), "empty file should not be included");
        assert!(
            f.matches_file(&nonempty),
            "nonempty file should be included"
        );
    }

    #[test]
    fn date_validation_rejects_invalid_days_in_month() {
        // Feb 30 does not exist in any year.
        let err = FileFilter::parse("fe: mtime > '2020-02-30'").unwrap_err();
        assert_eq!(err.kind, FilterParseErrorKind::InvalidExpression);

        // Feb 29 exists in leap year 2020.
        let f = FileFilter::parse("fe: mtime > '2020-02-29'").unwrap();
        assert!(!f.rules.is_empty());

        // Feb 29 does not exist in non-leap year 2021.
        let err = FileFilter::parse("fe: mtime > '2021-02-29'").unwrap_err();
        assert_eq!(err.kind, FilterParseErrorKind::InvalidExpression);

        // April 31 does not exist.
        let err = FileFilter::parse("fe: mtime > '2020-04-31'").unwrap_err();
        assert_eq!(err.kind, FilterParseErrorKind::InvalidExpression);

        // November 31 does not exist.
        let err = FileFilter::parse("fe: mtime > '2020-11-31'").unwrap_err();
        assert_eq!(err.kind, FilterParseErrorKind::InvalidExpression);

        // Valid dates should parse successfully.
        let f = FileFilter::parse("fe: mtime > '2020-04-30'").unwrap();
        assert!(!f.rules.is_empty());

        let f = FileFilter::parse("fe: mtime > '2020-12-31'").unwrap();
        assert!(!f.rules.is_empty());
    }

    #[test]
    fn error_message_lists_mtime_in_unsupported_attribute() {
        let err = FileFilter::parse("fe: mtim > 0").unwrap_err();
        assert_eq!(err.kind, FilterParseErrorKind::InvalidExpression);
        assert!(
            err.message.contains("mtime"),
            "error message should mention 'mtime'; got: {}",
            err.message
        );
    }

    #[test]
    fn regex_cache_handles_more_patterns_than_cache_limit() {
        // Exercise the hard cap by compiling more patterns than the cache can hold.
        // The reset must be transparent: every pattern still matches correctly.
        for i in 0..=FILTER_REGEX_CACHE_MAX_SIZE {
            let pattern = format!("^{}$", i);
            let text = i.to_string();
            assert!(
                regex_match(&pattern, &text, true),
                "pattern {i} should match"
            );
        }
        // A pattern that was evicted still works when re-encountered.
        assert!(regex_match("^0$", "0", true));
    }

    #[test]
    fn dir_size_recursive_stops_at_max_depth() {
        let tmp = TempDir::new();
        let mut dir = tmp.path().to_path_buf();
        for _ in 0..70 {
            dir = dir.join("d");
            std::fs::create_dir_all(&dir).unwrap();
        }
        std::fs::write(dir.join("leaf.txt"), b"x").unwrap();
        let size = dir_size_recursive(tmp.path(), 0);
        assert_eq!(
            size, 0,
            "files below the max depth cap should not be counted"
        );
    }
}
