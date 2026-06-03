# Scripts

- `release-smoke.sh`: validates desktop metadata, AppStream metadata, MIME XML,
  AppImage shell scripts, shared-MIME cache generation, third-party notices (via
  the drift guard), and parity-acceptance checklist coverage.
- `generate-credits.py`: the single source of truth for third-party crate
  attribution. Computes the exact set shipped in release binaries (via the
  locked `cargo tree` over the `cxxqt-app`+`web-engine` feature set), writes
  `docs/third-party-crates.json`, and refreshes the generated blocks in the
  notices md and the two in-app QML pages. `just credits` / `just credits-update`
  are thin wrappers; `scripts/check-credits.sh` delegates verify to it.
- `check-credits.sh`: thin guard used by release-smoke (and locally). Fails if
  the committed JSON is not up to date with the current graph.
- `gui-smoke.sh`: launches the QML compare workspace offscreen for text and
  folder fixtures when a Qt QML runner is installed. Set
  `LINSYNC_GUI_SMOKE_CXXQT=1` to also cover the feature-gated in-process
  `cxx-qt` host when Qt development headers and `qmake6` are installed.
- `check-parity-acceptance.py`: ensures every feature-coverage area has
  a corresponding release-checklist row.

Planned scripts:

- Localization extraction.
- Fixture generation.
- Screenshot smoke checks.
- Release packaging.
