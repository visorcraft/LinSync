# Distro Packaging Index

LinSync ships first-class packaging recipes for the following targets:

| Target            | Recipe                                | Tooling                         |
| ----------------- | -------------------------------------- | -------------------------------- |
| AppImage          | `packaging/appimage/build-appdir.sh`  | `linuxdeploy`, `linuxdeploy-plugin-qt`, `qmake6` |
| Flatpak           | `packaging/flatpak/com.visorcraft.LinSync.yml` | `flatpak-builder`, KDE Platform//6.10, rust-stable//25.08 |
| Arch / CachyOS    | `packaging/arch/PKGBUILD`             | `makepkg`                       |
| Debian / Ubuntu   | `packaging/debian/`                   | `dpkg-buildpackage`, `debhelper`|
| Fedora / RHEL     | `packaging/rpm/linsync.spec`          | `rpmbuild`                      |

Every recipe targets the workspace at version **1.11.0** and builds with the
`cxxqt` + `cxxqt-app` features so the Qt 6 / Kirigami UI is included.

These same recipes are driven automatically by
[`.github/workflows/release.yml`](../../.github/workflows/release.yml) on every
tag push: the workflow runs the AppImage, Arch, Debian, and RPM builds in
their native environments (Arch container for AppImage + pacman, Ubuntu host
for `.deb`, Fedora container for `.rpm`) and uploads the artefact bundle plus
`sha256sums.txt` to a GitHub Release. See `CONTRIBUTING.md` → *Releases* for
the step-by-step cut procedure.

## Common install layout

All recipes install:

- `linsync` to the desktop application binary path (`/usr/bin/linsync`).
- `linsync-cli` to the command-line binary path (`/usr/bin/linsync-cli`).
- The bundled QML tree under `share/linsync/qml`.
- `packaging/com.visorcraft.LinSync.desktop` under `share/applications`.
- `packaging/com.visorcraft.LinSync.metainfo.xml` under `share/metainfo`.
- `packaging/com.visorcraft.LinSync.mime.xml` under
  `share/mime/packages` (renamed to `com.visorcraft.LinSync.xml`).
- `packaging/icons/hicolor/scalable/apps/com.visorcraft.LinSync.svg` and
  the sized PNG icons under `packaging/icons/hicolor/<size>x<size>/apps/`
  into the matching hicolor icon directories.
- `packaging/dolphin/com.visorcraft.LinSync.desktop` under
  `share/kio/servicemenus/` for KDE Plasma's Dolphin service menu (only
  in distributions that ship Dolphin).
- `docs/third-party-notices.md` under
  `share/doc/linsync/third-party-notices.md` for the permissively
  licensed crate manifest.

Post-install hooks should refresh the desktop, MIME, icon, and AppStream
caches according to distribution policy.  Build recipes must preserve
the **GPL-3.0-only** license metadata and ship the third-party Cargo
notices required by the dependency set.

## Adding a new distro

1. Create `packaging/<distro>/` with the distro-native recipe.
2. Reference the install layout above.
3. Add a row to the table at the top of this file.
4. Update `AGENTS.md` if the new recipe introduces a new `just`
   target or CI hook.
