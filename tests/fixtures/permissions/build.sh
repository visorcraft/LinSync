#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# Build a deterministic permissions fixture tree under $1.
# Usage: build.sh <fixture-root>
#   fixture-root: the tests/fixtures/permissions directory (written by the caller)
#
# Requires: bash, chmod on PATH.

set -euo pipefail
ROOT="${1:?path required}"
rm -rf "$ROOT/left" "$ROOT/right"
mkdir -p "$ROOT/left" "$ROOT/right"

for SIDE in left right; do
    printf 'default\n'     > "$ROOT/$SIDE/644.txt";   chmod 0644 "$ROOT/$SIDE/644.txt"
    printf 'private\n'     > "$ROOT/$SIDE/600.txt";   chmod 0600 "$ROOT/$SIDE/600.txt"
    printf 'executable\n'  > "$ROOT/$SIDE/755.sh";    chmod 0755 "$ROOT/$SIDE/755.sh"
    printf 'none\n'        > "$ROOT/$SIDE/000.txt";   chmod 0000 "$ROOT/$SIDE/000.txt"
    mkdir                    "$ROOT/$SIDE/dir-0755";  chmod 0755 "$ROOT/$SIDE/dir-0755"
    mkdir                    "$ROOT/$SIDE/dir-0700";  chmod 0700 "$ROOT/$SIDE/dir-0700"
done

echo "Built: $ROOT/left  $ROOT/right"
