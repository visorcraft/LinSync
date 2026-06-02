#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# Generate a large folder-compare fixture pair that exceeds
# FOLDER_WINDOW_THRESHOLD (5000 entries), so a developer can exercise the
# GUI's *windowed* folder view by hand:
#
#     scripts/generate-large-folder-fixture.sh            # default 5050 files/side
#     scripts/generate-large-folder-fixture.sh 8000 /tmp  # custom count + dir
#     TMPDIR=/tmp cargo run -p linsync -- \
#         /tmp/linsync-large-folder/left /tmp/linsync-large-folder/right
#
# A handful of entries differ (changed / left-only / right-only) so the state
# filter has something to show. The windowing logic itself is unit-tested
# (`folder_query_paginates_a_large_windowed_folder` in apps/linsync-gui);
# this script is for manual/visual confirmation on a real desktop session,
# which the headless no-WM review harness cannot do (see PLAN.md Phase 10).
set -euo pipefail

COUNT="${1:-5050}"
BASE_DIR="${2:-${TMPDIR:-/tmp}/linsync-large-folder}"
LEFT="$BASE_DIR/left"
RIGHT="$BASE_DIR/right"

rm -rf "$BASE_DIR"
mkdir -p "$LEFT" "$RIGHT"

echo "Generating $COUNT entries per side under $BASE_DIR …"
for i in $(seq -w 1 "$COUNT"); do
    name="file-$i.txt"
    printf 'content %s\n' "$i" > "$LEFT/$name"
    # Make ~1 in 500 entries differ so the diff view is non-empty.
    if (( 10#$i % 500 == 0 )); then
        printf 'CHANGED %s\n' "$i" > "$RIGHT/$name"
    else
        printf 'content %s\n' "$i" > "$RIGHT/$name"
    fi
done

# A left-only and a right-only entry.
printf 'only on the left\n' > "$LEFT/only-left.txt"
printf 'only on the right\n' > "$RIGHT/only-right.txt"

echo "Done."
echo "  left:  $LEFT"
echo "  right: $RIGHT"
echo
echo "Launch the GUI against them (note TMPDIR=/tmp so the bridge sidecar matches):"
echo "  TMPDIR=/tmp cargo run -p linsync -- '$LEFT' '$RIGHT'"
