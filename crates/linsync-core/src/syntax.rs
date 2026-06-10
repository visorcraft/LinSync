//! Lightweight syntax highlighting used by the text compare views.
//!
//! Produces [`SyntaxSpan`] lists (char-indexed) for a small set of built-in
//! languages, plus an HTML renderer that wraps spans in `syn-*` classes.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::text::escape_html;

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
        _ => None,
    }
}

pub(crate) fn syntax_spans(text: &str, mode: TextSyntaxMode) -> Vec<SyntaxSpan> {
    match mode {
        TextSyntaxMode::Plain | TextSyntaxMode::Auto => Vec::new(),
        TextSyntaxMode::Json => json_syntax_spans(text),
        TextSyntaxMode::Html => html_syntax_spans(text),
        TextSyntaxMode::Rust
        | TextSyntaxMode::Markdown
        | TextSyntaxMode::Shell
        | TextSyntaxMode::Toml
        | TextSyntaxMode::Yaml => generic_syntax_spans(text, mode),
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
