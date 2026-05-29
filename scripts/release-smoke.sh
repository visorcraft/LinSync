#!/usr/bin/env bash
set -euo pipefail

# AppRun is now generated inline by build-appdir.sh via a heredoc
# (the linuxdeploy-based flow doesn't ship a separate AppRun file
# anymore). Syntax-checking build-appdir.sh covers both.
bash -n packaging/appimage/build-appdir.sh

tmpdata="$(mktemp -d)"
trap 'rm -rf "${tmpdata}"' EXIT
prefix="${tmpdata}/prefix"
install -Dm644 packaging/com.visorcraft.LinSync.desktop "${prefix}/share/applications/com.visorcraft.LinSync.desktop"
install -Dm644 packaging/com.visorcraft.LinSync.metainfo.xml "${prefix}/share/metainfo/com.visorcraft.LinSync.metainfo.xml"
install -Dm644 packaging/com.visorcraft.LinSync.mime.xml "${prefix}/share/mime/packages/com.visorcraft.LinSync.xml"
install -Dm644 packaging/icons/hicolor/scalable/apps/com.visorcraft.LinSync.svg "${prefix}/share/icons/hicolor/scalable/apps/com.visorcraft.LinSync.svg"
for size in 16 22 24 32 36 48 64 72 96 128 192 256 512; do
    install -Dm644 "packaging/icons/hicolor/${size}x${size}/apps/com.visorcraft.LinSync.png" \
        "${prefix}/share/icons/hicolor/${size}x${size}/apps/com.visorcraft.LinSync.png"
done
mkdir -p "${prefix}/share/linsync"
cp -R apps/linsync-gui/qml "${prefix}/share/linsync/qml"

desktop-file-validate "${prefix}/share/applications/com.visorcraft.LinSync.desktop"
appstreamcli validate --no-net "${prefix}/share/metainfo/com.visorcraft.LinSync.metainfo.xml"
xmllint --noout "${prefix}/share/mime/packages/com.visorcraft.LinSync.xml"
XDG_DATA_HOME="${prefix}/share" update-mime-database "${prefix}/share/mime" >/dev/null
test -f "${prefix}/share/mime/mime.cache"
test -s "${prefix}/share/linsync/qml/Main.qml"
test -s "${prefix}/share/linsync/qml/assets/com.visorcraft.LinSync.png"
test -s "${prefix}/share/icons/hicolor/512x512/apps/com.visorcraft.LinSync.png"

test -s docs/third-party-notices.md
grep -q "## Source Offer" docs/third-party-notices.md
python3 scripts/check-parity-acceptance.py
bash scripts/gui-smoke.sh
