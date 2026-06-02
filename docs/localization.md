# Localization (l10n) pipeline

LinSync's GUI strings are marked with Qt's `qsTr(...)` in the QML
(`apps/linsync-gui/qml/`). Translation flows through the standard Qt Linguist
toolchain; no third-party translation data is bundled (see the licensing
boundary in `docs/licensing.md`).

## Source catalog

`apps/linsync-gui/i18n/linsync_en.ts` is the source-language catalog — the
canonical list of every translatable string extracted from the QML. It is the
only `.ts` checked in; compiled `.qm` binaries are git-ignored and produced at
build/package time.

Compare-result schemas (the JSON emitted by the engines / CLI) stay
**language-neutral**: localization happens only at the presentation layer (QML),
never in the core data model.

## Tooling

The recipes need Qt's `lupdate`/`lrelease` from **qt6-tools**
(`pacman -S qt6-tools` on Arch/CachyOS; `qttools5-dev-tools`/`qt6-l10n-tools`
elsewhere). The recipes probe for `lupdate6` / `lupdate-qt6` / `lupdate` (and the
`/usr/lib/qt6/bin` fallback) so they work across distros.

```sh
just l10n-update    # scan QML qsTr() strings -> refresh every i18n/*.ts
just l10n-release   # compile every i18n/*.ts -> i18n/*.qm
```

## Adding a language

1. Copy the baseline: `cp apps/linsync-gui/i18n/linsync_en.ts \
   apps/linsync-gui/i18n/linsync_<lang>.ts` (e.g. `linsync_fr.ts`).
2. Translate the `<translation>` entries (Qt Linguist or by hand).
3. `just l10n-update` keeps the catalog in sync as QML strings change;
   `just l10n-release` compiles the `.qm`.
4. Commit the `.ts`; the `.qm` is rebuilt by packaging.

## Runtime wiring

A compiled `.qm` for the active locale is loaded and installed
(`QCoreApplication::installTranslator`) in the GUI host before the QML engine
loads. `cxx-qt-lib` does not expose `QTranslator`, so the in-process `cxxqt-app`
host installs it through a small `cxx` bridge (`linsync_install_translator` in
`apps/linsync-gui/src/cxxqt_session.rs`, called from `main.rs` at startup),
which loads `linsync_<locale>.qm` from the `i18n/` dir installed alongside the
QML in the runtime data dir. Verify by running the app under a non-default
`LANG`/`LC_ALL`.
