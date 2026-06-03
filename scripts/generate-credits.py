#!/usr/bin/env python3
#
# Canonical generator for third-party crate attribution.
#
# Single source of truth for the set of crates distributed in the release
# binaries: runs the authoritative `cargo tree` extraction (over the exact
# shipped feature set on the Linux target, build deps in, dev deps out,
# excluding workspace crates) and produces:
#
# - docs/third-party-crates.json (committed machine-readable list)
# - formatted table rows for docs/third-party-notices.md
# - crates array literal for apps/linsync-gui/qml/CreditsPage.qml
# - Cargo Dependencies table + license-use counts for the thirdPartyText
#   string in apps/linsync-gui/qml/LicensesPage.qml
#
# The `just credits` recipe prints the human table (for review).
# `just credits-update` (or direct --update) rewrites the JSON and patches
# the generated blocks inside the three surfaces using the markers below.
# `scripts/check-credits.sh` (and release-smoke) use --verify to ensure the
# committed JSON matches the current graph; drift for data is now caught
# early and the surfaces are refreshed by construction.
#
# Add new hand-written prose only outside the generated blocks. The runtime
# helpers table (pdftotext etc.) is intentionally not Cargo and stays manual.
#
# Never edit inside a BEGIN/END GENERATED block by hand.

from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
JSON_PATH = ROOT / "docs" / "third-party-crates.json"
MD_PATH = ROOT / "docs" / "third-party-notices.md"
CREDITS_QML = ROOT / "apps" / "linsync-gui" / "qml" / "CreditsPage.qml"
LICENSES_QML = ROOT / "apps" / "linsync-gui" / "qml" / "LicensesPage.qml"

# Extraction mirrors the previous `just credits` pipeline exactly so that
# committed data (and thus the generated blocks) stays byte-for-byte
# compatible with prior releases unless the graph actually changed.
TREE_PIPE = r"""
{
    cargo tree -p linsync --no-default-features \
        --features cxxqt,cxxqt-app,web-engine -e no-dev \
        --target x86_64-unknown-linux-gnu --format '{p}|{l}'
    cargo tree -p linsync-cli --features web-engine -e no-dev \
        --target x86_64-unknown-linux-gnu --format '{p}|{l}'
} | sed -E 's/^[^a-zA-Z0-9]*//; s/ \(proc-macro\)//; s/ \(\*\)$//' \
  | grep -E '^[a-zA-Z0-9_-]+ v[0-9]' \
  | grep -vE '^(linsync|linsync-core|linsync-cli|linsync-sandbox|linsync-webengine) ' \
  | awk -F'|' '{ split($1,a," v"); printf "| %s | %s | %s |\n", a[1], a[2], $2 }' \
  | sort -u
"""

BEGIN = "BEGIN GENERATED CREDITS"
END = "END GENERATED CREDITS"

# Markers used inside each surface. For the .md the HTML comment is
# invisible when GitHub renders it; in the in-app plain-text Licenses tab
# it appears as a small note (acceptable and self-documenting).
MD_BEGIN = f"<!-- {BEGIN} (do not edit inside; run `just credits-update`) -->"
MD_END = f"<!-- {END} -->"

QML_BEGIN = f"// {BEGIN} (do not edit inside; run `just credits-update`)"
QML_END = f"// {END}"

# For LicensesPage we put // comments (source only, never part of the
# displayed string value) around the generated blocks inside the big
# thirdPartyText expression. The marker text is plain for legibility.
LICENSES_TABLE_BEGIN = f"{BEGIN} TABLE (do not edit inside; run `just credits-update`)"
LICENSES_TABLE_END = f"{END} TABLE"

# The counts block in the Licenses intro (numeric lines only; the
# Unicode/Unlicense notes stay as hand-written explanatory text).
LICENSES_COUNTS_BEGIN = f"{BEGIN} COUNTS (do not edit inside; run `just credits-update`)"
LICENSES_COUNTS_END = f"{END} COUNTS"


def run_tree() -> list[dict[str, str]]:
    """Return the authoritative list of third-party crates as dicts."""
    out = subprocess.check_output(
        ["bash", "-c", TREE_PIPE], text=True, cwd=ROOT
    )
    rows = [ln.strip() for ln in out.strip().splitlines() if ln.strip().startswith("| ")]
    crates: list[dict[str, str]] = []
    for r in rows:
        m = re.match(r"^\| ([^|]+) \| ([^|]+) \| (.+) \|$", r)
        if m:
            crates.append(
                {
                    "name": m.group(1).strip(),
                    "version": m.group(2).strip(),
                    "license": m.group(3).strip(),
                }
            )
    return crates


def chosen_bucket(lic: str) -> str:
    """Replicate the manual bucketing used for the 'Licenses in use' summary.

    See prose in docs/third-party-notices.md and the AGENTS.md licensing
    section: crates with Apache alongside CC0/MIT-0/LLVM use the Apache
    option for GPL-3.0-only review; Zlib-only uses Zlib; everything else
    that has MIT/Unlicense goes under MIT (the selected compat option).
    """
    if lic == "Zlib":
        return "Zlib"
    if "Apache" in lic and ("CC0" in lic or "MIT-0" in lic or "LLVM" in lic):
        return "Apache"
    if "MIT" in lic or "Unlicense" in lic:
        return "MIT"
    if "Apache" in lic:
        return "Apache"
    if "BSD-2" in lic or "BSD 2" in lic:
        return "BSD-2"
    if "BSD-3" in lic or "BSD 3" in lic:
        return "BSD-3"
    if "Zlib" in lic:
        return "Zlib"
    return "Other"


def compute_counts(crates: list[dict[str, str]]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for c in crates:
        b = chosen_bucket(c["license"])
        counts[b] = counts.get(b, 0) + 1
    return counts


def format_md_rows(crates: list[dict[str, str]]) -> str:
    lines = [f"| `{c['name']}` | {c['version']} | {c['license']} |" for c in crates]
    return "\n".join(lines)


def format_qml_array(crates: list[dict[str, str]]) -> str:
    lines = []
    for c in crates:
        lines.append(
            f'        {{ name: "{c["name"]}",                  version: "{c["version"]}",     license: "{c["license"]}" }},'
        )
    return "\n".join(lines)


def format_licenses_table_rows_source(crates: list[dict[str, str]]) -> str:
    """Return source fragment for the data rows inside the + -concatenated string.

    Each line is:        "| pkg... |\n" +
    // comments for markers are inserted by the caller around the block.
    """
    pkg_w = 20
    ver_w = 8
    parts = []
    for c in crates:
        row = f"| {c['name']:<{pkg_w}} | {c['version']:<{ver_w}} | {c['license']} |"
        parts.append(f'        "{row}\\n" +')
    return "\n".join(parts)


def format_licenses_counts_source(counts: dict[str, int]) -> str:
    """Return the 5 source lines for the numeric license counts (with correct singular/plural)."""
    order = [
        ("MIT", "MIT License"),
        ("Apache", "Apache License 2.0"),
        ("BSD-3", "BSD 3-Clause"),
        ("BSD-2", "BSD 2-Clause"),
        ("Zlib", "Zlib"),
    ]
    lines = []
    for key, label in order:
        n = counts.get(key, 0)
        plural = "s" if n != 1 else ""
        lines.append(f'        " - {label:<28} ({n} crate{plural})\\n" +')
    return "\n".join(lines)


def patch_block(text: str, begin: str, end: str, replacement: str, *, keep_markers: bool = True) -> str:
    """Replace content strictly between begin/end markers.

    If keep_markers, the markers themselves remain and only interior changes.
    The replacement should not include the markers.
    """
    pattern = re.compile(
        rf"(?P<prefix>{re.escape(begin)}).*?(?P<suffix>{re.escape(end)})",
        re.DOTALL,
    )
    if not pattern.search(text):
        raise RuntimeError(f"markers not found: {begin} ... {end}")
    repl = f"{begin}\n{replacement}\n{end}" if keep_markers else replacement
    return pattern.sub(repl, text)


def write_json(crates: list[dict[str, str]]) -> None:
    JSON_PATH.parent.mkdir(parents=True, exist_ok=True)
    JSON_PATH.write_text(json.dumps(crates, indent=2) + "\n", encoding="utf-8")


def load_json() -> list[dict[str, str]]:
    return json.loads(JSON_PATH.read_text(encoding="utf-8"))


def update_md(crates: list[dict[str, str]]) -> None:
    text = MD_PATH.read_text(encoding="utf-8")
    # Update the regenerated date at the top.
    today = datetime.date.today().isoformat()
    text = re.sub(r"^Last regenerated: .*$", f"Last regenerated: {today}", text, flags=re.M)

    generated_rows = format_md_rows(crates)
    block = f"{MD_BEGIN}\n{generated_rows}\n{MD_END}"

    if MD_BEGIN not in text:
        # First run: the old data rows follow the separator up to the blank line + "The table above..."
        # Excise the old | `foo` | rows and insert fresh marked block after separator.
        sep = "| --- | --- | --- |"
        # Match from sep line through consecutive data rows (lines starting with | ` ) until we hit a non-row.
        md_pat = re.compile(
            rf'({re.escape(sep)})\n'
            r'(?:\| `[^`]+` \| [^|]+ \| [^|]+ \|\n)+'
            r'(?=\nThe table above is generated)'
        )
        def _ins(m: re.Match[str]) -> str:
            return m.group(1) + "\n" + block
        if md_pat.search(text):
            text = md_pat.sub(_ins, text, count=1)
        else:
            # Fallback: append after sep (may leave dups, but rare)
            text = re.sub(rf"({re.escape(sep)})", lambda m: m.group(0) + "\n" + block, text, count=1)
    else:
        text = patch_block(text, MD_BEGIN, MD_END, generated_rows)

    MD_PATH.write_text(text, encoding="utf-8")


def update_credits_qml(crates: list[dict[str, str]]) -> None:
    text = CREDITS_QML.read_text(encoding="utf-8")
    generated = format_qml_array(crates)
    # Indent the markers to sit at the same level as the opening "    readonly... ["
    # (the items themselves carry their 8-space indent from format_qml_array).
    marked_block = "    " + QML_BEGIN + "\n" + generated + "\n    " + QML_END

    if QML_BEGIN in text and QML_END in text:
        text = patch_block(text, QML_BEGIN, QML_END, generated)
    else:
        # First run: replace the entire old crates array (from the property decl to its closing ]).
        # The next property after is "property string filterText".
        arr_start = "    readonly property var crates: ["
        arr_end_follow = "\n\n    property string filterText: \"\""
        i = text.find(arr_start)
        j = text.find(arr_end_follow)
        if i != -1 and j != -1:
            # include the opening line, put marked (which includes the inner [ content ] ? wait no: the generated is the inner items only.
            # We need to emit the full "    readonly ... : [\n <marked items> \n    ]"
            new_arr = f"    readonly property var crates: [\n{marked_block}\n    ]"
            text = text[:i] + new_arr + text[j:]
        else:
            # last resort insert
            text = re.sub(r"(readonly property var crates:\s*\[)", lambda m: m.group(0) + "\n" + marked_block, text, count=1)
    CREDITS_QML.write_text(text, encoding="utf-8")


def _replace_counts_block(text: str, counts_src: str) -> str:
    """Replace the 5 numeric count lines (from first MIT to last Zlib) with marked version.

    Works for first insertion (strips old) and for refresh (via markers).
    """
    start_anchor = '        "## Licenses in use\\n" +\n        "\\n" +\n        " - MIT License                  (107 crates)\\n" +'
    end_anchor = '        " - Unicode-3.0                   (applies to unicode-ident)\\n" +'
    if LICENSES_COUNTS_BEGIN in text and LICENSES_COUNTS_END in text:
        return patch_block(text, LICENSES_COUNTS_BEGIN, LICENSES_COUNTS_END, counts_src)
    # first insertion: locate from the first MIT count line; we will overwrite the 5 numeric ones.
    i = text.find(start_anchor)
    j = text.find(end_anchor)
    if i == -1 or j == -1:
        return text
    # prefix up to (and including) the intro empty line before first count
    # then marked counts, then unicode note follows
    prefix = text[:i]
    suffix = text[j:]
    marked = (
        "        // " + LICENSES_COUNTS_BEGIN + "\n"
        + counts_src + "\n"
        + "        // " + LICENSES_COUNTS_END + "\n"
    )
    return prefix + marked + suffix


def _replace_table_rows_block(text: str, table_src: str) -> str:
    """Replace the data rows after the two header lines up to the \n" + before the "Where a crate..." para.

    Insert // markers as source comments (not in string value).
    """
    hdr2 = '        "| -------------------- | -------- | ------------------ |\\n" +'
    # After last data row comes the "Where a crate offers multiple licenses..." explanation (in the string).
    follow = '        "\\n" +\n        "Where a crate offers multiple licenses, LinSync selects the option\\n" +'
    if LICENSES_TABLE_BEGIN in text and LICENSES_TABLE_END in text:
        return patch_block(text, LICENSES_TABLE_BEGIN, LICENSES_TABLE_END, table_src)
    i = text.find(hdr2)
    j = text.find(follow)
    if i == -1 or j == -1:
        return text
    prefix = text[: i + len(hdr2)]
    suffix = text[j:]
    marked = (
        "\n        // " + LICENSES_TABLE_BEGIN + "\n"
        + table_src + "\n"
        + "        // " + LICENSES_TABLE_END + "\n"
    )
    return prefix + marked + suffix


def update_licenses_qml(crates: list[dict[str, str]]) -> None:
    text = LICENSES_QML.read_text(encoding="utf-8")
    counts = compute_counts(crates)
    counts_src = format_licenses_counts_source(counts)
    table_src = format_licenses_table_rows_source(crates)

    text = _replace_counts_block(text, counts_src)
    text = _replace_table_rows_block(text, table_src)

    LICENSES_QML.write_text(text, encoding="utf-8")


def cmd_print_table() -> None:
    crates = run_tree()
    rows = "\n".join(f"| {c['name']} | {c['version']} | {c['license']} |" for c in crates)
    print(rows)
    print(f"third-party crates distributed in the release build: {len(crates)}")


def cmd_json(out: Path | None) -> None:
    crates = run_tree()
    data = json.dumps(crates, indent=2) + "\n"
    if out:
        out.write_text(data, encoding="utf-8")
    else:
        sys.stdout.write(data)


def cmd_update() -> None:
    crates = run_tree()
    write_json(crates)
    update_md(crates)
    update_credits_qml(crates)
    update_licenses_qml(crates)
    print(f"Updated {JSON_PATH}, {MD_PATH}, {CREDITS_QML.name}, {LICENSES_QML.name}")
    print(f"Total third-party crates: {len(crates)}")


def cmd_verify() -> int:
    if not JSON_PATH.exists():
        print("ERROR: committed JSON missing:", JSON_PATH, file=sys.stderr)
        return 2
    committed = load_json()
    current = run_tree()
    if committed == current:
        print(f"OK: {JSON_PATH.name} matches current graph ({len(current)} crates)")
        return 0
    # Diff for the user
    cset = {(c["name"], c["version"], c["license"]) for c in committed}
    nset = {(c["name"], c["version"], c["license"]) for c in current}
    missing = sorted(nset - cset)
    extra = sorted(cset - nset)
    print("DRIFT: committed JSON does not match current `cargo tree` over shipped features.", file=sys.stderr)
    if missing:
        print("  missing (in graph but not committed):", file=sys.stderr)
        for m in missing:
            print(f"    - {m[0]} {m[1]} | {m[2]}", file=sys.stderr)
    if extra:
        print("  extra (committed but not in current graph):", file=sys.stderr)
        for e in extra:
            print(f"    + {e[0]} {e[1]} | {e[2]}", file=sys.stderr)
    print("Run `just credits-update` (or python scripts/generate-credits.py --update) then commit the result.", file=sys.stderr)
    return 1


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="Third-party credits generator + drift guard.")
    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("table", help="Print the authoritative | name | ver | lic | table (like `just credits`).")
    j = sub.add_parser("json", help="Print or write the JSON list.")
    j.add_argument("--out", type=Path, help="Write to file instead of stdout.")

    sub.add_parser("update", help="Regenerate JSON and patch the three credit surfaces (md + two QML).")

    sub.add_parser("verify", help="Fail (exit 1) if committed JSON does not match the current graph. Used by release gates.")

    args = p.parse_args(argv)

    if args.cmd == "table":
        cmd_print_table()
    elif args.cmd == "json":
        cmd_json(args.out)
    elif args.cmd == "update":
        cmd_update()
    elif args.cmd == "verify":
        return cmd_verify()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
