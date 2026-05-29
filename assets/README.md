# assets/

Master imagery for LinSync. These are the canonical sources — every
other reproduction of the logo across the repository (the FreeDesktop
hicolor tree under `packaging/icons/`, the QML window-icon at
`apps/linsync-gui/qml/assets/`, packaging recipes, social cards) is a
derived copy that must trace back to these files.

| File | Size | Purpose |
| ---- | ---- | ------- |
| `LinSync.svg` | scalable | Source-of-truth vector. Render to whatever size you need with `rsvg-convert -w <px>` or `magick`. |
| `LinSync.png` | 1024×1024 | Master raster — high-resolution PNG for documentation, slide decks, the README hero, and any consumer that cannot read SVG. |
| `LinSync.ico` | 16/32/48/64/128/256 | Multi-resolution Windows-style icon, used for GitHub repo display and any tooling that prefers `.ico` (favicons, etc.). |
| `social-1024x512.png` | 1024×512 | GitHub social preview / OpenGraph card. Upload via **Settings → Social preview** on github.com. |
| `splash-screen.png` | 800×500 | Reserved for a future GUI splash. Same palette as the social card. |

## Regenerating from the SVG

```sh
# 1024-px master PNG.
rsvg-convert -w 1024 -h 1024 assets/LinSync.svg -o assets/LinSync.png

# Multi-resolution ICO.
for s in 16 32 48 64 128 256; do
  rsvg-convert -w $s -h $s assets/LinSync.svg -o /tmp/linsync-$s.png
done
magick /tmp/linsync-{16,32,48,64,128,256}.png assets/LinSync.ico
rm /tmp/linsync-*.png
```

## Where the per-distro icons live

The FreeDesktop hicolor icon tree at
`packaging/icons/hicolor/<size>x<size>/apps/com.visorcraft.LinSync.png`
(plus the scalable SVG copy) is dictated by spec and referenced by every
packaging recipe (`packaging/arch/PKGBUILD`, `packaging/debian/rules`,
`packaging/rpm/linsync.spec`, `packaging/appimage/build-appdir.sh`).
Do **not** move those files; if the master art changes, re-export from
`assets/LinSync.svg` into the existing hicolor paths so packaging stays
intact.
