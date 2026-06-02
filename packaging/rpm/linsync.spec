# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# RPM spec for LinSync.  Targets Fedora 40+ and the RHEL/derivative
# ecosystem (CentOS Stream, AlmaLinux, Rocky Linux) where Qt 6 and
# Kirigami are available in the system repositories.
#
# Build from this directory (the spec assumes the working tree two
# levels up):
#
#     rpmbuild --define "_topdir $(pwd)/_rpmbuild" \
#              --define "_sourcedir $(pwd)/../.." \
#              -bb linsync.spec

Name:           linsync
Version:        1.8.0
Release:        1%{?dist}
Summary:        Linux-native visual file and folder comparison

License:        GPL-3.0-only
URL:            https://github.com/visorcraft/LinSync
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  qt6-qtbase-devel
BuildRequires:  qt6-qtdeclarative-devel
BuildRequires:  kf6-kirigami-devel
BuildRequires:  pkgconf-pkg-config
# lrelease, to compile the UI translation catalogs (linsync_*.ts -> .qm).
BuildRequires:  qt6-qttools-devel

Requires:       qt6-qtbase
Requires:       qt6-qtdeclarative
Requires:       kf6-kirigami
Requires:       hicolor-icon-theme
# QtWebEngine QML module for the rendered/screenshot webpage modes (the binary
# is built with the web-engine feature).
Requires:       qt6-qtwebengine
# Fallback plugin sandbox backend for kernels without Landlock (< 5.13).
Requires:       bubblewrap

Recommends:     dolphin

%description
LinSync is a Rust + Qt 6 desktop application for file and folder diffing.
It provides side-by-side compare with synchronised scrolling, recursive
folder diff with .gitignore-aware include/exclude globs, plugin-driven
engines for text/folder/table/hex modes, and full XDG/KDE desktop
integration.

%prep
%setup -q -n %{name}-%{version}

%build
QT_VERSION_MAJOR=6 cargo build --release --workspace \
    --features 'linsync/cxxqt linsync/cxxqt-app linsync/web-engine'

%install
# Honour CARGO_TARGET_DIR if the build set it (containerised builds put
# the target/ outside the source tree to avoid host/container collisions).
target_dir="${CARGO_TARGET_DIR:-target}"
install -Dm755 "${target_dir}/release/linsync"     %{buildroot}%{_bindir}/linsync
install -Dm755 "${target_dir}/release/linsync-cli" %{buildroot}%{_bindir}/linsync-cli

install -d %{buildroot}%{_datadir}/linsync
cp -R apps/linsync-gui/qml %{buildroot}%{_datadir}/linsync/qml
install -Dm644 packaging/distro/git-mergetool.gitconfig \
    %{buildroot}%{_datadir}/linsync/git-mergetool.gitconfig

# Compile + install UI translation catalogs next to the qml tree (the host
# loads linsync_<locale>.qm from qml's sibling i18n/ dir).
_lrelease=$(command -v lrelease-qt6 || command -v lrelease6 || command -v lrelease || echo /usr/lib64/qt6/bin/lrelease)
install -d %{buildroot}%{_datadir}/linsync/i18n
for _ts in apps/linsync-gui/i18n/*.ts; do
    "$_lrelease" "$_ts" -qm "%{buildroot}%{_datadir}/linsync/i18n/$(basename "${_ts%.ts}").qm"
done

install -Dm644 packaging/com.visorcraft.LinSync.desktop      %{buildroot}%{_datadir}/applications/com.visorcraft.LinSync.desktop
install -Dm644 packaging/com.visorcraft.LinSync.metainfo.xml %{buildroot}%{_datadir}/metainfo/com.visorcraft.LinSync.metainfo.xml
install -Dm644 packaging/com.visorcraft.LinSync.mime.xml     %{buildroot}%{_datadir}/mime/packages/com.visorcraft.LinSync.xml
install -Dm644 packaging/dolphin/com.visorcraft.LinSync.desktop \
    %{buildroot}%{_datadir}/kio/servicemenus/com.visorcraft.LinSync.desktop

install -Dm644 packaging/icons/hicolor/scalable/apps/com.visorcraft.LinSync.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/com.visorcraft.LinSync.svg
for size in 16 22 24 32 36 48 64 72 96 128 192 256 512; do
    install -Dm644 packaging/icons/hicolor/${size}x${size}/apps/com.visorcraft.LinSync.png \
        %{buildroot}%{_datadir}/icons/hicolor/${size}x${size}/apps/com.visorcraft.LinSync.png
done

# third-party-notices.md is installed via the %doc macro in %files.
# Do not also copy it manually — RPM would then see an "unpackaged
# file" since the duplicate path isn't claimed by %files.

%files
%license LICENSE
%doc README.md docs/third-party-notices.md
%{_bindir}/linsync
%{_bindir}/linsync-cli
%{_datadir}/linsync/qml
%{_datadir}/linsync/i18n
%{_datadir}/linsync/git-mergetool.gitconfig
%{_datadir}/applications/com.visorcraft.LinSync.desktop
%{_datadir}/metainfo/com.visorcraft.LinSync.metainfo.xml
%{_datadir}/mime/packages/com.visorcraft.LinSync.xml
%{_datadir}/kio/servicemenus/com.visorcraft.LinSync.desktop
%{_datadir}/icons/hicolor/*/apps/com.visorcraft.LinSync.*

%post
/usr/bin/update-mime-database %{_datadir}/mime &>/dev/null || :
/bin/touch --no-create %{_datadir}/icons/hicolor &>/dev/null || :
/usr/bin/gtk-update-icon-cache --quiet %{_datadir}/icons/hicolor &>/dev/null || :
/usr/bin/update-desktop-database --quiet %{_datadir}/applications &>/dev/null || :

%postun
if [ $1 -eq 0 ]; then
    /usr/bin/update-mime-database %{_datadir}/mime &>/dev/null || :
    /bin/touch --no-create %{_datadir}/icons/hicolor &>/dev/null || :
    /usr/bin/gtk-update-icon-cache --quiet %{_datadir}/icons/hicolor &>/dev/null || :
    /usr/bin/update-desktop-database --quiet %{_datadir}/applications &>/dev/null || :
fi

%changelog
* Mon Jun 01 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.8.0-1
- Correctness, security, and robustness hardening release.
- Folder sync: fix delete operations, which previously always failed without
  removing or trashing anything.
- Three-way merge: resolve from structured conflict regions instead of
  re-parsing marker text (file content that looks like a conflict marker no
  longer corrupts the result), preserve CRLF/CR line endings on save, and
  write output atomically (temp+rename, O_NOFOLLOW).
- Plugin sandbox: grant helpers read-only access to $HOME, apply fd/proc
  rlimits on the bubblewrap fallback, and block the full set of
  credential-changing syscalls.
- Image compare: bound decoder memory and dimensions (decompression-bomb
  guard) and fix an overlay-buffer integer overflow.
- Settings and sessions preserve unknown JSON keys across load/save.
- OCR language selection now reaches the extractor plugin.
- Further fixes across table, filter, trash, webpage, CLI argument parsing,
  and the GUI bridge (comma-containing folder paths, merge conflict count,
  plugin options dialog, perceptual deltaE scaling, and plugin-id
  path-traversal hardening).

* Fri May 29 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.7.1-1
- Rename application id io.visorcraft.LinSync -> com.visorcraft.LinSync to
  match the visorcraft.com domain (desktop, metainfo, MIME, icons, D-Bus /
  Wayland app id, plugin ids); fix stray visorcraft.io references.

* Fri May 29 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.7.0-1
- Security hardening: the plugin sandbox now fails closed when no backend
  (Landlock/bubblewrap) is available and reduces the helper environment to a
  safe allowlist; streaming helpers run under the same policy; web-fetch pins
  the validated address at connect time to defeat DNS rebinding; folder
  copy/delete refuse symlinked paths that escape the comparison root; the GUI
  bridge token file is written owner-only (0600)
- About page: real LinSync app icon and unified "Visit LinSync" button styling

* Thu May 28 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.6.0-1
- Webpage compare is now fully functional: sandbox grants network plugins
  read access to resolver config + TLS trust store (fixes getaddrinfo
  EAI_AGAIN), web-fetch sends a browser User-Agent, and results render as a
  side-by-side editor diff (cursor, selection, keyboard nav) with a
  diff-overview ruler
- Plugins page: built-in (always-on) toggles render with a themed track
  instead of black

* Thu May 28 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.5.0-1
- Version bump to 1.5.0
- Compare pages (Image / Webpage / Document) now fully theme-aware: correct
  Kirigami.Theme propagation, themed App* form controls (no stray dark borders)
- Image Compare: Left/Right file pickers added; Tolerance/ΔE spin boxes match
  the Settings styling
- Webpage Compare layout redesigned; Document Compare restyled to match the
  other compare pages
- Shared AppButton control; sidebar hover flash removed

* Thu May 28 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.4.0-1
- Version bump to 1.4.0
- QML diff-pane simplification: per-row TextArea → single Label (3× fewer nodes)
- Binding loops resolved across 7 QML pages
- Compare profiles: HTTP bridge fully profile-aware, cxx-qt fixup landed
- Phase 1 core + CLI + 7 built-in profiles shipped

* Wed May 27 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.3.1-2
- Fixed QML ScrollBar namespace crash on launch
- Default to external qml6 runner (bypass fragile cxx-qt in-process host)
- Pass bridge URL via environment variable
- New compare dialog with file-browse and raw-text-paste tabs
- Removed 2000-line cap on comparison results
- Fixed scroll jitter (debounced pane sync, Canvas overview ruler)
- Full keyboard navigation (Up/Down/Home/End/PgUp/PgDn)
- Text selection support in diff panes
- Pre-computed color theme cache for faster delegate painting
- LRU text-transform cache avoids regex per-line per-frame
- ListView reuseItems + cacheBuffer for instant item recycling
- 256 KB bridge request limit for raw-text paste

* Wed May 27 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.2.1-1
- New compare dialog with file-browse and raw-text-paste tabs
- Removed 2000-line cap on comparison results
- Fixed scroll jitter (debounced pane sync, Canvas overview ruler)
- Full keyboard navigation (Up/Down/Home/End/PgUp/PgDn)
- Text selection support in diff panes
- Pre-computed color theme cache for faster delegate painting
- LRU text-transform cache avoids regex per-line per-frame
- ListView reuseItems + cacheBuffer for instant item recycling
- 256 KB bridge request limit for raw-text paste

* Wed May 27 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.1.1-1
- Security: SSRF defenses for the bundled web-fetch plugin. Outbound requests are restricted to http/https; the resolved host must be a publicly-routable address (loopback, RFC1918, link-local, unique-local, multicast, unspecified, and reserved ranges are refused). Redirect targets are revalidated against the same policy. The opener no longer registers handlers for file://, ftp://, or data:.
* Wed May 27 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.1.0-1
- New compare modes: image (pure-Rust pixel + CIEDE2000 perceptual diff), document (PDF/DOCX/ODT via Tesseract OCR + Poppler + LibreOffice helpers), webpage (rendered DOM diff via Qt WebEngine, feature-gated).
- Sandbox foundation: new linsync-sandbox crate wires Landlock + seccompiler with a bubblewrap fallback and a degraded mode for plugin helpers.
- Plugin host: process-group propagation prevents soffice/poppler grandchild leaks; killpg on shutdown.
- Screenshot pipeline: launch-context startup-section bridge lets gui-screenshot.sh capture each sidebar section deterministically.
* Tue May 26 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.0.1-1
- GUI wiring complete for Settings/Filters/Plugins pages.
- Filter grammar: de:/e: expression families parse and evaluate; linsync-cli filter migrate for legacy .flt files.
- Merge UI: dedicated three-pane view with save-to-third-target; linsync-cli mergetool subcommand.
- Plugin protocol: unpack_folder operation, streaming output, bundled ZIP/tar unpackers.
- Text compare: moved-block detection (opt-in via settings).
- Accessibility audit + 11 P0 fixes; per-section GUI screenshots in CI.
* Tue May 19 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.0.0-1
- Initial RPM packaging stub for LinSync 1.0.0.
