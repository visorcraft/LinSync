#!/usr/bin/env bash
# AppImage builder for LinSync — mirrors the Grexa packaging flow for
# consistency. Uses linuxdeploy + linuxdeploy-plugin-qt to bundle a
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
    --features 'linsync/cxxqt linsync/cxxqt-app'

rm -rf "${appdir}"
install -Dm755 "${root}/target/release/linsync" "${appdir}/usr/bin/linsync"
install -Dm755 "${root}/target/release/linsync-cli" "${appdir}/usr/bin/linsync-cli"
mkdir -p "${appdir}/usr/share/linsync"
cp -R "${root}/apps/linsync-gui/qml" "${appdir}/usr/share/linsync/qml"
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

linuxdeploy --appdir "$appdir" --plugin qt --output appimage \
    --custom-apprun "$apprun_src" \
    --desktop-file "${appdir}/usr/share/applications/com.visorcraft.LinSync.desktop"

# linuxdeploy emits `LinSync-x86_64.AppImage` in the cwd; rename to
# the version-stamped path the user requested (or the default).
mv LinSync*.AppImage "$output"
echo "wrote $output"
