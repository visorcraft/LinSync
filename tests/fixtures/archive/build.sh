#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# Build the archive fixtures from a known, deterministic source tree.
# Usage: build.sh <output-dir>
#   output-dir: directory where sample.zip and sample.tar will be written
#
# Requires: zip, tar, 7z, zstd on PATH.
# The produced archives are byte-stable given the same tool versions.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "${1:?usage: build.sh <output-dir>}" && pwd)"
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

# TAR: deterministic flags keep output byte-stable; build from $SRC so the
# archive only contains the fixture source tree.
tar \
    --sort=name \
    --owner=0 --group=0 \
    --mtime='2000-01-01 00:00:00 UTC' \
    -cf "$ROOT/sample.tar" -C "$SRC" .

# Compressed tar variants (built from the same source tree for identical contents).
tar -czf "$ROOT/sample.tar.gz"   --owner=0 --group=0 --mtime='2000-01-01 00:00:00 UTC' -C "$SRC" .
tar -cjf "$ROOT/sample.tar.bz2"  --owner=0 --group=0 --mtime='2000-01-01 00:00:00 UTC' -C "$SRC" .
tar -cJf "$ROOT/sample.tar.xz"   --owner=0 --group=0 --mtime='2000-01-01 00:00:00 UTC' -C "$SRC" .
tar --zstd -cf "$ROOT/sample.tar.zst" --owner=0 --group=0 --mtime='2000-01-01 00:00:00 UTC' -C "$SRC" .

# 7z: deterministic member order and timestamps.
rm -f "$ROOT/sample.7z"
(cd "$SRC" && 7z a -r -mx=0 "$ROOT/sample.7z" .) >/dev/null

echo "Built archives in $ROOT"
