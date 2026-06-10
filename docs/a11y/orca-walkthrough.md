# Orca screen-reader walkthrough script

A scripted manual pass over every LinSync GUI section with the Orca screen
reader. A human runs this end-to-end and fills in the results tables; the
completed copy is attached to the release as the a11y verification artifact.

**Estimated time: 45–60 minutes** (Compare is ~15 minutes; every other
section is 2–5 minutes).

Every checklist item cites the QML file (and line at time of writing) that
provides the `Accessible.name` / `Accessible.description` under test, so a
failed row maps straight to code. Line numbers drift; the cited names do not.

---

## 1. Setup

1. Install and start Orca:

   ```sh
   sudo pacman -S orca
   orca &            # or enable via System Settings → Accessibility → Screen Reader
   ```

2. Recommended Orca settings (Orca Preferences, `Orca+Space`):
   - **Speech → Verbosity: Verbose** — we want role + name + description.
   - **Speech → Speak object mnemonics: on**.
   - **Key Echo: off** (reduces noise while tabbing).
   - Leave punctuation level at default.

3. Qt accessibility must be active. When launched from a desktop session with
   Orca running, Qt enables AT-SPI automatically; if announcements are silent,
   relaunch with `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1`.

4. Launch LinSync — either the installed package (`linsync`) or a dev build:

   ```sh
   cargo run -p linsync
   ```

5. Prepare fixtures (any scratch dir):

   ```sh
   mkdir -p /tmp/orca-fixtures/dir-a /tmp/orca-fixtures/dir-b
   printf 'alpha\nbeta\ngamma\ndelta\n'  > /tmp/orca-fixtures/left.txt
   printf 'alpha\nBETA\ngamma\nepsilon\n' > /tmp/orca-fixtures/right.txt
   cp /tmp/orca-fixtures/left.txt  /tmp/orca-fixtures/dir-a/common.txt
   cp /tmp/orca-fixtures/right.txt /tmp/orca-fixtures/dir-b/common.txt
   printf 'only left\n'  > /tmp/orca-fixtures/dir-a/left-only.txt
   printf 'only right\n' > /tmp/orca-fixtures/dir-b/right-only.txt
   ```

6. For the permanent-delete check (section 3, steps 16–20) set the delete
   preference to permanent in `$XDG_CONFIG_HOME/linsync/settings.json`
   (default `~/.config/linsync/settings.json`) **before** launching:

   ```json
   { "delete_preference": "permanent" }
   ```

   (merge into the existing file; the GUI exposes no toggle for this).
   Restore `"move_to_trash"` — or delete the key — after the run.

7. Recording results: work through each section's table in a copy of this
   file. Mark **pass** when Orca speaks the expected name/role (wording may
   vary slightly with Orca version; the *name* must match). Mark **fail**
   with the spoken text in *notes*. Attach the completed copy to the release.

### Known limitations (verify as behavior, do not log as failures)

- **Only the Compare status bar is announced live.** The single
  `Accessible.announce` site in the codebase is `Main.qml` (~line 19–24): it
  announces every change of `root.statusText` through the status-bar label.
  `Accessible.announce()` exists on Qt 6.8+ only; on older Qt the guard makes
  it a silent no-op — confirm your Qt version before failing announce rows.
- **Image, Document, and Merge pages keep page-local status text**
  (`ImageComparePage.qml`, `DocumentComparePage.qml`, `MergePage.qml` each
  declare their own `statusText` label) **with no announce handler.** Results
  on those pages must be read by navigating to the status label. The script
  below tests flat-review readability there, not live announcement.
- The static name-coverage gate is `bash scripts/gui-smoke.sh --check-a11y`;
  it passes at the time of writing (SessionsPage rename field gained its
  `Accessible.name` alongside this script).

### Announce-trigger inventory (what drives the live announcements)

All live announcements are values assigned to `root.statusText` in
`Main.qml`. The ones this script exercises:

| Trigger | Announced text | Main.qml site |
| --- | --- | --- |
| Run compare | "Comparing" → "Text compare complete" / "Folder compare complete" (status from bridge) | ~1905/1915, applyLaunchContext ~1425 |
| Cancel compare | "Cancelling…" → "Compare cancelled" | ~1958 / ~1940 |
| Compare with a side missing | "Select two paths" | ~1894 |
| Merge-copy a diff block | "Copied left to right" / "Copied right to left" | ~2327 / ~2338 |
| Save | "Saving" → bridge status, or "No dirty sides to save" | ~1604–1673 |
| Undo / redo | "Undoing" / "Redoing" / "Nothing to undo" / "Nothing to redo" | ~1678–1739 |
| Invalid regex in find bar | "Invalid find regex" | ~639 |
| Profile switch | "Active profile: \<id\>" | ~1121 |
| Folder op without a row selected | "Select a folder row first" | ~1220 |
| Folder op completes | "Folder op done: N succeeded / M failed of T" | ~3221 |
| Export report (clipboard) | "Report copied to clipboard" | ~2540 |
| Windowed compare progress | "Comparing X/Y" | ~5854 |

---

## 2. Window shell and sidebar (all sections start here)

The sidebar lists, top to bottom: **Compare, Image Compare, Webpage Compare,
Document Compare, Sessions, Filters, Plugins, Settings, About**
(`Main.qml` nav items ~3456–3560; each item's name comes from
`LinSyncNavItem.qml` `Accessible.name: nav.label`). Credits and Licenses are
reached from About; Merge is reached from the Compare toolbar.

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Launch LinSync, wait for the window | Orca announces the window title "LinSync" | | |
| 2. Tab until the sidebar collapse button has focus | "Collapse sidebar" (or "Expand sidebar" when collapsed), button (`Main.qml` ~3385) | | |
| 3. Arrow/Tab through the nav items | Each speaks its label: "Compare", "Image Compare", "Webpage Compare", "Document Compare", "Sessions", "Filters", "Plugins", "Settings", "About" (`LinSyncNavItem.qml`) | | |
| 4. Activate "Compare" with Enter/Space | Focus lands in the Compare section; no trap | | |

---

## 3. Section: Compare (`activeSection` 0, `Main.qml` body) — deepest checklist

### 3a. Text compare — focus traversal and run

Steps 2–11 are the expected first ~10 focus stops after entering the main
content area (toolbar row, then compare bar; layout order in `Main.qml`).

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Navigate to Compare via sidebar | section visible | | |
| 2. Tab to editor toolbar: Open | "Open", button (`Main.qml` ~3644) | | |
| 3. Tab: Save | "Save", button (~3657) | | |
| 4. Tab: Undo, Redo, Reload | "Undo" / "Redo" / "Reload", buttons (~3667–3691) | | |
| 5. Tab: difference navigation cluster | "First difference", "Previous difference", "Next difference", "Last difference" (~3700–3730) | | |
| 6. Tab: Find | "Find", button (~3740) | | |
| 7. Tab: merge-copy cluster | "Copy right to left", "Copy left to right", "Copy all right to left", "Copy all left to right" (~3753–3783) | | |
| 8. Tab to the compare bar: mode selector | "Compare mode", combo box (~3839) | | |
| 9. Tab: left path + browse | "Left path", text field (~3870); "Browse left", button (~3890) | | |
| 10. Tab: Swap sides, right path + browse | "Swap sides" (~3899); "Right path" (~3913); "Browse right" (~3933) | | |
| 11. Tab: run cluster | "Compare" (~3942), "Compare in new tab" (~3951), "Stop" (~3963) | | |
| 12. Type `/tmp/orca-fixtures/left.txt` and `/tmp/orca-fixtures/right.txt` into the path fields, activate "Compare" | Live announce: "Comparing", then "Text compare complete" (status-bar announce, `Main.qml` ~19–24) | | |
| 13. Flat-review the status bar | "Status: Text compare complete" (`Accessible.name: Status: %1`, ~4995) | | |
| 14. Activate "Compare" with one path cleared | Live announce: "Select two paths" (~1894) | | |
| 15. Press "Next difference" then "Copy left to right" | Announce: "Copied left to right" (~2327) | | |
| 16. Press "Undo" | Announce: "Undoing" then bridge status (~1683) | | |

### 3b. Options row and syntax selector

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Tab into the options row: profile selector | "Compare profile", combo box (`Main.qml` ~4076) | | |
| 2. Change the profile | Live announce: "Active profile: \<id\>" (~1121) | | |
| 3. Tab: render mode, fold toggle, context lines | "Text render mode" (~4149), "Fold unchanged context" (~4164), "Context lines" (~4177) | | |
| 4. Tab: changed-rows toggle | "Show only changed rows" (~4192) | | |
| 5. Tab: syntax selector | "Syntax mode", combo box (~4212) | | |
| 6. Arrow through syntax entries and select one | Each entry spoken; selection sticks (visual: re-colored panes) | | |
| 7. Tab: regex rule sets, encoding | "Text regex rule sets" (~4228), "Text encoding" (~4252) | | |
| 8. Tab: bookmark cluster | "Toggle bookmark" (~4268), "Previous bookmark" (~4278), "Next bookmark" (~4288) | | |

### 3c. Find bar

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Activate "Find" in the toolbar | Focus moves to find field: "Find text" (`Main.qml` ~4630) | | |
| 2. Tab: match navigation | "Previous match" (~4644), "Next match" (~4654) | | |
| 3. Tab: find options | "Regex find" (~4665), "Case-sensitive find" (~4679) | | |
| 4. Enable "Regex find", type `[` in the find field | Live announce: "Invalid find regex" (~639) | | |
| 5. Tab: close | "Close find" (~4699); Escape also closes without focus loss | | |

### 3d. Stop / cancel flow

Use a folder pair big enough to take a moment (e.g. two checkouts), or be
quick on the trigger with the fixture pair.

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Set paths to two large folders, activate "Compare" | Announce: "Comparing" (windowed compares: "Comparing X/Y" progress, ~5854) | | |
| 2. Immediately activate "Stop" (`Main.qml` ~3963) | Announce: "Cancelling…" (~1958) then "Compare cancelled" (~1940) | | |
| 3. Confirm "Stop" is disabled when no compare is running | button reported unavailable/dim | | |

### 3e. Folder compare and the permanent-delete confirmation dialog

Requires the `"delete_preference": "permanent"` setting from Setup step 6.

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Compare `/tmp/orca-fixtures/dir-a` vs `dir-b` (mode Folder or auto) | Announce: "Comparing" → "Folder compare complete" | | |
| 2. Tab to the folder search field | "Search folder entries by path" (`Main.qml` ~4419) | | |
| 3. Arrow through the folder rows | Each row's path/state readable | | |
| 4. With no row selected, trigger "Delete left" from the folder toolbar | Announce: "Select a folder row first" (~1220) | | |
| 5. Select the `left-only.txt` row, activate "Delete left" (toolbar, ~4342) | Folder-operation dialog opens | | |
| 6. Read the dialog warning | "Permanent delete warning", static text "Permanently deleting 1 item requires confirmation." (~3183; text from core `permanent_delete_warning`) | | |
| 7. Tab to the checkbox | "Confirm permanent delete", checkbox, unchecked — label "Permanently delete — this cannot be undone" (~3187–3191) | | |
| 8. Tab to the apply button while the checkbox is unchecked | "Apply folder operation", button, reported **disabled** (~3212–3213) | | |
| 9. Check the checkbox, return to the apply button | now enabled; state change perceivable | | |
| 10. Activate apply | Announce: "Folder op done: 1 succeeded / 0 failed of 1" (~3221) | | |
| 11. Re-open a delete plan (delete `right-only.txt` on the right) | Checkbox is **unchecked again** (re-confirmation per plan, `onOpened` ~3149) | | |
| 12. Escape closes the dialog with no action | focus returns to the folder view | | |

### 3f. Toolbar tail, tabs, and export

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Compare-bar tail buttons | "Copy paths" (~3977), "Reveal in file manager" (~3990), "Open externally" (~4000), "Export report" (~4014), "Reload compare" (~4023), "Swap sides" (~4032), "Open three-way merge" (~4041) | | |
| 2. Activate "Export report", pick format | "Report format" combo (~2530); clipboard export announces "Report copied to clipboard" (~2540) | | |
| 3. Activate "Swap sides" | paths exchange; re-compare announce fires | | |
| 4. Tab strip: close button | "Close current tab" (~4605) | | |

---

## 4. Section: Image Compare (`activeSection` 9, `ImageComparePage.qml`)

Path pickers come from the shared `FilePickerBar.qml`: field name is
"\<label\> \<kind\> path", browse is "Browse \<label\> \<kind\>".

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Sidebar → "Image Compare" | page shown | | |
| 2. Tab: left picker | "Left image path" + "Browse Left image" (`FilePickerBar.qml` ~41/~56) | | |
| 3. Tab: right picker | "Right image path" + "Browse Right image" | | |
| 4. Tab: mode + thresholds | "Compare mode" (~341), "Tolerance value (0-255)" spin (~363), "DeltaE threshold (×10)" spin (~385) | | |
| 5. Tab: frame mode + formats | "Frame compare mode" (~401), "Supported image formats" (~430) | | |
| 6. Tab: overlay controls | "Overlay opacity" (~460), "Save Overlay PNG" (~478) | | |
| 7. Tab: zoom cluster | "Zoom in" (~502), "Zoom out" (~510), "Fit to pane" (~518), "1:1" (~526), "Toggle split view" (~546) | | |
| 8. Pick two differing images, run compare | **No live announce expected** (known limitation). Flat-review the status label: "N of M pixels differ (…%)" or "Images are equal (…)" (~237–240) | | |

---

## 5. Section: Webpage Compare (`activeSection` 10, `WebpageComparePage.qml`)

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Sidebar → "Webpage Compare" | page shown; network-fetch notice readable | | |
| 2. Tab: URL fields | "Left URL" (~264), "Right URL" (~282) | | |
| 3. Tab: Compare button | "Compare" (~299) | | |
| 4. Tab: mode selector | "Compare mode" combo (~349) — entries "HTML source", "Extracted text", "Resource tree", rendered/screenshot when available | | |
| 5. If the build lacks a renderer, the hint label | "Rendered modes unavailable — no QML runner or Chromium found" (~371–374) | | |
| 6. Tab: cache button | "Clear webcompare cache" (~382) | | |
| 7. Run a tree compare (use two local `file://` or known URLs), Tab into the resource list | "Filter webpage resources by path" field (~560); rows readable | | |

---

## 6. Section: Document Compare (`activeSection` 11, `DocumentComparePage.qml`)

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Sidebar → "Document Compare" | page shown | | |
| 2. Tab: pickers | "Left document path" / "Browse Left document"; same for Right (`FilePickerBar.qml`) | | |
| 3. Tab: mode | "Document extraction mode" combo (~268) | | |
| 4. Tab: OCR language | "OCR language code" field (~285) | | |
| 5. Tab: run button | "Run Compare" (text-named AppButton, ~291) | | |
| 6. Run a compare on two text/PDF fixtures | **No live announce expected** (known limitation). Flat-review status: "Documents are equal (…)" or "N differing lines (…)" (~107–109) | | |
| 7. Review extracted-text panes | "Left" / "Right" headings and contents readable (~393/~410) | | |

---

## 7. Section: Sessions (`activeSection` 1, `SessionsPage.qml`)

Most Sessions buttons are text-labelled (`AppButton` derives the accessible
name from `text`).

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Sidebar → "Sessions" | "Sessions" heading; tab/recent count line readable (~108–114) | | |
| 2. Tab: header actions | "Save session" (~131), "Save project…" (~147), "Open project…" (~153) | | |
| 3. Open-tabs list rows | "Switch" (~244) and "Close" (~253) per row; or "Go to Compare" placeholder when empty (~285) | | |
| 4. Recent-sessions rows | "Reopen" (~361), "Rename" (~370), "Delete" (~379) per row; "Copy path" (~490) | | |
| 5. Activate "Rename" on a row | Prompt dialog "Rename session"; the text field announces "Session title" with description "New title for the selected recent session" (~548–549) | | |
| 6. Escape cancels the dialog | focus returns to the list | | |

---

## 8. Section: Filters (`activeSection` 2, `FiltersPage.qml`)

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Sidebar → "Filters" | page shown | | |
| 2. Include list: add field | "Add include glob" (~130) | | |
| 3. Include chips | "Remove include pattern \<glob\>" per chip (~184) | | |
| 4. Exclude list: add field + chips | "Add exclude glob" (~226); "Remove exclude pattern \<glob\>" (~280) | | |
| 5. Suggested-exclusion buttons | "Add \<pattern\> to exclude patterns" (~317) | | |
| 6. Walk options | "Maximum walk depth" spin (~485) | | |
| 7. Named filters | "Filter name" field (~519); "Delete filter \<name\>" per row (~587) | | |

---

## 9. Section: Plugins (`activeSection` 3, `PluginsPage.qml`)

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Sidebar → "Plugins" | page shown | | |
| 2. Tab: filter field | "Filter plugins" (~575) | | |
| 3. Plugin rows | each row's controls named by the plugin's label (`modelData.label`, ~332/~350/~370); enable/disable state spoken | | |
| 4. Built-in badge | builtin marker exposed (~815) | | |
| 5. Toggle a plugin off and on | toggle state change spoken by Orca (checkbox/switch semantics) | | |

---

## 10. Section: Settings (`activeSection` 4, `SettingsPage.qml`)

Settings controls are labelled through `Kirigami.FormData.label` plus control
`text`; there are no bespoke `Accessible.name` overrides — verify the derived
names are sufficient.

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Sidebar → "Settings" | "Settings" heading | | |
| 2. Appearance group | "Color scheme" combo; "Pane font family"; "Pane font size"; "Tab width"; checkboxes "Show on both panes", "Render spaces and tabs", "Wrap instead of horizontal scroll", "Reduce UI animation" | | |
| 3. Comparison-behavior group | "Default mode"; "Ignore case differences"; "Ignore leading + trailing whitespace"; "Treat empty lines as equal"; "Treat CR / LF / CRLF as equal"; "Detect reordered sections"; "EOL on save" | | |
| 4. Session group | "Prompt before closing a dirty tab"; "Remember between launches"; "Max recent paths" | | |
| 5. Storage group | "Open config folder" button (with description, ~973–978); "Reset to defaults" (~1010) | | |
| 6. Activate "Reset to defaults" | Confirmation dialog "Reset settings?" with subtitle readable (~1051–1052); Escape cancels | | |
| 7. Activate "Open config folder" | Live announce (Compare status bar): "Opened config folder" (`Main.qml` ~1046) | | |

---

## 11. Section: About → Credits → Licenses (`activeSection` 5/6/7)

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. Sidebar → "About" | "About" heading; version/license chips readable (`AboutPage.qml` ~161–195) | | |
| 2. Tab: links/buttons | "Visit LinSync" (~331), "Credits" (~365), "Licenses" (~389) — all text-named | | |
| 3. Activate "Credits" | Credits page; "Filter licenses" field (`CreditsPage.qml` ~294); per-crate links "Open \<crate\> on crates.io" (~408); "Visit \<name\> website" (~260) | | |
| 4. Return to About, activate "Licenses" | Licenses page; "Find in license document" field (`LicensesPage.qml` ~678); license text readable in flat review | | |
| 5. Navigate back to About via sidebar | no focus trap on either subpage | | |

---

## 12. Section: Merge (`activeSection` 8, `MergePage.qml`)

Reached from the Compare toolbar ("Open three-way merge", `Main.qml` ~4041),
not from the sidebar.

| step | expected | pass/fail | notes |
| --- | --- | --- | --- |
| 1. From Compare, activate "Open three-way merge" | Merge page shown | | |
| 2. Tab: path pickers and Start (text-named controls) | base/left/right fields and start button reachable and named | | |
| 3. Tab: conflict navigation | "Previous conflict" (~219), "Next conflict" (~237) | | |
| 4. Tab into the output editor | "Merged output" text area (~466) | | |
| 5. Start a merge on conflicting fixtures | **No live announce expected** (page-local status, known limitation). Flat-review the status label: "N conflicts remaining" / saved-path message (~92–172) | | |

---

## 13. Wrap-up

1. Restore `delete_preference` in settings.json (Setup step 6).
2. Delete `/tmp/orca-fixtures`.
3. File one issue per failed row, quoting the table row and the cited QML
   site. Attach the completed copy of this file to the release checklist
   (closes the gap noted in `docs/known-limitations-1.0.md`).
