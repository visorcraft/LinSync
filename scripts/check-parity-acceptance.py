#!/usr/bin/env python3
"""Validate that feature-parity rows have acceptance checklist rows."""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
FEATURES = ROOT / "docs" / "feature-parity.md"
ACCEPTANCE = ROOT / "docs" / "parity-acceptance.md"


def table_areas(path: Path) -> list[str]:
    areas: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("| "):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if not cells or cells[0] in {"Area", "---"} or set(cells[0]) == {"-"}:
            continue
        areas.append(cells[0])
    return areas


def main() -> int:
    feature_areas = table_areas(FEATURES)
    acceptance_areas = set(table_areas(ACCEPTANCE))
    missing = [area for area in feature_areas if area not in acceptance_areas]

    if missing:
        print(
            "docs/parity-acceptance.md is missing acceptance rows for:",
            file=sys.stderr,
        )
        for area in missing:
            print(f"- {area}", file=sys.stderr)
        return 1

    if "Current status: not parity-complete." not in ACCEPTANCE.read_text(
        encoding="utf-8"
    ):
        print(
            "docs/parity-acceptance.md must state the current parity status",
            file=sys.stderr,
        )
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
