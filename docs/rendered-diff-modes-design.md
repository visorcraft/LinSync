# Rendered Diff Modes Design

> Status: design — implementation pending follow-up plan.

## Goals

- Add three opt-in display modes to the Compare section: syntax-coloured, RTL, and prose-reflow.
- All modes layer on top of the existing plain-text diff; none alter `TextCompareResult`.
- Modes compose: a file can be syntax-coloured and RTL simultaneously.

## Non-goals

These modes are presentation-only. The underlying diff result (`TextCompareResult`, `DiffLine`, `DiffBlock` in `crates/linsync-core/src/text.rs`) is unchanged. No new compare logic, no new bridge endpoints beyond settings persistence.

---

## Mode 1: Syntax-coloured

### Highlighter pick

**`syntect`** — pure-Rust, MIT-licensed, Sublime Text grammar format, used by `bat`. Ships a bundled grammar set; no compiled C extensions. Produces token ranges with foreground colour per token.

`tree-sitter` is rejected: the compiled grammar binaries add significant dep weight, require C linkage, and the incremental-parse benefit is irrelevant for the one-shot highlight pass needed here.

`QSyntaxHighlighter` (Qt-side) is rejected: it requires per-character Qt signals across the bridge for every line, and keeps highlight logic outside the Rust core.

### How layering with diff highlight works

Diff colour is rendered as **background** on each `TextEdit` line row; syntax colour is rendered as **foreground** on inline spans. The two axes are orthogonal: QML's `StyledText` or `textFormat: TextEdit.RichText` carries `<font color="…">` spans for syntax, while the row wrapper `Rectangle.color` carries the diff background from `Main.qml`'s `lineBackground()` function. No conflict.

### File type detection

1. File extension, matched against syntect's built-in extension map.
2. Magic-bytes / shebang fallback for extensionless files (e.g. `#!/usr/bin/env python3` → Python).
3. If unrecognised, fall back to plain text (no spans emitted).

### Theme awareness

`syntect` ships multiple themes (Solarized, InspiredGitHub, etc.). Dark palette selects a dark syntect theme; light palette selects a light theme. The bridge returns the theme name alongside spans so QML can switch without re-running the highlighter.

---

## Mode 2: RTL (right-to-left)

### Bidi handling

Qt6 `TextEdit` handles Unicode bidi natively when `LayoutMirroring.enabled` is set or when text contains RTL characters. No manual bidi reordering is needed in Rust. `Qt.RightToLeft` layout direction is set on the pane's `ColumnLayout` when RTL mode is active.

### Mirror layout

Both panes flip text direction within their own `TextEdit`. The side assignment (left file / right file) is **not** swapped — swapping panes would confuse keyboard navigation and copy-left/copy-right operations. Only the text rendering direction changes inside each pane.

### Diff overlay correctness

`DiffLine.inline` records character offsets in **logical** (Unicode code-unit) order, matching Qt's internal cursor model. Because syntect also works in logical order, the inline diff spans (`InlineDiff.left_start` / `left_end`) remain valid when bidi reorders characters visually. No fixup is required.

### Detection

On compare, inspect the first 512 characters of each document for Unicode bidi class `R` or `AL` (Arabic Letter / Right-to-Left). If either document is RTL, `rtlAutoDetect` suggests enabling the mode; the user confirms or can force it via SettingsPage.

---

## Mode 3: Prose-reflow

### Paragraph boundary

A paragraph is a maximal run of non-empty lines bounded by at least one blank line (or by the start/end of the file). Single-line files with no blank lines are treated as one paragraph. This mirrors Markdown and plain-prose conventions.

### Engine integration

Prose-reflow is a **post-processor** on the existing `TextCompareResult`; it does not touch `compare_documents`. The post-processor groups `DiffLine` entries into paragraph clusters, re-wraps each cluster's text at the current pane width (measured in characters, provided by QML via the bridge), and emits a new sequence of display lines. The `TextCompareResult` is not mutated; the reflowed display lines are ephemeral and re-generated on pane resize.

### Diff block tracking

Each display line produced by the post-processor carries the source `DiffLine` index range it was derived from. The diff status of a display line is the union of its source lines' `DiffLineKind` values: if any source line is `Changed`, `LeftOnly`, or `RightOnly`, the display line is highlighted accordingly. `lineBackground()` in `Main.qml` receives a `state` string computed from this union.

---

## Performance budgets

| Mode | Budget | Mechanism |
|------|--------|-----------|
| Syntax-colour | < 200 ms for files up to 100 KB | syntect is fast enough; cache spans per file+theme key |
| RTL | O(file size) — free in Qt | no Rust work; bidi is Qt's renderer |
| Prose-reflow | O(file size) | single linear pass; cache per pane-width |
| All modes | Disabled automatically for files > 1 MB | show "Enable for this file" button in the pane toolbar |

The 1 MB threshold applies per file. `TextDocument.byte_len` (already present in `text.rs`) is the gate value.

---

## Settings keys

Three boolean keys added to `Settings` in `crates/linsync-core/src/storage.rs` and wired through `SettingsPage.qml` via the existing `settingChanged` / `persistUiSetting` / `applySingleSetting` path in `Main.qml`:

```
syntaxHighlighting: bool   // default: false
rtlAutoDetect: bool        // default: false
proseReflow: bool          // default: false
```

All three default to `false`. No mode is default-on.

---

## API surface

```rust
// In crates/linsync-core/src/render.rs (new file) or a new linsync-render crate
pub struct HighlightedSpan {
    pub start: usize,       // byte offset, logical order
    pub end: usize,
    pub foreground: u32,    // ARGB
}

pub fn highlight_syntax(
    text: &str,
    language_hint: Option<&str>,
    theme: SyntaxTheme,
) -> Vec<HighlightedSpan>;

pub enum SyntaxTheme { Light, Dark }
```

Pure-Rust; no Qt types cross the boundary. QML receives a JSON array of `{start, end, color}` objects via the existing HTTP bridge or cxx-qt invokable.

---

## QML integration

Text pane rows render via a `Repeater` over `root.leftRows` / `root.rightRows` in `Main.qml`. Each row is a `Rectangle` (diff background) wrapping a `Text` element.

When syntax-colour mode is enabled, the `Text` element switches to `textFormat: Text.StyledText` and its `text` property becomes an HTML string with `<font color="…">` tags constructed from the `HighlightedSpan` array. The diff background remains on the outer `Rectangle`, unchanged.

RTL mode sets `LayoutMirroring.enabled: true` and `horizontalAlignment: Text.AlignRight` on each row's `Text` element.

Prose-reflow mode replaces `root.leftRows` / `root.rightRows` with post-processed display rows before the `Repeater` sees them. Diff navigation (`rebuildDiffRows`, `scrollToCurrentDifference`) operates on display-row indices; the post-processor maps them back to source `DiffLine` indices for copy operations.

---

## Sandbox interaction

None. All three modes are pure-Rust computation with no subprocess, filesystem write, or network access.

---

## Test plan

- **Unit tests** in `crates/linsync-core/tests/`: syntax detection by extension and shebang; `highlight_syntax` returns non-empty spans for known languages; prose-reflow paragraph grouping; diff-state union on multi-source display lines.
- **Visual regression**: extend the Task 1.6 screenshot script (`scripts/gui-screenshot.sh`) with `--mode=syntax`, `--mode=rtl`, `--mode=prose` variants; upload as CI artifacts and review before 1.0 tagging.

---

## Blocking dependencies

None. Phase 12 is independent of all other phases (see dependency graph in the implementation plan).

---

## Open issues

1. **syntect grammar licensing**: confirm bundled Sublime grammars are MIT/permissive before adding the dep (`cargo deny check` gates this).
2. **Pane-width signalling**: QML must push the character-column width to the bridge on resize. Define the call (`/render/reflow?width=N` or a cxx-qt property) in the follow-up plan.
3. **Inline diff spans under prose-reflow**: `InlineDiff` offsets are per-source-line; a reflowed display line may cover multiple source lines. The mapping needs a worked example before coding starts.
4. **RTL + syntax-colour composition**: syntect token boundaries are byte-aligned in logical order — confirm correct behaviour with Qt's `StyledText` parser on Arabic input.
