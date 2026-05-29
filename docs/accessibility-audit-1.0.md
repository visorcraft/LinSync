# Accessibility Audit — LinSync 1.0

> Auditor: automated read-through of all QML source files. A final audit before
> tagging 1.0 should include a manual screen-reader pass with Orca on a live
> Wayland/X11 session.

> Date: 2026-05-26

## Methodology

Each QML file under `apps/linsync-gui/qml/` was read in full. For every
interactive control (`Button`, `ToolButton`, `Switch`, `SpinBox`, `ComboBox`,
`TextField`, `CheckBox`, nav items, dialog buttons) the following was checked:

- Does the control have `Accessible.name` set, or is one derivable from a
  visible `text:` property that Qt will forward to AT?
- For icon-only buttons (`display: IconOnly` or no text), is `Accessible.name`
  explicitly set?
- For custom composite items (e.g. `LinSyncNavItem`) that use a `MouseArea`
  instead of a focusable `AbstractButton`, is there a keyboard path?
- Do modal dialogs have `closePolicy: CloseOnEscape` (or equivalent) so
  keyboard users can dismiss them?
- Does tab order follow visual reading order? No explicit `KeyNavigation`
  overrides were found in any page; QML's default form-layout tab order is
  top-to-bottom which matches the visual layout.

Contrast and screen-reader runtime behaviour were not verified — see the
cross-cutting section below.

---

## Findings by page

### Compare (Main.qml)

All toolbar `ToolButton` items in the main compare toolbar and the merge
toolbar carry both `Controls.ToolTip.text` and `Accessible.name`. The path bar
`AppTextField` controls carry `Accessible.name: "Left path"` /
`"Right path"`. The mode `AppComboBox` carries `Accessible.name: "Compare mode"`.

The find bar `AppTextField` carries `Accessible.name: "Find text"`. The
Previous/Next match buttons carry `Accessible.name`.

The tab bar `Controls.TabButton` items derive their accessible name from
`text:` (the tab title). The Close current tab `ToolButton` carries
`Accessible.name: "Close current tab"`.

The comparison pane `ListView` rows are display-only labels — no interactive
controls inside them, so no accessible name needed.

The overview pane diff markers use a `MouseArea` — these are decorative jump
shortcuts with no keyboard equivalent. See P1 below.

**Dialogs:**

- `reloadDirtyDialog`: has `closePolicy: CloseOnEscape | CloseOnPressOutside`.
  Buttons carry `text:` labels. OK.
- `folderOpDialog`: same close policy. Buttons carry `text:`. OK.
- `closeDirtyDialog`: same close policy. Buttons carry `text:`. OK.
- `openLeftThenRightDialog` / merge file pickers: native platform
  `FileDialog`/`FolderDialog` — accessibility is the platform's
  responsibility. Titles are set. OK.

Findings:

- [P1] Main.qml:2674 — Overview pane diff-marker `MouseArea` rectangles have
  no keyboard equivalent. A keyboard user cannot click individual diff markers
  to jump; they must use F7/F8 shortcuts instead. The shortcuts exist and are
  documented, so this is a usability gap, not a complete block.

- [P1] Main.qml:2307 — "Swap sides" `ToolButton` carries `Accessible.name`
  but has no `onClicked` handler wired. The button is visually present but
  non-functional. Accessibility is moot until the feature is implemented.

### Sessions (SessionsPage.qml)

Findings before this audit:

- [P0 — FIXED] SessionsPage.qml:338 — "Copy path" `ToolButton` in the Recent
  paths list was icon-only with no `text:` and no `Accessible.name`. A screen
  reader would announce only the icon name or nothing. Fixed by adding
  `text: qsTr("Copy path")`, `display: IconOnly`, and
  `Accessible.name: qsTr("Copy path")`.

Tab switch / close `Button` items inside "Open tabs" use
`display: IconOnly` and have `text:` set (`"Switch"`, `"Close"`). Qt surfaces
`text` as the accessible name for `AbstractButton`, so these are acceptable.
The `Reopen` button in "Recent comparisons" has plain `text: qsTr("Reopen")`.

### Filters (FiltersPage.qml)

Findings before this audit:

- [P0 — FIXED] FiltersPage.qml:183 — Include-rule chip delete `ToolButton`
  was icon-only with no `text:` and no `Accessible.name`. Fixed: now carries
  `text: qsTr("Remove include pattern %1")` and matching `Accessible.name`.

- [P0 — FIXED] FiltersPage.qml:276 — Exclude-rule chip delete `ToolButton`
  same issue. Fixed the same way.

- [P0 — FIXED] FiltersPage.qml:578 — Saved-filter row delete `ToolButton`
  was icon-only with no `Accessible.name`. Fixed: now carries
  `text: qsTr("Delete filter %1")` and matching `Accessible.name`.

Additional findings:

- [P1 — FIXED] FiltersPage.qml:310 — Quick-add exclude buttons use `text: modelData`
  (the glob string, e.g. `".git/**"`). This is technically accessible — the
  glob text is the label — but the button purpose ("add to excludes") is
  implicit. Fixed: added `Accessible.name: qsTr("Add %1 to exclude patterns").arg(modelData)`
  and `Accessible.description: qsTr("Quickly add a common exclude pattern")`.

- [P1 — FIXED] FiltersPage.qml:402 — `AppSpinBox` for maximum walk depth had no
  `Accessible.name` and no `Kirigami.FormData.label`. The adjacent
  `Controls.Label { text: "Maximum depth" }` is not linked to the spinbox.
  Fixed: added `Accessible.name: qsTr("Maximum walk depth")` directly on the spinbox.

- [P1 — FIXED] FiltersPage.qml:510 — `AppTextField` for named-filter input had no
  `Accessible.name`. The placeholder text is present but not a reliable
  accessible name. Fixed: added `Accessible.name: qsTr("Filter name")`.

### Plugins (PluginsPage.qml)

Findings before this audit:

- [P0 — FIXED] PluginsPage.qml:591 — Per-plugin enable/disable `Controls.Switch`
  had no `Accessible.name`. Without it a screen reader can only announce
  "Switch, on/off" with no context about which plugin. Fixed: now carries
  `Accessible.name` that includes the plugin name and the current state
  intention ("Enable X" / "Disable X" / "X (built-in, always enabled)").

- [P0 — FIXED] PluginsPage.qml:252 — Plugin options dialog `Controls.Switch`
  (bool option) had no `Accessible.name`. Fixed: now carries
  `Accessible.name: modelData.label`.

- [P0 — FIXED] PluginsPage.qml:270 — Plugin options dialog `Controls.SpinBox`
  (int option) had no `Accessible.name`. Fixed: now carries
  `Accessible.name: modelData.label`.

- [P0 — FIXED] PluginsPage.qml:288 — Plugin options dialog `Controls.ComboBox`
  (enum option) had no `Accessible.name`. Fixed: now carries
  `Accessible.name: modelData.label`.

Additional findings:

- [P1 — FIXED] PluginsPage.qml:361 — "Rescan" `Controls.Button` has `text:` set but
  no `Accessible.description`. The ToolTip text changes on bridge connection state.
  Fixed: added `Accessible.description` mirroring the dynamic ToolTip expression so
  AT users hear the same bridge-state context as sighted users see on hover.

### Settings (SettingsPage.qml)

All `AppComboBox`, `AppCheckBox`, and `AppSpinBox` controls inside
`Kirigami.FormLayout` carry `Kirigami.FormData.label`. Kirigami wires
`FormData.label` as the accessible label for the paired control. These are
considered accessible without an additional `Accessible.name`.

`openConfigBtn` and `resetBtn` have `text:` set. OK.

`resetConfirm` (`Kirigami.PromptDialog`) uses `standardButtons` which Kirigami
renders as labelled platform buttons. OK.

No P0 findings on this page.

- [P1] SettingsPage.qml — The `AppSpinBox` controls for font size, tab width,
  and max recent paths obtain their accessible label through
  `Kirigami.FormData.label`, which relies on Kirigami's FormLayout
  implementation wiring `labelItem` to the paired control. This wiring is not
  guaranteed for custom `AppSpinBox` subclasses outside of a genuine
  `Kirigami.FormLayout` pair. A follow-up manual Orca test should verify these
  are announced correctly.

### About (AboutPage.qml)

`visitBtn` ("Visit LinSync") has `text:` set. `creditsBtn` and `viewLicBtn`
have `text:` set. All three are `Controls.Button` with a custom `contentItem`
that includes a `Controls.Label { text: ... }`. Qt reads `button.text` (not
the contentItem label) for accessibility — the `text:` property is set on the
button root so these are accessible.

No findings.

### Credits (CreditsPage.qml)

Findings before this audit:

- [P0 — FIXED] CreditsPage.qml:180 — Runtime component "visit website"
  `Controls.ToolButton` was icon-only with no `text:` or `Accessible.name`.
  A screen reader announced only the icon name. Fixed: now carries
  `text: qsTr("Visit %1 website").arg(modelData.name)`, `display: IconOnly`,
  and `Accessible.name` set to the same string.

- [P0 — FIXED] CreditsPage.qml:324 — Per-crate "Open on crates.io"
  `Controls.ToolButton` was icon-only with no `text:` or `Accessible.name`.
  Fixed: now carries `text: qsTr("Open %1 on crates.io").arg(modelData.name)`,
  `display: IconOnly`, and matching `Accessible.name`.

- [P1] CreditsPage.qml:200 — `AppTextField` for crate filter has
  `Accessible.name: "Filter licenses"`. OK.

### Licenses (LicensesPage.qml)

`Controls.TabButton` items ("LinSync License", "Third-party",
"Acknowledgements") have `text:` set. OK.

`Controls.Button` "Copy" has `text:` and a tooltip. OK.

`Controls.Button` "Dialog" has `text:` and a tooltip. OK.

`Controls.Button` "Clear" has `text:` set. OK.

`Controls.CheckBox` "Wrap" has `text: qsTr("Wrap")`. OK.

`AppTextField` `filterField` has `Accessible.name: qsTr("Find in license document")`. OK.

`licenseDialog` (GPL text popup) has `closePolicy: CloseOnEscape | CloseOnPressOutside`
and `standardButtons: Dialog.Close`. OK.

No findings.

### Navigation Sidebar (LinSyncNavItem.qml)

Finding before this audit:

- [P0 — FIXED] LinSyncNavItem.qml:99 — Sidebar nav items used a raw
  `MouseArea` inside a plain `Item`. Keyboard users could not Tab to nav items
  or activate them; AT got no role or name. Fixed by layering a
  `Controls.AbstractButton` with `text: nav.label`, `Accessible.name: nav.label`,
  `Accessible.role: Accessible.Button`, `Accessible.onPressAction`,
  `Keys.onReturnPressed`, and `Keys.onSpacePressed` behind the visual items.
  The `MouseArea` is retained for pointer interaction so hover styling is
  unchanged.

### Merge (MergePage.qml)

Toolbar `ToolButton` items ("Previous conflict", "Next conflict") carry
`Accessible.name`. OK.

`Controls.Button` items ("Keep Left", "Keep Base", "Keep Right", "Save to…")
have `text:` set. OK.

`savePicker` (native `FileDialog`) — platform accessibility. OK.

The `MergeFileColumn` `ListView` rows are display-only. No interactive
controls inside them. OK.

- [P1 — FIXED] MergePage.qml — The merged output `Controls.TextArea` (`outputArea`)
  had no `Accessible.name`. Fixed: added `Accessible.name: qsTr("Merged output")` and
  `Accessible.role: Accessible.EditableText` (role is inferred by Qt but made explicit).

---

## Cross-cutting

### Contrast (light + dark themes)

LinSync supports multiple colour schemes set through `DesignTokens.qml` and a
`themePreference` override. Contrast ratios were **not measured** in this
audit — all colour overrides are dynamic and would need a contrast analyser
driven against each theme mode at run time. A follow-up using a tool such as
`pacu` or the GNOME accessibility checker against both the default (system) and
the forced dark/light themes is recommended before release.

Known areas of concern:
- Opacity-reduced labels (e.g. `opacity: 0.5`, `opacity: 0.55`) may fall below
  3:1 for decorative text or 4.5:1 for informational text at some background
  colours. These are P2 — they affect readability but not navigability.

### Screen reader (Orca) — not automated

This audit did not exercise Orca. The following scenarios require a manual pass:

1. Tab through the entire sidebar and confirm each nav item is announced with
   its name ("Compare", "Sessions", …) and role ("button").
2. Tab through the Compare toolbar and confirm each button is announced.
3. Navigate the Sessions page tab list with keyboard.
4. Activate a Filters chip delete button via keyboard and confirm the chip is
   removed and focus does not trap.
5. Open the Plugin options dialog, confirm Switch/SpinBox/ComboBox labels are
   announced, and dismiss with Escape.
6. Open the GPL license dialog via the Licenses page and confirm Escape closes it.
7. Use Orca's flat review to verify the diff pane content is readable as plain
   text (the `textFormat: Text.PlainText` setting should help).

---

## P0 fixes applied

| # | File | Line (pre-fix) | Description |
|---|------|---------------|-------------|
| 1 | `LinSyncNavItem.qml` | 99 | Added `Controls.AbstractButton` overlay with `Accessible.name`, role, and keyboard handlers so sidebar nav items are reachable and activatable without a pointer. |
| 2 | `SessionsPage.qml` | 338 | Added `text:`, `display: IconOnly`, and `Accessible.name` to the "Copy path" `ToolButton` in the Recent paths list. |
| 3 | `FiltersPage.qml` | 183 | Added `text:`, `display: IconOnly`, and `Accessible.name` (includes pattern name) to include-rule chip delete button. |
| 4 | `FiltersPage.qml` | 276 | Added `text:`, `display: IconOnly`, and `Accessible.name` (includes pattern name) to exclude-rule chip delete button. |
| 5 | `FiltersPage.qml` | 578 | Added `text:`, `display: IconOnly`, and `Accessible.name` (includes filter name) to saved-filter row delete button. |
| 6 | `PluginsPage.qml` | 591 | Added `Accessible.name` (includes plugin name and enable/disable intent) to per-plugin enable/disable Switch. |
| 7 | `PluginsPage.qml` | 252 | Added `Accessible.name: modelData.label` to plugin options dialog Switch. |
| 8 | `PluginsPage.qml` | 270 | Added `Accessible.name: modelData.label` to plugin options dialog SpinBox. |
| 9 | `PluginsPage.qml` | 288 | Added `Accessible.name: modelData.label` to plugin options dialog ComboBox. |
| 10 | `CreditsPage.qml` | 180 | Added `text:`, `display: IconOnly`, and `Accessible.name` to runtime-component "visit website" ToolButton. |
| 11 | `CreditsPage.qml` | 324 | Added `text:`, `display: IconOnly`, and `Accessible.name` to per-crate "open on crates.io" ToolButton. |

---

## P1 fixes applied

| # | File | Fix applied |
|---|------|-------------|
| P1-3 | `FiltersPage.qml:310` | Added `Accessible.name: qsTr("Add %1 to exclude patterns").arg(modelData)` and `Accessible.description: qsTr("Quickly add a common exclude pattern")` to each quick-add button delegate. |
| P1-4 | `FiltersPage.qml:402` | Added `Accessible.name: qsTr("Maximum walk depth")` directly on the `AppSpinBox`. |
| P1-5 | `FiltersPage.qml:510` | Added `Accessible.name: qsTr("Filter name")` to the named-filter `AppTextField`. |
| P1-6 | `PluginsPage.qml:361` | Added `Accessible.description` mirroring the dynamic ToolTip expression on the Rescan button. |
| P1-8 | `MergePage.qml` | Added `Accessible.name: qsTr("Merged output")` and `Accessible.role: Accessible.EditableText` to the merged-output `TextArea`. |

---

## Deferred (P1/P2) — open issues

| # | File | Finding |
|---|------|---------|
| P1-1 | `Main.qml:2674` | Overview pane diff-marker rectangles have no keyboard equivalent. F7/F8 work but the visual shortcuts are mouse-only. Needs feature work (keyboard-accessible jump targets). |
| P1-2 | `Main.qml:2307` | "Swap sides" button has an accessible name but no implementation; non-issue until the feature is wired. |
| P1-7 | `SettingsPage.qml` | `AppSpinBox` controls inside `Kirigami.FormLayout` rely on Kirigami's label pairing. Needs a manual Orca verification pass. |
| P2-1 | All pages | Opacity-reduced decorative labels (`opacity: 0.5`–`0.6`) may fall below WCAG AA contrast in some theme combinations. Requires contrast-analyser pass per theme. |
| P2-2 | All pages | `Accessible.description` coverage is incomplete across the codebase. Broader coverage is a separate effort. |
