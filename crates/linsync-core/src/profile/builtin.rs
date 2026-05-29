//! Built-in compare profiles.
//!
//! Built-ins ship with the binary and cannot be overwritten or deleted
//! through [`ProfileStore`]. They live alongside user profiles in
//! returned listings but are never written to disk. To customise a
//! built-in, copy it to a new id and edit the copy (the CLI / GUI
//! "Save as…" surfaces do this).

use crate::binary::BinaryCompareOptions;
use crate::folder::{CompareMethod, FolderCompareOptions, SymlinkPolicy};
use crate::table::TableCompareOptions;
use crate::text::TextCompareOptions;
use crate::webpage::WebpageCompareOptions;

#[cfg(feature = "document-compare")]
use crate::document::DocumentCompareOptions;
#[cfg(feature = "image-compare")]
use crate::image::ImageCompareOptions;

use super::{CURRENT_PROFILE_SCHEMA_VERSION, CompareProfile, ProfileId};

/// Returns every built-in profile in a stable order. The "default"
/// profile is always first.
pub fn builtin_profiles() -> Vec<CompareProfile> {
    vec![
        default_profile(),
        strict_bytes_profile(),
        ignore_formatting_profile(),
        code_review_profile(),
        prose_review_profile(),
        folder_sync_preview_profile(),
        webpage_source_safe_profile(),
    ]
}

/// Returns the [`ProfileId`]s of every built-in profile in the same
/// order as [`builtin_profiles`]. Pass this into
/// [`crate::profile::ProfileStore::with_reserved_ids`] at startup so
/// the store refuses to let a user shadow a built-in id.
pub fn builtin_profile_ids() -> Vec<ProfileId> {
    builtin_profiles().into_iter().map(|p| p.id).collect()
}

/// Look up a built-in profile by id.
pub fn find_builtin(id: &ProfileId) -> Option<CompareProfile> {
    builtin_profiles().into_iter().find(|p| p.id == *id)
}

fn make_id(id: &str) -> ProfileId {
    // Built-in ids are constants we author here; they must always pass
    // validation. Panic if not — that's a programming error in this
    // file, not a runtime input.
    ProfileId::new(id).expect("built-in profile id should validate")
}

fn builtin_shell(id: &str, name: &str, description: &str) -> CompareProfile {
    CompareProfile {
        schema_version: CURRENT_PROFILE_SCHEMA_VERSION,
        id: make_id(id),
        name: name.to_owned(),
        description: description.to_owned(),
        builtin: true,
        text: TextCompareOptions::default(),
        folder: FolderCompareOptions::default(),
        table: TableCompareOptions::default(),
        binary: BinaryCompareOptions::default(),
        #[cfg(feature = "image-compare")]
        image: ImageCompareOptions::default(),
        #[cfg(feature = "document-compare")]
        document: DocumentCompareOptions::default(),
        webpage: WebpageCompareOptions::default(),
        extra: serde_json::Map::new(),
    }
}

/// `default` — out-of-the-box behaviour. Everything matches the option
/// struct `Default` impls.
pub fn default_profile() -> CompareProfile {
    builtin_shell(
        "default",
        "Default",
        "Out-of-the-box LinSync compare behaviour. Every per-mode option is at its default value.",
    )
}

/// `strict-bytes` — treat any byte difference as significant. Stricter
/// than `default` because folder compare is forced to `FullContents`
/// (every byte, no early exit), the large-file fallback is disabled,
/// and move detection is off so reordered lines still count as
/// differences.
pub fn strict_bytes_profile() -> CompareProfile {
    let mut p = builtin_shell(
        "strict-bytes",
        "Strict bytes",
        "Treat any byte-level difference as significant: no case folding, no whitespace folding, no blank-line skipping, no EOL normalisation, no move detection. Folder compare reads full contents on every entry with no large-file shortcut.",
    );
    p.text.ignore_case = false;
    p.text.ignore_whitespace = false;
    p.text.ignore_blank_lines = false;
    p.text.ignore_eol = false;
    p.text.detect_moves = false;
    p.folder.compare_method = CompareMethod::FullContents;
    p.folder.large_file_threshold = None;
    p.folder.large_file_fallback_method = CompareMethod::FullContents;
    p
}

/// `ignore-formatting` — be permissive about whitespace, EOL and
/// blank-line differences for "did the meaningful text change?"
/// reviews.
pub fn ignore_formatting_profile() -> CompareProfile {
    let mut p = builtin_shell(
        "ignore-formatting",
        "Ignore formatting",
        "Ignore differences in case, whitespace, EOL, and blank lines. Use when only meaningful text changes matter.",
    );
    p.text.ignore_case = true;
    p.text.ignore_whitespace = true;
    p.text.ignore_blank_lines = true;
    p.text.ignore_eol = true;
    p
}

/// `code-review` — preserve case + whitespace (real signal in code) but
/// turn on move detection.
pub fn code_review_profile() -> CompareProfile {
    let mut p = builtin_shell(
        "code-review",
        "Code review",
        "Source-code review preset: preserve case and whitespace, normalise EOL, detect moved blocks. Folder compare uses content hashing.",
    );
    p.text.ignore_case = false;
    p.text.ignore_whitespace = false;
    p.text.ignore_eol = true;
    p.text.ignore_blank_lines = false;
    p.text.detect_moves = true;
    p.folder.compare_method = CompareMethod::HashBlake3;
    p
}

/// `prose-review` — like `ignore-formatting` but with aggressive move
/// detection so that paragraphs reorganised between revisions surface
/// as moves rather than churn.
pub fn prose_review_profile() -> CompareProfile {
    let mut p = builtin_shell(
        "prose-review",
        "Prose review",
        "Prose review preset: case-insensitive, whitespace-folded, blank-line- and EOL-tolerant, with aggressive move detection (single-line paragraphs surface as moves). Useful for documentation, translated content, and reorganised text.",
    );
    p.text.ignore_case = true;
    p.text.ignore_whitespace = true;
    p.text.ignore_eol = true;
    p.text.ignore_blank_lines = true;
    p.text.detect_moves = true;
    p.text.min_move_lines = 1;
    p
}

/// `folder-sync-preview` — folder-sync mindset: full contents, no
/// symlink-following (to avoid surprise traversal), include skipped
/// rows so the user can confirm what was left out.
pub fn folder_sync_preview_profile() -> CompareProfile {
    let mut p = builtin_shell(
        "folder-sync-preview",
        "Folder sync preview",
        "Folder synchronisation preset: full content compare, symlinks reported as targets (never followed), large-file threshold off, all rows reported including skipped/error entries.",
    );
    p.folder.recursive = true;
    p.folder.compare_method = CompareMethod::FullContents;
    p.folder.symlink_policy = SymlinkPolicy::CompareTarget;
    p.folder.include_skipped = true;
    p.folder.large_file_threshold = None;
    p
}

/// `webpage-source-safe` — webpage compare with no rendering, no
/// network beyond a single page, fast timeout.
pub fn webpage_source_safe_profile() -> CompareProfile {
    let mut p = builtin_shell(
        "webpage-source-safe",
        "Webpage source-safe",
        "Webpage compare preset that never touches Qt WebEngine: HTML source / extracted text only, depth-1 resource tree, low request budget, short timeout.",
    );
    p.webpage.resource_tree_depth = 1;
    p.webpage.max_requests = 20;
    p.webpage.timeout_secs = 15;
    p.webpage.confirmed_by_user = false; // GUI must surface the consent dialog
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_validates() {
        for p in builtin_profiles() {
            p.validate()
                .unwrap_or_else(|e| panic!("built-in {} fails validation: {e}", p.id));
            assert!(p.builtin, "built-in {} should set builtin=true", p.id);
            assert!(
                !p.description.is_empty(),
                "built-in {} should describe itself",
                p.id
            );
        }
    }

    #[test]
    fn builtin_ids_are_unique() {
        let ids = builtin_profile_ids();
        let mut sorted = ids.clone();
        sorted.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        sorted.dedup_by(|a, b| a.as_str() == b.as_str());
        assert_eq!(ids.len(), sorted.len(), "duplicate built-in id");
    }

    #[test]
    fn find_builtin_round_trips() {
        let id = make_id("code-review");
        let found = find_builtin(&id).expect("code-review should exist");
        assert_eq!(found.id, id);
        assert!(found.text.detect_moves);
    }

    #[test]
    fn missing_builtin_returns_none() {
        let id = ProfileId::new("not-a-builtin").unwrap();
        assert!(find_builtin(&id).is_none());
    }

    #[test]
    fn strict_bytes_is_observably_stricter_than_default() {
        let p = strict_bytes_profile();
        assert!(!p.text.ignore_case);
        assert!(!p.text.ignore_whitespace);
        assert!(!p.text.ignore_eol);
        assert!(!p.text.ignore_blank_lines);
        assert!(!p.text.detect_moves);
        // The strict-bytes guarantee: every byte compared, no shortcut
        // for large files, no move detection. Differs from defaults
        // (which use BinaryContents and may early-exit on size).
        assert_eq!(p.folder.compare_method, CompareMethod::FullContents);
        assert!(p.folder.large_file_threshold.is_none());
        assert_eq!(
            p.folder.large_file_fallback_method,
            CompareMethod::FullContents
        );
    }

    #[test]
    fn prose_review_differs_from_ignore_formatting_via_move_detection() {
        let ignore = ignore_formatting_profile();
        let prose = prose_review_profile();
        // Both ignore the same formatting axes.
        assert_eq!(ignore.text.ignore_case, prose.text.ignore_case);
        assert_eq!(ignore.text.ignore_whitespace, prose.text.ignore_whitespace);
        assert_eq!(ignore.text.ignore_eol, prose.text.ignore_eol);
        assert_eq!(
            ignore.text.ignore_blank_lines,
            prose.text.ignore_blank_lines
        );
        // Prose review turns on move detection; ignore-formatting does
        // not. This is the observable distinction.
        assert!(prose.text.detect_moves);
        assert!(!ignore.text.detect_moves);
        assert_eq!(prose.text.min_move_lines, 1);
    }

    #[test]
    fn ignore_formatting_sets_all_text_options_on() {
        let p = ignore_formatting_profile();
        assert!(p.text.ignore_case);
        assert!(p.text.ignore_whitespace);
        assert!(p.text.ignore_eol);
        assert!(p.text.ignore_blank_lines);
    }

    #[test]
    fn webpage_source_safe_does_not_preconsent_to_network() {
        // The GUI must always re-prompt the user for network consent.
        // The profile preset shouldn't bypass that.
        assert!(!webpage_source_safe_profile().webpage.confirmed_by_user);
    }
}
