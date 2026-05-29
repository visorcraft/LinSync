# Debian Packaging

Debian packaging files for LinSync.  Targets Debian 12+ and Ubuntu 24.04+
(any release that ships `debhelper-compat = 13`).

## Build

From the repository root:

```sh
sudo apt install build-essential debhelper-compat cargo rustc \
                 qt6-base-dev qt6-declarative-dev \
                 qml6-module-org-kde-kirigami pkg-config
dpkg-buildpackage -us -uc -b
```

This produces `../linsync_1.0.1-1_amd64.deb` next to the source tree.

`debian/rules` builds the workspace with the `cxxqt` + `cxxqt-app`
features enabled and installs:

- `/usr/bin/linsync` and `/usr/bin/linsync-cli`
- `/usr/share/linsync/qml/` (the bundled QML module tree)
- `/usr/share/applications/com.visorcraft.LinSync.desktop`
- `/usr/share/metainfo/com.visorcraft.LinSync.metainfo.xml`
- `/usr/share/mime/packages/com.visorcraft.LinSync.xml`
- `/usr/share/kio/servicemenus/com.visorcraft.LinSync.desktop`
- the hicolor icons under `/usr/share/icons/hicolor/*/apps/`
- `/usr/share/doc/linsync/third-party-notices.md`

## Notes

- The full Cargo test suite is skipped during package builds; CI runs
  `cargo test --workspace` against the same source tree before the
  release artefact is produced.
- `debian/copyright` records LinSync's GPL-3.0-only licensing and
  points to the third-party Cargo dependency manifest for permissive
  upstream credits.
- This stub does not (yet) maintain a debian-source orig tarball or
  vendored Cargo sources; release-grade packaging should add a
  `debian/source/format` of `3.0 (quilt)` and run `cargo vendor` so
  the build does not fetch crates from the network.
