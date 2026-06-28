# Arch / CachyOS Packaging

LinSync ships a hand-rolled `PKGBUILD` for `pacman`-based distributions —
Arch Linux, CachyOS, EndeavourOS, Manjaro, and friends.

The recipe builds the GUI + CLI from the in-tree source, installs the
hicolor icons, desktop file, AppStream metainfo, MIME registration, and
the Dolphin service menu.

## Build

From this directory:

```sh
makepkg -si
```

`makepkg` will build a `.pkg.tar.zst` archive into the same directory and
optionally install it (`-i`).

The PKGBUILD references the repository two levels up (`../..`); if you
want to publish it on the AUR, replace the `_srcdir=` block with an
appropriate `source=("$pkgname-$pkgver.tar.gz::https://...")` entry plus
matching `sha256sums`.

## Runtime dependencies

- `qt6-base`, `qt6-declarative` — the Qt 6 runtime LinSync links against.
- `kirigami` — KDE's Quick Controls style.
- `hicolor-icon-theme` — fallback icon resolution.

## Optional integration

- `dolphin` — installs the Dolphin "Compare with LinSync" service menu
  under `/usr/share/kio/servicemenus/`.

## Source-tree assumption

This recipe is intentionally checked into `packaging/arch/` for in-tree
builds and as a reference for downstream packagers.  AUR publication
should mirror it under a dedicated repository with the proper
`source=`/`sha256sums=` fields and an explicit `pkgver()` function for
VCS tracking.
