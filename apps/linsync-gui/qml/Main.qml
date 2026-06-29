import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ApplicationWindow {
    id: root

    width: 1180
    height: 880
    minimumWidth: 860
    minimumHeight: 560
    title: "LinSync"

    property int activeSection: 0
    property string statusText: "Ready"
    property string statusSeverity: "info"
    // Announce status/error changes to assistive technology as they happen.
    // Accessible.announce() exists on Qt 6.8+; guarded so older Qt is a no-op.
    onStatusTextChanged: {
        if (typeof statusBarLabel !== "undefined" && statusBarLabel.Accessible
                && typeof statusBarLabel.Accessible.announce === "function")
            statusBarLabel.Accessible.announce(root.statusText)
    }
    property string leftPath: ""
    property string rightPath: ""
    onLeftPathChanged: root.refreshArchiveEditability()
    onRightPathChanged: root.refreshArchiveEditability()
    property string basePath: ""
    property bool threeWayMode: root.compareMode === "Three-way"
    // Inline editing state. When true, the corresponding text pane is editable.
    property bool editLeftMode: false
    property bool editRightMode: false
    property string editLeftDirtyText: ""
    property string editRightDirtyText: ""
    property string pendingRemovePluginId: ""
    property string pendingRemovePluginName: ""
    property string pendingEditToggleSide: ""
    property string compareMode: "Text"
    property string differenceText: "0 differences"
    property var summaryItems: []
    property var tabItems: []
    property var leftRows: makeBlankRows()
    property var rightRows: makeBlankRows()
    property var sessionState: ({})
    property int activeTabId: 0
    property bool syncingScroll: false
    property string bridgeUrl: ""
    // Compare cancellation state (Phase 3). `comparing` gates the Stop button;
    // `activeRequestId` is the id sent with the in-flight /compare so /cancel
    // can target it. `requestCounter` yields a session-unique id.
    property bool comparing: false
    property string activeRequestId: ""
    property int requestCounter: 0
    property string progressPhase: "none"
    property int progressCurrent: 0
    property int progressTotal: 0
    property string progressMessage: ""
    // Defensive iteration cap for the progress timer so a missed cancel path
    // cannot poll forever. At 200 ms intervals 30000 iterations == ~100 min.
    property int progressPollCount: 0
    readonly property int progressPollMax: 30000
    // Compare-profile selector state (Phase 1). `profileEntries` mirrors
    // /profiles/list (built-ins first, then user profiles); `activeProfileId`
    // is the persisted active pointer; `profileError` surfaces a 400/404 inline.
    property var profileEntries: []
    property string activeProfileId: ""
    property string profileError: ""
    property string pendingBrowseSide: "left"
    property var diffRowIndexes: []
    property int currentDiffPosition: -1
    property int currentDiffRow: -1
    property bool findVisible: false
    property string searchText: ""
    property bool searchRegex: false
    property bool searchCaseSensitive: false
    property var searchRowIndexes: []
    property int currentSearchPosition: -1
    property int currentSearchRow: -1
    property string textRenderMode: "side-by-side"
    property string syntaxMode: "plain"
    property bool contextFolding: false
    property int contextLines: 3
    property bool showOnlyChanges: false
    property string textEncoding: "auto"
    property var textRegexRuleSets: []
    property var bookmarkRows: []
    property int currentBookmarkPosition: -1
    property bool leftDirty: false
    property bool rightDirty: false
    property bool validationCompatible: false
    property string validationMessage: ""
    property string validationPathKind: ""
    // Keep in sync with the workspace version in Cargo.toml. bridge-info.json
    // overwrites this once loaded, but the default is shown during startup.
    property string appVersion: "1.16.0"
    property int bridgeModelRevision: 0
    property bool canUndo: false
    property bool canRedo: false
    property string folderFilter: ""
    // Free-text search over the relative path, and an entry-type filter
    // ("" = all, else "file"/"directory"/"symlink"/"special").
    property string folderSearch: ""
    property string folderTypeFilter: ""
    // folderTypeFilter split once per change, not once per entry / per binding.
    readonly property var folderTypeList: folderTypeFilter === "" ? [] : folderTypeFilter.split(",")
    // Archive member editing state
    property string archiveEditToken: ""
    // Tab that owns the active archive edit; the banner and commit/discard are
    // scoped to it so switching tabs never shows or commits the edit against an
    // unrelated comparison. -1 == no edit in progress.
    property int    archiveEditTabId:   -1
    property string archiveEditMember: ""
    property bool archiveEditInProgress: false
    property string archiveEditSide: ""  // "left" or "right"
    property string archiveEditPortalWarning: ""
    // Raw-text input capture on the main Compare page. Only used when both
    // paths are empty and compareMode is "Text".
    property string leftPaneText: ""
    property string rightPaneText: ""
    property bool liveCompareEnabled: false
    property int liveCompareDebounceMs: 300
    // Cached editability for the two compared archives. Updated asynchronously
    // whenever the paths change so the context menu can be shown synchronously
    // when the user right-clicks a folder row.
    property bool leftArchiveEditable: false
    property bool rightArchiveEditable: false
    property var unfilteredLeftRows: []
    property var unfilteredRightRows: []
    // Lazy text windowing. When a text diff is larger than the server's window
    // threshold, the compare response carries only the first window of rows
    // plus the full row count (textTotalRows) and the full change-/find-row
    // index lists; further windows are fetched from /compare/text/window as the
    // user scrolls toward the bottom or jumps to a change/match outside the
    // loaded range. textTotalRows == 0 means the whole diff is already loaded
    // (small/medium diffs — unchanged behavior).
    property int textTotalRows: 0
    property var serverDiffRowIndexes: []
    property var serverSearchRowIndexes: []
    property bool textWindowLoading: false
    // Hex navigation. Jump-to-offset stores the user-entered offset string;
    // hexWindowLoading mirrors textWindowLoading for the /binary/window path.
    property string hexJumpOffset: ""
    property bool hexWindowLoading: false
    // Lazy folder paging. When a folder comparison exceeds the server window
    // threshold the compare response embeds only the first page plus the full
    // entry count (folderTotalEntries); further pages — and all sort / filter /
    // search — are served by /folder/query so the whole tree never loads into
    // the view. 0 means the folder is small enough to load + filter/sort
    // entirely client-side (unchanged behavior).
    property int folderTotalEntries: 0
    property bool folderWindowLoading: false
    // Suppresses the pane's scroll-to-top reset when rows change because we are
    // appending a fetched window (vs. loading a fresh comparison).
    property bool suppressTextScrollReset: false
    property var folderEntries: []
    // Hard ceiling on how many folder rows the GUI will hold in memory for
    // windowed folders. Further lazy-load pages are dropped once the cap is
    // reached and the user is told to refine filters. Sort/filter/search changes
    // reset the model via queryFolderPage(0, false), so the cap only bounds
    // scroll-driven growth within one query.
    readonly property int folderEntriesMax: 50000
    property var visibleFolderEntries: []
    // Table compare grid data. tableCells holds the currently-loaded window of
    // rows; tableHeaders is populated when the input has a header row. Windowing
    // uses tableTotalRows the same way text compare uses textTotalRows.
    property var tableCells: []
    property var tableHeaders: []
    property int tableTotalRows: 0
    property bool tableWindowLoading: false
    property string folderSortColumn: ""
    property bool folderSortAscending: true
    property string folderGroupBy: ""
    onFolderFilterChanged: root.applyFolderFilter()
    // Route search/type changes through applyFolderFilter so a windowed folder
    // re-queries /folder/query server-side instead of filtering only the loaded
    // page (non-windowed folders still filter client-side via rebuildFolderView).
    // Windowed search is debounced: each /folder/query re-walks both trees
    // server-side, so firing one per keystroke stalls the bridge. Client-side
    // filtering stays immediate.
    Timer {
        id: folderSearchDebounce
        interval: 250
        onTriggered: root.applyFolderFilter()
    }
    // Debounce rapid compare retriggers — e.g. holding the context-lines
    // spinbox up/down or scrolling it fires onValueModified per step, and
    // without this each step kicks off a fresh /compare that piles onto the
    // bridge (the prior ones get cancelled by requestCompare's cancel-before-
    // fire, but skipping the storm entirely is cheaper). The last value wins.
    // One-shot triggers (swap, reload, render-mode change) call requestCompare
    // directly and fire immediately.
    Timer {
        id: compareRetriggerDebounce
        interval: 250
        property bool pendingNewTab: false
        onTriggered: root.requestCompare(pendingNewTab)
    }
    function scheduleCompare(newTab) {
        compareRetriggerDebounce.pendingNewTab = newTab
        compareRetriggerDebounce.restart()
    }
    onFolderSearchChanged: {
        if (root.folderTotalEntries > 0)
            folderSearchDebounce.restart()
        else
            root.applyFolderFilter()
    }
    onFolderTypeFilterChanged: root.applyFolderFilter()
    onFolderGroupByChanged: root.applyFolderFilter()
    onFolderSortColumnChanged: root.applyFolderSort()
    onFolderSortAscendingChanged: root.applyFolderSort()
    property int pendingCloseTabId: 0
    property string pendingFolderOpKind: ""
    property var pendingFolderOpEntries: []

    // Three-way merge paths — set by the open-merge flow, then read by MergePage.
    property string mergeBasePath:  ""
    property string mergeLeftPath:  ""
    property string mergeRightPath: ""
    // Predetermined output path for a Git-mergetool launch (empty = ad-hoc merge
    // opened from the toolbar, which prompts for a save location instead).
    property string mergeOutputPath: ""

    // -- Theming --------------------------------------------------------
    // The user-chosen theme integer. 0 follows the host palette;
    // the other values force an explicit palette by overriding Qt/Kirigami
    // colors at the window root.
    property bool   _settingsReady:     false
    property int    themePreference:    0
    property int    paneFontSize:       12
    property string paneFontFamily:     "monospace"
    property int    paneTabWidth:       4
    property bool   showLineNumbers:    true
    property bool   showWhitespace:     false
    property bool   wordWrap:           false
    property bool   ignoreCase:         false
    property bool   ignoreWhitespace:   false
    property bool   ignoreBlankLines:   false
    property bool   ignoreEol:          true
    property string eolNormalization:   "auto"
    property string defaultCompareMode: "Text"
    property bool   confirmOnClose:     true
    property bool   persistRecentPaths: true
    property bool   reduceMotion:       false
    property bool   detectMoves:        false
    property bool   keepArchiveBackup:  false
    property int    maxRecentPaths:     20
    readonly property string appIconSource: Qt.resolvedUrl("assets/com.visorcraft.LinSync.png")

    SystemPalette { id: sysActive; colorGroup: SystemPalette.Active }
    SystemPalette { id: sysDisabled; colorGroup: SystemPalette.Disabled }

    DesignTokens { id: themeTokens }

    readonly property var themeValues: themeTokens.themeValues
    readonly property var themeLabels: themeTokens.themeLabels
    readonly property string colorScheme: themeTokens.keyForTheme(themePreference)
    readonly property var activePalette: themeTokens.paletteForTheme(themePreference)

    // Per-slot resolved palette. Individual color properties (vs. a
    // single `var` object) so each one has a stable value identity —
    // returning a fresh object literal from a `var` binding fires
    // change notifications on every read and triggers Qt's binding-
    // loop detector.
    // For "Follow system" (themePreference 0), pick a fallback palette
    // from the named LinSync palettes based on Qt's system colorScheme
    // hint. Reading sysActive.* here would create a binding loop with
    // `palette.* : activeX` below — SystemPalette reads from the same
    // QPalette that the bindings write, and Qt's binding-loop detector
    // flags any descendant that reads `Kirigami.Theme.textColor` once
    // we let descendants inherit from this scope.
    readonly property var systemFallbackPalette:
        Qt.styleHints && Qt.styleHints.colorScheme === Qt.Light
            ? themeTokens.paletteForTheme(1)
            : themeTokens.paletteForTheme(2)

    readonly property color activeBg: themePreference === 0
        ? systemFallbackPalette.background
        : activePalette.background
    readonly property color activeBgAlt: themePreference === 0
        ? systemFallbackPalette.alternateBackground
        : activePalette.alternateBackground
    readonly property color activeBgLift: themePreference === 0
        ? systemFallbackPalette.tertiaryBackground
        : activePalette.tertiaryBackground
    readonly property color activeText: themePreference === 0
        ? systemFallbackPalette.text
        : activePalette.text
    readonly property color activeDisabledText: themePreference === 0
        ? systemFallbackPalette.disabledText
        : activePalette.disabledText
    readonly property color activeHighlight: themePreference === 0
        ? systemFallbackPalette.highlight
        : activePalette.highlight
    readonly property color activeHighlightedText: themePreference === 0
        ? systemFallbackPalette.highlightedText
        : activePalette.highlightedText
    readonly property color activePositiveText: themePreference === 0
        ? systemFallbackPalette.positiveText
        : activePalette.positiveText
    readonly property color activeNegativeText: themePreference === 0
        ? systemFallbackPalette.negativeText
        : activePalette.negativeText
    readonly property color activeNeutralText: themePreference === 0
        ? systemFallbackPalette.neutralText
        : activePalette.neutralText

    // Drive the Qt palette. Kirigami's BasicThemeDefinition derives its
    // colors from `palette` (window/base/text/highlight/etc.), so this
    // cascades to every Quick Controls widget and Kirigami widget.
    palette.window:          activeBg
    palette.windowText:      activeText
    palette.base:            activeBg
    palette.alternateBase:   activeBgAlt
    palette.text:            activeText
    palette.button:          activeBgAlt
    palette.buttonText:      activeText
    palette.highlight:       activeHighlight
    palette.highlightedText: activeHighlightedText
    palette.toolTipBase:     activeBgAlt
    palette.toolTipText:     activeText
    palette.placeholderText: activeDisabledText
    palette.mid:             activeBgAlt
    palette.midlight:        activeBgLift
    palette.dark:            activeBgAlt
    palette.shadow:          activeBg
    palette.light:           activeBgLift

    // Belt-and-suspenders: also override Kirigami.Theme so any widget
    // that reads through the Theme attached property (rather than the
    // raw palette) picks up our values.
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    Kirigami.Theme.backgroundColor:          activeBg
    Kirigami.Theme.alternateBackgroundColor: activeBgAlt
    Kirigami.Theme.textColor:                activeText
    Kirigami.Theme.disabledTextColor:        activeDisabledText
    Kirigami.Theme.highlightColor:           activeHighlight
    Kirigami.Theme.highlightedTextColor:     activeHighlightedText
    Kirigami.Theme.positiveTextColor:        activePositiveText
    Kirigami.Theme.negativeTextColor:        activeNegativeText
    Kirigami.Theme.neutralTextColor:         activeNeutralText
    color: activeBg

    readonly property color separatorColor: Kirigami.ColorUtils.tintWithAlpha(activeBg, activeText, 0.2)
    readonly property var textRegexRuleSetEntries: [
        { "id": "generated", "label": qsTr("Generated") },
        { "id": "volatile", "label": qsTr("Volatile") },
        { "id": "comments", "label": qsTr("Comments") },
        { "id": "whitespace", "label": qsTr("Whitespace") }
    ]

    // Apply user preferences to a single line of text before rendering.
    // - tabWidth replaces literal tabs with the configured number of spaces
    //   so the visible width matches the user's tab-width setting
    // - showWhitespace marks spaces (· / 0x00B7) and tabs (→ / 0x2192) so
    //   the user can see leading/trailing space
    function transformLineText(raw) {
        if (raw === undefined || raw === null)
            return ""
        let s = String(raw)
        if (root.showWhitespace) {
            // Replace tabs with arrow followed by spaces to preserve width
            const pad = root.paneTabWidth > 0 ? root.paneTabWidth - 1 : 0
            s = s.replace(/\t/g, "→" + " ".repeat(pad))
            s = s.replace(/ /g, "·")
        } else if (root.paneTabWidth > 0) {
            s = s.replace(/\t/g, " ".repeat(root.paneTabWidth))
        }
        return s
    }

    function cssColor(value) {
        const text = String(value)
        if (text.length === 9 && text[0] === "#")
            return "#" + text.slice(3)
        return text
    }

    function syntaxColor(kind) {
        if (kind === "keyword") return "#7a3e9d"
        if (kind === "string") return "#0b6b3a"
        if (kind === "number") return "#8a4b08"
        if (kind === "comment") return "#69717a"
        if (kind === "key") return "#005f9e"
        if (kind === "tag") return "#8a2b58"
        return cssColor(root.activeText)
    }

    function escapeRichText(raw) {
        if (raw === undefined || raw === null)
            return ""
        return String(raw)
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
            .replace(/ /g, "&nbsp;")
    }

    function richSegment(text, className) {
        const color = className ? root.syntaxColor(className) : root.cssColor(root.activeText)
        return "<span style=\"color:" + color + "\">" + root.escapeRichText(root.transformLineText(text)) + "</span>"
    }

    function syntaxRichTextForRow(row) {
        if (!row || row.text === undefined || row.text === null)
            return ""
        const text = String(row.text)
        const spans = row.syntax_spans || []
        if (spans.length === 0)
            return root.richSegment(text, "")

        const chars = Array.from(text)
        let cursor = 0
        let output = ""
        for (let i = 0; i < spans.length; i++) {
            const span = spans[i]
            const start = Math.max(0, Math.min(chars.length, Number(span.start || 0)))
            const end = Math.max(start, Math.min(chars.length, Number(span.end || start)))
            if (start > cursor)
                output += root.richSegment(chars.slice(cursor, start).join(""), "")
            const displayStart = Math.max(start, cursor)
            if (end > displayStart)
                output += root.richSegment(chars.slice(displayStart, end).join(""), String(span.class || ""))
            cursor = Math.max(cursor, end)
        }
        if (cursor < chars.length)
            output += root.richSegment(chars.slice(cursor).join(""), "")
        return output
    }

    function textRegexRuleSetEnabled(id) {
        return root.textRegexRuleSets.indexOf(id) >= 0
    }

    function setTextRegexRuleSet(id, enabled) {
        const sets = root.textRegexRuleSets.slice()
        const existing = sets.indexOf(id)
        if (enabled && existing < 0)
            sets.push(id)
        else if (!enabled && existing >= 0)
            sets.splice(existing, 1)
        root.textRegexRuleSets = sets
        root.requestCompare(false)
    }

    function textRegexRuleSetSummary() {
        if (root.textRegexRuleSets.length === 0)
            return qsTr("Rules")
        if (root.textRegexRuleSets.length === 1)
            return root.textRegexRuleSets[0]
        return qsTr("%1 rules").arg(root.textRegexRuleSets.length)
    }

    function appendBookmarkParams(url) {
        const seen = ({})
        let out = url
        for (let i = 0; i < root.bookmarkRows.length; i++) {
            const row = root.bookmarkRows[i]
            const left = root.leftRows[row]
            const right = root.rightRows[row]
            if (left && left.number !== undefined && left.number !== null) {
                const key = "left:" + left.number
                if (!seen[key]) {
                    seen[key] = true
                    out += "&bookmark=" + encodeURIComponent(key)
                }
            }
            if (right && right.number !== undefined && right.number !== null) {
                const key = "right:" + right.number
                if (!seen[key]) {
                    seen[key] = true
                    out += "&bookmark=" + encodeURIComponent(key)
                }
            }
        }
        return out
    }

    function makeBlankRows() {
        const rows = []
        for (let index = 0; index < 48; index++) {
            rows.push({
                "row_id": "blank:" + index,
                "number": index + 1,
                "text": "",
                "state": "empty"
            })
        }
        return rows
    }

    function rawTextInputActive() {
        return root.compareMode === "Text"
            && root.leftPath === ""
            && root.rightPath === ""
    }

    function rawTextInputReady() {
        return root.rawTextInputActive()
            && (root.leftPaneText !== "" || root.rightPaneText !== "")
    }

    // State-to-color lookup — computed once per row in the delegate.
    // Rather than JS with multiple Kirigami.ColorUtils.tintWithAlpha calls
    // per paint (slow on long files), each row's color is a simple inline
    // component binding so Qt has cached dependencies.
    readonly property var stateColors: (function() {
        var c = {}
        c['changed'] = Kirigami.ColorUtils.tintWithAlpha(activeBg, activeNeutralText, 0.16)
        c['left_only'] = Kirigami.ColorUtils.tintWithAlpha(activeBg, activeNegativeText, 0.14)
        c['right_only'] = Kirigami.ColorUtils.tintWithAlpha(activeBg, activePositiveText, 0.14)
        c['skipped'] = Kirigami.ColorUtils.tintWithAlpha(activeBg, activeDisabledText, 0.16)
        c['aborted'] = Kirigami.ColorUtils.tintWithAlpha(activeBg, activeDisabledText, 0.16)
        c['error'] = Kirigami.ColorUtils.tintWithAlpha(activeBg, activeNegativeText, 0.22)
        return c
    })()
    readonly property color searchRowColor: Kirigami.ColorUtils.tintWithAlpha(activeBg, activeHighlight, 0.16)
    readonly property color bookmarkRowColor: Kirigami.ColorUtils.tintWithAlpha(activeBg, activeHighlight, 0.08)
    // Precomputed zebra colors
    readonly property color zebra0: activeBg
    readonly property color zebra1: activeBgAlt
    function lineBackground(state, index) {
        if (index === root.currentDiffRow) return root.diffRowColor
        if (index === root.currentSearchRow) return root.searchRowColor
        var c = root.stateColors[state]
        if (c !== undefined) return c
        return root['zebra' + (index % 2)]
    }

    function showStatus(msg) {
        root.statusText = msg
        root.statusSeverity = "info"
    }

    function showError(msg) {
        root.statusText = msg
        root.statusSeverity = "error"
    }

    // Full-opacity change-bar color for a diff state, drawn as a thin marker at
    // the left edge of each changed row. The per-row background tint is only
    // 14–22% alpha — too faint under a high-contrast color scheme — so this
    // solid bar keeps differences clearly distinguishable regardless of scheme
    // and gives a non-background cue (helps low-vision use). "" = no marker.
    function lineMarkerColor(state) {
        if (state === "left_only" || state === "error") return root.activeNegativeText
        if (state === "right_only") return root.activePositiveText
        if (state === "changed") return root.activeNeutralText
        if (state === "skipped" || state === "aborted" || state === "folded") return root.activeDisabledText
        return ""
    }

    // Sync scroll between panes only when flick animation ends, not per-pixel.
    function isDifferenceState(state) {
        return state === "changed" || state === "left_only" || state === "right_only" || state === "error" || state === "aborted"
    }

    function isEditableArchive(path) {
        // Kept for compatibility: returns the cached bridge result for the
        // current left/right paths. The cache is refreshed asynchronously by
        // refreshArchiveEditability() whenever the paths change.
        if (path === root.leftPath)
            return root.leftArchiveEditable
        if (path === root.rightPath)
            return root.rightArchiveEditable
        return false
    }

    function refreshArchiveEditability() {
        function fetch(path, applyIfCurrent) {
            if (root.bridgeUrl === "" || path === "") {
                applyIfCurrent(false)
                return
            }
            const request = new XMLHttpRequest()
            request.onreadystatechange = function () {
                if (request.readyState === XMLHttpRequest.DONE && request.status === 200) {
                    let payload = null
                    try {
                        payload = JSON.parse(request.responseText)
                    } catch (e) {
                        payload = null
                    }
                    applyIfCurrent(payload && payload.editable === true)
                }
            }
            const url = root.bridgeUrl + "/archive/can-edit?path=" + encodeURIComponent(path)
            request.open("GET", url)
            request.send()
        }

        const leftAtRequest = root.leftPath
        fetch(leftAtRequest, function (editable) {
            if (root.leftPath === leftAtRequest)
                root.leftArchiveEditable = editable
        })
        const rightAtRequest = root.rightPath
        fetch(rightAtRequest, function (editable) {
            if (root.rightPath === rightAtRequest)
                root.rightArchiveEditable = editable
        })
    }

    function rebuildDiffRows() {
        const rows = []
        if (root.compareMode === "Folder") {
            for (let index = 0; index < root.visibleFolderEntries.length; index++) {
                const state = root.visibleFolderEntries[index] ? root.visibleFolderEntries[index].state : ""
                if (isDifferenceState(state))
                    rows.push(index)
            }
            root.diffRowIndexes = rows
            root.currentDiffPosition = rows.length > 0 ? 0 : -1
            root.currentDiffRow = rows.length > 0 ? rows[0] : -1
            scrollToCurrentDifference()
            return
        }
        // Windowed text diff: the server supplied the full change-row index list
        // (covering changes outside the loaded window), so use it directly
        // rather than scanning the partially-loaded rows.
        if (root.textTotalRows > 0) {
            root.diffRowIndexes = root.serverDiffRowIndexes
            root.currentDiffPosition = root.serverDiffRowIndexes.length > 0 ? 0 : -1
            root.currentDiffRow = root.serverDiffRowIndexes.length > 0 ? root.serverDiffRowIndexes[0] : -1
            scrollToCurrentDifference()
            return
        }
        for (let index = 0; index < root.leftRows.length; index++) {
            const leftState = root.leftRows[index] ? root.leftRows[index].state : ""
            const rightState = root.rightRows[index] ? root.rightRows[index].state : ""
            if (isDifferenceState(leftState) || isDifferenceState(rightState))
                rows.push(index)
        }

        root.diffRowIndexes = rows
        root.currentDiffPosition = rows.length > 0 ? 0 : -1
        root.currentDiffRow = rows.length > 0 ? rows[0] : -1
        scrollToCurrentDifference()
    }

    function selectDifference(position) {
        if (root.diffRowIndexes.length === 0) {
            root.currentDiffPosition = -1
            root.currentDiffRow = -1
            return
        }

        let nextPosition = position
        if (nextPosition < 0)
            nextPosition = root.diffRowIndexes.length - 1
        if (nextPosition >= root.diffRowIndexes.length)
            nextPosition = 0
        root.currentDiffPosition = nextPosition
        root.currentDiffRow = root.diffRowIndexes[nextPosition]
        scrollToCurrentDifference()
    }

    function setDifferenceCount(count) {
        root.differenceText = count === 1 ? "1 difference" : count + " differences"
    }

    function currentDifferenceCount() {
        return root.diffRowIndexes.length
    }

    function refreshDifferenceCountFromRows() {
        setDifferenceCount(currentDifferenceCount())
    }

    function nextDifference() {
        selectDifference(root.currentDiffPosition + 1)
    }

    function previousDifference() {
        selectDifference(root.currentDiffPosition - 1)
    }

    function firstDifference() {
        selectDifference(0)
    }

    function lastDifference() {
        selectDifference(root.diffRowIndexes.length - 1)
    }

    // Jump to a specific hex offset. Parses the offset string as hex and
    // scrolls the diff pane to the row containing that offset.
    function jumpToHexOffset(offsetStr) {
        const parsed = parseInt(offsetStr, 16)
        if (isNaN(parsed) || parsed < 0) {
            root.statusText = qsTr("Invalid hex offset")
            return
        }
        // Find the row containing this offset. The default bytes_per_row is 16
        // (BinaryCompareOptions::default().bytes_per_row); profile overrides are
        // not exposed to QML yet, so this is accurate for the common case.
        const bytesPerRow = 16
        const targetRow = Math.floor(parsed / bytesPerRow)
        if (targetRow < 0) {
            root.statusText = qsTr("Offset out of range")
            return
        }
        // For windowed hex files, load rows until the target is available.
        if (targetRow >= root.leftRows.length && root.textTotalRows > 0
                && root.unfilteredLeftRows.length < root.textTotalRows) {
            root.loadNextHexWindow(function (loadedMore) {
                if (loadedMore)
                    root.jumpToHexOffset(offsetStr)
            })
            return
        }
        if (targetRow >= root.leftRows.length) {
            root.statusText = qsTr("Offset out of range")
            return
        }
        root.currentDiffRow = targetRow
        root.scrollToCurrentDifference()
        root.statusText = qsTr("Jumped to offset 0x") + parsed.toString(16)
    }

    // Search for a sequence of hex bytes in the loaded rows.
    // The query is a space-separated hex string like "48 65 6c 6c 6f".
    // Searches the formatted hex dump text (e.g. "00000000  48 65 6c 6c 6f  ...Hello").
    function searchHexBytes(query) {
        const bytes = query.trim().split(/\s+/).map(function (s) { return parseInt(s, 16) }).filter(function (b) { return !isNaN(b) })
        if (bytes.length === 0) {
            root.statusText = qsTr("Invalid byte sequence")
            return
        }
        // Build a two-char uppercase hex needle and search in the hex portion.
        const needle = bytes.map(function (b) {
            return (b < 16 ? "0" : "") + b.toString(16).toUpperCase()
        }).join(" ")
        for (let i = 0; i < root.leftRows.length; i++) {
            const text = root.leftRows[i] ? (root.leftRows[i].text || "") : ""
            // The hex portion is between the offset (8 chars + 2 spaces) and the ASCII.
            const hexPart = text.substring(10, 10 + bytesPerRow * 3)
            if (hexPart.indexOf(needle) >= 0) {
                root.currentDiffRow = i
                root.scrollToCurrentDifference()
                root.statusText = qsTr("Found at row ") + (i + 1)
                return
            }
        }
        root.statusText = qsTr("Byte sequence not found in loaded rows")
    }

    // Scroll to the current-diff row.  Suppress scroll-sync during
    // programmatic positioning so the user's free-scroll isn't fought.
    function scrollToCurrentDifference() {
        if (root.currentDiffRow < 0)
            return
        if (root.compareMode === "Folder") {
            if (folderTable && root.currentDiffRow < folderTable.count)
                folderTable.positionViewAtIndex(root.currentDiffRow, ListView.Center)
            return
        }
        // Windowed diff: if the target change is past the loaded rows, fetch the
        // intervening windows first, then position once it is in view.
        if (root.textTotalRows > 0 && root.currentDiffRow >= root.unfilteredLeftRows.length
                && root.unfilteredLeftRows.length < root.textTotalRows) {
            if (root.compareMode === "Hex") {
                root.loadHexWindowsUntil(root.currentDiffRow, function () {
                    root.scrollToCurrentDifference()
                })
            } else {
                root.loadTextWindowsUntil(root.currentDiffRow, function () {
                    root.scrollToCurrentDifference()
                })
            }
            return
        }
        root.syncingScroll = true
        if (leftPane && rightPane) {
            leftPane.positionAtRow(root.currentDiffRow)
            rightPane.positionAtRow(root.currentDiffRow)
        }
        root.syncingScroll = false
    }

    function openFind() {
        root.findVisible = true
        searchField.forceActiveFocus()
        searchField.selectAll()
    }

    function toggleEditMode(side) {
        if (side === "left") {
            if (root.editLeftMode && root.editLeftDirtyText !== "") {
                root.pendingEditToggleSide = side
                editDiscardDialog.open()
                return
            }
            root.editLeftMode = !root.editLeftMode
            if (!root.editLeftMode)
                root.editLeftDirtyText = ""
        } else {
            if (root.editRightMode && root.editRightDirtyText !== "") {
                root.pendingEditToggleSide = side
                editDiscardDialog.open()
                return
            }
            root.editRightMode = !root.editRightMode
            if (!root.editRightMode)
                root.editRightDirtyText = ""
        }
    }

    function saveEdit(side) {
        const path = side === "left" ? root.leftPath : root.rightPath
        const content = side === "left" ? root.editLeftDirtyText : root.editRightDirtyText
        if (path === "")
            return
        const request = new XMLHttpRequest()
        const url = root.bridgeUrl + "/file/write?path=" + encodeURIComponent(path)
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE) {
                if (request.status === 200) {
                    root.statusText = qsTr("Saved ") + side
                    if (side === "left") {
                        root.editLeftDirtyText = ""
                        root.editLeftMode = false
                        root.leftDirty = false
                    } else {
                        root.editRightDirtyText = ""
                        root.editRightMode = false
                        root.rightDirty = false
                    }
                    root.requestCompare(false)
                } else {
                    root.statusText = qsTr("Save failed")
                }
            }
        }
        request.open("POST", url)
        request.send(content)
    }

    // `searchRe` is the regex compiled once per rebuild (or null for literal
    // search); compiling it per row froze the UI on large diffs.
    function rowMatchesSearch(index, searchRe) {
        if (root.searchText === "")
            return false

        if (root.compareMode === "Folder") {
            const entry = root.visibleFolderEntries[index]
            if (!entry)
                return false
            const folderText = [
                entry.path || "",
                entry.state || "",
                folderSizeLabel(entry.leftSize),
                folderSizeLabel(entry.rightSize),
                entry.method || ""
            ].join(" ")
            if (root.searchRegex) {
                return searchRe ? searchRe.test(folderText) : false
            }

            const needle = root.searchCaseSensitive ? root.searchText : root.searchText.toLocaleLowerCase()
            const haystack = root.searchCaseSensitive ? folderText : folderText.toLocaleLowerCase()
            return haystack.indexOf(needle) >= 0
        }

        const leftText = root.leftRows[index] ? String(root.leftRows[index].text || "") : ""
        const rightText = root.rightRows[index] ? String(root.rightRows[index].text || "") : ""
        if (root.searchRegex) {
            return searchRe ? (searchRe.test(leftText) || searchRe.test(rightText)) : false
        }

        const needle = root.searchCaseSensitive ? root.searchText : root.searchText.toLocaleLowerCase()
        const leftHaystack = root.searchCaseSensitive ? leftText : leftText.toLocaleLowerCase()
        const rightHaystack = root.searchCaseSensitive ? rightText : rightText.toLocaleLowerCase()
        return leftHaystack.indexOf(needle) >= 0 || rightHaystack.indexOf(needle) >= 0
    }

    function rebuildSearchRows() {
        const rows = []
        const rowCount = root.compareMode === "Folder" ? root.visibleFolderEntries.length : root.leftRows.length
        // Windowed text diff with an active find: the server matched the whole
        // document, so use its full match-row index list rather than scanning
        // only the loaded window (which would miss matches further down).
        if (root.compareMode !== "Folder" && root.textTotalRows > 0 && root.searchText !== "") {
            root.searchRowIndexes = root.serverSearchRowIndexes
            root.currentSearchPosition = root.serverSearchRowIndexes.length > 0 ? 0 : -1
            root.currentSearchRow = root.serverSearchRowIndexes.length > 0 ? root.serverSearchRowIndexes[0] : -1
            scrollToCurrentSearchResult()
            return
        }
        // Compile the find regex ONCE per rebuild rather than once per row.
        let searchRe = null
        if (root.searchRegex && root.searchText !== "") {
            try {
                searchRe = new RegExp(root.searchText, root.searchCaseSensitive ? "" : "i")
            } catch (e) {
                root.statusText = qsTr("Invalid find regex")
                root.searchRowIndexes = []
                root.currentSearchPosition = -1
                root.currentSearchRow = -1
                return
            }
        }
        for (let index = 0; index < rowCount; index++) {
            if (rowMatchesSearch(index, searchRe))
                rows.push(index)
        }

        root.searchRowIndexes = rows
        root.currentSearchPosition = rows.length > 0 ? 0 : -1
        root.currentSearchRow = rows.length > 0 ? rows[0] : -1
        scrollToCurrentSearchResult()
    }

    function selectSearchResult(position) {
        if (root.searchRowIndexes.length === 0) {
            root.currentSearchPosition = -1
            root.currentSearchRow = -1
            return
        }

        let nextPosition = position
        if (nextPosition < 0)
            nextPosition = root.searchRowIndexes.length - 1
        if (nextPosition >= root.searchRowIndexes.length)
            nextPosition = 0
        root.currentSearchPosition = nextPosition
        root.currentSearchRow = root.searchRowIndexes[nextPosition]
        scrollToCurrentSearchResult()
    }

    function nextSearchResult() {
        selectSearchResult(root.currentSearchPosition + 1)
    }

    function previousSearchResult() {
        selectSearchResult(root.currentSearchPosition - 1)
    }

    function scrollToCurrentSearchResult() {
        if (root.currentSearchRow < 0)
            return
        // Windowed diff: load up to the match row if it is past what is loaded.
        if (root.compareMode !== "Folder" && root.textTotalRows > 0
                && root.currentSearchRow >= root.unfilteredLeftRows.length
                && root.unfilteredLeftRows.length < root.textTotalRows) {
            root.loadTextWindowsUntil(root.currentSearchRow, function () {
                root.scrollToCurrentSearchResult()
            })
            return
        }
        root.syncingScroll = true
        if (leftPane && rightPane) {
            leftPane.positionAtRow(root.currentSearchRow)
            rightPane.positionAtRow(root.currentSearchRow)
        }
        root.syncingScroll = false
    }

    function rowBookmarked(index) {
        return root.bookmarkRows.indexOf(index) >= 0
    }

    function setBookmarkedInRows(rows, row, bookmarked) {
        if (!rows || row < 0 || row >= rows.length)
            return rows
        const copy = rows.slice()
        const current = copy[row] || {}
        copy[row] = Object.assign({}, current, { "bookmarked": bookmarked })
        return copy
    }

    function setBookmarkStateForRow(row, bookmarked) {
        root.leftRows = setBookmarkedInRows(root.leftRows, row, bookmarked)
        root.rightRows = setBookmarkedInRows(root.rightRows, row, bookmarked)
        root.unfilteredLeftRows = setBookmarkedInRows(root.unfilteredLeftRows, row, bookmarked)
        root.unfilteredRightRows = setBookmarkedInRows(root.unfilteredRightRows, row, bookmarked)
    }

    function syncBookmarkToBridge(row, bookmarked) {
        if (root.bridgeUrl === "")
            return
        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE && request.status !== 200)
                root.statusText = qsTr("Bookmark update failed")
        }
        const url = root.bridgeUrl
            + "/bookmark/set?row=" + encodeURIComponent(row)
            + "&bookmarked=" + (bookmarked ? "1" : "0")
        request.open("GET", url)
        request.send()
    }

    function rebuildBookmarkRows() {
        const rows = []
        for (let index = 0; index < root.leftRows.length; index++) {
            const left = root.leftRows[index]
            const right = root.rightRows[index]
            if ((left && left.bookmarked) || (right && right.bookmarked))
                rows.push(index)
        }
        root.bookmarkRows = rows
        root.currentBookmarkPosition = rows.length > 0 ? 0 : -1
    }

    function toggleBookmarkCurrentRow() {
        const row = root.currentDiffRow >= 0 ? root.currentDiffRow : root.currentSearchRow
        if (row < 0)
            return
        const rows = root.bookmarkRows.slice()
        const existing = rows.indexOf(row)
        const bookmarked = existing < 0
        if (existing >= 0)
            rows.splice(existing, 1)
        else
            rows.push(row)
        rows.sort(function(a, b) { return a - b })
        root.bookmarkRows = rows
        root.currentBookmarkPosition = rows.indexOf(row)
        root.setBookmarkStateForRow(row, bookmarked)
        root.updateActiveTabSnapshot()
        root.syncBookmarkToBridge(row, bookmarked)
    }

    function selectBookmark(position) {
        if (root.bookmarkRows.length === 0) {
            root.currentBookmarkPosition = -1
            return
        }
        let nextPosition = position
        if (nextPosition < 0)
            nextPosition = root.bookmarkRows.length - 1
        if (nextPosition >= root.bookmarkRows.length)
            nextPosition = 0
        root.currentBookmarkPosition = nextPosition
        const row = root.bookmarkRows[nextPosition]
        root.currentDiffRow = row
        root.currentDiffPosition = root.diffRowIndexes.indexOf(row)
        root.syncingScroll = true
        if (leftPane && rightPane) {
            leftPane.positionAtRow(row)
            rightPane.positionAtRow(row)
        }
        root.syncingScroll = false
    }

    function nextBookmark() {
        selectBookmark(root.currentBookmarkPosition + 1)
    }

    function previousBookmark() {
        selectBookmark(root.currentBookmarkPosition - 1)
    }

    function loadLaunchArguments() {
        // Bridge info is written by the Rust binary to /tmp/linsync/bridge-info.json
        // (or LINSYNC_BRIDGE_INFO env var on systems that support it).
        // qml6 treats everything after -f as file paths, so argv doesn't work.
        readBridgeInfoFile()
    }

    function isLoopbackBridgeUrl(url) {
        // The Rust bridge always binds 127.0.0.1, so a legitimate sidecar URL
        // begins with one of these loopback prefixes. Anything else is treated
        // as untrusted (e.g. a tampered/planted sidecar file).
        return url.indexOf("http://127.0.0.1:") === 0
            || url.indexOf("http://[::1]:") === 0
    }

    function readBridgeInfoFile() {
        // Read bridge config from well-known temp path written by Rust binary.
        // XMLHttpRequest on file:// URLs returns status 0 on Qt < 6.10,
        // but 200 on Qt 6.10+. Accept both.
        try {
            var xhr = new XMLHttpRequest()
            xhr.open("GET", "file:///tmp/linsync/bridge-info.json", false)
            xhr.send()
            if (xhr.status === 0 || xhr.status === 200) {
                var info = JSON.parse(xhr.responseText)
                if (info && info.bridge_url) {
                    // Only ever talk to a loopback bridge. This rejects a
                    // planted/tampered sidecar that points bridge_url at a
                    // non-loopback (attacker-controlled) host, which would
                    // otherwise receive our session, saves, and file content.
                    if (root.isLoopbackBridgeUrl(info.bridge_url)) {
                        root.bridgeUrl = info.bridge_url
                        console.log("LinSync bridge URL set to " + info.bridge_url)
                    } else {
                        console.warn("LinSync: refusing non-loopback bridge URL: " + info.bridge_url)
                    }
                }
                if (info && info.version)
                    root.appVersion = info.version
                if (info && info.context_path)
                    readLaunchContext(info.context_path)
                if (info && info.section) {
                    var sectionMap = {"compare":0, "sessions":1, "filters":2, "plugins":3, "settings":4, "about":5}
                    if (info.section in sectionMap)
                        root.activeSection = sectionMap[info.section]
                }
            } else {
                console.log("LinSync: XHR status " + xhr.status + " reading bridge info")
            }
        } catch(e) {
            console.log("LinSync: XHR error reading bridge info: " + e)
        }
    }


    function defaultUiSettings() {
        return {
            "themePreference": 0,
            "fontSize": 12,
            "fontFamily": "monospace",
            "tabWidth": 4,
            "showLineNumbers": true,
            "showWhitespace": false,
            "wordWrap": false,
            "ignoreCase": false,
            "ignoreWhitespace": false,
            "ignoreBlankLines": false,
            "ignoreEol": true,
            "eolNormalization": "auto",
            "defaultCompareMode": "Text",
            "confirmOnClose": true,
            "persistRecentPaths": true,
            "reduceMotion": false,
            "detectMoves": false,
            "keepArchiveBackup": false,
            "maxRecentPaths": 20
        }
    }

    function applyUiSettings(settings) {
        const merged = Object.assign(defaultUiSettings(), settings || {})
        root._settingsReady = false
        root.themePreference    = themeTokens.normalizeTheme(merged.themePreference)
        root.paneFontSize       = merged.fontSize
        root.paneFontFamily     = merged.fontFamily
        root.paneTabWidth       = merged.tabWidth
        root.showLineNumbers    = merged.showLineNumbers
        root.showWhitespace     = merged.showWhitespace
        root.wordWrap           = merged.wordWrap
        root.ignoreCase         = merged.ignoreCase
        root.ignoreWhitespace   = merged.ignoreWhitespace
        root.ignoreBlankLines   = merged.ignoreBlankLines
        root.ignoreEol          = merged.ignoreEol
        root.eolNormalization   = merged.eolNormalization
        root.defaultCompareMode = merged.defaultCompareMode
        root.confirmOnClose     = merged.confirmOnClose
        root.persistRecentPaths = merged.persistRecentPaths
        root.reduceMotion       = merged.reduceMotion
        root.detectMoves        = merged.detectMoves
        root.keepArchiveBackup  = merged.keepArchiveBackup
        root.maxRecentPaths     = merged.maxRecentPaths
        // Only reset the active compare mode when no comparison is in progress;
        // otherwise a late settings response would clobber the mode chosen by the
        // user or auto-detected from the launch context (e.g. table files).
        if (root.leftPath === "" && root.rightPath === "")
            root.compareMode = root.defaultCompareMode
        root._settingsReady = true
    }

    function applySingleSetting(key, value) {
        if      (key === "themePreference")    root.themePreference    = themeTokens.normalizeTheme(value)
        else if (key === "fontSize")           root.paneFontSize       = value
        else if (key === "fontFamily")         root.paneFontFamily     = value
        else if (key === "tabWidth")           root.paneTabWidth       = value
        else if (key === "showLineNumbers")    root.showLineNumbers    = value
        else if (key === "showWhitespace")     root.showWhitespace     = value
        else if (key === "wordWrap")           root.wordWrap           = value
        else if (key === "ignoreCase")         root.ignoreCase         = value
        else if (key === "ignoreWhitespace")   root.ignoreWhitespace   = value
        else if (key === "ignoreBlankLines")   root.ignoreBlankLines   = value
        else if (key === "ignoreEol")          root.ignoreEol          = value
        else if (key === "eolNormalization")   root.eolNormalization   = value
        else if (key === "defaultCompareMode") root.defaultCompareMode = value
        else if (key === "confirmOnClose")     root.confirmOnClose     = value
        else if (key === "persistRecentPaths") root.persistRecentPaths = value
        else if (key === "reduceMotion")       root.reduceMotion       = value
        else if (key === "detectMoves")        root.detectMoves        = value
        else if (key === "keepArchiveBackup")  root.keepArchiveBackup  = value
        else if (key === "maxRecentPaths")     root.maxRecentPaths     = value
    }

    function loadUiSettings() {
        if (root.bridgeUrl === "") {
            applyUiSettings(defaultUiSettings())
            return
        }

        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE) {
                if (request.status === 200)
                    applyUiSettings(JSON.parse(request.responseText))
                else
                    applyUiSettings(defaultUiSettings())
            }
        }
        request.open("GET", root.bridgeUrl + "/settings")
        request.send()
    }

    function persistUiSetting(key, value) {
        if (!root._settingsReady)
            return
        if (root.bridgeUrl === "")
            return

        const request = new XMLHttpRequest()
        const url = root.bridgeUrl + "/settings/set?key=" + encodeURIComponent(key)
            + "&value=" + encodeURIComponent(String(value))
        request.open("GET", url)
        request.send()
    }

    function updateUiSetting(key, value) {
        applySingleSetting(key, value)
        persistUiSetting(key, value)
    }

    function bridgeAvailable() {
        return root.bridgeUrl !== ""
    }

    function bridgeGet(path, onJson) {
        if (root.bridgeUrl === "")
            return false
        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE) {
                // Parse the body for error responses too: the bridge returns
                // structured JSON ({error, retryable, ...}) on failure, and
                // callbacks need it to surface the message and react.
                let payload = null
                try { payload = JSON.parse(request.responseText) } catch (_e) {}
                const ok = request.status >= 200 && request.status < 300
                if (onJson) onJson(ok, payload, request.status)
            }
        }
        request.open("GET", root.bridgeUrl + path)
        request.send()
        return true
    }

    function openConfigFolder() {
        bridgeGet("/folder/open?key=config", function (ok) {
            root.statusText = ok ? "Opened config folder" : "Could not open config folder"
        })
    }

    function reopenRecentSession(index) {
        bridgeGet("/sessions/reopen?index=" + encodeURIComponent(index), function (ok, payload) {
            if (ok && payload) {
                applyLaunchContext(payload, false)
                root.activeSection = 0
            } else {
                root.statusText = "Could not reopen session"
            }
        })
    }

    function loadRecentSessions(callback) {
        bridgeGet("/sessions/recent", function (ok, payload) {
            if (ok && payload && callback)
                callback(payload.sessions || [])
        })
    }

    function loadPlugins(callback) {
        bridgeGet("/plugins/list", function (ok, payload) {
            if (ok && payload && callback)
                callback(payload)
        })
    }

    // Populate the Compare-page profile selector from /profiles/list and
    // pre-select the active id (falling back to `default`).
    function loadProfiles() {
        if (root.bridgeUrl === "")
            return
        bridgeGet("/profiles/list", function (ok, payload, status) {
            if (!ok || !payload) {
                root.profileError = qsTr("Could not load profiles")
                return
            }
            root.profileError = ""
            root.profileEntries = payload.profiles || []
            root.activeProfileId = payload.active || "default"
            var idx = root.profileIndexOf(root.activeProfileId)
            if (idx < 0)
                idx = root.profileIndexOf("default")
            if (idx >= 0)
                profileSelector.currentIndex = idx
        })
    }

    function profileIndexOf(id) {
        for (var i = 0; i < root.profileEntries.length; i++) {
            if (root.profileEntries[i] && root.profileEntries[i].id === id)
                return i
        }
        return -1
    }

    // Persist the active profile via /profiles/active/set. Unknown ids return
    // 400/404, which we surface inline and recover from by re-syncing to the
    // server's persisted selection.
    function setActiveProfile(id) {
        if (!id || root.bridgeUrl === "")
            return
        bridgeGet("/profiles/active/set?id=" + encodeURIComponent(id), function (ok, payload, status) {
            if (ok) {
                root.activeProfileId = id
                root.profileError = ""
                root.statusText = qsTr("Active profile: %1").arg(id)
            } else if (status === 404 || status === 400) {
                root.profileError = qsTr("Profile “%1” could not be selected").arg(id)
                root.loadProfiles()
            } else {
                root.profileError = qsTr("Failed to set profile (HTTP %1)").arg(status)
                root.loadProfiles()
            }
        })
    }

    onBridgeUrlChanged: {
        if (root.bridgeUrl !== "") {
            root.loadProfiles()
            root.refreshArchiveEditability()
        }
    }

    function loadFilters(callback) {
        bridgeGet("/filters/list", function (ok, payload) {
            if (ok && payload && callback)
                callback(payload)
        })
    }

    function loadWalkOptions(callback) {
        bridgeGet("/walk", function (ok, payload) {
            if (ok && payload && callback)
                callback(payload)
        })
    }

    function saveWalkOption(key, value) {
        bridgeGet("/walk/set?key=" + encodeURIComponent(key)
                  + "&value=" + encodeURIComponent(String(value)),
                  null)
    }

    function saveNamedFilter(body, callback) {
        bridgeGet("/filters/save?body=" + encodeURIComponent(body), function (ok, payload, status) {
            if (callback) callback(ok, payload, status)
        })
    }

    function deleteNamedFilter(name, callback) {
        bridgeGet("/filters/delete?name=" + encodeURIComponent(name), function (ok, payload, status) {
            if (callback) callback(ok, payload, status)
        })
    }

    function validateFilterRule(body, callback) {
        bridgeGet("/filters/validate?body=" + encodeURIComponent(body), function (ok, payload) {
            if (callback) callback(ok, payload)
        })
    }

    function fetchMergeConflicts(callback) {
        bridgeGet("/merge/conflicts", function (ok, payload) {
            if (callback) callback(ok, payload)
        })
    }

    function currentFolderEntryPath() {
        if (root.compareMode !== "Folder")
            return ""
        if (root.currentDiffRow < 0)
            return ""
        if (root.visibleFolderEntries.length > root.currentDiffRow) {
            const entry = root.visibleFolderEntries[root.currentDiffRow]
            if (entry && entry.path)
                return String(entry.path)
        }
        const row = root.leftRows[root.currentDiffRow] || root.rightRows[root.currentDiffRow]
        if (!row) return ""
        const id = String(row.row_id || "")
        if (id.indexOf("folder:") === 0)
            return id.substring("folder:".length)
        return String(row.text || "").replace(/\/$/, "")
    }

    // Encode each selected entry as its own `entries=` query param so paths
    // that contain a comma are not split apart server-side.
    function encodeEntries(entries) {
        var qs = ""
        var list = entries || []
        for (var i = 0; i < list.length; i++)
            qs += "&entries=" + encodeURIComponent(list[i])
        return qs
    }

    function planFolderOp(kind, entries, callback) {
        const qs = "/folder/op/plan?kind=" + encodeURIComponent(kind)
                 + root.encodeEntries(entries)
        bridgeGet(qs, function (ok, payload) {
            if (callback) callback(ok, payload)
        })
    }

    function runFolderOp(kind) {
        const entry = root.currentFolderEntryPath()
        if (entry === "") {
            root.statusText = "Select a folder row first"
            return
        }
        pendingFolderOpKind = kind
        pendingFolderOpEntries = [entry]
        root.planFolderOp(kind, [entry], function (ok, payload) {
            if (!ok || !payload) {
                root.statusText = "Folder op plan failed"
                return
            }
            const counts = payload.counts || {}
            const warnings = payload.warnings || []
            const opCount = (payload.operations || []).length
            folderOpDialog.summary = qsTr("%1 operation(s), %2 warning(s)")
                .arg(opCount).arg(warnings.length)
            folderOpDialog.details = JSON.stringify(payload, null, 2)
            folderOpDialog.permanentDelete = payload.permanent_delete === true
            folderOpDialog.permanentWarning = String(payload.permanent_warning || "")
            folderOpDialog.open()
        })
    }

    function executeFolderOp(kind, entries, options, callback) {
        let qs = "/folder/op/execute?kind=" + encodeURIComponent(kind)
               + root.encodeEntries(entries)
        if (options && options.new_name)
            qs += "&new_name=" + encodeURIComponent(options.new_name)
        if (options && options.confirm_permanent)
            qs += "&confirm_permanent=1"
        bridgeGet(qs, function (ok, payload) {
            if (callback) callback(ok, payload)
        })
    }

    function clearArchiveEditState() {
        root.archiveEditInProgress = false
        root.archiveEditToken = ""
        root.archiveEditTabId = -1
        root.archiveEditMember = ""
        root.archiveEditSide = ""
        root.archiveEditPortalWarning = ""
    }

    function startArchiveMemberEdit(side) {
        const entry = root.currentFolderEntryPath()
        if (entry === "") {
            root.statusText = "Select an archive member first"
            return
        }
        // Editability (zip-only, member rules) is core's decision: the bridge
        // returns a precise 400 for anything unsupported, surfaced below.
        const archivePath = side === "left" ? root.leftPath : root.rightPath
        const qs = "/archive/member/edit?archive=" + encodeURIComponent(archivePath)
                 + "&member=" + encodeURIComponent(entry)
        bridgeGet(qs, function (ok, payload) {
            if (!ok || !payload || !payload.ok) {
                root.statusText = payload && payload.error ? payload.error : "Failed to extract archive member for editing"
                return
            }
            root.archiveEditToken = payload.token || ""
            root.archiveEditTabId = root.activeTabId
            root.archiveEditMember = entry
            root.archiveEditSide = side
            root.archiveEditInProgress = true
            root.archiveEditPortalWarning = payload.atomic === false
                ? "Portal archive: commit is non-atomic and keeps a backup in app state."
                : ""
            // Open in external editor
            root.bridgeGet("/open-external?path=" + encodeURIComponent(payload.staged_path), function (ok2) {
                root.statusText = ok2 ? "Editing " + entry + " in external editor" : "Could not open external editor"
            })
        })
    }

    function commitArchiveMemberEdit() {
        if (!root.archiveEditInProgress || root.archiveEditToken === "") {
            root.statusText = "No active archive edit to commit"
            return
        }
        const qs = "/archive/member/commit?token=" + encodeURIComponent(root.archiveEditToken)
                 + (root.keepArchiveBackup ? "&keep_backup=1" : "")
        bridgeGet(qs, function (ok, payload) {
            if (ok && payload && payload.ok) {
                root.statusText = "Archive member updated successfully"
                root.clearArchiveEditState()
                // Refresh the compare to show updated content
                root.requestCompare(false)
            } else {
                let message = payload && payload.error ? payload.error : "Failed to commit archive edit"
                if (payload && payload.backup_path)
                    message += " — original archive backup: " + payload.backup_path
                root.statusText = message
                // The bridge keeps the token (and the staged edit) registered
                // on failure so the user can retry or discard; only drop local
                // state when it reports the token was not retained.
                if (!payload || payload.token_retained !== true)
                    root.clearArchiveEditState()
            }
        })
    }

    function discardArchiveMemberEdit() {
        if (!root.archiveEditInProgress || root.archiveEditToken === "") {
            root.archiveEditInProgress = false
            return
        }
        const qs = "/archive/member/discard?token=" + encodeURIComponent(root.archiveEditToken)
        bridgeGet(qs, function (ok) {
            root.clearArchiveEditState()
            root.statusText = ok ? "Archive edit discarded" : "Archive edit discarded (cleanup may have failed)"
        })
    }

    function resetUiSettings() {
        if (root.bridgeUrl === "") {
            applyUiSettings(defaultUiSettings())
            return
        }

        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE) {
                if (request.status === 200)
                    applyUiSettings(JSON.parse(request.responseText))
                else
                    applyUiSettings(defaultUiSettings())
            }
        }
        request.open("GET", root.bridgeUrl + "/settings/reset")
        request.send()
    }

    function summaryItemsFromBridge(fallback, preferBridge) {
        return fallback
    }

    function recentPathsFromBridge(fallback, preferBridge) {
        // Honour the user's persistRecentPaths toggle — when off we just
        // surface an empty list so nothing leaks into the Sessions page
        // or onto disk via the bridge.
        if (!root.persistRecentPaths)
            return []

        let items = fallback
        // Apply the user's maxRecentPaths cap (lower bound 1) so the
        // Sessions page and the bridge persistence stay in sync.
        const cap = Math.max(1, root.maxRecentPaths)
        if (items && items.length > cap)
            return items.slice(0, cap)
        return items
    }

    function tabItemsFromSession(tabs) {
        const items = []
        if (!tabs)
            return items

        for (let index = 0; index < tabs.length; index++) {
            const tab = tabs[index]
            items.push({
                "id": tab.id || 0,
                "title": tab.title || "Compare",
                "dirty": tab.left_dirty || tab.right_dirty || false,
                "can_undo": tab.can_undo || false,
                "can_redo": tab.can_redo || false
            })
        }
        return items
    }

    function tabItemsFromBridge(fallback, preferBridge) {
        return fallback
    }

    function applySessionContextJson(contextJson) {
        if (!contextJson || contextJson === "") {
            root.statusText = "Session bridge returned no state"
            return
        }

        applyLaunchContext(JSON.parse(contextJson), true)
    }

    function readLaunchContext(path) {
        const request = new XMLHttpRequest()
        const url = path.startsWith("file:") ? path : "file://" + path
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE) {
                if (request.status === 0 || request.status === 200) {
                    applyLaunchContext(JSON.parse(request.responseText), false)
                } else {
                    root.statusText = "Unable to load launch context"
                }
            }
        }
        request.open("GET", url)
        request.send()
    }

    function applyLaunchContext(context, preferBridge) {
        const session = context.session || {}
        const recentPaths = recentPathsFromBridge(session.recent_paths || [], preferBridge)
        root.sessionState = Object.assign({}, session, { "recent_paths": recentPaths })
        root.tabItems = tabItemsFromBridge(tabItemsFromSession(root.sessionState.tabs), preferBridge)
        const activeTab = activeSessionTab(context)
        root.activeTabId = activeTab.id || 0
        applySessionTab(activeTab, preferBridge)
        // Honour startup_section forwarded by the Rust binary (sourced from
        // LINSYNC_STARTUP_SECTION). Used by the screenshot capture pipeline.
        if (context.startup_section) {
            const sectionMap = { "compare": 0, "sessions": 1, "filters": 2,
                                 "plugins": 3, "settings": 4, "about": 5,
                                 "credits": 6, "licenses": 7, "merge": 8,
                                 "image": 9, "webpage": 10, "document": 11 }
            if (context.startup_section in sectionMap) {
                root.activeSection = sectionMap[context.startup_section]
            }
        }

        // Git-mergetool launch: open the Merge workspace with the three inputs
        // and a predetermined output path, then start the three-way session.
        if (context.merge && context.merge.base && context.merge.left && context.merge.right) {
            root.mergeBasePath = context.merge.base
            root.mergeLeftPath = context.merge.left
            root.mergeRightPath = context.merge.right
            root.mergeOutputPath = context.merge.output || ""
            root.activeSection = 8
            mergePage.start()
        }
    }

    function applySessionTab(tab, preferBridge) {
        if (!tab)
            return

        root.activeTabId = tab.id || 0
        root.leftPath = tab.left_path || ""
        root.rightPath = tab.right_path || ""
        root.basePath = tab.base_path || ""
        root.compareMode = tab.mode || "Text"
        root.statusText = tab.status || "Ready"
        root.summaryItems = summaryItemsFromBridge(tab.summary || [], preferBridge)
        const fallbackLeftRows = tab.left_rows && tab.left_rows.length > 0 ? tab.left_rows : makeBlankRows()
        const fallbackRightRows = tab.right_rows && tab.right_rows.length > 0 ? tab.right_rows : makeBlankRows()
        root.unfilteredLeftRows = fallbackLeftRows
        root.unfilteredRightRows = fallbackRightRows
        // Windowed large text diffs: the response embeds only the first window
        // plus the full row count and navigation index lists. A small diff has
        // no total_rows, so textTotalRows stays 0 (fully loaded).
        const windowedTotal = (root.compareMode !== "Folder" && root.compareMode !== "Table" && tab.total_rows)
            ? Number(tab.total_rows) : 0
        root.textTotalRows = windowedTotal
        root.serverDiffRowIndexes = (windowedTotal > 0 && tab.diff_row_indexes) ? tab.diff_row_indexes : []
        root.serverSearchRowIndexes = (windowedTotal > 0 && tab.search_row_indexes) ? tab.search_row_indexes : []
        root.folderEntries = tab.folder_entries || []
        // Windowed large folder: the response embeds only the first page plus the
        // full entry count; further pages (and any sort/filter/search) are served
        // by /folder/query. A small folder has no folder_total, so
        // folderTotalEntries stays 0 (loaded + sorted/filtered client-side).
        root.folderTotalEntries = (root.compareMode === "Folder" && tab.folder_total)
            ? Number(tab.folder_total) : 0
        root.tableCells = tab.table_cells || []
        root.tableHeaders = tab.table_headers || []
        root.tableTotalRows = (root.compareMode === "Table" && tab.total_rows)
            ? Number(tab.total_rows) : 0
        // Render the embedded page directly (no re-query — the first page is
        // already here); sort/filter/search/scroll re-query when windowed.
        root.rebuildFolderView()
        const validation = tab.validation || {}
        root.validationCompatible = validation.compatible || false
        root.validationMessage = validation.message || ""
        root.validationPathKind = validation.path_kind || ""
        const count = tab.difference_count || 0
        setDifferenceCount(count)
        const modeIndex = modeSelector.model.indexOf(root.compareMode)
        modeSelector.currentIndex = modeIndex >= 0 ? modeIndex : 0
        root.leftDirty = tab.left_dirty || false
        root.rightDirty = tab.right_dirty || false
        root.canUndo = tab.can_undo || false
        root.canRedo = tab.can_redo || false
        rebuildDiffRows()
        rebuildSearchRows()
        rebuildBookmarkRows()
    }

    function activeSessionTab(context) {
        if (!context.session || !context.session.tabs || context.session.tabs.length === 0)
            return context

        const activeId = context.session.active_tab_id || 0
        for (let index = 0; index < context.session.tabs.length; index++) {
            if (context.session.tabs[index].id === activeId)
                return context.session.tabs[index]
        }
        return context.session.tabs[0]
    }

    function activateSessionTab(tabId) {
        if (tabId === root.activeTabId)
            return

        if (root.bridgeUrl !== "") {
            const request = new XMLHttpRequest()
            const url = root.bridgeUrl + "/tab/activate?id=" + encodeURIComponent(tabId)
            request.onreadystatechange = function () {
                if (request.readyState === XMLHttpRequest.DONE) {
                    if (request.status === 200) {
                        applyLaunchContext(JSON.parse(request.responseText), false)
                    } else {
                        root.statusText = "Tab switch failed"
                    }
                }
            }
            request.open("GET", url)
            request.send()
            return
        }

        if (!root.sessionState.tabs)
            return

        updateActiveTabSnapshot()
        for (let index = 0; index < root.sessionState.tabs.length; index++) {
            const tab = root.sessionState.tabs[index]
            if (tab.id === tabId) {
                const nextSession = Object.assign({}, root.sessionState, { "active_tab_id": tabId })
                root.sessionState = nextSession
                applySessionTab(tab, false)
                return
            }
        }
    }

    function updateActiveTabSnapshot() {
        if (!root.sessionState.tabs)
            return

        const tabs = root.sessionState.tabs.slice()
        for (let index = 0; index < tabs.length; index++) {
            if (tabs[index].id === root.activeTabId) {
                const tab = Object.assign({}, tabs[index])
                tab.mode = root.compareMode
                tab.left_path = root.leftPath
                tab.right_path = root.rightPath
                tab.base_path = root.basePath || undefined
                tab.status = root.statusText
                tab.difference_count = currentDifferenceCount()
                tab.left_dirty = root.leftDirty
                tab.right_dirty = root.rightDirty
                tab.can_undo = root.canUndo
                tab.can_redo = root.canRedo
                tab.summary = root.summaryItems
                tab.left_rows = root.compareMode === "Folder" ? [] : root.leftRows
                tab.right_rows = root.compareMode === "Folder" ? [] : root.rightRows
                tab.folder_entries = root.folderEntries
                tabs[index] = tab
                root.sessionState = Object.assign({}, root.sessionState, { "active_tab_id": root.activeTabId, "tabs": tabs })
                root.tabItems = tabItemsFromSession(tabs)
                return
            }
        }
    }

    function closeActiveTab() {
        if (root.activeTabId === 0)
            return

        if ((root.leftDirty || root.rightDirty) && root.confirmOnClose) {
            root.pendingCloseTabId = root.activeTabId
            closeDirtyDialog.open()
            return
        }

        performCloseTab(root.activeTabId)
    }

    function performCloseTab(tabId) {
        if (tabId === 0)
            return

        // Closing the active tab while a compare is running: cancel it so
        // the bridge stops the work instead of finishing it for a tab that
        // is about to be gone.
        if (tabId === root.activeTabId)
            cancelActiveCompare()

        // Closing the tab that owns an in-flight archive edit would strand it
        // (no banner to reach commit/discard). Discard it first so its staging
        // and token are released rather than leaked to the startup sweep.
        if (root.archiveEditInProgress && root.archiveEditTabId === tabId)
            root.discardArchiveMemberEdit()

        if (root.bridgeUrl === "") {
            closeLocalTab(tabId)
            return
        }

        const request = new XMLHttpRequest()
        const url = root.bridgeUrl + "/tab/close?id=" + encodeURIComponent(tabId)
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE) {
                if (request.status === 200) {
                    applyLaunchContext(JSON.parse(request.responseText), false)
                } else {
                    root.statusText = "Close tab failed"
                }
            }
        }
        request.open("GET", url)
        request.send()
    }

    function saveDirtySidesThenClose() {
        const tabId = root.pendingCloseTabId || root.activeTabId
        root.pendingCloseTabId = 0
        if (tabId === 0)
            return

        if (!root.leftDirty && !root.rightDirty) {
            performCloseTab(tabId)
            return
        }

        if (root.bridgeUrl !== "") {
            root.statusText = "Saving"
            const request = new XMLHttpRequest()
            const url = root.bridgeUrl + "/save?side=dirty"
            request.onreadystatechange = function () {
                if (request.readyState === XMLHttpRequest.DONE) {
                    if (request.status === 200) {
                        applyLaunchContext(JSON.parse(request.responseText), false)
                        performCloseTab(tabId)
                    } else {
                        root.statusText = "Save failed"
                    }
                }
            }
            request.open("GET", url)
            request.send()
            return
        }

        root.statusText = "Save bridge unavailable"
    }

    function discardDirtyTabAndClose() {
        const tabId = root.pendingCloseTabId || root.activeTabId
        root.pendingCloseTabId = 0
        performCloseTab(tabId)
    }

    function saveDirtySides() {
        if (!root.leftDirty && !root.rightDirty) {
            root.statusText = "No dirty sides to save"
            return
        }

        if (root.bridgeUrl !== "") {
            root.statusText = "Saving"
            const request = new XMLHttpRequest()
            const url = root.bridgeUrl + "/save?side=dirty"
            request.onreadystatechange = function () {
                if (request.readyState === XMLHttpRequest.DONE) {
                    if (request.status === 200) {
                        applyLaunchContext(JSON.parse(request.responseText), false)
                    } else {
                        root.statusText = "Save failed"
                    }
                }
            }
            request.open("GET", url)
            request.send()
            return
        }

        root.statusText = "Save bridge unavailable"
    }

    function undoLastMergeAction() {
        if (!root.canUndo) {
            root.statusText = "Nothing to undo"
            return
        }

        if (root.bridgeUrl !== "") {
            root.statusText = "Undoing"
            const request = new XMLHttpRequest()
            const url = root.bridgeUrl + "/undo"
            request.onreadystatechange = function () {
                if (request.readyState === XMLHttpRequest.DONE) {
                    if (request.status === 200) {
                        applyLaunchContext(JSON.parse(request.responseText), false)
                    } else {
                        root.statusText = "Undo failed"
                    }
                }
            }
            request.open("GET", url)
            request.send()
            return
        }

        root.statusText = "Undo bridge unavailable"
    }

    function redoLastMergeAction() {
        if (!root.canRedo) {
            root.statusText = "Nothing to redo"
            return
        }

        if (root.bridgeUrl !== "") {
            root.statusText = "Redoing"
            const request = new XMLHttpRequest()
            const url = root.bridgeUrl + "/redo"
            request.onreadystatechange = function () {
                if (request.readyState === XMLHttpRequest.DONE) {
                    if (request.status === 200) {
                        applyLaunchContext(JSON.parse(request.responseText), false)
                    } else {
                        root.statusText = "Redo failed"
                    }
                }
            }
            request.open("GET", url)
            request.send()
            return
        }

        root.statusText = "Redo bridge unavailable"
    }

    function closeLocalTab(tabId) {
        if (!root.sessionState.tabs)
            return

        const tabs = []
        for (let index = 0; index < root.sessionState.tabs.length; index++) {
            if (root.sessionState.tabs[index].id !== tabId)
                tabs.push(root.sessionState.tabs[index])
        }
        const activeTab = tabs.length > 0 ? tabs[tabs.length - 1] : null
        root.sessionState = {
            "active_tab_id": activeTab ? activeTab.id : 0,
            "tabs": tabs,
            "recent_paths": root.sessionState.recent_paths || []
        }
        root.tabItems = tabItemsFromSession(tabs)
        if (activeTab) {
            applySessionTab(activeTab, false)
        } else {
            root.activeTabId = 0
            root.leftPath = ""
            root.rightPath = ""
            root.summaryItems = []
            root.tabItems = []
            root.leftRows = makeBlankRows()
            root.rightRows = makeBlankRows()
            root.textTotalRows = 0
            root.serverDiffRowIndexes = []
            root.serverSearchRowIndexes = []
            root.folderTotalEntries = 0
            root.leftDirty = false
            root.rightDirty = false
            root.validationCompatible = false
            root.validationMessage = ""
            root.validationPathKind = ""
            root.canUndo = false
            root.canRedo = false
            root.statusText = "Ready"
            setDifferenceCount(0)
            rebuildDiffRows()
        }
    }

    // The compare-option query params shared by /compare and the windowed
    // /compare/text/window fetches, so a fetched window is built with exactly
    // the same render mode / ignore flags / syntax / find as the first window.
    function textOptionParams() {
        let params = ""
        params += "&ignore_case=" + (root.ignoreCase ? "1" : "0")
        params += "&ignore_whitespace=" + (root.ignoreWhitespace ? "1" : "0")
        params += "&ignore_blank_lines=" + (root.ignoreBlankLines ? "1" : "0")
        params += "&ignore_eol=" + (root.ignoreEol ? "1" : "0")
        params += "&eol=" + encodeURIComponent(root.eolNormalization)
        params += "&render_mode=" + encodeURIComponent(root.textRenderMode)
        params += "&syntax=" + encodeURIComponent(root.syntaxMode)
        params += "&encoding=" + encodeURIComponent(root.textEncoding)
        for (let ruleIndex = 0; ruleIndex < root.textRegexRuleSets.length; ruleIndex++)
            params += "&regex_rule_set=" + encodeURIComponent(root.textRegexRuleSets[ruleIndex])
        if (root.contextFolding)
            params += "&context_lines=" + encodeURIComponent(root.contextLines)
        if (root.showOnlyChanges)
            params += "&show_only_changes=1"
        if (root.searchText !== "") {
            params += "&find=" + encodeURIComponent(root.searchText)
            params += "&find_regex=" + (root.searchRegex ? "1" : "0")
            params += "&find_case_sensitive=" + (root.searchCaseSensitive ? "1" : "0")
        }
        return params
    }

    // Fetch the next window of a windowed text diff and append it to the loaded
    // rows. `onDone(loadedMore)` (optional) fires after the append (or
    // immediately when there is nothing more to load / no bridge).
    function loadNextTextWindow(onDone) {
        const finish = function (loaded) { if (onDone) onDone(loaded) }
        if (root.textTotalRows <= 0 || root.bridgeUrl === "" || root.textWindowLoading) {
            finish(false)
            return
        }
        const offset = root.unfilteredLeftRows.length
        if (offset >= root.textTotalRows) {
            finish(false)
            return
        }
        root.textWindowLoading = true
        const request = new XMLHttpRequest()
        let url = root.bridgeUrl + "/compare/text/window?left=" + encodeURIComponent(root.leftPath)
            + "&right=" + encodeURIComponent(root.rightPath)
            + "&offset=" + offset + "&limit=2000"
        url += root.textOptionParams()
        request.onreadystatechange = function () {
            if (request.readyState !== XMLHttpRequest.DONE)
                return
            root.textWindowLoading = false
            if (request.status !== 200) {
                root.statusText = qsTr("Failed to load more rows")
                finish(false)
                return
            }
            const payload = JSON.parse(request.responseText)
            const lw = payload.left_rows || []
            const rw = payload.right_rows || []
            if (lw.length === 0 && rw.length === 0) {
                finish(false)
                return
            }
            // Append without yanking the viewport back to the top.
            root.suppressTextScrollReset = true
            root.unfilteredLeftRows = root.unfilteredLeftRows.concat(lw)
            root.unfilteredRightRows = root.unfilteredRightRows.concat(rw)
            root.leftRows = root.unfilteredLeftRows
            root.rightRows = root.unfilteredRightRows
            root.suppressTextScrollReset = false
            finish(true)
        }
        request.open("GET", url)
        request.send()
    }

    // Keep fetching hex windows until row `targetRow` is loaded (or no more remain),
    // then invoke `onDone`.
    function loadHexWindowsUntil(targetRow, onDone) {
        if (root.textTotalRows <= 0
                || root.unfilteredLeftRows.length > targetRow
                || root.unfilteredLeftRows.length >= root.textTotalRows) {
            if (onDone) onDone()
            return
        }
        root.loadNextHexWindow(function (loadedMore) {
            if (loadedMore && root.unfilteredLeftRows.length <= targetRow
                    && root.unfilteredLeftRows.length < root.textTotalRows)
                root.loadHexWindowsUntil(targetRow, onDone)
            else if (onDone)
                onDone()
        })
    }

    // Keep fetching windows until row `targetRow` is loaded (or no more remain),
    // then invoke `onDone`. Used when navigation jumps to a change/match that
    // lives outside the currently loaded window.
    function loadTextWindowsUntil(targetRow, onDone) {
        if (root.textTotalRows <= 0
                || root.unfilteredLeftRows.length > targetRow
                || root.unfilteredLeftRows.length >= root.textTotalRows) {
            if (onDone) onDone()
            return
        }
        root.loadNextTextWindow(function (loadedMore) {
            if (loadedMore && root.unfilteredLeftRows.length <= targetRow
                    && root.unfilteredLeftRows.length < root.textTotalRows)
                root.loadTextWindowsUntil(targetRow, onDone)
            else if (onDone)
                onDone()
        })
    }

    // Prefetch the next window when the user scrolls within ~two screenfuls of
    // the bottom of the loaded text. Called from the left pane's scroll handler.
    function maybeLoadMoreTextRows(inner) {
        if (root.textTotalRows <= 0 || root.textWindowLoading || !inner)
            return
        if (root.unfilteredLeftRows.length >= root.textTotalRows)
            return
        const remaining = inner.contentHeight - (inner.contentY + inner.height)
        if (remaining < inner.height * 2)
            root.loadNextTextWindow()
    }

    // Fetch the next window of a windowed hex diff and append it.
    function loadNextHexWindow(onDone) {
        const finish = function (loaded) { if (onDone) onDone(loaded) }
        if (root.textTotalRows <= 0 || root.bridgeUrl === "" || root.hexWindowLoading) {
            finish(false)
            return
        }
        const offset = root.unfilteredLeftRows.length
        if (offset >= root.textTotalRows) {
            finish(false)
            return
        }
        root.hexWindowLoading = true
        const request = new XMLHttpRequest()
        const url = root.bridgeUrl + "/binary/window?offset=" + offset + "&limit=2000"
        request.onreadystatechange = function () {
            if (request.readyState !== XMLHttpRequest.DONE)
                return
            root.hexWindowLoading = false
            if (request.status !== 200) {
                root.statusText = qsTr("Failed to load more hex rows")
                finish(false)
                return
            }
            const payload = JSON.parse(request.responseText)
            const lw = payload.left_rows || []
            const rw = payload.right_rows || []
            if (lw.length === 0 && rw.length === 0) {
                finish(false)
                return
            }
            root.suppressTextScrollReset = true
            root.unfilteredLeftRows = root.unfilteredLeftRows.concat(lw)
            root.unfilteredRightRows = root.unfilteredRightRows.concat(rw)
            root.leftRows = root.unfilteredLeftRows
            root.rightRows = root.unfilteredRightRows
            root.suppressTextScrollReset = false
            finish(true)
        }
        request.open("GET", url)
        request.send()
    }

    // Prefetch the next window of a large hex diff as the user nears the bottom.
    function maybeLoadMoreHexRows(inner) {
        if (root.textTotalRows <= 0 || root.hexWindowLoading || !inner)
            return
        if (root.unfilteredLeftRows.length >= root.textTotalRows)
            return
        const remaining = inner.contentHeight - (inner.contentY + inner.height)
        if (remaining < inner.height * 2)
            root.loadNextHexWindow()
    }

    // Fetch the next window of a windowed table compare and append it.
    function loadNextTableWindow(onDone) {
        const finish = function (loaded) { if (onDone) onDone(loaded) }
        if (root.tableTotalRows <= 0 || root.bridgeUrl === "" || root.tableWindowLoading) {
            finish(false)
            return
        }
        if (root.tableCells.length >= root.tableTotalRows) {
            finish(false)
            return
        }
        root.tableWindowLoading = true
        const request = new XMLHttpRequest()
        const url = root.bridgeUrl + "/compare/table/window?offset=" + root.tableCells.length + "&limit=2000"
        request.onreadystatechange = function () {
            if (request.readyState !== XMLHttpRequest.DONE)
                return
            root.tableWindowLoading = false
            if (request.status !== 200) {
                root.statusText = qsTr("Failed to load more table rows")
                finish(false)
                return
            }
            const payload = JSON.parse(request.responseText)
            const rows = payload.rows || []
            if (rows.length === 0) {
                finish(false)
                return
            }
            root.tableCells = root.tableCells.concat(rows)
            finish(true)
        }
        request.open("GET", url)
        request.send()
    }

    // Prefetch the next window of a large table compare as the user nears the bottom.
    function maybeLoadMoreTableRows(view) {
        if (root.tableTotalRows <= 0 || root.tableWindowLoading || !view)
            return
        if (root.tableCells.length >= root.tableTotalRows)
            return
        if (view.contentHeight - (view.contentY + view.height) < view.height)
            root.loadNextTableWindow()
    }

    function requestCompare(newTab) {
        if (root.compareMode === "Three-way") {
            if (root.basePath === "" || root.leftPath === "" || root.rightPath === "") {
                root.statusText = qsTr("Select base, left, and right paths")
                return
            }
            root.activeSection = 8
            root.mergeBasePath = root.basePath
            root.mergeLeftPath = root.leftPath
            root.mergeRightPath = root.rightPath
            mergePage.compareOnly = true
            mergePage.start()
            return
        }

        if (root.leftPath === "" || root.rightPath === "") {
            root.statusText = "Select two paths"
            return
        }

        if (root.compareMode === "Webpage") {
            root.activeSection = 10
            webpageComparePage.startFromMain(root.leftPath, root.rightPath, newTab)
            return
        }

        if (root.bridgeUrl === "") {
            root.statusText = "Compare bridge unavailable"
            return
        }

        // Cancel any in-flight compare so its (now-superseded) work stops
        // instead of running concurrently on the bridge. The stale-response
        // guard below ensures the cancelled request's late response can't
        // clobber this one's state.
        cancelActiveCompare()

        root.statusText = "Comparing"
        root.requestCounter += 1
        var reqId = "req-" + root.requestCounter
        root.activeRequestId = reqId
        root.comparing = true
        const request = new XMLHttpRequest()
        let url = root.bridgeUrl + "/compare?left=" + encodeURIComponent(root.leftPath) + "&right=" + encodeURIComponent(root.rightPath)
        url += "&mode=" + encodeURIComponent(root.compareMode)
        // Per-request id so the Stop button can cancel this exact compare.
        url += "&request_id=" + encodeURIComponent(reqId)
        // Surface every compare-related setting on the wire even if the
        // current Rust bridge only consumes a subset. Unknown query
        // params are ignored server-side; getting them in the URL means
        // a future bridge build can opt in without QML changes.
        url += root.textOptionParams()
        url = root.appendBookmarkParams(url)
        if (newTab)
            url += "&new_tab=1"
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE) {
                // Stale-response guard: if a newer compare was started (or
                // this one was cancelled), reqId no longer matches the active
                // id — drop the response without touching comparing/state so
                // the in-flight request owns those flags.
                if (reqId !== root.activeRequestId)
                    return
                root.comparing = false
                root.activeRequestId = ""
                if (request.status === 200) {
                    var payload = JSON.parse(request.responseText)
                    if (payload && payload.cancelled === true) {
                        root.statusText = "Compare cancelled"
                    } else {
                        applyLaunchContext(payload, false)
                    }
                } else {
                    var errPayload = null
                    try { errPayload = JSON.parse(request.responseText) } catch (_e) {}
                    root.statusText = (errPayload && errPayload.error)
                        ? qsTr("Compare failed: %1").arg(errPayload.error)
                        : qsTr("Compare failed")
                }
            }
        }
        // The progressTimer already provides a defensive ceiling for stuck
        // compares; any non-200 response in onreadystatechange above also clears
        // the flags. Keep the request path simple because QML's XMLHttpRequest
        // subset does not expose timeout/onerror callbacks.
        request.open("GET", url)
        request.send()
    }

    // Cancel the in-flight compare (if any) by flipping its bridge-side cancel
    // flag. The /compare response handler then reports "Compare cancelled".
    function cancelActiveCompare() {
        if (!root.comparing || root.activeRequestId === "" || root.bridgeUrl === "")
            return
        root.statusText = "Cancelling…"
        root.bridgeGet("/cancel?id=" + encodeURIComponent(root.activeRequestId), function (ok) {})
    }

    function swapSides() {
        var tmp = root.leftPath
        root.leftPath = root.rightPath
        root.rightPath = tmp
        root.requestCompare(false)
    }

    function reloadCompare() {
        // Reloading re-reads from disk and discards any unsaved edits, so prompt
        // (Save-then-reload / Discard-and-reload) when a side is dirty.
        if ((root.leftDirty || root.rightDirty) && root.confirmOnClose) {
            reloadDirtyDialog.open()
            return
        }
        root.requestCompare(false)
    }

    function folderSizeLabel(size) {
        if (size === undefined || size === null)
            return ""
        const n = Number(size)
        if (n < 1024) return n + " B"
        if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB"
        if (n < 1024 * 1024 * 1024) return (n / (1024 * 1024)).toFixed(1) + " MB"
        return (n / (1024 * 1024 * 1024)).toFixed(1) + " GB"
    }

    function folderEntryMatchesFilter(entry) {
        const state = entry && entry.state ? String(entry.state) : ""
        // State filter.
        if (root.folderFilter === "changed" || root.folderFilter === "diff") {
            if (!(state === "left_only" || state === "right_only" || state === "changed"))
                return false
        } else if (root.folderFilter === "left_only") {
            if (state !== "left_only")
                return false
        } else if (root.folderFilter === "right_only") {
            if (state !== "right_only")
                return false
        }
        // Free-text search over the relative path (case-insensitive).
        if (root.folderSearch !== "") {
            const path = entry && entry.path ? String(entry.path).toLowerCase() : ""
            if (path.indexOf(root.folderSearch.toLowerCase()) === -1)
                return false
        }
        // Entry-type filter (file / directory / symlink / special).
        // folderTypeFilter is a comma-separated list; match if entry type is in it.
        if (root.folderTypeList.length > 0) {
            const ty = entry && entry.entryType
                ? String(entry.entryType)
                : (entry && entry.isDir ? "directory" : "file")
            if (root.folderTypeList.indexOf(ty) === -1)
                return false
        }
        return true
    }

    // Mirror of core's folder_group_label (folder.rs): the bucket label an
    // entry falls under for the current folderGroupBy mode.
    function folderGroupLabel(entry) {
        if (root.folderGroupBy === "state")
            return entry && entry.state ? String(entry.state) : ""
        if (root.folderGroupBy === "type")
            return entry && entry.entryType
                ? String(entry.entryType)
                : (entry && entry.isDir ? "directory" : "file")
        if (root.folderGroupBy === "dir") {
            const path = entry && entry.path ? String(entry.path) : ""
            const slash = path.lastIndexOf("/")
            return slash > 0 ? path.substring(0, slash) : "."
        }
        return ""
    }

    function toggleFolderType(ty) {
        var types = root.folderTypeFilter === "" ? [] : root.folderTypeFilter.split(",")
        var idx = types.indexOf(ty)
        if (idx >= 0)
            types.splice(idx, 1)
        else
            types.push(ty)
        root.folderTypeFilter = types.join(",")
    }

    function rebuildFolderView() {
        if (root.compareMode !== "Folder") {
            root.leftRows = root.unfilteredLeftRows
            root.rightRows = root.unfilteredRightRows
            root.visibleFolderEntries = []
            return
        }
        if (!root.folderEntries || root.folderEntries.length === 0) {
            root.leftRows = []
            root.rightRows = []
            root.visibleFolderEntries = []
            rebuildDiffRows()
            rebuildSearchRows()
            return
        }
        // Windowed folder: /folder/query already filtered + sorted the page that
        // populated folderEntries, so render it as-is (no client filter/sort over
        // a partial set, which would be wrong).
        if (root.folderTotalEntries > 0) {
            root.visibleFolderEntries = root.folderEntries
            root.leftRows = []
            root.rightRows = []
            rebuildDiffRows()
            rebuildSearchRows()
            return
        }
        var entries = []
        for (var i = 0; i < root.folderEntries.length; i++) {
            if (folderEntryMatchesFilter(root.folderEntries[i]))
                entries.push(root.folderEntries[i])
        }
        var col = root.folderSortColumn
        if (col !== "") {
            var asc = root.folderSortAscending
            entries.sort(function (ea, eb) {
                var va, vb
                if (col === "path") {
                    va = ea.path || ""
                    vb = eb.path || ""
                } else if (col === "state") {
                    va = ea.state || ""
                    vb = eb.state || ""
                } else if (col === "leftSize") {
                    va = ea.leftSize || 0
                    vb = eb.leftSize || 0
                } else if (col === "rightSize") {
                    va = ea.rightSize || 0
                    vb = eb.rightSize || 0
                } else if (col === "method") {
                    va = ea.method || ""
                    vb = eb.method || ""
                } else {
                    return 0
                }
                if (va < vb) return asc ? -1 : 1
                if (va > vb) return asc ? 1 : -1
                return 0
            })
        }
        // Grouping, mirroring core's FolderQuery: bucket the (sorted) entries
        // by label, group order following first appearance. The server-side
        // /folder/query path does the same for windowed folders.
        if (root.folderGroupBy !== "") {
            var labels = []
            var buckets = {}
            for (var gi = 0; gi < entries.length; gi++) {
                var key = "g:" + folderGroupLabel(entries[gi])
                if (buckets[key] === undefined) {
                    buckets[key] = []
                    labels.push(key)
                }
                buckets[key].push(entries[gi])
            }
            var grouped = []
            for (var li = 0; li < labels.length; li++)
                grouped = grouped.concat(buckets[labels[li]])
            entries = grouped
        }
        root.visibleFolderEntries = entries
        root.leftRows = []
        root.rightRows = []
        rebuildDiffRows()
        rebuildSearchRows()
    }

    function applyFolderFilter() {
        // Windowed folders sort/filter server-side: re-query from offset 0.
        if (root.folderTotalEntries > 0)
            root.queryFolderPage(0, false)
        else
            rebuildFolderView()
    }

    function toggleFolderSort(column) {
        if (root.folderSortColumn === column) {
            root.folderSortAscending = !root.folderSortAscending
        } else {
            root.folderSortColumn = column
            root.folderSortAscending = true
        }
    }

    function applyFolderSort() {
        if (root.folderTotalEntries > 0)
            root.queryFolderPage(0, false)
        else
            rebuildFolderView()
    }

    // Map the folder view's current sort/filter/search state onto a
    // /folder/query request and (re)populate folderEntries. `append` adds the
    // page to the existing entries (lazy scroll); otherwise it replaces them
    // (a fresh sort/filter/search). Used only for windowed (large) folders.
    function queryFolderPage(offset, append) {
        if (root.bridgeUrl === "" || root.compareMode !== "Folder" || root.folderWindowLoading)
            return
        if (append && root.folderEntries.length >= root.folderEntriesMax) {
            root.statusText = qsTr("Folder view limited to %1 entries; refine filters to load more.").arg(root.folderEntriesMax)
            return
        }
        root.folderWindowLoading = true
        let url = root.bridgeUrl + "/folder/query?left=" + encodeURIComponent(root.leftPath)
            + "&right=" + encodeURIComponent(root.rightPath)
            + "&offset=" + offset + "&limit=5000"
        if (root.folderFilter === "changed" || root.folderFilter === "diff")
            url += "&state=changed"
        else if (root.folderFilter === "left_only")
            url += "&state=left_only"
        else if (root.folderFilter === "right_only")
            url += "&state=right_only"
        if (root.folderSearch !== "")
            url += "&search=" + encodeURIComponent(root.folderSearch)
        if (root.folderTypeFilter !== "")
            url += "&types=" + encodeURIComponent(root.folderTypeFilter)
        if (root.folderSortColumn !== "")
            url += "&sort=" + encodeURIComponent(root.folderSortColumn)
        url += "&descending=" + (root.folderSortAscending ? "0" : "1")
        if (root.folderGroupBy !== "")
            url += "&group_by=" + encodeURIComponent(root.folderGroupBy)
        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState !== XMLHttpRequest.DONE)
                return
            root.folderWindowLoading = false
            if (request.status !== 200) {
                root.statusText = qsTr("Failed to load folder entries")
                return
            }
            const payload = JSON.parse(request.responseText)
            const entries = payload.entries || []
            if (append && root.folderEntries.length + entries.length > root.folderEntriesMax) {
                root.statusText = qsTr("Folder view limited to %1 entries; refine filters to load more.").arg(root.folderEntriesMax)
                return
            }
            root.folderEntries = append ? root.folderEntries.concat(entries) : entries
            if (payload.totalMatched !== undefined)
                root.folderTotalEntries = Math.max(Number(payload.totalMatched), root.folderEntries.length)
            root.visibleFolderEntries = root.folderEntries
            rebuildDiffRows()
            rebuildSearchRows()
        }
        request.open("GET", url)
        request.send()
    }

    // Prefetch the next folder page when the table nears the bottom of the
    // loaded entries (windowed folders only).
    function maybeLoadMoreFolderRows(view) {
        if (root.folderTotalEntries <= 0 || root.folderWindowLoading || !view)
            return
        if (root.folderEntries.length >= root.folderTotalEntries)
            return
        if (root.folderEntries.length >= root.folderEntriesMax)
            return
        if (view.contentHeight - (view.contentY + view.height) < view.height)
            root.queryFolderPage(root.folderEntries.length, true)
    }

    function copyToClipboard(text) {
        root.bridgeGet("/copy-clipboard?text=" + encodeURIComponent(text))
    }

    // Holds the fetched report so the user can review it before it leaves the
    // app (copied to the clipboard) — preview-before-export.
    property string exportPreviewContent: ""
    property string exportPreviewFormat: "unified"

    function exportReport() {
        if (!root.bridgeUrl || root.leftPath === "")
            return
        root.fetchExportPreview(root.exportPreviewFormat)
        exportPreviewDialog.open()
    }

    function fetchExportPreview(format) {
        root.exportPreviewFormat = format
        root.exportPreviewContent = qsTr("Generating preview…")
        var req = new XMLHttpRequest()
        req.onreadystatechange = function () {
            if (req.readyState !== XMLHttpRequest.DONE)
                return
            if (req.status === 200) {
                var payload = JSON.parse(req.responseText)
                root.exportPreviewContent = (payload && payload.content)
                    ? payload.content
                    : qsTr("(empty report)")
            } else {
                root.exportPreviewContent = qsTr("Failed to generate report (status %1)").arg(req.status)
            }
        }
        req.open("GET", root.bridgeUrl + "/report?format=" + encodeURIComponent(format))
        req.send()
    }

    function browseSide(side) {
        root.pendingBrowseSide = side
        if (root.compareMode === "Folder") {
            folderDialog.open()
        } else {
            fileDialog.open()
        }
    }

    function setBrowsedPath(path) {
        if (root.pendingBrowseSide === "left")
            root.leftPath = path
        else if (root.pendingBrowseSide === "base")
            root.basePath = path
        else
            root.rightPath = path
        updateActiveTabSnapshot()
    }

    function urlToLocalPath(url) {
        let value = url.toString()
        if (value.startsWith("file://"))
            value = value.substring(7)
        return decodeURIComponent(value)
    }

    // Derive a human project name from a file path: basename without the
    // .linsync-project extension.
    function projectNameFromPath(path) {
        const parts = String(path).split("/")
        let base = parts[parts.length - 1] || String(path)
        return base.replace(/\.linsync-project$/, "") || "Untitled project"
    }

    function copyCurrentDifference(direction) {
        if (root.currentDiffRow < 0)
            return

        if (root.bridgeUrl !== "") {
            root.statusText = "Applying copy"
            const request = new XMLHttpRequest()
            const url = root.bridgeUrl + "/copy?row=" + encodeURIComponent(root.currentDiffRow) + "&direction=" + encodeURIComponent(direction)
            request.onreadystatechange = function () {
                if (request.readyState === XMLHttpRequest.DONE) {
                    if (request.status === 200) {
                        applyLaunchContext(JSON.parse(request.responseText), false)
                    } else {
                        root.statusText = "Copy failed"
                    }
                }
            }
            request.open("GET", url)
            request.send()
            return
        }

        const row = root.currentDiffRow
        const leftSource = root.leftRows[row] || { "number": null, "text": "", "state": "empty" }
        const rightSource = root.rightRows[row] || { "number": null, "text": "", "state": "empty" }
        let nextLeftDirty = root.leftDirty
        let nextRightDirty = root.rightDirty

        if (direction === "left_to_right") {
            const rows = root.rightRows.slice()
            rows[row] = {
                "row_id": rightSource.row_id || leftSource.row_id || "",
                "number": rightSource.number || leftSource.number,
                "text": leftSource.text || "",
                "state": "equal"
            }
            root.rightRows = rows
            nextRightDirty = true
            root.statusText = "Copied left to right"
        } else {
            const rows = root.leftRows.slice()
            rows[row] = {
                "row_id": leftSource.row_id || rightSource.row_id || "",
                "number": leftSource.number || rightSource.number,
                "text": rightSource.text || "",
                "state": "equal"
            }
            root.leftRows = rows
            nextLeftDirty = true
            root.statusText = "Copied right to left"
        }

        root.leftDirty = nextLeftDirty
        root.rightDirty = nextRightDirty
        normalizeCurrentRow()
        rebuildDiffRows()
        rebuildSearchRows()
        refreshDifferenceCountFromRows()
        updateActiveTabSnapshot()
    }

    function copyAllDifferences(direction) {
        if (root.diffRowIndexes.length === 0)
            return

        if (root.bridgeUrl !== "") {
            root.statusText = "Applying copy"
            const request = new XMLHttpRequest()
            const url = root.bridgeUrl + "/copy-all?direction=" + encodeURIComponent(direction)
            request.onreadystatechange = function () {
                if (request.readyState === XMLHttpRequest.DONE) {
                    if (request.status === 200) {
                        applyLaunchContext(JSON.parse(request.responseText), false)
                    } else {
                        root.statusText = "Copy all failed"
                    }
                }
            }
            request.open("GET", url)
            request.send()
            return
        }

        while (root.diffRowIndexes.length > 0) {
            root.currentDiffRow = root.diffRowIndexes[root.diffRowIndexes.length - 1]
            root.copyCurrentDifference(direction)
        }
    }

    function normalizeCurrentRow() {
        if (root.currentDiffRow < 0)
            return

        const row = root.currentDiffRow
        if (root.leftRows[row] && root.rightRows[row] && root.leftRows[row].text === root.rightRows[row].text) {
            const left = root.leftRows.slice()
            const right = root.rightRows.slice()
            left[row] = {
                "row_id": left[row].row_id || right[row].row_id || "",
                "number": left[row].number,
                "text": left[row].text,
                "state": "equal"
            }
            right[row] = {
                "row_id": right[row].row_id || left[row].row_id || "",
                "number": right[row].number,
                "text": right[row].text,
                "state": "equal"
            }
            root.leftRows = left
            root.rightRows = right
        }
    }

    Component.onCompleted: {
        loadLaunchArguments()
        loadUiSettings()
    }

    // On quit: cancel any in-flight compare so the bridge worker thread
    // stops promptly instead of running to completion after the window is
    // gone (the process waits on the bridge server thread to finish).
    Component.onDestruction: cancelActiveCompare()


    Shortcut {
        sequences: ["F7"]
        enabled: root.diffRowIndexes.length > 0
        onActivated: root.previousDifference()
    }

    Shortcut {
        sequences: ["F8"]
        enabled: root.diffRowIndexes.length > 0
        onActivated: root.nextDifference()
    }

    Shortcut {
        sequences: ["Ctrl+Home"]
        enabled: root.diffRowIndexes.length > 0
        onActivated: root.firstDifference()
    }

    Shortcut {
        sequences: ["Ctrl+End"]
        enabled: root.diffRowIndexes.length > 0
        onActivated: root.lastDifference()
    }

    Shortcut {
        sequences: ["Ctrl+F"]
        onActivated: root.openFind()
    }

    Shortcut {
        sequences: ["Ctrl+Z"]
        enabled: root.canUndo
        onActivated: root.undoLastMergeAction()
    }

    Shortcut {
        sequences: ["Ctrl+Shift+Z", "Ctrl+Y"]
        enabled: root.canRedo
        onActivated: root.redoLastMergeAction()
    }

    Shortcut {
        sequences: ["F3"]
        enabled: root.searchRowIndexes.length > 0
        onActivated: root.nextSearchResult()
    }

    Shortcut {
        sequences: ["Shift+F3"]
        enabled: root.searchRowIndexes.length > 0
        onActivated: root.previousSearchResult()
    }

    Dialogs.FileDialog {
        id: fileDialog

        title: root.pendingBrowseSide === "left" ? "Select left file" : "Select right file"
        onAccepted: root.setBrowsedPath(root.urlToLocalPath(selectedFile))
    }

    Dialogs.FolderDialog {
        id: folderDialog

        title: root.pendingBrowseSide === "left" ? "Select left folder" : "Select right folder"
        onAccepted: root.setBrowsedPath(root.urlToLocalPath(selectedFolder))
    }

    Dialogs.FolderDialog {
        id: pluginInstallDialog

        title: qsTr("Select a plugin directory to install")
        onAccepted: {
            const localPath = root.urlToLocalPath(selectedFolder)
            root.bridgeGet("/plugins/install?path=" + encodeURIComponent(localPath),
                function (ok, payload, status) {
                    if (ok && payload && payload.id) {
                        pluginsPage.showActionResult(qsTr("Installed %1").arg(payload.id))
                        root.loadPlugins(function (p) { pluginsPage.applyDiscovery(p) })
                    } else if (status === 409) {
                        pluginsPage.showActionResult(qsTr("That plugin is already installed"))
                    } else if (status === 400) {
                        pluginsPage.showActionResult(qsTr("Not a valid plugin directory"))
                    } else {
                        pluginsPage.showActionResult(qsTr("Plugin install failed"))
                    }
                })
        }
    }

    // Preview-before-export: review the generated report (and pick a format)
    // before it is copied to the clipboard, so privacy-sensitive content is
    // never exported without a look.
    Controls.Dialog {
        id: exportPreviewDialog
        modal: true
        title: qsTr("Export report — preview")
        anchors.centerIn: Overlay.overlay
        width: Math.min(root.width - 80, 900)
        height: Math.min(root.height - 120, 640)
        standardButtons: Controls.Dialog.Close

        ColumnLayout {
            anchors.fill: parent
            spacing: 8

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Controls.Label {
                    text: qsTr("Format:")
                    color: root.activeText
                }
                Controls.ComboBox {
                    id: exportFormatCombo
                    model: ["unified", "json", "summary"]
                    Accessible.name: qsTr("Report format")
                    currentIndex: Math.max(0, model.indexOf(root.exportPreviewFormat))
                    onActivated: root.fetchExportPreview(currentText)
                }
                Item { Layout.fillWidth: true }
                Controls.Button {
                    text: qsTr("Copy to clipboard")
                    icon.name: "edit-copy"
                    onClicked: {
                        root.copyToClipboard(root.exportPreviewContent)
                        root.statusText = qsTr("Report copied to clipboard")
                        exportPreviewDialog.close()
                    }
                }
            }

            Controls.ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                Controls.TextArea {
                    id: exportPreviewArea
                    readOnly: true
                    wrapMode: TextEdit.NoWrap
                    font.family: "monospace"
                    font.pixelSize: 12
                    color: root.activeText
                    text: root.exportPreviewContent
                    background: Rectangle {
                        color: root.activeBg
                        border.color: root.separatorColor
                        border.width: 1
                    }
                }
            }
        }
    }

    Dialogs.FileDialog {
        id: projectSaveDialog
        title: qsTr("Save project as…")
        fileMode: Dialogs.FileDialog.SaveFile
        nameFilters: ["LinSync project (*.linsync-project)", "All files (*)"]
        onAccepted: {
            const localPath = root.urlToLocalPath(selectedFile)
            root.bridgeGet("/project/save?path=" + encodeURIComponent(localPath) + "&name=" + encodeURIComponent(root.projectNameFromPath(localPath)),
                function (ok, payload) {
                    if (ok && payload && payload.ok) {
                        sessionsPage.projectStatus = qsTr("Saved %1 comparison(s)").arg(payload.sessions)
                        root.loadRecentProjects()
                    } else {
                        sessionsPage.projectStatus = qsTr("Project save failed")
                    }
                })
        }
    }

    Dialogs.FileDialog {
        id: projectOpenDialog
        title: qsTr("Open project")
        nameFilters: ["LinSync project (*.linsync-project)", "All files (*)"]
        onAccepted: root.openProjectFile(root.urlToLocalPath(selectedFile))
    }

    // Open a project file by path (shared by the file dialog and the recent
    // workspaces list).
    function openProjectFile(localPath) {
        root.bridgeGet("/project/open?path=" + encodeURIComponent(localPath),
            function (ok, payload, status) {
                if (ok && payload && payload.session) {
                    root.applySessionContextJson(JSON.stringify(payload))
                    root.activeSection = 0
                    sessionsPage.projectStatus = qsTr("Opened %1").arg(payload.name || "project")
                    root.loadRecentProjects()
                } else {
                    sessionsPage.projectStatus = (status === 400 || status === 404)
                        ? qsTr("Not a valid project file")
                        : qsTr("Project open failed")
                }
            })
    }

    function loadRecentProjects() {
        root.bridgeGet("/project/recent", function (ok, payload) {
            sessionsPage.recentProjects = (ok && payload && payload.projects) ? payload.projects : []
        })
    }

    Dialogs.FileDialog {
        id: migrateFileDialog

        title: qsTr("Select legacy .flt file to migrate")
        nameFilters: ["Filter files (*.flt)", "All files (*)"]
        onAccepted: {
            const localPath = root.urlToLocalPath(selectedFile)
            root.bridgeGet(
                "/filters/migrate?path=" + encodeURIComponent(localPath),
                function (ok, payload) {
                    if (ok && payload)
                        filtersPage.migrateResult = payload
                    else
                        filtersPage.migrateResult = { ok: false, error: "bridge request failed" }
                }
            )
        }
    }

    // Tab for the unified New Compare dialog — "files" or "paste".
    property string newCompareTab: "files"

    // Three-way merge open flow: base → left → right, then switch to MergePage.
    QtObject {
        id: openMergeDialog
        property string stage: "idle" // "base" | "left" | "right"
        function startFlow() {
            stage = "base"
            mergeBaseFilePicker.open()
        }
    }

    Dialogs.FileDialog {
        id: mergeBaseFilePicker
        title: qsTr("Three-way merge: select BASE file")
        onAccepted: {
            root.mergeBasePath = root.urlToLocalPath(selectedFile)
            openMergeDialog.stage = "left"
            mergeLeftFilePicker.open()
        }
        onRejected: openMergeDialog.stage = "idle"
    }

    Dialogs.FileDialog {
        id: mergeLeftFilePicker
        title: qsTr("Three-way merge: select LEFT file")
        onAccepted: {
            root.mergeLeftPath = root.urlToLocalPath(selectedFile)
            openMergeDialog.stage = "right"
            mergeRightFilePicker.open()
        }
        onRejected: openMergeDialog.stage = "idle"
    }

    Dialogs.FileDialog {
        id: mergeRightFilePicker
        title: qsTr("Three-way merge: select RIGHT file")
        onAccepted: {
            root.mergeRightPath = root.urlToLocalPath(selectedFile)
            openMergeDialog.stage = "idle"
            mergePage.compareOnly = false
            // Switch to the merge page (index 8) and start the session.
            root.activeSection = 8
            mergePage.start()
        }
        onRejected: openMergeDialog.stage = "idle"
    }

    Dialogs.FileDialog {
        id: openLeftFileDialog
        title: qsTr("Compare: select LEFT file")
        onAccepted: {
            root.leftPath = root.urlToLocalPath(selectedFile)
        }
    }

    Dialogs.FileDialog {
        id: openRightFileDialog
        title: qsTr("Compare: select RIGHT file")
        onAccepted: {
            root.rightPath = root.urlToLocalPath(selectedFile)
        }
    }

    Dialogs.FolderDialog {
        id: openLeftFolderDialog
        title: qsTr("Compare: select LEFT folder")
        onAccepted: {
            root.leftPath = root.urlToLocalPath(selectedFolder)
        }
    }

    Dialogs.FolderDialog {
        id: openRightFolderDialog
        title: qsTr("Compare: select RIGHT folder")
        onAccepted: {
            root.rightPath = root.urlToLocalPath(selectedFolder)
        }
    }

    // ── New Compare dialog: unified file-pick + raw-text paste ────────────
    Controls.Dialog {
        id: newCompareDialog

        modal: true
        title: qsTr("New Compare")
        width: Math.min(root.width - 48, 680)
        height: Math.min(root.height - 80, 520)
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

        property string newCompareMode: "Text"
        property string leftTextName: "Left"
        property string rightTextName: "Right"

        contentItem: ColumnLayout {
            spacing: 8

            Controls.TabBar {
                id: newCompareDialogTabBar
                Layout.fillWidth: true

                Controls.TabButton {
                    text: qsTr("From files")
                    checked: true
                    onClicked: root.newCompareTab = "files"
                }
                Controls.TabButton {
                    text: qsTr("Paste text")
                    onClicked: root.newCompareTab = "paste"
                }
            }

            // ── Files tab ────────────────────────────────────────────────
            ColumnLayout {
                visible: root.newCompareTab === "files"
                Layout.fillWidth: true
                spacing: 12

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    AppTextField {
                        Layout.fillWidth: true
                        text: root.leftPath
                        placeholderText: qsTr("Left file or folder")
                        Accessible.name: "Compare left path"
                        color: root.activeText
                        placeholderTextColor: root.activeDisabledText
                        background: Rectangle {
                            color: root.activeBg
                            border.color: root.separatorColor
                            border.width: 1
                            radius: 4
                        }
                        onEditingFinished: root.leftPath = text
                    }

                    Controls.ToolButton {
                        icon.name: "document-open-folder"
                        icon.color: root.activeText
                        Controls.ToolTip.text: qsTr("Browse left")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: "Browse left"
                        onClicked: {
                            if (newCompareDialog.newCompareMode === "Folder")
                                openLeftFolderDialog.open()
                            else
                                openLeftFileDialog.open()
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    AppTextField {
                        Layout.fillWidth: true
                        text: root.rightPath
                        placeholderText: qsTr("Right file or folder")
                        Accessible.name: "Compare right path"
                        color: root.activeText
                        placeholderTextColor: root.activeDisabledText
                        background: Rectangle {
                            color: root.activeBg
                            border.color: root.separatorColor
                            border.width: 1
                            radius: 4
                        }
                        onEditingFinished: root.rightPath = text
                    }

                    Controls.ToolButton {
                        icon.name: "document-open-folder"
                        icon.color: root.activeText
                        Controls.ToolTip.text: qsTr("Browse right")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: "Browse right"
                        onClicked: {
                            if (newCompareDialog.newCompareMode === "Folder")
                                openRightFolderDialog.open()
                            else
                                openRightFileDialog.open()
                        }
                    }
                }

                RowLayout {
                    spacing: 8

                    Controls.Label {
                        text: qsTr("Mode:")
                        color: root.activeText
                    }

                    AppComboBox {
                        id: newCompareModeCombo
                        model: ["Text", "Folder", "Table", "Hex", "Image", "Document", "Webpage", "Archive", "Three-way"]
                        Accessible.name: "Compare mode"
                        implicitWidth: 140
                        implicitHeight: 30
                        palette.button: root.activeBgAlt
                        palette.buttonText: root.activeText
                        palette.text: root.activeText
                        palette.base: root.activeBg
                        contentItem: Controls.Label {
                            text: newCompareModeCombo.displayText
                            leftPadding: 8
                            rightPadding: 28
                            verticalAlignment: Text.AlignVCenter
                            color: root.activeText
                        }
                        background: Rectangle {
                            color: root.activeBg
                            border.color: root.separatorColor
                            border.width: 1
                            radius: 4
                        }
                        onActivated: newCompareDialog.newCompareMode = currentText
                    }

                    Item { Layout.fillWidth: true }

                    Controls.Button {
                        enabled: root.leftPath !== "" && root.rightPath !== ""
                        icon.name: "media-playback-start"
                        icon.color: root.activeText
                        text: qsTr("Compare")
                        onClicked: {
                            root.compareMode = newCompareDialog.newCompareMode
                            root.requestCompare(false)
                            newCompareDialog.close()
                        }
                    }

                    Controls.Button {
                        enabled: root.leftPath !== "" && root.rightPath !== ""
                        icon.name: "tab-new"
                        icon.color: root.activeText
                        text: qsTr("Compare in new tab")
                        onClicked: {
                            root.compareMode = newCompareDialog.newCompareMode
                            root.requestCompare(true)
                            newCompareDialog.close()
                        }
                    }
                }
            }

            // ── Paste text tab ─────────────────────────────────────────────
            ColumnLayout {
                visible: root.newCompareTab === "paste"
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 8

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: 4

                        Controls.Label {
                            text: qsTr("Left text ")
                            color: root.activeNeutralText
                            font.bold: true
                        }

                        AppTextField {
                            Layout.fillWidth: true
                            implicitHeight: 28
                            text: newCompareDialog.leftTextName
                            placeholderText: qsTr("Label")

                            Accessible.name: "Left text name"
                            color: root.activeText
                            placeholderTextColor: root.activeDisabledText
                            background: Rectangle {
                                color: root.activeBg
                                border.color: root.separatorColor
                                border.width: 1
                                radius: 4
                            }
                            onTextChanged: newCompareDialog.leftTextName = text
                        }

                        Controls.ScrollView {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true

                            Controls.TextArea {
                                id: rawLeftTextArea
                                placeholderText: qsTr("Paste left content here…")
                                font.family: "monospace"
                                font.pixelSize: 11
                                wrapMode: Controls.TextArea.Wrap
    
                                background: Rectangle {
                                    color: root.activeBg
                                    border.color: root.separatorColor
                                    border.width: 1
                                    radius: 4
                                }
                            }
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: 4

                        Controls.Label {
                            text: qsTr("Right text")
                            color: root.activePositiveText
                            font.bold: true
                        }

                        AppTextField {
                            Layout.fillWidth: true
                            implicitHeight: 28
                            text: newCompareDialog.rightTextName
                            placeholderText: qsTr("Label")

                            Accessible.name: "Right text name"
                            color: root.activeText
                            placeholderTextColor: root.activeDisabledText
                            background: Rectangle {
                                color: root.activeBg
                                border.color: root.separatorColor
                                border.width: 1
                                radius: 4
                            }
                            onTextChanged: newCompareDialog.rightTextName = text
                        }

                        Controls.ScrollView {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true

                            Controls.TextArea {
                                id: rawRightTextArea
                                placeholderText: qsTr("Paste right content here…")
                                font.family: "monospace"
                                font.pixelSize: 11
                                wrapMode: Controls.TextArea.Wrap
    
                                background: Rectangle {
                                    color: root.activeBg
                                    border.color: root.separatorColor
                                    border.width: 1
                                    radius: 4
                                }
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    Controls.CheckBox {
                        id: rawCompareIgnoreCase
                        text: qsTr("Ignore case")
                        checked: root.ignoreCase
                        palette.windowText: root.activeText
                    }

                    Controls.CheckBox {
                        id: rawCompareIgnoreWhitespace
                        text: qsTr("Ignore whitespace")
                        checked: root.ignoreWhitespace
                        palette.windowText: root.activeText
                    }

                    Controls.CheckBox {
                        id: rawCompareIgnoreBlankLines
                        text: qsTr("Ignore blank lines")
                        checked: root.ignoreBlankLines
                        palette.windowText: root.activeText
                    }

                    Item { Layout.fillWidth: true }

                    Controls.Button {
                        id: rawCompareButton
                        enabled: rawLeftTextArea.text !== "" && rawRightTextArea.text !== ""
                        icon.name: "media-playback-start"
                        icon.color: root.activeText
                        text: qsTr("Compare")
                        onClicked: {
                            var params = "left_text=" + encodeURIComponent(rawLeftTextArea.text)
                                + "&right_text=" + encodeURIComponent(rawRightTextArea.text)
                                + "&left_name=" + encodeURIComponent(newCompareDialog.leftTextName)
                                + "&right_name=" + encodeURIComponent(newCompareDialog.rightTextName)
                            if (rawCompareIgnoreCase.checked)
                                params += "&ignore_case=1"
                            if (rawCompareIgnoreWhitespace.checked)
                                params += "&ignore_whitespace=1"
                            if (rawCompareIgnoreBlankLines.checked)
                                params += "&ignore_blank_lines=1"

                            root.bridgeGet("/raw-compare?" + params, function (ok, payload) {
                                if (ok && payload) {
                                    root.applyLaunchContext(payload, false)
                                    root.activeSection = 0
                                    newCompareDialog.close()
                                } else {
                                    root.statusText = "Raw compare failed"
                                }
                            })
                        }
                    }

                    Controls.Button {
                        enabled: rawLeftTextArea.text !== "" && rawRightTextArea.text !== ""
                        icon.name: "tab-new"
                        icon.color: root.activeText
                        text: qsTr("Compare in new tab")
                        onClicked: {
                            var params = "left_text=" + encodeURIComponent(rawLeftTextArea.text)
                                + "&right_text=" + encodeURIComponent(rawRightTextArea.text)
                                + "&left_name=" + encodeURIComponent(newCompareDialog.leftTextName)
                                + "&right_name=" + encodeURIComponent(newCompareDialog.rightTextName)
                                + "&new_tab=1"
                            if (rawCompareIgnoreCase.checked)
                                params += "&ignore_case=1"
                            if (rawCompareIgnoreWhitespace.checked)
                                params += "&ignore_whitespace=1"
                            if (rawCompareIgnoreBlankLines.checked)
                                params += "&ignore_blank_lines=1"

                            root.bridgeGet("/raw-compare?" + params, function (ok, payload) {
                                if (ok && payload) {
                                    root.applyLaunchContext(payload, false)
                                    root.activeSection = 0
                                    newCompareDialog.close()
                                } else {
                                    root.statusText = "Raw compare failed"
                                }
                            })
                        }
                    }
                }
            }
        }
    }

    Controls.Dialog {
        id: reloadDirtyDialog
        modal: true
        title: qsTr("Discard unsaved edits?")
        width: Math.min(root.width - 48, 420)
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

        contentItem: Controls.Label {
            text: qsTr("Reloading from disk will lose unsaved edits in the current tab.")
            wrapMode: Text.WordWrap
        }

        footer: RowLayout {
            spacing: Kirigami.Units.smallSpacing
            Controls.Button {
                icon.name: "document-save"
                icon.color: root.activeText
                text: qsTr("Save then reload")
                onClicked: {
                    reloadDirtyDialog.close()
                    root.saveDirtySides()
                    root.requestCompare(false)
                }
            }
            Controls.Button {
                icon.name: "view-refresh"
                icon.color: root.activeText
                text: qsTr("Discard and reload")
                onClicked: {
                    reloadDirtyDialog.close()
                    root.requestCompare(false)
                }
            }
            Item { Layout.fillWidth: true }
            Controls.Button {
                icon.name: "dialog-cancel"
                icon.color: root.activeText
                text: qsTr("Cancel")
                onClicked: reloadDirtyDialog.close()
            }
        }
    }

    Controls.Dialog {
        id: folderOpDialog
        property string summary: ""
        property string details: ""
        property bool permanentDelete: false
        property string permanentWarning: ""
        modal: true
        title: qsTr("Run folder operation?")
        // Permanent deletes must be re-confirmed for every plan: never
        // carry a checked box over from a previous dialog invocation.
        onOpened: permanentConfirmCheck.checked = false
        width: Math.min(root.width - 48, 520)
        height: Math.min(root.height - 80, 460)
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

        contentItem: ColumnLayout {
            spacing: 8
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Kind: %1").arg(root.pendingFolderOpKind)
                font.bold: true
            }
            Controls.Label {
                Layout.fillWidth: true
                text: folderOpDialog.summary
                opacity: 0.75
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                visible: folderOpDialog.permanentDelete
                Kirigami.Icon {
                    source: "data-warning"
                    color: root.activeNegativeText
                    Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                    Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: folderOpDialog.permanentWarning
                    color: root.activeNegativeText
                    wrapMode: Text.WordWrap
                    Accessible.name: qsTr("Permanent delete warning")
                }
            }
            AppCheckBox {
                id: permanentConfirmCheck
                Layout.fillWidth: true
                visible: folderOpDialog.permanentDelete
                text: qsTr("Permanently delete — this cannot be undone")
                Accessible.name: qsTr("Confirm permanent delete")
            }
            Controls.ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Controls.TextArea {
                    text: folderOpDialog.details
                    readOnly: true
                    font.family: "monospace"
                    font.pixelSize: 10
                    wrapMode: Controls.TextArea.Wrap
                }
            }
        }

        footer: RowLayout {
            spacing: Kirigami.Units.smallSpacing
            Controls.Button {
                icon.name: "dialog-ok"
                icon.color: root.activeText
                text: qsTr("Apply")
                enabled: !folderOpDialog.permanentDelete || permanentConfirmCheck.checked
                Accessible.name: qsTr("Apply folder operation")
                onClicked: {
                    const opts = (folderOpDialog.permanentDelete && permanentConfirmCheck.checked)
                        ? { confirm_permanent: true } : {}
                    folderOpDialog.close()
                    root.executeFolderOp(root.pendingFolderOpKind, root.pendingFolderOpEntries, opts, function (ok, payload) {
                        if (ok && payload) {
                            const summary = payload.summary || {}
                            root.statusText = qsTr("Folder op done: %1 succeeded / %2 failed of %3")
                                .arg(summary.succeeded || 0)
                                .arg(summary.failed || 0)
                                .arg(summary.total || 0)
                            root.requestCompare(false)
                        } else {
                            // Surface the bridge's reason (e.g. the 409
                            // permanent-delete confirmation message).
                            root.statusText = payload && payload.error
                                ? payload.error : "Folder op execute failed"
                        }
                    })
                }
            }
            Item { Layout.fillWidth: true }
            Controls.Button {
                icon.name: "dialog-cancel"
                icon.color: root.activeText
                text: qsTr("Cancel")
                onClicked: folderOpDialog.close()
            }
        }
    }

    Controls.Dialog {
        id: closeDirtyDialog

        modal: true
        title: "Unsaved Changes"
        width: Math.min(root.width - 48, 420)
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside
        onRejected: root.pendingCloseTabId = 0

        contentItem: Controls.Label {
            text: "Save changes before closing this tab?"
            wrapMode: Text.WordWrap
        }

        footer: RowLayout {
            spacing: Kirigami.Units.smallSpacing

            Controls.Button {
                icon.name: "document-save"
                icon.color: root.activeText
                text: "Save"
                enabled: root.bridgeUrl !== ""
                onClicked: {
                    closeDirtyDialog.close()
                    root.saveDirtySidesThenClose()
                }
            }

            Controls.Button {
                icon.name: "edit-delete"
                icon.color: root.activeText
                text: "Discard"
                onClicked: {
                    closeDirtyDialog.close()
                    root.discardDirtyTabAndClose()
                }
            }

            Item {
                Layout.fillWidth: true
            }

            Controls.Button {
                icon.name: "dialog-cancel"
                icon.color: root.activeText
                text: "Cancel"
                onClicked: {
                    root.pendingCloseTabId = 0
                    closeDirtyDialog.close()
                }
            }
        }
    }

    Controls.Dialog {
        id: pluginRemoveDialog
        modal: true
        title: qsTr("Remove plugin?")
        width: Math.min(root.width - 48, 400)
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

        contentItem: Controls.Label {
            text: qsTr("Uninstall \"%1\" from the user plugins folder? This cannot be undone.").arg(root.pendingRemovePluginName)
            wrapMode: Text.WordWrap
        }

        footer: RowLayout {
            spacing: Kirigami.Units.smallSpacing
            Controls.Button {
                icon.name: "edit-delete-remove"
                icon.color: root.activeText
                text: qsTr("Remove")
                onClicked: {
                    const id = root.pendingRemovePluginId
                    const name = root.pendingRemovePluginName
                    pluginRemoveDialog.close()
                    root.bridgeGet("/plugins/remove?id=" + encodeURIComponent(id),
                        function (ok, payload, status) {
                            if (ok) {
                                pluginsPage.showActionResult(qsTr("Removed %1").arg(name))
                                root.loadPlugins(function (p) { pluginsPage.applyDiscovery(p) })
                            } else if (status === 404) {
                                pluginsPage.showActionResult(qsTr("%1 is not installed").arg(name))
                            } else {
                                pluginsPage.showActionResult(qsTr("Could not remove %1").arg(name))
                            }
                        })
                }
            }
            Item { Layout.fillWidth: true }
            Controls.Button {
                icon.name: "dialog-cancel"
                icon.color: root.activeText
                text: qsTr("Cancel")
                onClicked: pluginRemoveDialog.close()
            }
        }
    }

    Controls.Dialog {
        id: editDiscardDialog
        modal: true
        title: qsTr("Discard unsaved edits?")
        width: Math.min(root.width - 48, 420)
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        closePolicy: Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

        contentItem: Controls.Label {
            text: qsTr("Exiting edit mode will discard your unsaved changes.")
            wrapMode: Text.WordWrap
        }

        footer: RowLayout {
            spacing: Kirigami.Units.smallSpacing
            Controls.Button {
                icon.name: "document-save"
                icon.color: root.activeText
                text: qsTr("Save then exit")
                onClicked: {
                    const side = root.pendingEditToggleSide
                    editDiscardDialog.close()
                    root.saveEdit(side)
                }
            }
            Controls.Button {
                icon.name: "edit-delete"
                icon.color: root.activeText
                text: qsTr("Discard")
                onClicked: {
                    const side = root.pendingEditToggleSide
                    editDiscardDialog.close()
                    if (side === "left") {
                        root.editLeftMode = false
                        root.editLeftDirtyText = ""
                    } else {
                        root.editRightMode = false
                        root.editRightDirtyText = ""
                    }
                }
            }
            Item { Layout.fillWidth: true }
            Controls.Button {
                icon.name: "dialog-cancel"
                icon.color: root.activeText
                text: qsTr("Cancel")
                onClicked: editDiscardDialog.close()
            }
        }
    }

    globalDrawer: Kirigami.OverlayDrawer {
        id: drawer
        edge: Qt.LeftEdge
        modal: false
        drawerOpen: true
        handleVisible: false

        // Kirigami.OverlayDrawer creates its own theme island, so the
        // ApplicationWindow's theme bindings don't propagate into it on
        // their own. Re-bind explicitly so the sidebar (and any nav items
        // inside) tracks the user's chosen LinSync theme — without this,
        // the drawer falls back to system Kirigami (Breeze) colors and the
        // labels are illegible in Light mode.
        Kirigami.Theme.inherit: false
        Kirigami.Theme.colorSet: Kirigami.Theme.Window
        Kirigami.Theme.backgroundColor:          root.activeBg
        Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
        Kirigami.Theme.textColor:                root.activeText
        Kirigami.Theme.disabledTextColor:        root.activeDisabledText
        Kirigami.Theme.highlightColor:           root.activeHighlight
        Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
        Kirigami.Theme.positiveTextColor:        root.activePositiveText
        Kirigami.Theme.negativeTextColor:        root.activeNegativeText
        Kirigami.Theme.neutralTextColor:         root.activeNeutralText

        property bool sidebarCollapsed: false
        readonly property int expandedWidth: Kirigami.Units.gridUnit * 13
        readonly property int collapsedWidth: 60

        width: sidebarCollapsed ? collapsedWidth : expandedWidth
        Behavior on width { NumberAnimation { duration: root.reduceMotion ? 0 : 160; easing.type: Easing.OutCubic } }

        readonly property color sidebarBg: Kirigami.ColorUtils.tintWithAlpha(
            root.activeBg, root.activeText, 0.035)
        readonly property color sectionLabelColor: root.activeText
        readonly property color footerTextColor: root.activeText

        background: Rectangle {
            color: drawer.sidebarBg
            Rectangle {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: 1
                color: root.separatorColor
            }
        }

        contentItem: ColumnLayout {
            spacing: 0

            // OverlayDrawer's contentItem doesn't inherit the drawer's
            // Kirigami.Theme on its own — re-bind so all the sidebar
            // children (LinSyncNavItem labels, section headers, etc.)
            // pick up the LinSync palette.
            Kirigami.Theme.inherit: false
            Kirigami.Theme.colorSet: Kirigami.Theme.Window
            Kirigami.Theme.backgroundColor:          root.activeBg
            Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
            Kirigami.Theme.textColor:                root.activeText
            Kirigami.Theme.disabledTextColor:        root.activeDisabledText
            Kirigami.Theme.highlightColor:           root.activeHighlight
            Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
            Kirigami.Theme.positiveTextColor:        root.activePositiveText
            Kirigami.Theme.negativeTextColor:        root.activeNegativeText
            Kirigami.Theme.neutralTextColor:         root.activeNeutralText

            // Toggle + brand
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 14
                Layout.leftMargin: drawer.sidebarCollapsed ? 0 : 16
                Layout.rightMargin: drawer.sidebarCollapsed ? 0 : 16
                Layout.bottomMargin: 14
                spacing: drawer.sidebarCollapsed ? 0 : 12

                Item {
                    visible: drawer.sidebarCollapsed
                    Layout.fillWidth: true
                }

                Controls.ToolButton {
                    icon.name: "application-menu"
                    icon.color: root.activeText
                    Controls.ToolTip.text: drawer.sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.delay: 400
                    Accessible.name: drawer.sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"
                    display: Controls.AbstractButton.IconOnly
                    onClicked: drawer.sidebarCollapsed = !drawer.sidebarCollapsed
                }

                Rectangle {
                    visible: !drawer.sidebarCollapsed
                    Layout.preferredWidth: 40
                    Layout.preferredHeight: 40
                    Layout.alignment: Qt.AlignVCenter
                    radius: 8
                    color: "transparent"

                    Image {
                        anchors.fill: parent
                        source: root.appIconSource
                        sourceSize.width: 80
                        sourceSize.height: 80
                        fillMode: Image.PreserveAspectFit
                        smooth: true
                        mipmap: true
                    }
                }

                ColumnLayout {
                    visible: !drawer.sidebarCollapsed
                    Layout.fillWidth: true
                    spacing: 1
                    Controls.Label {
                        text: "LinSync"
                        font.pixelSize: 17
                        font.weight: Font.Bold
                        font.letterSpacing: 0
                        color: root.activeText
                    }
                    Controls.Label {
                        text: qsTr("File compare & merge")
                        font.pixelSize: 11
                        opacity: 0.55
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                        color: root.activeText
                    }
                }

                Item {
                    visible: drawer.sidebarCollapsed
                    Layout.fillWidth: true
                }
            }

            // WORKSPACE
            Controls.Label {
                visible: !drawer.sidebarCollapsed
                Layout.fillWidth: true
                Layout.leftMargin: 18
                Layout.rightMargin: 18
                Layout.topMargin: 8
                Layout.bottomMargin: 6
                text: qsTr("WORKSPACE")
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.letterSpacing: 1.6
                color: drawer.sectionLabelColor
                opacity: 0.5
            }
            Item {
                visible: drawer.sidebarCollapsed
                Layout.preferredHeight: 10
            }
            LinSyncNavItem {
                label: qsTr("Compare")
                iconName: "view-split-left-right"
                active: root.activeSection === 0
                collapsed: drawer.sidebarCollapsed
                reduceMotion: root.reduceMotion
                onTriggered: root.activeSection = 0
            }
            LinSyncNavItem {
                label: qsTr("Image Compare")
                iconName: "image-compare"
                active: root.activeSection === 9
                collapsed: drawer.sidebarCollapsed
                reduceMotion: root.reduceMotion
                onTriggered: root.activeSection = 9
            }
            LinSyncNavItem {
                label: qsTr("Webpage Compare")
                iconName: "internet-web-browser-symbolic"
                active: root.activeSection === 10
                collapsed: drawer.sidebarCollapsed
                reduceMotion: root.reduceMotion
                onTriggered: root.activeSection = 10
            }
            LinSyncNavItem {
                label: qsTr("Document Compare")
                iconName: "document-open"
                active: root.activeSection === 11
                collapsed: drawer.sidebarCollapsed
                reduceMotion: root.reduceMotion
                onTriggered: root.activeSection = 11
            }
            LinSyncNavItem {
                label: qsTr("Sessions")
                iconName: "view-history"
                active: root.activeSection === 1
                collapsed: drawer.sidebarCollapsed
                reduceMotion: root.reduceMotion
                onTriggered: root.activeSection = 1
            }

            // TOOLS
            Controls.Label {
                visible: !drawer.sidebarCollapsed
                Layout.fillWidth: true
                Layout.leftMargin: 18
                Layout.rightMargin: 18
                Layout.topMargin: 18
                Layout.bottomMargin: 6
                text: qsTr("TOOLS")
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.letterSpacing: 1.6
                color: drawer.sectionLabelColor
                opacity: 0.5
            }
            Item {
                visible: drawer.sidebarCollapsed
                Layout.preferredHeight: 14
            }
            LinSyncNavItem {
                label: qsTr("Filters")
                iconName: "view-filter"
                active: root.activeSection === 2
                collapsed: drawer.sidebarCollapsed
                reduceMotion: root.reduceMotion
                onTriggered: root.activeSection = 2
            }
            LinSyncNavItem {
                label: qsTr("Plugins")
                iconName: "preferences-plugin"
                active: root.activeSection === 3
                collapsed: drawer.sidebarCollapsed
                reduceMotion: root.reduceMotion
                onTriggered: root.activeSection = 3
            }

            // PREFERENCES
            Controls.Label {
                visible: !drawer.sidebarCollapsed
                Layout.fillWidth: true
                Layout.leftMargin: 18
                Layout.rightMargin: 18
                Layout.topMargin: 18
                Layout.bottomMargin: 6
                text: qsTr("PREFERENCES")
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.letterSpacing: 1.6
                color: drawer.sectionLabelColor
                opacity: 0.5
            }
            Item {
                visible: drawer.sidebarCollapsed
                Layout.preferredHeight: 14
            }
            LinSyncNavItem {
                label: qsTr("Settings")
                iconName: "settings-configure"
                active: root.activeSection === 4
                collapsed: drawer.sidebarCollapsed
                reduceMotion: root.reduceMotion
                onTriggered: root.activeSection = 4
            }
            LinSyncNavItem {
                label: qsTr("About")
                iconName: "help-about"
                active: root.activeSection === 5
                collapsed: drawer.sidebarCollapsed
                reduceMotion: root.reduceMotion
                onTriggered: root.activeSection = 5
            }

            Item { Layout.fillHeight: true; Layout.fillWidth: true }

            // Footer
            RowLayout {
                visible: !drawer.sidebarCollapsed
                Layout.fillWidth: true
                Layout.leftMargin: 16
                Layout.rightMargin: 16
                Layout.bottomMargin: 14
                Layout.topMargin: 8
                spacing: 8

                Rectangle {
                    radius: 999
                    color: Kirigami.ColorUtils.tintWithAlpha(
                        root.activeBg, root.activeText, 0.07)
                    border.color: root.separatorColor
                    border.width: 1
                    implicitHeight: 22
                    implicitWidth: versionLabel.implicitWidth + 20
                    Controls.Label {
                        id: versionLabel
                        anchors.centerIn: parent
                        text: "v" + root.appVersion
                        font.pixelSize: 11
                        font.family: "monospace"
                        opacity: 0.7
                        color: drawer.footerTextColor
                    }
                }
                Item { Layout.fillWidth: true }
            }
        }
    }

    pageStack.initialPage: Kirigami.Page {
        id: comparePage

        readonly property var sectionTitles: ["Compare", "Sessions", "Filters", "Plugins", "Settings", "About", "Credits", "Licenses", "Three-way Merge", "Image Compare", "Webpage Compare", "Document Compare"]
        title: sectionTitles[root.activeSection] || "LinSync"
        padding: 0
        // Hide the auto-rendered title bar — each section paints its own
        // header (compare panes, settings banner, etc.) so the Kirigami
        // default chrome would only sit on top dark-on-dark.
        titleDelegate: Item {}
        globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None
        background: Rectangle { color: root.activeBg }

        header: Rectangle {
            visible: root.activeSection === 0
            height: visible ? 40 : 0
            color: root.activeBgAlt
            // Force descendant ToolButtons to read live theme values so
            // their icons aren't dark-on-dark in light mode.
            Kirigami.Theme.inherit: false
            Kirigami.Theme.colorSet: Kirigami.Theme.Window
            Kirigami.Theme.backgroundColor: root.activeBgAlt
            Kirigami.Theme.textColor: root.activeText
            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 1
                color: root.separatorColor
            }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 6

                Controls.ToolButton {
                    icon.name: "document-open"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Open files or folders"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Open"
                    onClicked: {
                        root.newCompareTab = "files"
                        newCompareDialog.newCompareMode = root.compareMode
                        newCompareDialog.open()
                    }
                }

                Controls.ToolButton {
                    icon.name: "document-save"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Save"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Save"
                    enabled: root.leftDirty || root.rightDirty
                    onClicked: root.saveDirtySides()
                }

                Controls.ToolButton {
                    icon.name: "edit-undo"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Undo"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Undo"
                    enabled: root.canUndo
                    onClicked: root.undoLastMergeAction()
                }

                Controls.ToolButton {
                    icon.name: "edit-redo"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Redo"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Redo"
                    enabled: root.canRedo
                    onClicked: root.redoLastMergeAction()
                }

                Kirigami.Separator {
                    Layout.fillHeight: true
                }

                Controls.ToolButton {
                    icon.name: "view-refresh"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Reload from disk"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Reload"
                    onClicked: root.reloadCompare()
                }

                Controls.ToolButton {
                    icon.name: "go-first"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "First difference"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "First difference"
                    enabled: root.diffRowIndexes.length > 0
                    onClicked: root.firstDifference()
                }

                Controls.ToolButton {
                    icon.name: "go-previous"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Previous difference"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Previous difference"
                    enabled: root.diffRowIndexes.length > 0
                    onClicked: root.previousDifference()
                }

                Controls.ToolButton {
                    icon.name: "go-next"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Next difference"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Next difference"
                    enabled: root.diffRowIndexes.length > 0
                    onClicked: root.nextDifference()
                }

                Controls.ToolButton {
                    icon.name: "go-last"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Last difference"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Last difference"
                    enabled: root.diffRowIndexes.length > 0
                    onClicked: root.lastDifference()
                }

                Controls.ToolButton {
                    icon.name: "edit-find"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Find"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Find"
                    onClicked: root.openFind()
                }

                Kirigami.Separator {
                    Layout.fillHeight: true
                }

                Controls.ToolButton {
                    icon.name: "go-previous-view"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Copy right to left"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Copy right to left"
                    enabled: root.currentDiffRow >= 0
                    onClicked: root.copyCurrentDifference("right_to_left")
                }

                Controls.ToolButton {
                    icon.name: "go-next-view"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Copy left to right"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Copy left to right"
                    enabled: root.currentDiffRow >= 0
                    onClicked: root.copyCurrentDifference("left_to_right")
                }

                Controls.ToolButton {
                    icon.name: "go-first-view"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Copy all right to left"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Copy all right to left"
                    enabled: root.diffRowIndexes.length > 0
                    onClicked: root.copyAllDifferences("right_to_left")
                }

                Controls.ToolButton {
                    icon.name: "go-last-view"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Copy all left to right"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Copy all left to right"
                    enabled: root.diffRowIndexes.length > 0
                    onClicked: root.copyAllDifferences("left_to_right")
                }

                Item {
                    Layout.fillWidth: true
                }
            }
        }

        Item {
            id: sectionStack
            anchors.fill: parent
            clip: true

            ColumnLayout {
                anchors.fill: parent
                anchors.bottomMargin: 28
                visible: root.activeSection === 0
                spacing: 0

                // Force descendant Quick Controls (ComboBox, TextField,
                // TabBar, ToolButton) to read live theme values in this
                // section. Without this, the path bar + tabs ship dark
                // in light mode.
                Kirigami.Theme.inherit: false
                Kirigami.Theme.colorSet: Kirigami.Theme.Window
                Kirigami.Theme.backgroundColor: root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor: root.activeText
                Kirigami.Theme.highlightColor: root.activeHighlight

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 54
                    color: root.activeBg
                    border.color: root.separatorColor

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 8

                    AppComboBox {
                        id: modeSelector
                        indicator: Kirigami.Icon {
                            x: parent.width - width - 10
                            y: (parent.height - height) / 2
                            width: 16
                            height: 16
                            source: "arrow-down"
                            color: root.activeText
                            isMask: true
                        }
                        implicitHeight: 36
                        Layout.preferredWidth: 140
                        implicitWidth: 140
                        model: ["Text", "Folder", "Table", "Hex", "Image", "Document", "Webpage", "Archive", "Three-way"]
                        Accessible.name: "Compare mode"
                        palette.button: root.activeBgAlt
                        palette.buttonText: root.activeText
                        palette.windowText: root.activeText
                        palette.text: root.activeText
                        palette.base: root.activeBg
                        contentItem: Controls.Label {
                            text: modeSelector.displayText
                            leftPadding: 8
                            rightPadding: 28
                            verticalAlignment: Text.AlignVCenter
                            color: root.activeText
                            elide: Text.ElideRight
                        }
                        background: Rectangle {
                            color: root.activeBg
                            border.color: root.separatorColor
                            border.width: 1
                            radius: 4
                        }
                        onActivated: {
                            root.compareMode = currentText
                            root.updateActiveTabSnapshot()
                        }
                    }

                    AppTextField {
                        id: basePathField
                        implicitHeight: 36
                        Layout.fillWidth: true
                        visible: root.threeWayMode
                        text: root.basePath
                        placeholderText: qsTr("Base path")
                        Accessible.name: qsTr("Base path")
                        color: root.activeText
                        placeholderTextColor: root.activeDisabledText
                        background: Rectangle {
                            color: root.activeBg
                            border.color: root.separatorColor
                            border.width: 1
                            radius: 4
                        }
                        onEditingFinished: {
                            root.basePath = text
                            root.updateActiveTabSnapshot()
                        }
                    }

                    Controls.ToolButton {
                        icon.name: "document-open-folder"
                        icon.color: root.activeText
                        visible: root.threeWayMode
                        Controls.ToolTip.text: qsTr("Browse base")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Browse base")
                        onClicked: root.browseSide("base")
                    }

                    AppTextField {
                        implicitHeight: 36
                        Layout.fillWidth: true
                        text: root.leftPath
                        placeholderText: qsTr("Left path")
                        Accessible.name: "Left path"
                        color: root.activeText
                        placeholderTextColor: root.activeDisabledText
                        background: Rectangle {
                            color: root.activeBg
                            border.color: root.separatorColor
                            border.width: 1
                            radius: 4
                        }
                        onEditingFinished: {
                            root.leftPath = text
                            root.updateActiveTabSnapshot()
                        }
                    }

                    Controls.ToolButton {
                        icon.name: "document-open-folder"
                        icon.color: root.activeText
                        Controls.ToolTip.text: "Browse left"
                        Controls.ToolTip.visible: hovered
                        Accessible.name: "Browse left"
                        onClicked: root.browseSide("left")
                    }

                    Controls.ToolButton {
                        icon.name: "exchange-positions"
                        icon.color: root.activeText
                        Controls.ToolTip.text: "Swap sides"
                        Controls.ToolTip.visible: hovered
                        Accessible.name: "Swap sides"
                        onClicked: {
                            var tmp = root.leftPath
                            root.leftPath = root.rightPath
                            root.rightPath = tmp
                            root.updateActiveTabSnapshot()
                        }
                    }

                    AppTextField {
                        implicitHeight: 36
                        Layout.fillWidth: true
                        text: root.rightPath
                        placeholderText: "Right path"
                        Accessible.name: "Right path"
                        color: root.activeText
                        placeholderTextColor: root.activeDisabledText
                        background: Rectangle {
                            color: root.activeBg
                            border.color: root.separatorColor
                            border.width: 1
                            radius: 4
                        }
                        onEditingFinished: {
                            root.rightPath = text
                            root.updateActiveTabSnapshot()
                        }
                    }

                    Controls.ToolButton {
                        icon.name: "document-open-folder"
                        icon.color: root.activeText
                        Controls.ToolTip.text: "Browse right"
                        Controls.ToolTip.visible: hovered
                        Accessible.name: "Browse right"
                        onClicked: root.browseSide("right")
                    }

                    Controls.ToolButton {
                        icon.name: "media-playback-start"
                        icon.color: root.activeText
                        Controls.ToolTip.text: "Compare"
                        Controls.ToolTip.visible: hovered
                        Accessible.name: "Compare"
                        onClicked: root.requestCompare(false)
                    }

                    Controls.ToolButton {
                        icon.name: "tab-new"
                        icon.color: root.activeText
                        Controls.ToolTip.text: "Compare in new tab"
                        Controls.ToolTip.visible: hovered
                        Accessible.name: "Compare in new tab"
                        onClicked: root.requestCompare(true)
                    }

                    Controls.ToolButton {
                        icon.name: "process-stop"
                        icon.color: root.activeText
                        enabled: root.comparing
                        Controls.ToolTip.text: root.comparing
                            ? qsTr("Stop the running compare")
                            : qsTr("No compare is running")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Stop")
                        onClicked: root.cancelActiveCompare()
                    }

                    Kirigami.Separator {
                        Layout.fillHeight: true
                    }

                    Controls.ToolButton {
                        icon.name: "edit-copy"
                        icon.color: root.activeText
                        enabled: root.leftPath !== "" || root.rightPath !== ""
                        Controls.ToolTip.text: qsTr("Copy paths to clipboard")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Copy paths")
                        onClicked: {
                            var text = root.leftPath + "\n" + root.rightPath
                            root.copyToClipboard(text)
                        }
                    }

                    Controls.ToolButton {
                        icon.name: "folder"
                        icon.color: root.activeText
                        enabled: root.activeTabId >= 0 && root.leftPath !== ""
                        Controls.ToolTip.text: qsTr("Reveal left in file manager")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Reveal in file manager")
                        onClicked: root.bridgeGet("/reveal?path=" + encodeURIComponent(root.leftPath))
                    }

                    Controls.ToolButton {
                        icon.name: "window"
                        icon.color: root.activeText
                        enabled: root.activeTabId >= 0 && root.leftPath !== ""
                        Controls.ToolTip.text: qsTr("Open left with default application")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Open externally")
                        onClicked: root.bridgeGet("/open-external?path=" + encodeURIComponent(root.leftPath))
                    }

                    Kirigami.Separator {
                        Layout.fillHeight: true
                    }

                    Controls.ToolButton {
                        icon.name: "document-export"
                        icon.color: root.activeText
                        enabled: root.activeTabId >= 0 && root.leftPath !== ""
                        Controls.ToolTip.text: qsTr("Export unified diff report")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Export report")
                        onClicked: root.exportReport()
                    }

                    Controls.ToolButton {
                        icon.name: "view-refresh"
                        icon.color: root.activeText
                        Controls.ToolTip.text: qsTr("Reload compare")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Reload compare")
                        onClicked: root.reloadCompare()
                    }

                    Controls.ToolButton {
                        icon.name: "object-flip-horizontal"
                        icon.color: root.activeText
                        Controls.ToolTip.text: qsTr("Swap sides")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Swap sides")
                        onClicked: root.swapSides()
                    }

                    Controls.ToolButton {
                        icon.name: "vcs-merge"
                        icon.color: root.activeText
                        Controls.ToolTip.text: qsTr("Open three-way merge…")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Open three-way merge")
                        onClicked: openMergeDialog.startFlow()
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    Controls.ToolButton {
                        icon.name: "document-edit"
                        icon.color: root.activeText
                        visible: root.compareMode === "Text"
                        checkable: true
                        checked: root.editLeftMode
                        Controls.ToolTip.text: qsTr("Edit left file inline")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Edit left file")
                        onClicked: root.toggleEditMode("left")
                    }

                    Controls.ToolButton {
                        icon.name: "document-save"
                        icon.color: root.activeText
                        visible: root.compareMode === "Text"
                        enabled: root.editLeftMode && root.editLeftDirtyText !== ""
                        Controls.ToolTip.text: qsTr("Save left file")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Save left file")
                        onClicked: root.saveEdit("left")
                    }

                    Controls.ToolButton {
                        icon.name: "document-edit"
                        icon.color: root.activeText
                        visible: root.compareMode === "Text"
                        checkable: true
                        checked: root.editRightMode
                        Controls.ToolTip.text: qsTr("Edit right file inline")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Edit right file")
                        onClicked: root.toggleEditMode("right")
                    }

                    Controls.ToolButton {
                        icon.name: "document-save"
                        icon.color: root.activeText
                        visible: root.compareMode === "Text"
                        enabled: root.editRightMode && root.editRightDirtyText !== ""
                        Controls.ToolTip.text: qsTr("Save right file")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Save right file")
                        onClicked: root.saveEdit("right")
                    }
                }
            }

            // Compare-profile selector (Phase 1). A thin secondary row keeps the
            // primary path/mode toolbar uncluttered. Populated from
            // /profiles/list; selecting an entry calls /profiles/active/set.
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 40
                color: root.activeBgAlt
                border.color: root.separatorColor

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 8

                    Controls.Label {
                        text: qsTr("Profile:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    AppComboBox {
                        id: profileSelector
                        implicitHeight: 30
                        Layout.preferredWidth: 240
                        implicitWidth: 240
                        model: root.profileEntries
                        textRole: "name"
                        valueRole: "id"
                        Accessible.name: qsTr("Compare profile")
                        palette.button: root.activeBgAlt
                        palette.buttonText: root.activeText
                        palette.windowText: root.activeText
                        palette.text: root.activeText
                        palette.base: root.activeBg
                        indicator: Kirigami.Icon {
                            x: parent.width - width - 10
                            y: (parent.height - height) / 2
                            width: 16
                            height: 16
                            source: "arrow-down"
                            color: root.activeText
                            isMask: true
                        }
                        contentItem: Controls.Label {
                            text: profileSelector.displayText
                            leftPadding: 8
                            rightPadding: 28
                            verticalAlignment: Text.AlignVCenter
                            color: root.activeText
                            elide: Text.ElideRight
                        }
                        background: Rectangle {
                            color: root.activeBg
                            border.color: root.separatorColor
                            border.width: 1
                            radius: 4
                        }
                        Controls.ToolTip.visible: hovered && Controls.ToolTip.text !== ""
                        Controls.ToolTip.text: {
                            var e = root.profileEntries[profileSelector.currentIndex]
                            return e && e.description ? e.description : ""
                        }
                        onActivated: root.setActiveProfile(currentValue)
                    }

                    Controls.Label {
                        text: root.profileError
                        visible: root.profileError !== ""
                        color: Kirigami.Theme.negativeTextColor
                    }

                    Item {
                        Layout.fillWidth: true
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: visible ? 40 : 0
                visible: root.compareMode === "Text"
                color: root.activeBgAlt
                border.color: root.separatorColor

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 8

                    Controls.Label {
                        text: qsTr("View:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    AppComboBox {
                        id: textRenderSelector
                        implicitHeight: 30
                        Layout.preferredWidth: 150
                        model: [qsTr("Side by side"), qsTr("Unified"), qsTr("Context"), qsTr("Normal"), qsTr("HTML")]
                        Accessible.name: qsTr("Text render mode")
                        onActivated: {
                            const values = ["side-by-side", "unified", "context", "normal", "html"]
                            root.textRenderMode = values[currentIndex] || "side-by-side"
                            root.requestCompare(false)
                        }
                    }

                    Controls.ToolButton {
                        icon.name: "view-filter"
                        icon.color: root.activeText
                        checkable: true
                        checked: root.contextFolding
                        Controls.ToolTip.text: qsTr("Fold unchanged context")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Fold unchanged context")
                        onClicked: {
                            root.contextFolding = checked
                            root.requestCompare(false)
                        }
                    }

                    AppSpinBox {
                        implicitHeight: 30
                        from: 0
                        to: 99
                        value: root.contextLines
                        enabled: root.contextFolding
                        Accessible.name: qsTr("Context lines")
                        onValueModified: {
                            root.contextLines = value
                            if (root.contextFolding)
                                root.scheduleCompare(false)
                        }
                    }

                    Controls.ToolButton {
                        icon.name: "view-list-details"
                        icon.color: root.activeText
                        checkable: true
                        checked: root.showOnlyChanges
                        Controls.ToolTip.text: qsTr("Show only changed rows")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Show only changed rows")
                        onClicked: {
                            root.showOnlyChanges = checked
                            root.requestCompare(false)
                        }
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    Controls.Label {
                        text: qsTr("Syntax:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    AppComboBox {
                        id: syntaxSelector
                        implicitHeight: 30
                        Layout.preferredWidth: 130
                        model: [qsTr("Plain"), qsTr("Auto"), qsTr("Rust"), qsTr("JSON"), qsTr("HTML"), qsTr("Markdown"), qsTr("Shell"), qsTr("TOML"), qsTr("YAML"), qsTr("C"), qsTr("C++"), qsTr("Python"), qsTr("JavaScript"), qsTr("TypeScript"), qsTr("Go"), qsTr("Java"), qsTr("CSS")]
                        Accessible.name: qsTr("Syntax mode")
                        onActivated: {
                            const values = ["plain", "auto", "rust", "json", "html", "markdown", "shell", "toml", "yaml", "c", "cpp", "python", "javascript", "typescript", "go", "java", "css"]
                            root.syntaxMode = values[currentIndex] || "plain"
                            root.requestCompare(false)
                        }
                    }

                    Controls.ToolButton {
                        id: regexRuleButton
                        icon.name: "view-filter"
                        icon.color: root.activeText
                        text: root.textRegexRuleSetSummary()
                        display: Controls.AbstractButton.TextBesideIcon
                        Controls.ToolTip.text: qsTr("Text regex rule sets")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Text regex rule sets")
                        onClicked: regexRuleMenu.open()

                        Controls.Menu {
                            id: regexRuleMenu

                            Repeater {
                                model: root.textRegexRuleSetEntries
                                delegate: Controls.MenuItem {
                                    required property var modelData
                                    text: modelData.label
                                    checkable: true
                                    checked: root.textRegexRuleSetEnabled(modelData.id)
                                    onTriggered: root.setTextRegexRuleSet(modelData.id, checked)
                                }
                            }
                        }
                    }

                    AppComboBox {
                        id: textEncodingSelector
                        implicitHeight: 30
                        Layout.preferredWidth: 122
                        model: [qsTr("Auto"), qsTr("UTF-8"), qsTr("UTF-8 BOM"), qsTr("UTF-16 LE"), qsTr("UTF-16 BE"), qsTr("Lossy UTF-8")]
                        Accessible.name: qsTr("Text encoding")
                        onActivated: {
                            const values = ["auto", "utf8", "utf8-bom", "utf16le", "utf16be", "lossy-utf8"]
                            root.textEncoding = values[currentIndex] || "auto"
                            root.requestCompare(false)
                        }
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    Controls.ToolButton {
                        icon.name: "bookmark-new"
                        icon.color: root.activeText
                        enabled: root.currentDiffRow >= 0 || root.currentSearchRow >= 0
                        Controls.ToolTip.text: qsTr("Toggle bookmark")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Toggle bookmark")
                        onClicked: root.toggleBookmarkCurrentRow()
                    }

                    Controls.ToolButton {
                        icon.name: "go-up"
                        icon.color: root.activeText
                        enabled: root.bookmarkRows.length > 0
                        Controls.ToolTip.text: qsTr("Previous bookmark")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Previous bookmark")
                        onClicked: root.previousBookmark()
                    }

                    Controls.ToolButton {
                        icon.name: "go-down"
                        icon.color: root.activeText
                        enabled: root.bookmarkRows.length > 0
                        Controls.ToolTip.text: qsTr("Next bookmark")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Next bookmark")
                        onClicked: root.nextBookmark()
                    }

                    Item { Layout.fillWidth: true }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: visible ? 40 : 0
                visible: root.compareMode === "Hex"
                color: root.activeBgAlt
                border.color: root.separatorColor

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 8

                    Controls.Label {
                        text: qsTr("Offset:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    AppTextField {
                        id: hexOffsetField
                        implicitHeight: 30
                        Layout.preferredWidth: 120
                        text: root.hexJumpOffset
                        placeholderText: qsTr("hex (e.g. 1a3f)")
                        Accessible.name: qsTr("Jump to hex offset")
                        onAccepted: root.jumpToHexOffset(text)
                    }

                    Controls.ToolButton {
                        icon.name: "go-jump"
                        icon.color: root.activeText
                        Controls.ToolTip.text: qsTr("Jump to offset")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Jump to offset")
                        onClicked: root.jumpToHexOffset(hexOffsetField.text)
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    Controls.Label {
                        text: qsTr("Search bytes:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    AppTextField {
                        id: hexSearchField
                        implicitHeight: 30
                        Layout.preferredWidth: 160
                        placeholderText: qsTr("hex (e.g. 48 65 6c)")
                        Accessible.name: qsTr("Search bytes in hex")
                        onAccepted: root.searchHexBytes(text)
                    }

                    Controls.ToolButton {
                        icon.name: "edit-find"
                        icon.color: root.activeText
                        Controls.ToolTip.text: qsTr("Search bytes")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Search bytes")
                        onClicked: root.searchHexBytes(hexSearchField.text)
                    }

                    Item { Layout.fillWidth: true }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: visible ? 40 : 0
                visible: root.compareMode === "Folder"
                color: root.activeBgAlt
                border.color: root.separatorColor

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 6

                    Controls.Label {
                        text: qsTr("Folder ops:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    Controls.ToolButton {
                        icon.name: "edit-copy"
                        icon.color: root.activeText
                        text: qsTr("Copy →")
                        display: Controls.AbstractButton.TextBesideIcon
                        Controls.ToolTip.text: qsTr("Copy selected entry left → right")
                        Controls.ToolTip.visible: hovered
                        enabled: root.currentDiffRow >= 0
                        onClicked: root.runFolderOp("copy_left_to_right")
                    }

                    Controls.ToolButton {
                        icon.name: "edit-copy"
                        icon.color: root.activeText
                        text: qsTr("Copy ←")
                        display: Controls.AbstractButton.TextBesideIcon
                        Controls.ToolTip.text: qsTr("Copy selected entry right → left")
                        Controls.ToolTip.visible: hovered
                        enabled: root.currentDiffRow >= 0
                        onClicked: root.runFolderOp("copy_right_to_left")
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    Controls.ToolButton {
                        icon.name: "edit-delete"
                        icon.color: root.activeText
                        text: qsTr("Delete left")
                        display: Controls.AbstractButton.TextBesideIcon
                        enabled: root.currentDiffRow >= 0
                        onClicked: root.runFolderOp("delete_left")
                    }

                    Controls.ToolButton {
                        icon.name: "edit-delete"
                        icon.color: root.activeText
                        text: qsTr("Delete right")
                        display: Controls.AbstractButton.TextBesideIcon
                        enabled: root.currentDiffRow >= 0
                        onClicked: root.runFolderOp("delete_right")
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    Controls.ToolButton {
                        icon.name: "view-refresh"
                        icon.color: root.activeText
                        text: qsTr("Refresh")
                        display: Controls.AbstractButton.TextBesideIcon
                        onClicked: root.requestCompare(false)
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    Controls.Label {
                        text: qsTr("Filter:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    AppButton {
                        text: qsTr("Changed")
                        flat: true
                        highlighted: root.folderFilter === "changed"
                        onClicked: root.folderFilter = root.folderFilter === "changed" ? "" : "changed"
                    }

                    AppButton {
                        text: qsTr("Left only")
                        flat: true
                        highlighted: root.folderFilter === "left_only"
                        onClicked: root.folderFilter = root.folderFilter === "left_only" ? "" : "left_only"
                    }

                    AppButton {
                        text: qsTr("Right only")
                        flat: true
                        highlighted: root.folderFilter === "right_only"
                        onClicked: root.folderFilter = root.folderFilter === "right_only" ? "" : "right_only"
                    }

                    AppButton {
                        text: qsTr("Different")
                        flat: true
                        highlighted: root.folderFilter === "diff"
                        onClicked: root.folderFilter = root.folderFilter === "diff" ? "" : "diff"
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    AppTextField {
                        id: folderSearchField
                        implicitHeight: 32
                        Layout.preferredWidth: 200
                        text: root.folderSearch
                        placeholderText: qsTr("Search paths…")
                        color: root.activeText
                        placeholderTextColor: root.activeDisabledText
                        background: Rectangle {
                            color: root.activeBg
                            border.color: root.separatorColor
                            border.width: 1
                            radius: 4
                        }
                        Accessible.name: qsTr("Search folder entries by path")
                        onTextChanged: root.folderSearch = text
                    }

                    Controls.Label {
                        text: qsTr("Type:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    AppButton {
                        text: qsTr("Files")
                        flat: true
                        highlighted: root.folderTypeList.indexOf("file") >= 0
                        onClicked: root.toggleFolderType("file")
                    }

                    AppButton {
                        text: qsTr("Folders")
                        flat: true
                        highlighted: root.folderTypeList.indexOf("directory") >= 0
                        onClicked: root.toggleFolderType("directory")
                    }

                    AppButton {
                        text: qsTr("Symlinks")
                        flat: true
                        highlighted: root.folderTypeList.indexOf("symlink") >= 0
                        onClicked: root.toggleFolderType("symlink")
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    Controls.Label {
                        text: qsTr("Sort:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    AppButton {
                        text: qsTr("Path") + (root.folderSortColumn === "path" ? (root.folderSortAscending ? " ▲" : " ▼") : "")
                        flat: true
                        highlighted: root.folderSortColumn === "path"
                        onClicked: root.toggleFolderSort("path")
                    }

                    AppButton {
                        text: qsTr("Status") + (root.folderSortColumn === "state" ? (root.folderSortAscending ? " ▲" : " ▼") : "")
                        flat: true
                        highlighted: root.folderSortColumn === "state"
                        onClicked: root.toggleFolderSort("state")
                    }

                    AppButton {
                        text: qsTr("Size") + (root.folderSortColumn === "leftSize" ? (root.folderSortAscending ? " ▲" : " ▼") : "")
                        flat: true
                        highlighted: root.folderSortColumn === "leftSize"
                        onClicked: root.toggleFolderSort("leftSize")
                    }

                    AppButton {
                        text: qsTr("Method") + (root.folderSortColumn === "method" ? (root.folderSortAscending ? " ▲" : " ▼") : "")
                        flat: true
                        highlighted: root.folderSortColumn === "method"
                        onClicked: root.toggleFolderSort("method")
                    }

                    Kirigami.Separator { Layout.fillHeight: true }

                    Controls.Label {
                        text: qsTr("Group:")
                        color: root.activeText
                        opacity: 0.7
                    }

                    AppButton {
                        text: qsTr("None")
                        flat: true
                        highlighted: root.folderGroupBy === ""
                        onClicked: root.folderGroupBy = ""
                    }

                    AppButton {
                        text: qsTr("State")
                        flat: true
                        highlighted: root.folderGroupBy === "state"
                        onClicked: root.folderGroupBy = root.folderGroupBy === "state" ? "" : "state"
                    }

                    AppButton {
                        text: qsTr("Type")
                        flat: true
                        highlighted: root.folderGroupBy === "type"
                        onClicked: root.folderGroupBy = root.folderGroupBy === "type" ? "" : "type"
                    }

                    AppButton {
                        text: qsTr("Dir")
                        flat: true
                        highlighted: root.folderGroupBy === "dir"
                        onClicked: root.folderGroupBy = root.folderGroupBy === "dir" ? "" : "dir"
                    }

                    Item { Layout.fillWidth: true }

                    Controls.Label {
                        text: root.currentDiffRow >= 0
                            ? qsTr("Selected: %1").arg(root.currentFolderEntryPath())
                            : qsTr("Select a folder row to enable operations.")
                        color: root.activeText
                        opacity: 0.65
                        font.pixelSize: 11
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true

                Controls.TabBar {
                    id: compareTabBar
                    Layout.fillWidth: true
                    background: Rectangle { color: root.activeBgAlt }

                    Repeater {
                        model: root.tabItems

                        Controls.TabButton {
                            id: compareTab
                            required property var modelData

                            text: (modelData.dirty ? "* " : "") + (modelData.title || "Compare")
                            checked: modelData.id === root.activeTabId
                            onClicked: root.activateSessionTab(modelData.id)
                            contentItem: Controls.Label {
                                text: compareTab.text
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                                color: compareTab.checked ? root.activeHighlight : root.activeText
                                font.bold: compareTab.checked
                            }
                            background: Rectangle {
                                color: compareTab.checked
                                    ? Kirigami.ColorUtils.tintWithAlpha(root.activeBgAlt, root.activeHighlight, 0.1)
                                    : root.activeBgAlt
                                Rectangle {
                                    visible: compareTab.checked
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.bottom: parent.bottom
                                    height: 2
                                    color: root.activeHighlight
                                }
                            }
                        }
                    }

                    Controls.TabButton {
                        id: defaultTab
                        visible: root.tabItems.length === 0
                        text: "Untitled compare"
                        contentItem: Controls.Label {
                            text: defaultTab.text
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                            color: root.activeText
                        }
                        background: Rectangle {
                            color: root.activeBgAlt
                            Rectangle {
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                height: 2
                                color: root.activeHighlight
                            }
                        }
                    }
                }

                Controls.ToolButton {
                    icon.name: "tab-close"
                    icon.color: root.activeText
                    enabled: root.activeTabId !== 0
                    Controls.ToolTip.text: "Close current tab"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Close current tab"
                    onClicked: root.closeActiveTab()
                }
            }

            RowLayout {
                Layout.fillWidth: true
                visible: root.findVisible
                spacing: 8

                AppTextField {
                    implicitHeight: 36
                    id: searchField

                    Layout.preferredWidth: 280
                    text: root.searchText
                    placeholderText: "Find"
                    color: root.activeText
                    placeholderTextColor: root.activeDisabledText
                    background: Rectangle {
                        color: root.activeBg
                        border.color: root.separatorColor
                        border.width: 1
                        radius: 4
                    }
                    Accessible.name: "Find text"
                    onTextChanged: {
                        root.searchText = text
                        root.rebuildSearchRows()
                    }
                    onAccepted: root.nextSearchResult()
                }

                Controls.ToolButton {
                    icon.name: "go-up"
                    icon.color: root.activeText
                    enabled: root.searchRowIndexes.length > 0
                    Controls.ToolTip.text: "Previous match"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Previous match"
                    onClicked: root.previousSearchResult()
                }

                Controls.ToolButton {
                    icon.name: "go-down"
                    icon.color: root.activeText
                    enabled: root.searchRowIndexes.length > 0
                    Controls.ToolTip.text: "Next match"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Next match"
                    onClicked: root.nextSearchResult()
                }

                Controls.ToolButton {
                    text: ".*"
                    checkable: true
                    checked: root.searchRegex
                    implicitWidth: 36
                    Controls.ToolTip.text: qsTr("Regex find")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("Regex find")
                    onClicked: {
                        root.searchRegex = checked
                        root.rebuildSearchRows()
                    }
                }

                Controls.ToolButton {
                    text: "Aa"
                    checkable: true
                    checked: root.searchCaseSensitive
                    implicitWidth: 36
                    Controls.ToolTip.text: qsTr("Case-sensitive find")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("Case-sensitive find")
                    onClicked: {
                        root.searchCaseSensitive = checked
                        root.rebuildSearchRows()
                    }
                }

                Controls.Label {
                    text: root.searchRowIndexes.length > 0 ? (root.currentSearchPosition + 1) + "/" + root.searchRowIndexes.length : "0/0"
                }

                Item {
                    Layout.fillWidth: true
                }

                Controls.ToolButton {
                    icon.name: "window-close"
                    icon.color: root.activeText
                    Controls.ToolTip.text: "Close find"
                    Controls.ToolTip.visible: hovered
                    Accessible.name: "Close find"
                    onClicked: root.findVisible = false
                }
            }

            Rectangle {
                visible: root.compareMode === "Folder"
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: root.activeBg
                border.color: root.separatorColor
                clip: true

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 0

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 34
                        color: root.activeBgAlt
                        border.color: root.separatorColor

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            spacing: 12

                            Controls.Label { text: qsTr("Path"); Layout.fillWidth: true; color: root.activeText; font.bold: true }
                            Controls.Label { text: qsTr("Status"); Layout.preferredWidth: 96; color: root.activeText; font.bold: true }
                            Controls.Label { text: qsTr("Left size"); Layout.preferredWidth: 92; color: root.activeText; font.bold: true; horizontalAlignment: Text.AlignRight }
                            Controls.Label { text: qsTr("Right size"); Layout.preferredWidth: 92; color: root.activeText; font.bold: true; horizontalAlignment: Text.AlignRight }
                            Controls.Label { text: qsTr("Method"); Layout.preferredWidth: 120; color: root.activeText; font.bold: true }
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: visible ? 40 : 0
                        // Scoped to the tab that started the edit so switching
                        // tabs never overlays an unrelated comparison.
                        visible: root.archiveEditInProgress
                                 && root.archiveEditTabId === root.activeTabId
                        color: Kirigami.ColorUtils.tintWithAlpha(root.activeBg, Kirigami.Theme.positiveTextColor, 0.12)
                        border.color: root.separatorColor

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            spacing: 12

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2

                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: root.archiveEditInProgress
                                        ? qsTr("Editing %1 in %2 archive — save in your external editor, then commit or discard")
                                            .arg(root.archiveEditMember)
                                            .arg(root.archiveEditSide)
                                        : ""
                                    color: root.activeText
                                    elide: Text.ElideMiddle
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    visible: root.archiveEditPortalWarning !== ""
                                    text: root.archiveEditPortalWarning
                                    color: Kirigami.Theme.negativeTextColor
                                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                                    elide: Text.ElideMiddle
                                }
                            }
                            AppButton {
                                text: qsTr("Commit")
                                enabled: root.archiveEditInProgress
                                onClicked: root.commitArchiveMemberEdit()
                                Accessible.name: qsTr("Commit archive member edit")
                            }
                            AppButton {
                                text: qsTr("Discard")
                                enabled: root.archiveEditInProgress
                                onClicked: root.discardArchiveMemberEdit()
                                Accessible.name: qsTr("Discard archive member edit")
                            }
                        }
                    }

                    // One shared context menu for every folder row (declaring
                    // it per delegate would build a Menu per visible row in a
                    // perf-sensitive list). Only popped for archive-compare
                    // file rows, so it is never shown empty.
                    Controls.Menu {
                        id: archiveEntryContextMenu

                        Controls.MenuItem {
                            text: qsTr("Edit member in left archive")
                            visible: root.leftArchiveEditable
                            enabled: !root.archiveEditInProgress
                            onTriggered: root.startArchiveMemberEdit("left")
                            Accessible.name: text
                        }
                        Controls.MenuItem {
                            text: qsTr("Edit member in right archive")
                            visible: root.rightArchiveEditable
                            enabled: !root.archiveEditInProgress
                            onTriggered: root.startArchiveMemberEdit("right")
                            Accessible.name: text
                        }
                    }

                    ListView {
                        id: folderTable
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        model: root.visibleFolderEntries
                        boundsBehavior: Flickable.StopAtBounds
                        reuseItems: true
                        // Lazily fetch the next page of a windowed (large)
                        // folder as the table nears the bottom.
                        onContentYChanged: root.maybeLoadMoreFolderRows(folderTable)

                        delegate: Rectangle {
                            required property var modelData
                            required property int index

                            width: folderTable.width
                            height: 34
                            color: index === root.currentDiffRow
                                ? Kirigami.ColorUtils.tintWithAlpha(root.activeBg, root.activeHighlight, 0.18)
                                : root.lineBackground(modelData.state || "equal", index)
                            border.color: index === root.currentDiffRow ? root.activeHighlight : "transparent"
                            border.width: index === root.currentDiffRow ? 1 : 0

                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 10
                                anchors.rightMargin: 10
                                spacing: 12

                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: (modelData.isDir ? modelData.path + "/" : modelData.path)
                                    elide: Text.ElideMiddle
                                    color: root.activeText
                                    font.family: "monospace"
                                }
                                Controls.Label {
                                    Layout.preferredWidth: 96
                                    text: modelData.state || "equal"
                                    color: root.activeText
                                    elide: Text.ElideRight
                                }
                                Controls.Label {
                                    Layout.preferredWidth: 92
                                    text: root.folderSizeLabel(modelData.leftSize)
                                    horizontalAlignment: Text.AlignRight
                                    color: root.activeText
                                    font.family: "monospace"
                                }
                                Controls.Label {
                                    Layout.preferredWidth: 92
                                    text: root.folderSizeLabel(modelData.rightSize)
                                    horizontalAlignment: Text.AlignRight
                                    color: root.activeText
                                    font.family: "monospace"
                                }
                                Controls.Label {
                                    Layout.preferredWidth: 120
                                    text: modelData.method || ""
                                    color: root.activeDisabledText
                                    elide: Text.ElideRight
                                }
                            }

                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                onClicked: function (mouse) {
                                    root.currentDiffRow = index
                                    root.currentDiffPosition = root.diffRowIndexes.indexOf(index)
                                    if (mouse.button === Qt.RightButton
                                            && root.validationPathKind === "Archives"
                                            && !modelData.isDir) {
                                        archiveEntryContextMenu.popup()
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Controls.SplitView {
                visible: root.compareMode !== "Folder" && root.compareMode !== "Table"
                Layout.fillHeight: root.compareMode !== "Folder" && root.compareMode !== "Table"
                Layout.fillWidth: true
                orientation: Qt.Horizontal

                PaneColumn {
                    id: leftPane

                    Controls.SplitView.fillWidth: true
                    Controls.SplitView.minimumWidth: 320
                    sideName: "Left"
                    sideKey: "left"
                    accentColor: root.activeNeutralText
                    pathText: root.leftPath
                    rows: root.compareMode === "Folder" ? [] : root.leftRows
                    useBridgeModel: false
                    modelRevision: root.bridgeModelRevision
                    editMode: root.editLeftMode
                }

                PaneColumn {
                    id: rightPane

                    Controls.SplitView.fillWidth: true
                    Controls.SplitView.minimumWidth: 320
                    sideName: "Right"
                    sideKey: "right"
                    accentColor: root.activePositiveText
                    pathText: root.rightPath
                    rows: root.compareMode === "Folder" ? [] : root.rightRows
                    useBridgeModel: false
                    modelRevision: root.bridgeModelRevision
                    editMode: root.editRightMode
                }
                }

                Rectangle {
                    id: overviewPane

                    Controls.SplitView.preferredWidth: 36
                    Controls.SplitView.minimumWidth: 28
                    color: root.activeBgAlt
                    border.color: root.separatorColor

                    // Overview ruler — single Canvas element instead of
                    // hundreds of QML items (one per diff row). O(1) paint.
                    Canvas {
                        id: overviewCanvas
                        anchors.fill: parent
                        anchors.margins: 6

                        property var diffRows: root.diffRowIndexes
                        // Reflect the FULL diff length when windowed so the diff
                        // markers and viewport indicator stay proportional even
                        // though only part of the diff is loaded.
                        property int totalRows: root.textTotalRows > 0 ? root.textTotalRows : root.leftRows.length
                        property color normalColor: root.activeNegativeText
                        property color highlightColor: root.activeHighlight
                        property int currentHighlight: root.currentDiffRow

                        onDiffRowsChanged: requestPaint()
                        onTotalRowsChanged: requestPaint()
                        onNormalColorChanged: requestPaint()
                        onHighlightColorChanged: requestPaint()
                        onCurrentHighlightChanged: requestPaint()
                        onWidthChanged: requestPaint()
                        onHeightChanged: requestPaint()

                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.clearRect(0, 0, width, height)
                            if (totalRows < 2) return

                            var w = width
                            var h = height
                            var rows = diffRows
                            var n = rows.length
                            var total = totalRows - 1
                            var normColor = normalColor
                            var highColor = highlightColor
                            var highRow = currentHighlight
                            var step = h / total

                            for (var i = 0; i < n; i++) {
                                var r = Number(rows[i])
                                var y = r * step
                                // Highlighted row is taller and brighter
                                var mh = r === highRow ? Math.min(12, step * 2) : Math.max(3, step * 0.8)
                                y = Math.max(0, Math.min(h - mh, y))
                                ctx.fillStyle = r === highRow ? highColor : normColor
                                ctx.fillRect(0, y, w, mh)
                            }
                        }

                        // Click anywhere on the overview ruler to jump to
                        // that position in both panes. Drag (press + move)
                        // jumps continuously, so this also acts like a
                        // scrollbar handle.
                        MouseArea {
                            id: overviewMA
                            anchors.fill: parent

                            function jumpToY(y) {
                                if (overviewCanvas.totalRows < 2) return
                                var ratio = y / height
                                var targetRow = Math.round(ratio * (overviewCanvas.totalRows - 1))
                                targetRow = Math.max(0, Math.min(overviewCanvas.totalRows - 1, targetRow))
                                // Windowed diff: load up to the target before
                                // positioning so the jump lands on real content.
                                if (root.textTotalRows > 0 && targetRow >= root.unfilteredLeftRows.length
                                        && root.unfilteredLeftRows.length < root.textTotalRows) {
                                    root.currentDiffRow = targetRow
                                    root.currentDiffPosition = -1
                                    root.loadTextWindowsUntil(targetRow, function () {
                                        root.syncingScroll = true
                                        if (leftPane) leftPane.positionAtRow(targetRow)
                                        if (rightPane) rightPane.positionAtRow(targetRow)
                                        root.syncingScroll = false
                                    })
                                    return
                                }
                                root.syncingScroll = true
                                if (leftPane) leftPane.positionAtRow(targetRow)
                                if (rightPane) rightPane.positionAtRow(targetRow)
                                root.syncingScroll = false
                                root.currentDiffRow = targetRow
                                root.currentDiffPosition = -1
                            }

                            onPressed: function(mouse) { jumpToY(mouse.y) }
                            onPositionChanged: function(mouse) {
                                if (pressed) jumpToY(mouse.y)
                            }
                        }

                        // Viewport indicator — a 10px-tall bar that shows
                        // where in the file the editor panes are currently
                        // scrolled to. Position is read from leftPane's
                        // top visible row (left/right are scroll-synced),
                        // so the bar tracks scrolls from any source. The
                        // MouseArea above already handles dragging, so this
                        // is just a visual marker.
                        Rectangle {
                            id: viewportIndicator
                            x: 0
                            width: parent.width
                            height: 10
                            radius: 2
                            color: root.activeHighlight
                            opacity: 0.65
                            visible: overviewCanvas.totalRows > 1

                            readonly property real _track: Math.max(0, parent.height - height)
                            readonly property real _ratio:
                                leftPane && leftPane.topVisibleRow !== undefined
                                    && overviewCanvas.totalRows > 1
                                    ? Math.max(0, Math.min(1,
                                        leftPane.topVisibleRow / (overviewCanvas.totalRows - 1)))
                                    : 0
                            y: _track * _ratio
                        }
                    }
                }
            }

            TableComparePane {
                visible: root.compareMode === "Table"
                Layout.fillHeight: true
                Layout.fillWidth: true
                headers: root.tableHeaders
                rows: root.tableCells
                onLoadMoreRequested: root.loadNextTableWindow()
            }

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 28
                visible: root.activeSection === 0
                Layout.fillWidth: true
                Layout.preferredHeight: 28
                color: root.activeBgAlt
                border.color: root.separatorColor

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 10
                    spacing: 16

                    Controls.Label {
                        id: statusBarLabel
                        text: root.statusText
                        color: {
                            var lower = root.statusText.toLowerCase()
                            if (lower.indexOf("failed") >= 0 || lower.indexOf("could not") >= 0 || lower.indexOf("unavailable") >= 0 || lower.indexOf("error") >= 0)
                                return Kirigami.Theme.negativeTextColor
                            return root.activeText
                        }
                        // Expose the status line as a live region so screen
                        // readers announce status/error changes as they happen.
                        Accessible.role: Accessible.StaticText
                        Accessible.name: qsTr("Status: %1").arg(root.statusText)
                    }
                    Controls.ProgressBar {
                        visible: root.comparing && root.progressTotal > 0
                        from: 0
                        to: root.progressTotal
                        value: root.progressCurrent
                        implicitWidth: 120
                        implicitHeight: root.activeText !== "" ? 16 : 16
                    }
                    Controls.Label { text: root.differenceText; color: root.activeText }
                    Controls.Label {
                        text: root.currentDiffPosition >= 0 ? "Current: " + (root.currentDiffPosition + 1) + "/" + root.diffRowIndexes.length : "Current: -"
                        color: root.activeText
                    }
                    Controls.Label {
                        text: root.searchText !== "" ? "Find: " + (root.currentSearchPosition >= 0 ? (root.currentSearchPosition + 1) + "/" + root.searchRowIndexes.length : "0/0") : "Find: -"
                        color: root.activeText
                    }
                    Controls.Label { text: root.compareMode; color: root.activeText }
                    Controls.Label {
                        text: "Tabs: " + (root.sessionState.tabs ? root.sessionState.tabs.length : 0)
                        color: root.activeText
                    }
                    Controls.Label {
                        text: (root.leftDirty ? "Left modified" : "Left clean") + " / " + (root.rightDirty ? "Right modified" : "Right clean")
                        color: root.activeText
                    }
                    Repeater {
                        model: root.summaryItems
                        Controls.Label {
                            required property var modelData
                            text: modelData.label + ": " + modelData.value
                            color: root.activeText
                        }
                    }

                    Item {
                        Layout.fillWidth: true
                    }
                }
            }
            }

            SessionsPage {
                id: sessionsPage
                anchors.fill: parent
                visible: root.activeSection === 1
                Kirigami.Theme.backgroundColor:          root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor:                root.activeText
                Kirigami.Theme.disabledTextColor:        root.activeDisabledText
                Kirigami.Theme.highlightColor:           root.activeHighlight
                Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
                Kirigami.Theme.positiveTextColor:        root.activePositiveText
                Kirigami.Theme.negativeTextColor:        root.activeNegativeText
                Kirigami.Theme.neutralTextColor:         root.activeNeutralText
                bridgeUrl: root.bridgeUrl
                sessionState: root.sessionState
                activeTabId: root.activeTabId
                onTabActivated: tabId => root.activateSessionTab(tabId)
                onTabClosed: tabId => root.performCloseTab(tabId)
                onNavigateRequested: section => root.activeSection = section
                onReopenRecentRequested: index => root.reopenRecentSession(index)
                onDeleteRecentSessionRequested: index => {
                    bridgeGet("/sessions/delete?index=" + encodeURIComponent(index), function (ok) {
                        if (ok) root.loadRecentSessions(function (items) {
                            sessionsPage.recentSessions = items
                        })
                    })
                }
                onRenameRecentSessionRequested: (index, title) => {
                    bridgeGet("/sessions/rename?index=" + encodeURIComponent(index) + "&title=" + encodeURIComponent(title), function (ok) {
                        if (ok) root.loadRecentSessions(function (items) {
                            sessionsPage.recentSessions = items
                        })
                    })
                }
                onRefreshRecentRequested: {
                    root.loadRecentSessions(function (items) {
                        sessionsPage.recentSessions = items
                    })
                }
                onSaveProjectRequested: projectSaveDialog.open()
                onOpenProjectRequested: projectOpenDialog.open()
                onOpenRecentProjectRequested: path => root.openProjectFile(path)
                Component.onCompleted: root.loadRecentProjects()
            }

            FiltersPage {
                id: filtersPage
                anchors.fill: parent
                visible: root.activeSection === 2
                Kirigami.Theme.backgroundColor:          root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor:                root.activeText
                Kirigami.Theme.disabledTextColor:        root.activeDisabledText
                Kirigami.Theme.highlightColor:           root.activeHighlight
                Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
                Kirigami.Theme.positiveTextColor:        root.activePositiveText
                Kirigami.Theme.negativeTextColor:        root.activeNegativeText
                Kirigami.Theme.neutralTextColor:         root.activeNeutralText
                bridgeConnected: root.bridgeAvailable()
                onIncludesEdited: rules => {
                    root.saveWalkOption("includes", rules.join(","))
                }
                onExcludesEdited: rules => {
                    root.saveWalkOption("excludes", rules.join(","))
                }
                onGitignoreToggled: value => {
                    root.saveWalkOption("respect_gitignore", value)
                }
                onFollowSymlinksToggled: value => {
                    root.saveWalkOption("follow_symlinks", value)
                }
                onMaxDepthEdited: value => {
                    root.saveWalkOption("max_walk_depth", value)
                }
                onValidateRequested: body => {
                    root.validateFilterRule(body, function (ok, payload) {
                        if (ok && payload)
                            filtersPage.validationResult = payload
                    })
                }
                onSaveFilterRequested: body => {
                    root.saveNamedFilter(body, function (ok, payload, status) {
                        if (ok && payload) {
                            filtersPage.savedFilters = payload.filters || []
                            root.statusText = "Filter saved"
                        } else {
                            root.statusText = "Filter save failed (" + status + ")"
                        }
                    })
                }
                onDeleteFilterRequested: name => {
                    root.deleteNamedFilter(name, function (ok, payload) {
                        if (ok && payload)
                            filtersPage.savedFilters = payload.filters || []
                    })
                }
                onOpenMigratePickerRequested: {
                    migrateFileDialog.open()
                }
                Component.onCompleted: {
                    root.loadWalkOptions(function (options) {
                        filtersPage.includeRules = options.includes || []
                        filtersPage.excludeRules = options.excludes || []
                        filtersPage.respectGitignore = options.respect_gitignore !== false
                        filtersPage.followSymlinks = !!options.follow_symlinks
                        filtersPage.maxDepth = options.max_walk_depth || 0
                    })
                    root.loadFilters(function (payload) {
                        filtersPage.savedFilters = payload.filters || []
                    })
                }
            }

            PluginsPage {
                id: pluginsPage
                anchors.fill: parent
                visible: root.activeSection === 3
                Kirigami.Theme.backgroundColor:          root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor:                root.activeText
                Kirigami.Theme.disabledTextColor:        root.activeDisabledText
                Kirigami.Theme.highlightColor:           root.activeHighlight
                Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
                Kirigami.Theme.positiveTextColor:        root.activePositiveText
                Kirigami.Theme.negativeTextColor:        root.activeNegativeText
                Kirigami.Theme.neutralTextColor:         root.activeNeutralText
                bridgeConnected: root.bridgeAvailable()
                reduceMotion: root.reduceMotion
                onRefreshRequested: {
                    root.loadPlugins(function (payload) {
                        pluginsPage.applyDiscovery(payload)
                    })
                }
                onPluginToggled: function(id, enabled) {
                    if (root.bridgeUrl === "") return
                    const url = root.bridgeUrl + "/plugins/toggle?id="
                        + encodeURIComponent(id) + "&enabled=" + (enabled ? "true" : "false")
                    const xhr = new XMLHttpRequest()
                    xhr.open("GET", url)
                    xhr.send()
                }
                onPluginDiagnoseRequested: function(id) {
                    root.bridgeGet("/plugins/diagnostic?id=" + encodeURIComponent(id),
                        function (ok, payload) {
                            if (ok)
                                pluginsPage.showDiagnosticResult(id, payload)
                            else
                                pluginsPage.showDiagnosticResult(id, null)
                        })
                }
                onPluginTrustAndEnableRequested: function(id, name) {
                    // Record trust, then enable, then refresh so the row shows
                    // both the new trusted state and the enabled toggle.
                    root.bridgeGet("/plugins/trust?id=" + encodeURIComponent(id) + "&trusted=true",
                        function (ok) {
                            if (!ok) {
                                pluginsPage.showActionResult(qsTr("Could not trust %1").arg(name))
                                return
                            }
                            root.bridgeGet("/plugins/toggle?id=" + encodeURIComponent(id) + "&enabled=true",
                                function () {
                                    pluginsPage.showActionResult(qsTr("Trusted and enabled %1").arg(name))
                                    root.loadPlugins(function (p) { pluginsPage.applyDiscovery(p) })
                                })
                        })
                }
                onPluginProfilePrediffferToggled: function(id, enabled) {
                    root.bridgeGet("/profiles/active/prediffer?id=" + encodeURIComponent(id)
                            + "&enabled=" + (enabled ? "true" : "false"),
                        function (ok, payload, status) {
                            if (ok) {
                                pluginsPage.showActionResult(enabled
                                    ? qsTr("Added to profile prediffers")
                                    : qsTr("Removed from profile prediffers"))
                                root.loadPlugins(function (p) { pluginsPage.applyDiscovery(p) })
                            } else if (status === 409) {
                                pluginsPage.showActionResult(qsTr("Select a user profile to edit its prediffers"))
                            } else {
                                pluginsPage.showActionResult(qsTr("Could not update profile prediffers"))
                            }
                        })
                }
                onPluginProfileEnabledToggled: function(id, enabled) {
                    root.bridgeGet("/profiles/active/plugin-enabled?id=" + encodeURIComponent(id)
                            + "&enabled=" + (enabled ? "true" : "false"),
                        function (ok, payload, status) {
                            if (ok) {
                                pluginsPage.showActionResult(enabled
                                    ? qsTr("Enabled in active profile")
                                    : qsTr("Disabled in active profile"))
                                root.loadPlugins(function (p) { pluginsPage.applyDiscovery(p) })
                            } else if (status === 409) {
                                pluginsPage.showActionResult(qsTr("Select a user profile to set per-profile plugin state"))
                            } else {
                                pluginsPage.showActionResult(qsTr("Could not update per-profile plugin state"))
                            }
                        })
                }
                onPluginInstallRequested: pluginInstallDialog.open()
                onPluginRemoveRequested: function(id, name) {
                    root.pendingRemovePluginId = id
                    root.pendingRemovePluginName = name
                    pluginRemoveDialog.open()
                }
                onPluginOptionsRequested: function(id, name) {
                    if (root.bridgeUrl === "") return
                    const url = root.bridgeUrl + "/plugins/options/get?id=" + encodeURIComponent(id)
                    const xhr = new XMLHttpRequest()
                    xhr.open("GET", url)
                    xhr.onreadystatechange = function() {
                        if (xhr.readyState !== XMLHttpRequest.DONE) return
                        if (xhr.status === 200) {
                            try {
                                const data = JSON.parse(xhr.responseText)
                                pluginsPage.openOptionsDialog(id, name, data.schema || [], data.values || {})
                            } catch (e) {
                                // Ignore a malformed options payload.
                            }
                        }
                    }
                    xhr.send()
                }
                onPluginOptionSaved: function(id, key, ok, error) {
                    if (root.bridgeUrl === "") return
                    const dirtyValues = pluginsPage._optionsDirty
                    if (!(key in dirtyValues)) return
                    const value = dirtyValues[key]
                    const url = root.bridgeUrl + "/plugins/options/set?id="
                        + encodeURIComponent(id) + "&key=" + encodeURIComponent(key)
                        + "&value=" + encodeURIComponent(String(value))
                    const xhr = new XMLHttpRequest()
                    xhr.open("GET", url)
                    xhr.send()
                }
                Component.onCompleted: {
                    root.loadPlugins(function (payload) {
                        pluginsPage.applyDiscovery(payload)
                    })
                }
            }

            SettingsPage {
                id: settingsPage
                anchors.fill: parent
                visible: root.activeSection === 4
                Kirigami.Theme.backgroundColor:          root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor:                root.activeText
                Kirigami.Theme.disabledTextColor:        root.activeDisabledText
                Kirigami.Theme.highlightColor:           root.activeHighlight
                Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
                Kirigami.Theme.positiveTextColor:        root.activePositiveText
                Kirigami.Theme.negativeTextColor:        root.activeNegativeText
                Kirigami.Theme.neutralTextColor:         root.activeNeutralText
                themePreference:    root.themePreference
                themeValues:        root.themeValues
                themeLabels:        root.themeLabels
                fontSize:           root.paneFontSize
                fontFamily:         root.paneFontFamily
                tabWidth:           root.paneTabWidth
                showLineNumbers:    root.showLineNumbers
                showWhitespace:     root.showWhitespace
                wordWrap:           root.wordWrap
                ignoreCase:         root.ignoreCase
                ignoreWhitespace:   root.ignoreWhitespace
                ignoreBlankLines:   root.ignoreBlankLines
                ignoreEol:          root.ignoreEol
                eolNormalization:   root.eolNormalization
                defaultCompareMode: root.defaultCompareMode
                confirmOnClose:     root.confirmOnClose
                persistRecentPaths: root.persistRecentPaths
                reduceMotion:       root.reduceMotion
                detectMoves:        root.detectMoves
                keepArchiveBackup:  root.keepArchiveBackup
                maxRecentPaths:     root.maxRecentPaths
                bridgeConnected:    root.bridgeAvailable()
                onSettingChanged: (key, value) => {
                    if (key === "__reset")
                        root.resetUiSettings()
                    else
                        root.updateUiSetting(key, value)
                }
                onOpenConfigFolderRequested: root.openConfigFolder()
            }

            AppAboutPage {
                id: aboutPage
                anchors.fill: parent
                visible: root.activeSection === 5
                Kirigami.Theme.backgroundColor:          root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor:                root.activeText
                Kirigami.Theme.disabledTextColor:        root.activeDisabledText
                Kirigami.Theme.highlightColor:           root.activeHighlight
                Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
                Kirigami.Theme.positiveTextColor:        root.activePositiveText
                Kirigami.Theme.negativeTextColor:        root.activeNegativeText
                Kirigami.Theme.neutralTextColor:         root.activeNeutralText
                appVersion: root.appVersion
                onNavigateRequested: section => root.activeSection = section
                onCreditsRequested: root.activeSection = 6
                onLicensesRequested: {
                    root.activeSection = 7
                    licensesPage.activeDocument = 0
                }
            }

            CreditsPage {
                id: creditsPage
                anchors.fill: parent
                visible: root.activeSection === 6
            }

            LicensesPage {
                id: licensesPage
                anchors.fill: parent
                visible: root.activeSection === 7
            }

            MergePage {
                id: mergePage
                anchors.fill: parent
                visible: root.activeSection === 8
                bridgeUrl:      root.bridgeUrl
                basePath:       root.mergeBasePath
                leftPath:       root.mergeLeftPath
                rightPath:      root.mergeRightPath
                outputPath:     root.mergeOutputPath
                activeBg:             root.activeBg
                activeBgAlt:          root.activeBgAlt
                activeText:           root.activeText
                activeDisabledText:   root.activeDisabledText
                activeHighlight:      root.activeHighlight
                activeNeutralText:    root.activeNeutralText
                activeNegativeText:   root.activeNegativeText
                activePositiveText:   root.activePositiveText
                separatorColor:       root.separatorColor
            }

            ImageComparePage {
                id: imageComparePage
                anchors.fill: parent
                visible: root.activeSection === 9
                Kirigami.Theme.backgroundColor:          root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor:                root.activeText
                Kirigami.Theme.disabledTextColor:        root.activeDisabledText
                Kirigami.Theme.highlightColor:           root.activeHighlight
                Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
                Kirigami.Theme.positiveTextColor:        root.activePositiveText
                Kirigami.Theme.negativeTextColor:        root.activeNegativeText
                Kirigami.Theme.neutralTextColor:         root.activeNeutralText
                bridgeUrl:          root.bridgeUrl
                activeBg:           root.activeBg
                activeBgAlt:        root.activeBgAlt
                activeText:         root.activeText
                activeDisabledText: root.activeDisabledText
                activeHighlight:    root.activeHighlight
                separatorColor:     root.separatorColor
                leftPath:           root.leftPath
                rightPath:          root.rightPath
                onSessionUpdated: context => root.applyLaunchContext(context, false)
            }

            WebpageComparePage {
                id: webpageComparePage
                anchors.fill: parent
                visible: root.activeSection === 10
                Kirigami.Theme.backgroundColor:          root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor:                root.activeText
                Kirigami.Theme.disabledTextColor:        root.activeDisabledText
                Kirigami.Theme.highlightColor:           root.activeHighlight
                Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
                Kirigami.Theme.positiveTextColor:        root.activePositiveText
                Kirigami.Theme.negativeTextColor:        root.activeNegativeText
                Kirigami.Theme.neutralTextColor:         root.activeNeutralText
                bridgeUrl:          root.bridgeUrl
                activeBg:           root.activeBg
                activeBgAlt:        root.activeBgAlt
                activeText:         root.activeText
                activeDisabledText: root.activeDisabledText
                activeHighlight:    root.activeHighlight
                separatorColor:     root.separatorColor
                onSessionUpdated: context => root.applyLaunchContext(context, false)
                Component.onCompleted: root.bridgeGet("/capabilities", function (ok, payload) {
                    if (ok && payload) {
                        webpageComparePage.webEngineAvailable = payload.web_engine === true
                        webpageComparePage.webRenderer =
                            (payload.web_renderer === "qml" || payload.web_renderer === "chromium")
                                ? payload.web_renderer : "none"
                    }
                })
            }

            DocumentComparePage {
                id: documentComparePage
                anchors.fill: parent
                visible: root.activeSection === 11
                Kirigami.Theme.backgroundColor:          root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor:                root.activeText
                Kirigami.Theme.disabledTextColor:        root.activeDisabledText
                Kirigami.Theme.highlightColor:           root.activeHighlight
                Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
                Kirigami.Theme.positiveTextColor:        root.activePositiveText
                Kirigami.Theme.negativeTextColor:        root.activeNegativeText
                Kirigami.Theme.neutralTextColor:         root.activeNeutralText
                bridgeUrl:          root.bridgeUrl
                activeBg:           root.activeBg
                activeBgAlt:        root.activeBgAlt
                activeText:         root.activeText
                activeDisabledText: root.activeDisabledText
                activeHighlight:    root.activeHighlight
                separatorColor:     root.separatorColor
                leftPath:           root.leftPath
                rightPath:          root.rightPath
                onSessionUpdated: context => root.applyLaunchContext(context, false)
            }
        }

    component PaneColumn: Rectangle {
        id: pane

        required property string sideName
        required property string sideKey
        required property color accentColor
        required property string pathText
        required property var rows
        property bool useBridgeModel: false
        property int modelRevision: 0
        property bool syntaxOverlayActive: root.compareMode === "Text"
            && root.syntaxMode !== "plain"
            && !(pane.editMode || root.rawTextInputActive())
        property bool editMode: false

        // External code (sibling pane sync) sets pane.contentY to mirror
        // scroll position. Internally we proxy to the ScrollView's inner
        // Flickable. (No alias because ScrollView.contentItem is a runtime
        // child, not a static reference.)
        property real contentY: 0
        onContentYChanged: {
            if (lineScroll && lineScroll.contentItem
                && lineScroll.contentItem.contentY !== contentY)
                lineScroll.contentItem.contentY = contentY
        }

        // Index of the topmost visible row, derived from the scroll offset
        // and the actual rendered line height. Exposed for the overview
        // ruler so its viewport indicator can show where we are in the file.
        property real topVisibleRow:
            lineScroll && lineScroll.contentItem && paneStack && paneStack.lineHeight > 0
                ? lineScroll.contentItem.contentY / paneStack.lineHeight
                : 0

        FontMetrics {
            id: paneFontMetrics
            font.family: root.paneFontFamily
            font.pixelSize: root.paneFontSize
        }

        function computeJoinedText() {
            if (!pane.rows || pane.rows.length === 0) return ""
            var parts = []
            for (var i = 0; i < pane.rows.length; i++) {
                var r = pane.rows[i]
                var s = r && r.text !== undefined && r.text !== null ? String(r.text) : ""
                parts.push(root.transformLineText(s))
            }
            return parts.join("\n")
        }

        function positionAtRow(row) {
            var inner = lineScroll ? lineScroll.contentItem : null
            if (!inner) return
            // Must use the TextArea's actual rendered line height (the same
            // metric the gutter is bound to), not FontMetrics.height — the
            // two can differ by a fraction of a pixel per row, which over
            // thousands of rows compounds into a meaningful scroll offset
            // and lands the jump on the wrong row.
            var lh = paneStack.lineHeight
            var target = Math.max(0, row)
            var y = target * lh
            var maxY = Math.max(0, inner.contentHeight - inner.height)
            inner.contentY = Math.max(0, Math.min(maxY, y - (inner.height - lh) / 2))
        }

        function loadRawContent() {
            if (pane.pathText === "") return
            const request = new XMLHttpRequest()
            request.onreadystatechange = function () {
                if (request.readyState === XMLHttpRequest.DONE && request.status === 200) {
                    var data = JSON.parse(request.responseText)
                    if (data && data.content !== undefined) {
                        contentArea.text = data.content
                    }
                }
            }
            request.open("GET", root.bridgeUrl + "/file/read?path=" + encodeURIComponent(pane.pathText))
            request.send()
        }

        function resetRowsModel() {
            if (root.rawTextInputActive()) return
            if (pane.editMode) loadRawContent()
            else contentArea.text = computeJoinedText()
        }

        onRowsChanged: {
            if (!pane.editMode)
                resetRowsModel()
            // Appending a fetched window must not yank the viewport to the top.
            if (!root.suppressTextScrollReset)
                scrollToTopTimer.restart()
        }
        onUseBridgeModelChanged: { if (!pane.editMode) resetRowsModel() }
        onModelRevisionChanged: { if (!pane.editMode) resetRowsModel() }
        onEditModeChanged: resetRowsModel()
        Component.onCompleted: resetRowsModel()

        Timer {
            id: scrollToTopTimer
            interval: 50
            repeat: false
            onTriggered: {
                if (lineScroll && lineScroll.contentItem)
                    lineScroll.contentItem.contentY = 0
            }
        }

        color: root.activeBg
        border.color: root.separatorColor

        // Mirror the ScrollView's inner Flickable contentY back onto the
        // pane property so the sibling pane sync (which sets
        // leftPane.contentY = rightPane.contentY) stays current as the
        // user scrolls via wheel, scrollbar, or keyboard navigation.
        Connections {
            target: lineScroll && lineScroll.contentItem ? lineScroll.contentItem : null
            ignoreUnknownSignals: true
            function onContentYChanged() {
                var cy = lineScroll.contentItem.contentY
                if (pane.contentY === cy) return
                pane.contentY = cy
                // Prefetch the next window of a large diff as we near the bottom
                // (left pane only, so a single fetch is issued per scroll).
                if (pane.sideKey === "left") {
                    if (root.compareMode === "Hex")
                        root.maybeLoadMoreHexRows(lineScroll.contentItem)
                    else
                        root.maybeLoadMoreTextRows(lineScroll.contentItem)
                }
                if (root.syncingScroll) return
                root.syncingScroll = true
                if (pane.sideKey === "left" && rightPane)
                    rightPane.contentY = cy
                else if (pane.sideKey === "right" && leftPane)
                    leftPane.contentY = cy
                root.syncingScroll = false
            }
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 34
                color: root.activeBgAlt
                border.color: root.separatorColor

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 8

                    Rectangle {
                        Layout.preferredWidth: 4
                        Layout.fillHeight: true
                        color: pane.accentColor
                    }

                    Controls.Label {
                        text: pane.sideName + (pane.sideName === "Left" && root.leftDirty ? " *" : pane.sideName === "Right" && root.rightDirty ? " *" : "")
                        font.bold: true
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        text: pane.pathText || "No file loaded"
                        elide: Text.ElideMiddle
                        color: root.activeDisabledText
                    }
                }
            }

            // Stacked editor: three layers + a focus-forwarding overlay.
            //   z=0   Per-row diff-color rectangles (mouse-transparent).
            //   z=1   ScrollView { TextArea } — owns all keyboard/mouse input.
            //   z=2   Line-number gutter + separator (mouse-transparent).
            //   z=9   MouseArea that captures clicks and calls
            //         contentArea.forceActiveFocus() — workaround for the
            //         Breeze-on-Qt-6.11 binding break that otherwise keeps
            //         the pane TextArea from receiving focus on click.
            //         propagateComposedEvents + mouse.accepted=false lets
            //         the press continue through to the TextArea so the
            //         cursor still lands at the click point.
            //
            // The underlay/gutter Columns translate by -paneStack.scrollY so
            // they scroll in lockstep with the TextArea inside ScrollView.
            Item {
                id: paneStack
                Layout.fillWidth: true
                Layout.fillHeight: true

                // Bind to the TextArea's own rendered line height so gutter
                // rectangles stay aligned with text lines. `cursorRectangle`
                // returns one-line-tall rect at the current caret position —
                // its height is the line height the TextEdit actually uses
                // for layout, which can differ from FontMetrics.height
                // depending on font/style/leading.
                property real lineHeight: contentArea && contentArea.cursorRectangle.height > 0
                    ? contentArea.cursorRectangle.height
                    : paneFontMetrics.height
                property int gutterWidth: root.showLineNumbers ? 48 : 0
                property int separatorWidth: root.showLineNumbers ? 1 : 0
                property int textLeftPadding: paneStack.gutterWidth + paneStack.separatorWidth + 4
                property real scrollY: lineScroll && lineScroll.contentItem ? lineScroll.contentItem.contentY : 0

                // Layer 1 (z=0): per-row diff backgrounds.
                Item {
                    anchors.fill: parent
                    clip: true
                    z: 0
                    visible: !pane.editMode

                    Column {
                        id: rowBackgrounds
                        x: paneStack.gutterWidth + paneStack.separatorWidth
                        y: -paneStack.scrollY
                        width: Math.max(paneStack.width - x, 0)
                        spacing: 0

                        Repeater {
                            model: pane.rows
                            delegate: Rectangle {
                                required property int index
                                required property var modelData

                                property string _state: modelData && modelData.state ? String(modelData.state) : "empty"
                                // "" when the row is not a difference; else the
                                // full-opacity change-bar color (a string the
                                // child Rectangle parses).
                                property string _markerColor: root.lineMarkerColor(_state)

                                width: rowBackgrounds.width
                                height: paneStack.lineHeight
                                color: {
                                    if (root.searchText !== "" && ((modelData && modelData.has_find_match) || index === root.currentSearchRow))
                                        return root.searchRowColor
                                    if ((modelData && modelData.bookmarked) || root.rowBookmarked(index))
                                        return root.bookmarkRowColor
                                    var st = _state
                                    if (st === "folded") return root.stateColors["skipped"]
                                    if (st === "left_only") return root.stateColors["left_only"]
                                    if (st === "right_only") return root.stateColors["right_only"]
                                    if (st === "changed") return root.stateColors["changed"]
                                    if (st === "skipped" || st === "aborted") return root.stateColors["skipped"]
                                    if (st === "error") return root.stateColors["error"]
                                    return root['zebra' + (index % 2)]
                                }
                                border.color: index === root.currentDiffRow ? root.activeHighlight : "transparent"
                                border.width: index === root.currentDiffRow ? 1 : 0

                                // Full-opacity change bar at the left edge of a
                                // diff row, so differences stay visible even
                                // when the faint background tint washes out
                                // under a high-contrast scheme (and as a cue
                                // that does not rely on reading the tint).
                                Rectangle {
                                    anchors.left: parent.left
                                    anchors.top: parent.top
                                    anchors.bottom: parent.bottom
                                    width: 3
                                    visible: parent._markerColor !== ""
                                    color: visible ? parent._markerColor : "transparent"
                                }
                            }
                        }
                    }
                }

                // Layer 2 (z=1): the editable text.
                Controls.ScrollView {
                    id: lineScroll
                    anchors.fill: parent
                    clip: true
                    z: 1

                    Controls.TextArea {
                        id: contentArea
                        readOnly: !(pane.editMode || root.rawTextInputActive())
                        font.family: root.paneFontFamily
                        font.pixelSize: root.paneFontSize
                        textFormat: Controls.TextArea.PlainText
                        color: pane.syntaxOverlayActive && !pane.editMode ? "transparent" : root.activeText
                        wrapMode: Controls.TextArea.NoWrap
                        selectByMouse: true
                        selectByKeyboard: true
                        persistentSelection: true
                        leftPadding: pane.editMode ? 8 : paneStack.textLeftPadding
                        rightPadding: 4
                        topPadding: 0
                        bottomPadding: 0
                        verticalAlignment: Controls.TextArea.AlignTop

                        background: Rectangle { color: "transparent" }

                        onTextChanged: {
                            if (pane.editMode) {
                                if (pane.sideKey === "left") {
                                    root.editLeftDirtyText = text
                                    root.leftDirty = true
                                } else {
                                    root.editRightDirtyText = text
                                    root.rightDirty = true
                                }
                            } else if (root.rawTextInputActive()) {
                                if (pane.sideKey === "left")
                                    root.leftPaneText = text
                                else
                                    root.rightPaneText = text
                            }
                        }

                        // Qt TextArea inside ScrollView doesn't translate
                        // PageUp/PageDown into view scrolling on its own.
                        // Move the cursor by one viewport (so it stays the
                        // active editor position) and the ScrollView follows.
                        Keys.onPressed: function(event) {
                            if (event.key !== Qt.Key_PageUp && event.key !== Qt.Key_PageDown)
                                return
                            var inner = lineScroll.contentItem
                            if (!inner) return
                            var dir = event.key === Qt.Key_PageDown ? 1 : -1
                            var pageDist = Math.max(paneStack.lineHeight,
                                                    inner.height - paneStack.lineHeight)
                            var curRect = contentArea.cursorRectangle
                            var targetY = curRect.y + dir * pageDist
                            targetY = Math.max(0, Math.min(contentArea.contentHeight - 1, targetY))
                            contentArea.cursorPosition =
                                contentArea.positionAt(curRect.x, targetY)
                            event.accepted = true
                        }
                    }
                }

                // Layer 2.5: syntax-coloured line text. The TextArea above
                // still owns selection, editing, copy, and keyboard behavior;
                // this overlay only paints the bridge-provided syntax spans.
                Item {
                    anchors.fill: parent
                    clip: true
                    z: 1.5
                    visible: pane.syntaxOverlayActive && !pane.editMode

                    Column {
                        id: syntaxColumn
                        x: paneStack.textLeftPadding
                        y: -paneStack.scrollY
                        width: Math.max(paneStack.width - x, 0)
                        spacing: 0

                        Repeater {
                            model: pane.rows
                            delegate: Text {
                                required property int index
                                required property var modelData

                                width: syntaxColumn.width
                                height: paneStack.lineHeight
                                text: root.syntaxRichTextForRow(modelData)
                                textFormat: Text.RichText
                                font.family: root.paneFontFamily
                                font.pixelSize: root.paneFontSize
                                verticalAlignment: Text.AlignVCenter
                                clip: true
                            }
                        }
                    }
                }

                // Layer 3 (z=2): line-number gutter + separator.
                Item {
                    anchors.fill: parent
                    clip: true
                    z: 2
                    visible: root.showLineNumbers && !pane.editMode

                    Column {
                        id: gutterColumn
                        x: 0
                        y: -paneStack.scrollY
                        width: paneStack.gutterWidth
                        spacing: 0

                        Repeater {
                            model: pane.rows
                            delegate: Rectangle {
                                required property int index
                                required property var modelData

                                width: gutterColumn.width
                                height: paneStack.lineHeight
                                color: root.activeBg

                                Controls.Label {
                                    anchors.fill: parent
                                    text: modelData && modelData.number !== undefined && modelData.number !== null ? String(modelData.number) : ""
                                    color: root.activeDisabledText
                                    font.family: root.paneFontFamily
                                    font.pixelSize: root.paneFontSize
                                    horizontalAlignment: Text.AlignRight
                                    verticalAlignment: Text.AlignVCenter
                                    rightPadding: 8
                                }
                            }
                        }
                    }

                    Rectangle {
                        x: paneStack.gutterWidth
                        y: 0
                        width: paneStack.separatorWidth
                        height: parent.height
                        color: root.separatorColor
                    }
                }

                // Focus-forwarding overlay (see comment above).
                MouseArea {
                    anchors.fill: parent
                    z: 9
                    propagateComposedEvents: true
                    onPressed: function(mouse) {
                        contentArea.forceActiveFocus()
                        mouse.accepted = false
                    }
                }
            }
        }
    }

    Timer {
        id: progressTimer
        interval: 200
        repeat: true
        onTriggered: {
            if (!root.comparing || !root.activeRequestId || !root.bridgeUrl) {
                progressTimer.stop()
                return
            }
            root.progressPollCount += 1
            if (root.progressPollCount > root.progressPollMax) {
                root.comparing = false
                root.activeRequestId = ""
                root.statusText = qsTr("Compare monitoring timed out")
                progressTimer.stop()
                return
            }
            var req = new XMLHttpRequest()
            req.onreadystatechange = function () {
                if (req.readyState === XMLHttpRequest.DONE && req.status === 200) {
                    var p = JSON.parse(req.responseText)
                    if (p) {
                        root.progressPhase = p.phase || "none"
                        root.progressCurrent = p.current || 0
                        root.progressTotal = p.total || 0
                        root.progressMessage = p.message || ""
                        if (root.progressTotal > 0) {
                            root.statusText = qsTr("Comparing %1/%2").arg(root.progressCurrent).arg(root.progressTotal)
                        } else if (root.progressMessage) {
                            root.statusText = qsTr("Comparing — %1").arg(root.progressMessage)
                        }
                    }
                }
            }
            req.open("GET", root.bridgeUrl + "/progress?id=" + encodeURIComponent(root.activeRequestId))
            req.send()
        }
    }

    onComparingChanged: {
        if (root.comparing) {
            root.progressPhase = "starting"
            root.progressCurrent = 0
            root.progressTotal = 0
            root.progressMessage = ""
            root.progressPollCount = 0
            progressTimer.start()
        } else {
            progressTimer.stop()
            root.progressPhase = "none"
            root.progressCurrent = 0
            root.progressTotal = 0
            root.progressMessage = ""
            root.progressPollCount = 0
        }
    }
}
