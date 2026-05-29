#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# Build the archive fixtures from a known, deterministic source tree.
# Usage: build.sh <output-dir>
#   output-dir: directory where sample.zip and sample.tar will be written
#
# Requires: zip, tar, python3 on PATH.
# The produced archives are byte-stable given the same tool versions.

set -euo pipefail

ROOT="${1:?usage: build.sh <output-dir>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$SCRIPT_DIR/source"

# Build a reproducible source tree.
rm -rf "$SRC"
mkdir -p "$SRC/sub"
printf 'alpha\n' > "$SRC/alpha.txt"
printf 'beta\n'  > "$SRC/sub/beta.txt"
printf 'gamma\n' > "$SRC/sub/gamma.txt"

mkdir -p "$ROOT"

# ZIP: -X strips extra OS-specific attributes for reproducibility.
(cd "$SRC" && zip -X -r "$ROOT/sample.zip" .) >/dev/null

# TAR: deterministic flags keep output byte-stable.
(cd "$SRC" && tar \
    --sort=name \
    --owner=0 --group=0 \
    --mtime='2000-01-01 00:00:00 UTC' \
    -cf "$ROOT/sample.tar" .) >/dev/null

echo "Built: $ROOT/sample.zip  $ROOT/sample.tar"
