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
Version:        1.9.5
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
    --features 'linsync/cxxqt linsync/cxxqt-app linsync/web-engine linsync-cli/web-engine'

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
* Wed Jun 03 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.9.5-1
- Align Image, Webpage, and Document Compare top controls with the main Compare page toolbar style.
- Match the Sessions page header layout to the Plugins page header.

* Wed Jun 03 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.9.4-1
- Stop auto-defaulting *any* previous paths (from recent sessions, last launch, /tmp/ dev fixtures like bigfolder, or anything else) into the Compare page's Left and Right input fields on bare GUI startup. The "Restore last session" setting and recent-sessions store are still present for explicit resume via the Sessions sidebar and re-open; they no longer mutate the primary path editors or pre-load a diff view on launch. This eliminates the ridiculous "defaults to fake folder names" experience entirely. Bare launches (the common case) now always start with blank editors showing the nice placeholders ("Left file or folder", "Right file or folder"), ready for the user to choose fresh paths. Only explicit CLI `linsync left right`, drag-and-drop, browse, or explicit reopen from Sessions/projects will populate the fields.
- The root cause was the combination of open_last_session=true (default) + using the most-recent SessionFile to synthesize a launch context that drove the QML path properties.

* Wed Jun 03 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.9.3-1
- UX fix for Compare page defaults: paths under the source tree's tests/fixtures/ (used by gui-smoke.sh, release-smoke, unit tests, and manual dev launches like `cargo run -p linsync -- <fixture>`) are now excluded from recent-paths and recent-sessions recording, and are filtered out on load/restore/reopen. This prevents bare GUI launches (no args, with open_last_session=true) from pre-filling the Left/Right editors with ugly internal absolute paths such as .../tests/fixtures/folders/left on every startup. The Sessions page list and reopen-by-index also hide them. (The root cause was that fixture folders are valid comparable data, so the old "persistable" guard let dev usage pollute the user's XDG state dir.)
- Updated the affected unit tests (that asserted recording of fixture launches) to use clean temp files from test_file_root() instead.

* Wed Jun 03 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.9.2-1
- CI and test reliability: set LINSYNC_SANDBOX_SKIP=1 in the Justfile test recipe (and documented in ci.yml, sandbox_integration.rs) so `just ci` / `just test` pass consistently; real enforcement tests exercised by unsetting the var on a good kernel+env.
- Packaging fixes: build-appdir.sh now exports QMAKE pointing at qmake6/qmake-qt6 before linuxdeploy --plugin qt (prevents Qt5 plugin misdetection in mixed environments); Justfile package: defaults to NO_STRIP=1 (workaround for linuxdeploy's internal strip choking on relr.dyn objects from host libs); `just package` and the makepkg path now produce artifacts cleanly.
- Release gate: extend docs/parity-acceptance.md with explicit rows for Specialized compare — image/document/webpage/archive and Plugin sandbox so check-parity-acceptance.py (and thus release-smoke.sh) passes.
- Minor: fix stale git index stat on tests/fixtures/document/simple.odt after regeneration in smokes.

* Tue Jun 02 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.9.1-1
- Third-party attribution: the in-app Credits and Licenses pages and the
  bundled third-party notices now list every crate distributed in the release
  build (the cxx-qt bridge and image-decoder stacks were previously unlisted)
  and bundle the full verbatim Apache-2.0 and Zlib license texts.
- A `just credits` regenerator and a release/CI drift-guard keep the three
  attribution surfaces in sync with the dependency graph.
- Documentation and internal tooling cleanup.

* Tue Jun 02 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.9.0-1
- Completes the previously-deferred roadmap niceties.
- Image: animated GIF/APNG/WebP frame-by-frame compare (--image-frames
  first|all); Radiance HDR and OpenEXR decode (tone-mapped to RGBA8); decoded
  color-type metadata on every result.
- Document/OCR: word-level positional data (optional, backward-compatible
  word_positions plugin-protocol field) parsed from Tesseract TSV.
- Plugins: per-profile enable/disable override map (profile > global >
  default), threaded into prediffer/virtualizer routing with a GUI toggle and
  bridge endpoint; overlapping-prediffer conflict policy
  (--prediffer-conflict-policy chain|first-wins|last-wins).
- Reports: image and document results round-trip as versioned JSON
  (compare --save-result <-> report --from-json) with metadata HTML reports.
- Fixes ImageCompareMode::Tolerance serialization (struct variant).

* Tue Jun 02 2026 VisorCraft LLC <licensing@visorcraft.com> - 1.8.1-1
- Feature release completing roadmap Phases 4-10.
- Performance: lazy windowed rendering for large text diffs and large folder
  comparisons; paged core APIs and bridge endpoints (/compare/text/window,
  /folder/query with sort/filter/state/paging).
- Engines: rendered document compare (pdftoppm) with --document-pages range
  selection; webpage rendered/screenshot via an out-of-process Qt WebEngine
  renderer + filterable resource tree; archive compare via unpacker/virtualizer
  plugins with nested-archive recursion; table date/time tolerance + per-column
  case/trim/regex rules.
- Plugins: option-schema validation + option/enabled store; install/remove
  across core/CLI/GUI; trust prompt; per-profile prediffer routing + chaining;
  sandbox-confinement reporting and per-plugin diagnostics.
- Merge: interactive Git mergetool that launches the GUI and validates output.
- Reports: versioned save-result/from-json for text/folder/table/binary;
  --relative-paths portable reports; preview-before-export.
- Sessions/projects: multi-tab persistence + restore; CLI session and project
  commands; per-entry compare profiles; recent-workspaces; privacy control.
- Accessibility: screen-reader status/error announcements and high-contrast
  diff change bars. Localization: runtime QTranslator + lupdate/lrelease.

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
