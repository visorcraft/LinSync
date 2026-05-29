# File Manager Integration

LinSync replaces Windows Explorer shell-extension behavior with Linux desktop
integration points.

## Dolphin

Dolphin/KDE is the first-class target. Packaging includes a Dolphin service-menu
file under `packaging/dolphin/` and distro packages should install it into the
desktop's service-menu path only when the KDE integration package is enabled.

Current service-menu behavior is intentionally simple:

- Open selected files or folders in LinSync.
- Defer the full select-for-compare workflow until the GUI can receive and
  persist file-manager handoff state.

The CLI also provides `open-external` and `reveal` commands for scripts and
fallback integration. `open-external --preset` covers common Linux editor
launchers: Kate, KWrite, VS Code, VSCodium, GNOME Text Editor, Sublime Text, a
Neovim terminal wrapper, `xdg-open`, and JetBrains launcher commands.

`reveal` uses `org.freedesktop.FileManager1.ShowItems` when the desktop exposes
that DBus interface. If the call fails or the interface is unavailable, it opens
the containing folder with `xdg-open`. Scripts can still set `LINSYNC_REVEAL`
to force a file-manager-specific command.

## GNOME Files / Nautilus

Nautilus integration should wait until after the core app behavior and GUI
handoff are stable. Feasible approaches are:

- A small `nautilus-python` extension that shells out to `linsync` or
  `linsync-cli launch`.
- A distro-packaged extension with conservative dependencies and no bundled
  Python modules.
- No extension by default, relying on `Open With`, `xdg-open`, and CLI
  integration if maintenance or sandbox constraints outweigh the benefit.

Before adding a Nautilus extension, verify:

- Current Nautilus extension API stability in target distros.
- Packaging policy for Python extensions in Flatpak and distro packages.
- How selected files are passed, including multiple selection and directories.
- Whether the extension can support select-for-compare without fragile global
  state.

Until those checks are complete, Nautilus support remains a documented
post-MVP investigation rather than a shipped integration.
