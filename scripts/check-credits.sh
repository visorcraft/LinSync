#!/usr/bin/env bash
#
# Thin wrapper around the canonical generator for the pre-release drift guard.
#
# The authoritative list of third-party crates distributed in the release
# binaries lives in docs/third-party-crates.json (maintained by the generator
# from `cargo tree` over the exact shipped feature set).
#
# This guard (invoked from scripts/release-smoke.sh) simply asks the generator
# to recompute the current set and compare it to the committed JSON. If they
# differ, the surfaces may be out of date; run `just credits-update`, commit
# the result, and re-run the smoke.
#
# The generator also keeps the *data portions* of the three credit surfaces
# (the md table, the QML crates array, and the Licenses table+counts) in sync
# via generated blocks delimited by BEGIN/END GENERATED CREDITS markers.
# Hand edits inside those blocks are overwritten on the next update.
set -euo pipefail

cd "$(dirname "$0")/.."

python3 scripts/generate-credits.py verify "$@"
