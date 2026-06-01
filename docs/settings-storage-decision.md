# Settings Storage Decision

LinSync uses inspectable JSON files under XDG base directories for settings,
filters, sessions, projects, recent paths, recent sessions, logs, and cache
metadata.

## Decision

Keep JSON storage as the default for early releases. This matches the current
Grexa-aligned direction, keeps user data portable, and makes migration and bug
reports easier because users can inspect files without a KDE-specific tool.

KConfig remains a possible future integration only if it provides a clear KDE
desktop benefit that outweighs portability and schema-migration complexity.

## Current Paths

Core `AppPaths` resolves:

- `$XDG_CONFIG_HOME/linsync/settings.json`
- `$XDG_CONFIG_HOME/linsync/filters.json`
- `$XDG_DATA_HOME/linsync/recent-paths.json`
- `$XDG_DATA_HOME/linsync/recent-sessions.json`
- `$XDG_DATA_HOME/linsync/sessions/`
- `$XDG_DATA_HOME/linsync/projects/`
- `$XDG_DATA_HOME/linsync/plugins/`
- `$XDG_CACHE_HOME/linsync/comparisons/`
- `$XDG_STATE_HOME/linsync/linsync.log`

## GUI Surface

The Settings sidebar page (`apps/linsync-gui/qml/SettingsPage.qml`) currently
emits a `settingChanged(key, value)` signal for the following keys, grouped
into four cards:

- Appearance: `themePreference`, `fontFamily`, `fontSize`, `tabWidth`,
  `showLineNumbers`, `showWhitespace`, `wordWrap`, `reduceMotion`.
  `themePreference` uses the Grex/Grexa integer contract: `0` system, `1` light, `2` dark,
  `3` Gentle Gecko, `4` Black Knight, `5` Diamond, `6` Dreams, `7` Paranoid,
  `8` Red Velvet, `9` Subspace, `10` Tiefling, `11` Vibes, `12` OLED Black.
- Comparison behavior: `defaultCompareMode`, `ignoreCase`, `ignoreWhitespace`,
  `ignoreBlankLines`, `ignoreEol`, `eolNormalization`.
- Session: `openLastSession`, `confirmOnClose`, `persistRecentPaths`,
  `maxRecentPaths`.
- Storage: open-config-folder shortcut and a `Kirigami.PromptDialog`-gated
  reset that emits `__reset`.

These keys are persisted through the Rust bridge into
`$XDG_CONFIG_HOME/linsync/settings.json` via `linsync-core::SettingsStore`.
Legacy string theme keys such as `dark`, `oled-black`, and `high-contrast`
are accepted on import, but new writes use the Grex/Grexa numeric format.

## Rules

- Keep storage human-readable and resilient to unknown future fields.
- Preserve schema-versioned migration for settings.
- Persist window size in settings when the GUI supplies it, but do not persist
  window placement. Placement remains under the window manager's control.
- Avoid opaque databases unless a future feature proves JSON cannot satisfy
  correctness or performance requirements.
- Do not use KConfig solely for platform branding; use it only if it improves
  real desktop behavior.
