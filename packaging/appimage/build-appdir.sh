#!/usr/bin/env bash
# AppImage builder for LinSync. Uses linuxdeploy + linuxdeploy-plugin-qt to bundle a
# self-contained Qt 6 stack so the resulting bundle runs on any
# glibc-modern Linux without needing the host to ship Qt. The bundle
# uses the in-process cxx-qt host (`cxxqt-app` feature) so QML loads
# directly from the binary without an external `qml6` subprocess.
#
# Requires `linuxdeploy` + `linuxdeploy-plugin-qt` + `qmake6` on PATH.
# If linuxdeploy isn't installed, the script falls back to staging
# the AppDir without producing the final .AppImage so CI can still
# verify the layout.
#
# Newer librsvg releases drop the gdk-pixbuf SVG loader, which makes
# linuxdeploy's bundled `strip` choke on relr.dyn-only ELF objects.
# Set `NO_STRIP=1` to skip the strip step on those hosts; the result
# is functionally identical and only slightly larger.

set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

VERSION="$(awk -F'"' '
    /^\[workspace\.package\]/ { in_section = 1; next }
    in_section && /^\[/ { exit }
    in_section && $1 ~ /^version[[:space:]]*=/ { print $2; exit }
' Cargo.toml)"

appdir="${1:-"${root}/target/appimage/LinSync.AppDir"}"
output="${2:-"${root}/target/appimage/LinSync-${VERSION}-x86_64.AppImage"}"

QT_VERSION_MAJOR=6 cargo build --workspace --release \
    --features 'linsync/cxxqt linsync/cxxqt-app linsync/web-engine linsync-cli/web-engine'

rm -rf "${appdir}"
install -Dm755 "${root}/target/release/linsync" "${appdir}/usr/bin/linsync"
install -Dm755 "${root}/target/release/linsync-cli" "${appdir}/usr/bin/linsync-cli"

# The rendered/screenshot webpage modes (web-engine feature) shell out to a
# `qml6` process running a QtWebEngine view, so bundle the qml runner alongside
# the binaries. If qml6 isn't present at build time the AppImage still works —
# those modes degrade to the HTML-source fallback at runtime.
if qml6_bin="$(command -v qml6 2>/dev/null)"; then
    install -Dm755 "$qml6_bin" "${appdir}/usr/bin/qml6"
fi
mkdir -p "${appdir}/usr/share/linsync"
cp -R "${root}/apps/linsync-gui/qml" "${appdir}/usr/share/linsync/qml"

# Compile + bundle UI translation catalogs next to the qml tree (the in-process
# host loads linsync_<locale>.qm from qml's sibling i18n/ dir).
_lrelease=$(command -v lrelease6 || command -v lrelease-qt6 || command -v lrelease || echo /usr/lib/qt6/bin/lrelease)
mkdir -p "${appdir}/usr/share/linsync/i18n"
for _ts in "${root}"/apps/linsync-gui/i18n/*.ts; do
    "$_lrelease" "$_ts" -qm "${appdir}/usr/share/linsync/i18n/$(basename "${_ts%.ts}").qm" || true
done
install -Dm644 "${root}/packaging/distro/git-mergetool.gitconfig" \
    "${appdir}/usr/share/linsync/git-mergetool.gitconfig"
install -Dm644 "${root}/packaging/com.visorcraft.LinSync.desktop" "${appdir}/usr/share/applications/com.visorcraft.LinSync.desktop"
install -Dm644 "${root}/packaging/com.visorcraft.LinSync.metainfo.xml" "${appdir}/usr/share/metainfo/com.visorcraft.LinSync.metainfo.xml"
install -Dm644 "${root}/packaging/com.visorcraft.LinSync.mime.xml" "${appdir}/usr/share/mime/packages/com.visorcraft.LinSync.xml"
install -Dm644 "${root}/packaging/icons/hicolor/scalable/apps/com.visorcraft.LinSync.svg" "${appdir}/usr/share/icons/hicolor/scalable/apps/com.visorcraft.LinSync.svg"
for size in 16 22 24 32 36 48 64 72 96 128 192 256 512; do
    install -Dm644 "${root}/packaging/icons/hicolor/${size}x${size}/apps/com.visorcraft.LinSync.png" \
        "${appdir}/usr/share/icons/hicolor/${size}x${size}/apps/com.visorcraft.LinSync.png"
done

# AppRun shim — lives outside the AppDir so linuxdeploy's
# --custom-apprun copy has distinct source / destination paths.
apprun_src="${root}/target/appimage/AppRun"
cat > "$apprun_src" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
export QT_PLUGIN_PATH="$HERE/usr/plugins${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}"
export QML2_IMPORT_PATH="$HERE/usr/qml${QML2_IMPORT_PATH:+:$QML2_IMPORT_PATH}"
export LD_LIBRARY_PATH="$HERE/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$HERE/usr/bin/linsync" "$@"
APPRUN
chmod +x "$apprun_src"

if ! command -v linuxdeploy >/dev/null 2>&1; then
    echo "linuxdeploy not on PATH; AppDir staged at ${appdir} but no AppImage produced." >&2
    echo "Install linuxdeploy + linuxdeploy-plugin-qt from https://github.com/linuxdeploy/." >&2
    exit 0
fi

# Force Qt6 qmake for linuxdeploy-plugin-qt (in mixed Qt5/Qt6 envs the
# first qmake on PATH is often Qt5's, causing "Could not find Qt modules").
QMAKE="${QMAKE:-$(command -v qmake6 || command -v qmake-qt6 || echo /usr/bin/qmake6)}"
export QMAKE

# EXTRA_QT_MODULES forces linuxdeploy-plugin-qt to bundle QtWebEngine (and its
# QtWebEngineProcess + resources): the app uses the in-process cxx-qt host, so
# the plugin would not otherwise detect WebEngine usage from the app's QML.
EXTRA_QT_MODULES="${EXTRA_QT_MODULES:-webenginequick}" \
linuxdeploy --appdir "$appdir" --plugin qt --output appimage \
    --custom-apprun "$apprun_src" \
    --desktop-file "${appdir}/usr/share/applications/com.visorcraft.LinSync.desktop"

# linuxdeploy emits `LinSync-x86_64.AppImage` in the cwd; rename to
# the version-stamped path the user requested (or the default).
mv LinSync*.AppImage "$output"
echo "wrote $output"
