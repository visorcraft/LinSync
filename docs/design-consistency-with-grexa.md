# Design Consistency With Grexa

LinSync follows Grexa's Linux product family direction:

- Rust core with Qt 6/QML/Kirigami GUI.
- KDE Plasma first, without intentionally breaking other Linux desktops.
- Dense, quiet utility UI with splitters, panes, grids, and restrained color.
- Grexa-aligned appearance choices: follow system, light, dark, OLED black, and
  the named high-contrast/accent palettes Gentle Gecko, Black Knight, Diamond,
  Dreams, Paranoid, Red Velvet, Subspace, Tiefling, and Vibes, persisted with
  Grex/Grexa's numeric `themePreference` contract.
- Icon-first command strips with Breeze icon names, tooltips, and accessible
  names.
- Themed `App*` QML control wrappers for form controls where
  `qqc2-desktop-style` does not reliably inherit page-level palette overrides.
- XDG paths for user data and logs.
- Flatpak-first packaging with AppImage as a secondary target.
