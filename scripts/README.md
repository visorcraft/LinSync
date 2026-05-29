# Scripts

- `release-smoke.sh`: validates desktop metadata, AppStream metadata, MIME XML,
  AppImage shell scripts, shared-MIME cache generation, third-party notices, and
  parity-acceptance checklist coverage.
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
