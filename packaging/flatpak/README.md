# Flatpak Packaging

Flatpak packaging uses the KDE runtime and installs the GUI, CLI, desktop file,
AppStream metadata, shared MIME package, and scalable icon.

The current manifest is a development stub that builds from the repository
source directory. A release-ready Flatpak still needs generated or vendored
Cargo sources so the build does not fetch crates from the network inside the
Flatpak build context.

The current manifest grants:

- Wayland access and fallback X11 support.
- IPC sharing required by common desktop integration paths.
- Home filesystem access so users can compare arbitrary local files and folders
  during the early desktop-app phase.

Before release, review whether home access can be narrowed through portals or
documented sandbox prompts. Host helper execution, archive tools, external
editors, and file-manager service menus may need additional permissions or
non-Flatpak fallback documentation.
