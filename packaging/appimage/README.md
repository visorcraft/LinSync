# AppImage Packaging

The AppImage build uses `linuxdeploy` + `linuxdeploy-plugin-qt` to
bundle a self-contained Qt 6 stack so the
resulting `.AppImage` runs on any glibc-modern Linux without needing
the host to ship Qt. The bundle uses the in-process cxx-qt host
(`linsync/cxxqt linsync/cxxqt-app` feature combo) so QML loads
directly from the binary without an external `qml6` subprocess.

```sh
bash packaging/appimage/build-appdir.sh
# or
just package
```

Output: `target/appimage/LinSync-<version>-x86_64.AppImage`. If
`linuxdeploy` isn't on `$PATH`, the script falls back to staging the
`AppDir` only (no `.AppImage`) and prints an install pointer for the
two tools.

## Prerequisites

- `linuxdeploy` and `linuxdeploy-plugin-qt` from
  <https://github.com/linuxdeploy/>. Both are AppImages — drop them
  on `$PATH` under unsuffixed names.
- `qmake6` (the plugin auto-detects this when set as the `QMAKE` env
  var, e.g. `QMAKE=/usr/bin/qmake6`).
- The `jxrlib` system package on hosts whose `kimageformats` Qt
  plugin bundles `kimg_jxr.so`. Without it, the qt-plugin step exits
  with `Could not find dependency: libjxrglue.so.0`.
- On hosts whose `librsvg` (≥ 2.62 on Arch) dropped the gdk-pixbuf
  SVG loader, `linuxdeploy`'s bundled `strip` can't read modern
  `relr.dyn`-only ELF objects. Set `NO_STRIP=1` to skip the strip
  step; the resulting AppImage is functionally identical and only
  slightly larger.
