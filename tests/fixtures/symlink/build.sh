#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# Build a deterministic symlink fixture tree under $1.
# Usage: build.sh <fixture-root>
#   fixture-root: the tests/fixtures/symlink directory (written by the caller)
#
# Requires: bash, ln on PATH.

set -euo pipefail
ROOT="${1:?path required}"
rm -rf "$ROOT/left" "$ROOT/right"
mkdir -p "$ROOT/left" "$ROOT/right"

# Left tree
printf 'hello\n' > "$ROOT/left/target.txt"
ln -s target.txt          "$ROOT/left/symlink-to-file"
ln -s ../left/target.txt  "$ROOT/left/symlink-relative"
ln -s /nonexistent        "$ROOT/left/dangling"
mkdir                      "$ROOT/left/subdir"
printf 'in subdir\n' > "$ROOT/left/subdir/inner.txt"
ln -s subdir              "$ROOT/left/symlink-to-dir"

# Right tree — same shape; dangling points to a different (also absent) target
printf 'hello\n' > "$ROOT/right/target.txt"
ln -s target.txt           "$ROOT/right/symlink-to-file"
ln -s ../right/target.txt  "$ROOT/right/symlink-relative"
ln -s /also-nonexistent   "$ROOT/right/dangling"
mkdir                       "$ROOT/right/subdir"
printf 'in subdir\n' > "$ROOT/right/subdir/inner.txt"
ln -s subdir               "$ROOT/right/symlink-to-dir"

echo "Built: $ROOT/left  $ROOT/right"
