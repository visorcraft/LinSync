#!/usr/bin/env python3
"""Generate a per-locale translation completeness report from i18n/*.ts files.

Outputs a Markdown table to stdout and writes target/l10n-report.md.
Diagnostic-only: does not gate CI or exit non-zero for incomplete translations.
"""

import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
I18N_DIR = REPO_ROOT / "apps" / "linsync-gui" / "i18n"
OUTPUT_PATH = REPO_ROOT / "target" / "l10n-report.md"


@dataclass
class LocaleStats:
    locale: str
    total: int = 0
    finished: int = 0
    unfinished: int = 0
    vanished: int = 0
    obsolete: int = 0

    @property
    def completion_pct(self) -> float:
        if self.total == 0:
            return 100.0
        return 100.0 * self.finished / self.total


def _classify_message(message: ET.Element) -> str:
    """Return 'finished', 'unfinished', 'vanished', or 'obsolete'."""
    msg_type = message.get("type", "")
    trans = message.find("translation")
    trans_type = trans.get("type", "") if trans is not None else ""

    effective_type = msg_type or trans_type
    if effective_type == "vanished":
        return "vanished"
    if effective_type == "obsolete":
        return "obsolete"
    if effective_type == "unfinished":
        return "unfinished"

    # No explicit type: finished if a non-empty translation exists.
    if trans is not None:
        # Plural forms use <numerusform> children; any non-empty form counts.
        numerus_forms = trans.findall("numerusform")
        if numerus_forms:
            if any((f.text or "").strip() for f in numerus_forms):
                return "finished"
        elif (trans.text or "").strip():
            return "finished"

    return "unfinished"


def _extract_locale(ts_path: Path, ts_root: ET.Element) -> str:
    language = ts_root.get("language", "").strip()
    if language:
        return language
    # Fallback: linsync_<locale>.ts
    stem = ts_path.stem
    if stem.startswith("linsync_"):
        return stem[len("linsync_"):]
    return stem


def _parse_ts(ts_path: Path) -> LocaleStats:
    tree = ET.parse(ts_path)
    root = tree.getroot()

    stats = LocaleStats(locale=_extract_locale(ts_path, root))

    for message in root.iter("message"):
        # Plural forms count as one message.
        stats.total += 1
        category = _classify_message(message)
        if category == "finished":
            stats.finished += 1
        elif category == "unfinished":
            stats.unfinished += 1
        elif category == "vanished":
            stats.vanished += 1
        elif category == "obsolete":
            stats.obsolete += 1

    return stats


def _build_table(rows: list[LocaleStats]) -> str:
    header = (
        "| Locale | Total | Finished | Unfinished | Vanished | Obsolete | Completion |\n"
        "|--------|------:|---------:|-----------:|---------:|---------:|-----------:|"
    )
    lines = [header]
    for row in sorted(rows, key=lambda r: r.locale):
        lines.append(
            f"| {row.locale} | {row.total} | {row.finished} | "
            f"{row.unfinished} | {row.vanished} | {row.obsolete} | "
            f"{row.completion_pct:.1f}% |"
        )
    return "\n".join(lines)


def main() -> int:
    ts_files = sorted(I18N_DIR.glob("*.ts"))
    if not ts_files:
        print(f"No .ts files found in {I18N_DIR}", file=sys.stderr)
        return 0

    rows: list[LocaleStats] = []
    for ts_path in ts_files:
        try:
            rows.append(_parse_ts(ts_path))
        except ET.ParseError as exc:
            print(f"Warning: failed to parse {ts_path}: {exc}", file=sys.stderr)
            continue

    table = _build_table(rows)
    print(table)

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(table + "\n", encoding="utf-8")
    print(f"Wrote {OUTPUT_PATH}", file=sys.stderr)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
