//! Lightweight syntax highlighting used by the text compare views.
//!
//! Produces [`SyntaxSpan`] lists (char-indexed) for a small set of built-in
//! languages, plus an HTML renderer that wraps spans in `syn-*` classes.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub(crate) fn escape_html(value: &str) -> String {
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextSyntaxMode {
    #[default]
    Plain,
    Auto,
    Rust,
    Json,
    Html,
    Markdown,
    Shell,
    Toml,
    Yaml,
    C,
    Cpp,
    Python,
    #[serde(rename = "javascript")]
    JavaScript,
    #[serde(rename = "typescript")]
    TypeScript,
    Go,
    Java,
    Css,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxSpan {
    pub start: usize,
    pub end: usize,
    pub class: String,
}

pub(crate) fn syntax_mode_from_path(path: &Path) -> Option<TextSyntaxMode> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "rs" => Some(TextSyntaxMode::Rust),
        "json" => Some(TextSyntaxMode::Json),
        "html" | "htm" | "xml" => Some(TextSyntaxMode::Html),
        "md" | "markdown" => Some(TextSyntaxMode::Markdown),
        "sh" | "bash" | "zsh" | "fish" => Some(TextSyntaxMode::Shell),
        "toml" => Some(TextSyntaxMode::Toml),
        "yaml" | "yml" => Some(TextSyntaxMode::Yaml),
        "c" | "h" => Some(TextSyntaxMode::C),
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => Some(TextSyntaxMode::Cpp),
        "py" => Some(TextSyntaxMode::Python),
        "js" | "mjs" | "jsx" => Some(TextSyntaxMode::JavaScript),
        "ts" | "tsx" => Some(TextSyntaxMode::TypeScript),
        "go" => Some(TextSyntaxMode::Go),
        "java" => Some(TextSyntaxMode::Java),
        "css" => Some(TextSyntaxMode::Css),
        _ => None,
    }
}

pub(crate) fn syntax_spans(text: &str, mode: TextSyntaxMode) -> Vec<SyntaxSpan> {
    #[cfg(feature = "syntax-rich")]
    if let Some(spans) = rich::spans(text, mode) {
        return spans;
    }
    match mode {
        TextSyntaxMode::Plain | TextSyntaxMode::Auto => Vec::new(),
        TextSyntaxMode::Json => json_syntax_spans(text),
        TextSyntaxMode::Html => html_syntax_spans(text),
        TextSyntaxMode::Rust
        | TextSyntaxMode::Markdown
        | TextSyntaxMode::Shell
        | TextSyntaxMode::Toml
        | TextSyntaxMode::Yaml
        | TextSyntaxMode::C
        | TextSyntaxMode::Cpp
        | TextSyntaxMode::Python
        | TextSyntaxMode::JavaScript
        | TextSyntaxMode::TypeScript
        | TextSyntaxMode::Go
        | TextSyntaxMode::Java
        | TextSyntaxMode::Css => generic_syntax_spans(text, mode),
    }
}

pub(crate) fn syntax_highlight_html(text: &str, mode: TextSyntaxMode) -> String {
    let spans = syntax_spans(text, mode);
    if spans.is_empty() {
        return escape_html(text);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::new();
    let mut cursor = 0usize;
    for span in spans {
        let start = span.start.min(chars.len());
        let end = span.end.min(chars.len());
        if start > cursor {
            output.push_str(&escape_html(
                &chars[cursor..start].iter().collect::<String>(),
            ));
        }
        let display_start = start.max(cursor);
        if end > display_start {
            output.push_str(&format!(
                "<span class=\"syn-{}\">{}</span>",
                span.class,
                escape_html(&chars[display_start..end].iter().collect::<String>())
            ));
        }
        cursor = end.max(cursor);
    }
    if cursor < chars.len() {
        output.push_str(&escape_html(&chars[cursor..].iter().collect::<String>()));
    }
    output
}

fn json_syntax_spans(text: &str) -> Vec<SyntaxSpan> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let class = if j < chars.len() && chars[j] == ':' {
                "key"
            } else {
                "string"
            };
            spans.push(span(start, i, class));
        } else if ch.is_ascii_digit() || ch == '-' {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | 'e' | 'E' | '+' | '-'))
            {
                i += 1;
            }
            spans.push(span(start, i, "number"));
        } else if starts_keyword(&chars, i, &["true", "false", "null"]) {
            let end = keyword_end(&chars, i);
            spans.push(span(i, end, "keyword"));
            i = end;
        } else {
            i += 1;
        }
    }
    spans
}

fn html_syntax_spans(text: &str) -> Vec<SyntaxSpan> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '<' {
            let start = i;
            while i < chars.len() && chars[i] != '>' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            spans.push(span(start, i, "tag"));
        } else {
            i += 1;
        }
    }
    spans
}

fn generic_syntax_spans(text: &str, mode: TextSyntaxMode) -> Vec<SyntaxSpan> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    if let Some(comment_start) = comment_start(&chars, mode) {
        spans.push(span(comment_start, chars.len(), "comment"));
    }
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            spans.push(span(start, i, "string"));
        } else if ch.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            spans.push(span(start, i, "number"));
        } else if ch.is_alphabetic() || ch == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if keyword_list(mode).contains(&word.as_str()) {
                spans.push(span(start, i, "keyword"));
            }
        } else {
            i += 1;
        }
    }
    spans.sort_by_key(|s| (s.start, s.end));
    spans
}

fn starts_keyword(chars: &[char], start: usize, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| {
        let len = candidate.chars().count();
        start + len <= chars.len()
            && chars[start..start + len].iter().collect::<String>() == *candidate
            && (start + len == chars.len()
                || !chars[start + len].is_alphanumeric() && chars[start + len] != '_')
    })
}

fn keyword_end(chars: &[char], start: usize) -> usize {
    let mut end = start;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }
    end
}

fn comment_start(chars: &[char], mode: TextSyntaxMode) -> Option<usize> {
    for i in 0..chars.len() {
        match mode {
            TextSyntaxMode::Rust
                if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/' =>
            {
                return Some(i);
            }
            TextSyntaxMode::Shell
            | TextSyntaxMode::Toml
            | TextSyntaxMode::Yaml
            | TextSyntaxMode::Markdown
                if chars[i] == '#' =>
            {
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

fn keyword_list(mode: TextSyntaxMode) -> &'static [&'static str] {
    match mode {
        TextSyntaxMode::Rust => &[
            "as", "async", "await", "break", "const", "continue", "crate", "else", "enum", "false",
            "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
            "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
            "unsafe", "use", "where", "while",
        ],
        TextSyntaxMode::Shell => &[
            "case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in",
            "then", "while",
        ],
        TextSyntaxMode::Toml | TextSyntaxMode::Yaml | TextSyntaxMode::Markdown => {
            &["true", "false", "null"]
        }
        _ => &[],
    }
}

fn span(start: usize, end: usize, class: &str) -> SyntaxSpan {
    SyntaxSpan {
        start,
        end,
        class: class.to_owned(),
    }
}

#[cfg(test)]
mod path_mode_tests {
    use super::*;

    #[track_caller]
    fn assert_ext(name: &str, expected: Option<TextSyntaxMode>) {
        assert_eq!(syntax_mode_from_path(Path::new(name)), expected, "{name}");
    }

    #[test]
    fn maps_original_extensions() {
        assert_ext("a.rs", Some(TextSyntaxMode::Rust));
        assert_ext("a.json", Some(TextSyntaxMode::Json));
        assert_ext("a.html", Some(TextSyntaxMode::Html));
        assert_ext("a.md", Some(TextSyntaxMode::Markdown));
        assert_ext("a.sh", Some(TextSyntaxMode::Shell));
        assert_ext("a.toml", Some(TextSyntaxMode::Toml));
        assert_ext("a.yml", Some(TextSyntaxMode::Yaml));
    }

    #[test]
    fn maps_python() {
        assert_ext("a.py", Some(TextSyntaxMode::Python));
    }

    #[test]
    fn maps_c_family() {
        assert_ext("a.c", Some(TextSyntaxMode::C));
        assert_ext("a.h", Some(TextSyntaxMode::C));
        assert_ext("a.cc", Some(TextSyntaxMode::Cpp));
        assert_ext("a.cpp", Some(TextSyntaxMode::Cpp));
        assert_ext("a.cxx", Some(TextSyntaxMode::Cpp));
        assert_ext("a.hpp", Some(TextSyntaxMode::Cpp));
        assert_ext("a.hh", Some(TextSyntaxMode::Cpp));
    }

    #[test]
    fn maps_javascript_and_typescript() {
        assert_ext("a.js", Some(TextSyntaxMode::JavaScript));
        assert_ext("a.mjs", Some(TextSyntaxMode::JavaScript));
        assert_ext("a.jsx", Some(TextSyntaxMode::JavaScript));
        assert_ext("a.ts", Some(TextSyntaxMode::TypeScript));
        assert_ext("a.tsx", Some(TextSyntaxMode::TypeScript));
    }

    #[test]
    fn maps_go_java_css() {
        assert_ext("a.go", Some(TextSyntaxMode::Go));
        assert_ext("a.java", Some(TextSyntaxMode::Java));
        assert_ext("a.css", Some(TextSyntaxMode::Css));
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        assert_ext("A.PY", Some(TextSyntaxMode::Python));
        assert_ext("A.CPP", Some(TextSyntaxMode::Cpp));
    }

    #[test]
    fn unknown_or_missing_extension_is_none() {
        assert_ext("a.unknownext", None);
        assert_ext("noext", None);
    }
}

/// syntect-backed span computation. Stateless per line by design: callers
/// highlight one row at a time, so multi-line constructs (block comments, raw
/// strings) degrade gracefully per line. Returns `None` when syntect cannot
/// handle the mode so the hand-rolled lexers above take over.
#[cfg(feature = "syntax-rich")]
mod rich {
    use std::sync::OnceLock;

    use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

    use super::{SyntaxSpan, TextSyntaxMode};

    const MAX_LINE_BYTES: usize = 20_000;

    pub(super) fn syntax_set() -> &'static SyntaxSet {
        static SET: OnceLock<SyntaxSet> = OnceLock::new();
        SET.get_or_init(SyntaxSet::load_defaults_newlines)
    }

    pub(super) fn token_for(mode: TextSyntaxMode) -> Option<&'static str> {
        match mode {
            TextSyntaxMode::Plain | TextSyntaxMode::Auto => None,
            TextSyntaxMode::Rust => Some("rs"),
            TextSyntaxMode::Json => Some("json"),
            TextSyntaxMode::Html => Some("html"),
            TextSyntaxMode::Markdown => Some("md"),
            TextSyntaxMode::Shell => Some("sh"),
            // TOML is absent from syntect's default set; the hand-rolled
            // lexer keeps covering it via the `None` fallback.
            TextSyntaxMode::Toml => None,
            TextSyntaxMode::Yaml => Some("yaml"),
            TextSyntaxMode::C => Some("c"),
            TextSyntaxMode::Cpp => Some("cpp"),
            TextSyntaxMode::Python => Some("py"),
            TextSyntaxMode::JavaScript => Some("js"),
            // TypeScript is absent from syntect's default set; JavaScript is
            // the closest available grammar.
            TextSyntaxMode::TypeScript => Some("js"),
            TextSyntaxMode::Go => Some("go"),
            TextSyntaxMode::Java => Some("java"),
            TextSyntaxMode::Css => Some("css"),
        }
    }

    /// Priority-ordered scope prefixes mapped onto the closed six-class GUI
    /// vocabulary. Key prefixes outrank `string` because mapping keys (JSON,
    /// YAML) are also scoped as strings and must keep the `key` class.
    fn class_rules() -> &'static [(&'static str, Vec<Scope>)] {
        static RULES: OnceLock<Vec<(&'static str, Vec<Scope>)>> = OnceLock::new();
        RULES.get_or_init(|| {
            let scopes = |names: &[&str]| {
                names
                    .iter()
                    .map(|name| Scope::new(name).expect("valid scope literal"))
                    .collect::<Vec<_>>()
            };
            vec![
                ("comment", scopes(&["comment"])),
                (
                    "key",
                    scopes(&[
                        "meta.mapping.key",
                        // The bundled (legacy Sublime packages) JSON grammar
                        // scopes keys as meta.structure.dictionary.key.
                        "meta.structure.dictionary.key",
                        "support.type.property-name",
                        "meta.object-literal.key",
                    ]),
                ),
                ("string", scopes(&["string"])),
                ("number", scopes(&["constant.numeric"])),
                ("tag", scopes(&["entity.name.tag", "meta.tag"])),
                ("keyword", scopes(&["keyword", "storage"])),
            ]
        })
    }

    fn class_for(stack: &ScopeStack) -> Option<&'static str> {
        let scopes = stack.as_slice();
        for (class, prefixes) in class_rules() {
            if scopes
                .iter()
                .any(|scope| prefixes.iter().any(|prefix| prefix.is_prefix_of(*scope)))
            {
                return Some(class);
            }
        }
        None
    }

    pub(super) fn spans(text: &str, mode: TextSyntaxMode) -> Option<Vec<SyntaxSpan>> {
        // Oversized lines return Some(empty) rather than None: this
        // deliberately suppresses the hand-rolled fallback lexers too, so a
        // pathological line gets no highlighting at all instead of a slow scan.
        if text.len() > MAX_LINE_BYTES {
            return Some(Vec::new());
        }
        let token = token_for(mode)?;
        let set = syntax_set();
        let syntax = set.find_syntax_by_token(token)?;
        let mut state = ParseState::new(syntax);
        // The "newlines" grammar set requires every parsed line to end with a
        // trailing '\n', so append one when the caller's line lacks it.
        let line = if text.ends_with('\n') {
            text.to_owned()
        } else {
            format!("{text}\n")
        };
        let ops = state.parse_line(&line, set).ok()?;

        // syntect reports byte offsets; spans are char-indexed by contract.
        let mut byte_to_char = vec![0usize; text.len() + 1];
        let mut char_idx = 0usize;
        for (byte_idx, _) in text.char_indices() {
            byte_to_char[byte_idx] = char_idx;
            char_idx += 1;
        }
        byte_to_char[text.len()] = char_idx;

        let mut stack = ScopeStack::new();
        let mut spans: Vec<SyntaxSpan> = Vec::new();
        let mut emit = |start_byte: usize, end_byte: usize, stack: &ScopeStack| {
            let Some(class) = class_for(stack) else {
                return;
            };
            let start = byte_to_char[start_byte];
            let end = byte_to_char[end_byte];
            if start >= end {
                return;
            }
            if let Some(last) = spans.last_mut()
                && last.end == start
                && last.class == class
            {
                last.end = end;
            } else {
                spans.push(super::span(start, end, class));
            }
        };
        let mut cursor = 0usize;
        for (offset, op) in &ops {
            // Ops may reference the synthetic newline byte appended above;
            // the clamp keeps byte_to_char indexing in bounds and the newline
            // out of emitted spans.
            let end = (*offset).min(text.len());
            if end > cursor {
                emit(cursor, end, &stack);
                cursor = end;
            }
            stack.apply(op).ok()?;
        }
        if text.len() > cursor {
            emit(cursor, text.len(), &stack);
        }
        Some(spans)
    }
}

#[cfg(all(test, feature = "syntax-rich"))]
mod syntect_tests {
    use super::*;

    const GUI_CLASSES: [&str; 6] = ["keyword", "string", "number", "comment", "key", "tag"];

    #[test]
    fn syntect_rust_keywords_and_strings() {
        let spans = syntax_spans(
            "pub fn main() { let s = \"hi\"; } // note",
            TextSyntaxMode::Rust,
        );
        assert!(spans.iter().any(|s| s.class == "keyword"));
        assert!(spans.iter().any(|s| s.class == "string"));
        assert!(spans.iter().any(|s| s.class == "comment"));
        for s in &spans {
            assert!(GUI_CLASSES.contains(&s.class.as_str()), "class {}", s.class);
        }
    }

    #[test]
    fn syntect_spans_are_char_indexed_not_byte_indexed() {
        // 'é' is 2 bytes, 1 char; the string literal must start at the char index.
        let line = "let é = \"x\";";
        let spans = syntax_spans(line, TextSyntaxMode::Rust);
        let string_span = spans.iter().find(|s| s.class == "string").unwrap();
        let chars: Vec<char> = line.chars().collect();
        assert_eq!(chars[string_span.start], '"');
    }

    #[test]
    fn syntect_python_via_new_mode() {
        // Callers highlight one line at a time, so feed single lines.
        let spans = syntax_spans("def f():", TextSyntaxMode::Python);
        assert!(spans.iter().any(|s| s.class == "keyword"));

        let spans = syntax_spans("    return 1  # c", TextSyntaxMode::Python);
        assert!(spans.iter().any(|s| s.class == "keyword"));
        assert!(spans.iter().any(|s| s.class == "comment"));
        for s in &spans {
            assert!(GUI_CLASSES.contains(&s.class.as_str()), "class {}", s.class);
        }
    }

    #[test]
    fn syntect_json_keys_keep_key_class() {
        let spans = syntax_spans("{\"name\": \"x\", \"n\": 3}", TextSyntaxMode::Json);
        assert!(spans.iter().any(|s| s.class == "key"));
        assert!(spans.iter().any(|s| s.class == "string"));
        assert!(spans.iter().any(|s| s.class == "number"));
    }

    #[test]
    fn syntect_tokens_resolve_for_every_mapped_mode() {
        for mode in [
            TextSyntaxMode::Rust,
            TextSyntaxMode::Json,
            TextSyntaxMode::Html,
            TextSyntaxMode::Markdown,
            TextSyntaxMode::Shell,
            TextSyntaxMode::Toml,
            TextSyntaxMode::Yaml,
            TextSyntaxMode::C,
            TextSyntaxMode::Cpp,
            TextSyntaxMode::Python,
            TextSyntaxMode::JavaScript,
            TextSyntaxMode::TypeScript,
            TextSyntaxMode::Go,
            TextSyntaxMode::Java,
            TextSyntaxMode::Css,
        ] {
            if let Some(token) = rich::token_for(mode) {
                assert!(
                    rich::syntax_set().find_syntax_by_token(token).is_some(),
                    "token {token:?} for {mode:?} does not resolve"
                );
            }
        }
    }

    #[test]
    fn syntect_spans_uphold_structural_invariants() {
        // Representative lines: a trailing comment (exercises the synthetic
        // newline clamp) and a multi-keyword run (exercises adjacent-span
        // merging), plus a mixed JSON line.
        let cases = [
            ("let x = 1; // trailing comment", TextSyntaxMode::Rust),
            ("pub unsafe fn f() {}", TextSyntaxMode::Rust),
            ("{\"k\": [true, 2]}", TextSyntaxMode::Json),
        ];
        for (line, mode) in cases {
            let spans = syntax_spans(line, mode);
            let char_count = line.chars().count();
            for s in &spans {
                assert!(s.start < s.end, "{line:?}: empty/inverted span {s:?}");
                assert!(s.end <= char_count, "{line:?}: span past end {s:?}");
            }
            for pair in spans.windows(2) {
                assert!(
                    pair[0].start <= pair[1].start,
                    "{line:?}: spans not sorted: {pair:?}"
                );
                assert!(
                    !(pair[0].end == pair[1].start && pair[0].class == pair[1].class),
                    "{line:?}: unmerged contiguous same-class spans: {pair:?}"
                );
            }
        }
    }

    #[test]
    fn syntect_perf_guard_skips_oversized_lines() {
        let big = "let x = 1; ".repeat(2_000);
        assert!(big.len() > 20_000);
        assert!(syntax_spans(&big, TextSyntaxMode::Rust).is_empty());
    }
}
