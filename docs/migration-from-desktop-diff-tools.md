# Migrating From Desktop Diff Tools

LinSync is a Linux-native comparison application. It uses a Rust core, a CLI,
and a Qt/Kirigami GUI rather than Windows shell extensions, registry-backed
settings, or in-process Windows plugin models.

## What Carries Over

- Side-by-side text compare and merge workflows.
- Folder comparison with filters, metadata checks, and content methods.
- Patch/report generation.
- Session/project style saved state through portable JSON files.
- External helper support through an explicit plugin protocol.

## Linux-Specific Differences

- Configuration, recent paths, sessions, and projects are stored under XDG
  directories.
- File-manager integration uses FreeDesktop APIs, `xdg-open`, and
  desktop-entry metadata.
- Delete workflows prefer FreeDesktop Trash and require explicit confirmation
  before permanent deletion when Trash cannot be used.
- Plugins are external helper processes with manifest validation and bounded
  IO, not in-process DLL/scriptlet extensions.
- Flatpak and other sandboxed packages may require portals or extra helper
  permissions.

## Compatibility Policy

LinSync can provide migration diagnostics for common rule families and workflows,
but it does not copy source code, assets, filters, translations, or plugin
implementations from other applications. Any compatibility data added later must
have a file-specific license review and tests.
