# Linux Decisions

LinSync is Linux-only. Windows-specific implementation details from other tools are
behavioral references only and are replaced with Linux-native equivalents.

- Windows Explorer shell extensions are replaced with Dolphin service menus and
  FreeDesktop-compatible file-manager integration where practical. Current
  file-manager integration notes are in `docs/file-manager-integration.md`.
- Registry settings are replaced with XDG config/data/cache/state paths.
  `docs/settings-storage-decision.md` records the JSON-over-KConfig decision for
  early releases.
- COM plugins are replaced with a Linux-native helper process protocol.
- Windows installers are replaced with Flatpak, AppImage, and distro packaging.
- Windows path behavior is replaced with mounted Linux paths and portal-aware
  file dialogs.
