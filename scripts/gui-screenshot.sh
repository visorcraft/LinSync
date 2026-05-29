#!/usr/bin/env bash
# Capture screenshots of LinSync GUI at two window sizes for regression visibility.
# Uses Xvfb + ImageMagick's `import`. Outputs target/screenshots/{tag}-{section}.png.
#
# Requires: Xvfb, ImageMagick (import), and a built linsync binary in target/release/ (or
# falling back to `cargo run`). Set LINSYNC_BIN to override the binary path.
set -euo pipefail

SIZES=("1600x900:desktop" "412x915:mobile")
SECTIONS=("compare" "sessions" "filters" "plugins" "settings" "about" "merge" "image" "webpage" "document")
OUT="target/screenshots"
LINSYNC_BIN="${LINSYNC_BIN:-target/release/linsync}"
STARTUP_SLEEP="${STARTUP_SLEEP:-5}"

mkdir -p "$OUT"

cleanup() {
    [[ -n "${APP_PID:-}" ]] && kill "$APP_PID" 2>/dev/null || true
    [[ -n "${XVFB_PID:-}" ]] && kill "$XVFB_PID" 2>/dev/null || true
    wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Choose a free DISPLAY number per run to avoid clashes with any existing X server.
DISPLAY_NUM=99

for spec in "${SIZES[@]}"; do
    geo="${spec%:*}"
    tag="${spec#*:}"

    Xvfb ":${DISPLAY_NUM}" -screen 0 "${geo}x24" &
    XVFB_PID=$!
    sleep 1

    for section in "${SECTIONS[@]}"; do
        echo "Capturing ${tag}-${section} at ${geo} ..."
        DISPLAY=":${DISPLAY_NUM}" \
        QT_QPA_PLATFORM=xcb \
        QT_QPA_PLATFORM_PLUGIN_PATH="" \
            LINSYNC_STARTUP_SECTION="$section" \
            LINSYNC_QML_ROOT="${LINSYNC_QML_ROOT:-$(pwd)/apps/linsync-gui/qml}" \
            "$LINSYNC_BIN" &
        APP_PID=$!
        sleep "$STARTUP_SLEEP"

        DISPLAY=":${DISPLAY_NUM}" import -window root "$OUT/${tag}-${section}.png" || {
            echo "ERROR: screenshot capture failed for ${tag}-${section}" >&2
            exit 1
        }

        kill "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
        unset APP_PID
    done

    kill "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
    unset XVFB_PID
    ((DISPLAY_NUM++))
done

captured=$(find "$OUT" -name '*.png' | wc -l)
echo "Captured ${captured} screenshots in ${OUT}"

# Fail if we got nothing at all.
if [[ "$captured" -eq 0 ]]; then
    echo "ERROR: no screenshots produced" >&2
    exit 1
fi
