//! Structural invariants that every text-diff algorithm must satisfy,
//! brute-forced over a small alphabet.
//!
//! These guard against a class of bug where post-processing reorders or
//! relocates lines and produces a diff that no longer reconstructs its inputs
//! or whose line numbers run backwards (which corrupts `diff_blocks`, GUI
//! scroll-to-line, and next/prev-change navigation). For every `(left, right)`
//! pair we assert:
//!
//! * **A/B** each side reconstructs, in order, from the lines that carry it;
//! * **C** `left_line` / `right_line` are each strictly increasing;
//! * **D** every `Equal` line has identical text on both sides;
//! * **E** (minimal algorithms only) the number of equal lines matches the LCS
//!   reference, i.e. the edit script is optimal.

use linsync_core::{
    DiffAlgorithm, DiffLineKind, TextCompareOptions, TextCompareResult, compare_text,
};

fn opts(algo: DiffAlgorithm) -> TextCompareOptions {
    TextCompareOptions {
        diff_algorithm: algo,
        ..TextCompareOptions::default()
    }
}

fn to_text(words: &[&str]) -> String {
    if words.is_empty() {
        return String::new();
    }
    let mut s = words.join("\n");
    s.push('\n');
    s
}

/// Returns the number of `Equal` lines, or an error describing the first
/// violated invariant.
fn check_structure(
    left: &[&str],
    right: &[&str],
    result: &TextCompareResult,
) -> Result<usize, String> {
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
    if recon_left != left {
        return Err(format!("A: left reconstruction {recon_left:?} != {left:?}"));
    }
    if recon_right != right {
        return Err(format!(
            "B: right reconstruction {recon_right:?} != {right:?}"
        ));
    }

    let mut last_left = 0;
    let mut last_right = 0;
    let mut equal = 0;
    for line in &result.lines {
        if let Some(ll) = line.left_line {
            if ll <= last_left {
                return Err(format!("C: left_line {ll} after {last_left}"));
            }
            last_left = ll;
        }
        if let Some(rl) = line.right_line {
            if rl <= last_right {
                return Err(format!("C: right_line {rl} after {last_right}"));
            }
            last_right = rl;
        }
        if line.kind == DiffLineKind::Equal {
            if line.left != line.right {
                return Err(format!("D: Equal {:?} != {:?}", line.left, line.right));
            }
            equal += 1;
        }
    }
    Ok(equal)
}

/// All sequences of length 0..=`max_len` over `alphabet`.
fn enumerate(alphabet: &[&'static str], max_len: usize) -> Vec<Vec<&'static str>> {
    let mut out = vec![vec![]];
    let mut frontier: Vec<Vec<&'static str>> = vec![vec![]];
    for _ in 0..max_len {
        let mut next = Vec::new();
        for seq in &frontier {
            for &c in alphabet {
                let mut s = seq.clone();
                s.push(c);
                next.push(s);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

fn brute_force(algo: DiffAlgorithm, minimal: bool, max_len: usize) {
    let alphabet = ["a", "b", "c"];
    let seqs = enumerate(&alphabet, max_len);
    let mut failures: Vec<String> = Vec::new();

    for left in &seqs {
        let lt = to_text(left);
        for right in &seqs {
            let rt = to_text(right);
            let result = compare_text("L", &lt, "R", &rt, &opts(algo));
            match check_structure(left, right, &result) {
                Err(e) if failures.len() < 10 => {
                    failures.push(format!("{algo:?} left={left:?} right={right:?}: {e}"));
                }
                Err(_) => {}
                Ok(equal) if minimal => {
                    let reference = compare_text("L", &lt, "R", &rt, &opts(DiffAlgorithm::Lcs));
                    let reference_equal = reference
                        .lines
                        .iter()
                        .filter(|l| l.kind == DiffLineKind::Equal)
                        .count();
                    if equal != reference_equal && failures.len() < 10 {
                        failures.push(format!(
                            "{algo:?} left={left:?} right={right:?}: E: {equal} equal lines != LCS {reference_equal}"
                        ));
                    }
                }
                Ok(_) => {}
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} invariant violation(s) (showing up to 10):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn myers_diff_preserves_invariants() {
    // Myers was rewritten and previously carried a buggy "stabilize" pass; it
    // is the highest-risk path, so it gets the minimality cross-check in
    // addition to the structural invariants.
    brute_force(DiffAlgorithm::Myers, true, 4);
}

#[test]
fn lcs_diff_preserves_invariants() {
    brute_force(DiffAlgorithm::Lcs, true, 4);
}

#[test]
fn patience_diff_preserves_invariants() {
    // Patience trades minimality for readable anchors, so only the structural
    // invariants (A–D) are asserted, not the minimal edit count.
    brute_force(DiffAlgorithm::Patience, false, 4);
}
