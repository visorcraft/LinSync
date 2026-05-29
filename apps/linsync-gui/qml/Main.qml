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
    property string leftPath: ""
    property string rightPath: ""
    property string compareMode: "Text"
    property string differenceText: "0 differences"
    property var summaryItems: []
    property var tabItems: []
    property var leftRows: makeBlankRows()
    property var rightRows: makeBlankRows()
    property var sessionBridge: null
    property var sessionState: ({})
    property int activeTabId: 0
    property bool syncingScroll: false
    property string bridgeUrl: ""
    property string pendingBrowseSide: "left"
    property var diffRowIndexes: []
    property int currentDiffPosition: -1
    property int currentDiffRow: -1
    property bool findVisible: false
    property string searchText: ""
    property var searchRowIndexes: []
    property int currentSearchPosition: -1
    property int currentSearchRow: -1
    property bool leftDirty: false
    property bool rightDirty: false
    property bool validationCompatible: false
    property string validationMessage: ""
    property string validationPathKind: ""
    property string appVersion: "1.7.0"
    property int bridgeModelRevision: 0
    property bool canUndo: false
    property bool canRedo: false
    property int pendingCloseTabId: 0
    property string pendingFolderOpKind: ""
    property var pendingFolderOpEntries: []

    // Three-way merge paths — set by the open-merge flow, then read by MergePage.
    property string mergeBasePath:  ""
    property string mergeLeftPath:  ""
    property string mergeRightPath: ""

    // -- Theming --------------------------------------------------------
    // The user-chosen Grex/Grexa theme integer. 0 follows the host palette;
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
    property bool   openLastSession:    true
    property bool   confirmOnClose:     true
    property bool   persistRecentPaths: true
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

    // Sync scroll between panes only when flick animation ends, not per-pixel.
    function isDifferenceState(state) {
        return state === "changed" || state === "left_only" || state === "right_only" || state === "error" || state === "aborted"
    }

    function rebuildDiffRows() {
        const rows = []
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

    // Scroll to the current-diff row.  Suppress scroll-sync during
    // programmatic positioning so the user's free-scroll isn't fought.
    function scrollToCurrentDifference() {
        if (root.currentDiffRow < 0)
            return
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

    function rowMatchesSearch(index) {
        if (root.searchText === "")
            return false

        const needle = root.searchText.toLocaleLowerCase()
        const leftText = root.leftRows[index] ? root.leftRows[index].text.toLocaleLowerCase() : ""
        const rightText = root.rightRows[index] ? root.rightRows[index].text.toLocaleLowerCase() : ""
        return leftText.indexOf(needle) >= 0 || rightText.indexOf(needle) >= 0
    }

    function rebuildSearchRows() {
        const rows = []
        for (let index = 0; index < root.leftRows.length; index++) {
            if (rowMatchesSearch(index))
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
        root.syncingScroll = true
        if (leftPane && rightPane) {
            leftPane.positionAtRow(root.currentSearchRow)
            rightPane.positionAtRow(root.currentSearchRow)
        }
        root.syncingScroll = false
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


    function hasSessionBridge() {
        return root.sessionBridge !== null && root.sessionBridge !== undefined
    }

    function sessionBridgeMemberName(name) {
        return name.replace(/([A-Z])/g, "_$1").toLowerCase()
    }

    function sessionBridgeCall(name, args) {
        if (!hasSessionBridge())
            return ""

        const snakeName = sessionBridgeMemberName(name)
        let method = root.sessionBridge[name]
        if (typeof method !== "function")
            method = root.sessionBridge[snakeName]
        if (typeof method !== "function")
            return ""
        return method.apply(root.sessionBridge, args || [])
    }

    function sessionBridgeProperty(name) {
        if (!hasSessionBridge())
            return undefined

        const value = root.sessionBridge[name]
        if (value !== undefined && value !== null)
            return value

        const snakeValue = root.sessionBridge[sessionBridgeMemberName(name)]
        if (snakeValue !== undefined && snakeValue !== null)
            return snakeValue
        return undefined
    }

    function sessionBridgeError(fallback) {
        const error = sessionBridgeProperty("lastError")
        if (error && error !== "")
            return error
        return fallback
    }

    function sessionBridgeValue(name, fallback, preferBridge) {
        if (preferBridge && hasSessionBridge()) {
            const value = sessionBridgeProperty(name)
            if (value !== undefined && value !== null)
                return value
        }
        return fallback
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
            "openLastSession": true,
            "confirmOnClose": true,
            "persistRecentPaths": true,
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
        root.openLastSession    = merged.openLastSession
        root.confirmOnClose     = merged.confirmOnClose
        root.persistRecentPaths = merged.persistRecentPaths
        root.maxRecentPaths     = merged.maxRecentPaths
        root.compareMode        = root.defaultCompareMode
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
        else if (key === "openLastSession")    root.openLastSession    = value
        else if (key === "confirmOnClose")     root.confirmOnClose     = value
        else if (key === "persistRecentPaths") root.persistRecentPaths = value
        else if (key === "maxRecentPaths")     root.maxRecentPaths     = value
    }

    function loadUiSettings() {
        if (hasSessionBridge()) {
            const json = sessionBridgeCall("loadSettings")
            if (json && json !== "")
                applyUiSettings(JSON.parse(json))
            return
        }
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
        if (hasSessionBridge()) {
            sessionBridgeCall("saveSetting", [key, String(value)])
            return
        }
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
        return hasSessionBridge() || root.bridgeUrl !== ""
    }

    function bridgeGet(path, onJson) {
        if (root.bridgeUrl === "")
            return false
        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE) {
                if (request.status >= 200 && request.status < 300) {
                    let payload = null
                    try { payload = JSON.parse(request.responseText) } catch (_e) {}
                    if (onJson) onJson(true, payload, request.status)
                } else {
                    if (onJson) onJson(false, null, request.status)
                }
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
        if (hasSessionBridge()) {
            // Bridge JS path: ask the in-process bridge to load recent and reopen
            const json = sessionBridgeCall("reopenRecentSession", [index])
            if (json && json !== "")
                applySessionContextJson(json)
            return
        }
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
        const row = root.leftRows[root.currentDiffRow] || root.rightRows[root.currentDiffRow]
        if (!row) return ""
        const id = String(row.row_id || "")
        if (id.indexOf("folder:") === 0)
            return id.substring("folder:".length)
        return String(row.text || "").replace(/\/$/, "")
    }

    function planFolderOp(kind, entries, callback) {
        const qs = "/folder/op/plan?kind=" + encodeURIComponent(kind)
                 + "&entries=" + encodeURIComponent((entries || []).join(","))
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
            folderOpDialog.open()
        })
    }

    function executeFolderOp(kind, entries, options, callback) {
        let qs = "/folder/op/execute?kind=" + encodeURIComponent(kind)
               + "&entries=" + encodeURIComponent((entries || []).join(","))
        if (options && options.new_name)
            qs += "&new_name=" + encodeURIComponent(options.new_name)
        bridgeGet(qs, function (ok, payload) {
            if (callback) callback(ok, payload)
        })
    }

    function resetUiSettings() {
        if (hasSessionBridge()) {
            const json = sessionBridgeCall("resetSettings")
            if (json && json !== "")
                applyUiSettings(JSON.parse(json))
            return
        }
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
        if (!preferBridge || !hasSessionBridge())
            return fallback

        const count = sessionBridgeCall("summaryCount")
        if (count <= 0)
            return fallback

        const items = []
        for (let index = 0; index < count; index++)
            items.push({
                "label": sessionBridgeCall("summaryLabelAt", [index]),
                "value": sessionBridgeCall("summaryValueAt", [index])
            })
        return items
    }

    function recentPathsFromBridge(fallback, preferBridge) {
        // Honour the user's persistRecentPaths toggle — when off we just
        // surface an empty list so nothing leaks into the Sessions page
        // or onto disk via the bridge.
        if (!root.persistRecentPaths)
            return []

        let items = fallback
        if (preferBridge && hasSessionBridge()) {
            const count = sessionBridgeCall("recentPathCount")
            if (count > 0) {
                items = []
                for (let index = 0; index < count; index++)
                    items.push(sessionBridgeCall("recentPathAt", [index]))
            }
        }
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
        if (!preferBridge || !hasSessionBridge())
            return fallback

        const count = sessionBridgeValue("tabCount", fallback.length, true)
        if (count <= 0)
            return fallback

        const items = []
        for (let index = 0; index < count; index++) {
            const id = sessionBridgeCall("tabIdAt", [index])
            items.push({
                "id": id,
                "title": sessionBridgeCall("tabTitleAt", [index]) || "Compare",
                "dirty": sessionBridgeCall("tabDirtyAt", [index]),
                "can_undo": sessionBridgeCall("tabCanUndoAt", [index]),
                "can_redo": sessionBridgeCall("tabCanRedoAt", [index])
            })
        }
        return items
    }

    function applySessionContextJson(contextJson) {
        if (!contextJson || contextJson === "") {
            root.statusText = sessionBridgeError("Session bridge returned no state")
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
        root.activeTabId = sessionBridgeValue("activeTabId", activeTab.id || 0, preferBridge)
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
    }

    function applySessionTab(tab, preferBridge) {
        if (!tab)
            return

        root.activeTabId = sessionBridgeValue("activeTabId", tab.id || 0, preferBridge)
        root.leftPath = sessionBridgeValue("leftPath", tab.left_path || "", preferBridge)
        root.rightPath = sessionBridgeValue("rightPath", tab.right_path || "", preferBridge)
        root.compareMode = sessionBridgeValue("compareMode", tab.mode || "Text", preferBridge)
        root.statusText = sessionBridgeValue("status", tab.status || "Ready", preferBridge)
        root.summaryItems = summaryItemsFromBridge(tab.summary || [], preferBridge)
        const fallbackLeftRows = tab.left_rows && tab.left_rows.length > 0 ? tab.left_rows : makeBlankRows()
        const fallbackRightRows = tab.right_rows && tab.right_rows.length > 0 ? tab.right_rows : makeBlankRows()
        root.leftRows = fallbackLeftRows
        root.rightRows = fallbackRightRows
        if (preferBridge && hasSessionBridge())
            root.bridgeModelRevision += 1
        const validation = tab.validation || {}
        root.validationCompatible = sessionBridgeValue("validationCompatible", validation.compatible || false, preferBridge)
        root.validationMessage = sessionBridgeValue("validationMessage", validation.message || "", preferBridge)
        root.validationPathKind = sessionBridgeValue("validationPathKind", validation.path_kind || "", preferBridge)
        const count = sessionBridgeValue("differenceCount", tab.difference_count || 0, preferBridge)
        setDifferenceCount(count)
        const modeIndex = modeSelector.model.indexOf(root.compareMode)
        modeSelector.currentIndex = modeIndex >= 0 ? modeIndex : 0
        root.leftDirty = sessionBridgeValue("leftDirty", tab.left_dirty || false, preferBridge)
        root.rightDirty = sessionBridgeValue("rightDirty", tab.right_dirty || false, preferBridge)
        root.canUndo = sessionBridgeValue("canUndo", tab.can_undo || false, preferBridge)
        root.canRedo = sessionBridgeValue("canRedo", tab.can_redo || false, preferBridge)
        rebuildDiffRows()
        rebuildSearchRows()
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

        if (hasSessionBridge()) {
            applySessionContextJson(sessionBridgeCall("activateTab", [tabId]))
            return
        }

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
                tab.status = root.statusText
                tab.difference_count = currentDifferenceCount()
                tab.left_dirty = root.leftDirty
                tab.right_dirty = root.rightDirty
                tab.can_undo = root.canUndo
                tab.can_redo = root.canRedo
                tab.summary = root.summaryItems
                tab.left_rows = root.leftRows
                tab.right_rows = root.rightRows
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

        if (hasSessionBridge()) {
            applySessionContextJson(sessionBridgeCall("closeTab", [tabId]))
            return
        }

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

        if (hasSessionBridge()) {
            root.statusText = "Saving"
            const contextJson = sessionBridgeCall("saveSide", ["dirty"])
            if (contextJson && contextJson !== "") {
                applySessionContextJson(contextJson)
                performCloseTab(tabId)
            } else {
                root.statusText = sessionBridgeError("Save failed")
            }
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

        if (hasSessionBridge()) {
            root.statusText = "Saving"
            applySessionContextJson(sessionBridgeCall("saveSide", ["dirty"]))
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

        if (hasSessionBridge()) {
            root.statusText = "Undoing"
            applySessionContextJson(sessionBridgeCall("undo"))
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

        if (hasSessionBridge()) {
            root.statusText = "Redoing"
            applySessionContextJson(sessionBridgeCall("redo"))
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

    function requestCompare(newTab) {
        if (hasSessionBridge()) {
            if (root.leftPath === "" || root.rightPath === "") {
                root.statusText = "Select two paths"
                return
            }

            root.statusText = "Comparing"
            applySessionContextJson(sessionBridgeCall("comparePaths", [root.leftPath, root.rightPath, root.compareMode, newTab]))
            return
        }

        if (root.bridgeUrl === "") {
            root.statusText = "Compare bridge unavailable"
            return
        }

        if (root.leftPath === "" || root.rightPath === "") {
            root.statusText = "Select two paths"
            return
        }

        root.statusText = "Comparing"
        const request = new XMLHttpRequest()
        let url = root.bridgeUrl + "/compare?left=" + encodeURIComponent(root.leftPath) + "&right=" + encodeURIComponent(root.rightPath)
        url += "&mode=" + encodeURIComponent(root.compareMode)
        // Surface every compare-related setting on the wire even if the
        // current Rust bridge only consumes a subset. Unknown query
        // params are ignored server-side; getting them in the URL means
        // a future bridge build can opt in without QML changes.
        url += "&ignore_case=" + (root.ignoreCase ? "1" : "0")
        url += "&ignore_whitespace=" + (root.ignoreWhitespace ? "1" : "0")
        url += "&ignore_blank_lines=" + (root.ignoreBlankLines ? "1" : "0")
        url += "&ignore_eol=" + (root.ignoreEol ? "1" : "0")
        url += "&eol=" + encodeURIComponent(root.eolNormalization)
        if (newTab)
            url += "&new_tab=1"
        request.onreadystatechange = function () {
            if (request.readyState === XMLHttpRequest.DONE) {
                if (request.status === 200) {
                    applyLaunchContext(JSON.parse(request.responseText), false)
                } else {
                    root.statusText = "Compare failed"
                }
            }
        }
        request.open("GET", url)
        request.send()
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

    function copyCurrentDifference(direction) {
        if (root.currentDiffRow < 0)
            return

        if (hasSessionBridge()) {
            root.statusText = "Applying copy"
            applySessionContextJson(sessionBridgeCall("copyCurrentRow", [root.currentDiffRow, direction]))
            return
        }

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

        if (hasSessionBridge()) {
            root.statusText = "Applying copy"
            applySessionContextJson(sessionBridgeCall("copyAll", [direction]))
            return
        }

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
                        model: ["Text", "Folder", "Table", "Hex"]
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
        modal: true
        title: qsTr("Run folder operation?")
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
                onClicked: {
                    folderOpDialog.close()
                    root.executeFolderOp(root.pendingFolderOpKind, root.pendingFolderOpEntries, {}, function (ok, payload) {
                        if (ok && payload) {
                            const summary = payload.summary || {}
                            root.statusText = qsTr("Folder op done: %1 succeeded / %2 failed of %3")
                                .arg(summary.succeeded || 0)
                                .arg(summary.failed || 0)
                                .arg(summary.total || 0)
                            root.requestCompare(false)
                        } else {
                            root.statusText = "Folder op execute failed"
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
                enabled: root.hasSessionBridge() || root.bridgeUrl !== ""
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
        Behavior on width { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }

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
                onTriggered: root.activeSection = 0
            }
            LinSyncNavItem {
                label: qsTr("Image Compare")
                iconName: "image-compare"
                active: root.activeSection === 9
                collapsed: drawer.sidebarCollapsed
                onTriggered: root.activeSection = 9
            }
            LinSyncNavItem {
                label: qsTr("Webpage Compare")
                iconName: "internet-web-browser-symbolic"
                active: root.activeSection === 10
                collapsed: drawer.sidebarCollapsed
                onTriggered: root.activeSection = 10
            }
            LinSyncNavItem {
                label: qsTr("Document Compare")
                iconName: "document-open"
                active: root.activeSection === 11
                collapsed: drawer.sidebarCollapsed
                onTriggered: root.activeSection = 11
            }
            LinSyncNavItem {
                label: qsTr("Sessions")
                iconName: "view-history"
                active: root.activeSection === 1
                collapsed: drawer.sidebarCollapsed
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
                onTriggered: root.activeSection = 2
            }
            LinSyncNavItem {
                label: qsTr("Plugins")
                iconName: "preferences-plugin"
                active: root.activeSection === 3
                collapsed: drawer.sidebarCollapsed
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
                onTriggered: root.activeSection = 4
            }
            LinSyncNavItem {
                label: qsTr("About")
                iconName: "help-about"
                active: root.activeSection === 5
                collapsed: drawer.sidebarCollapsed
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

        readonly property var sectionTitles: ["Compare", "Sessions", "Filters", "Plugins", "Settings", "About", "Credits", "Licenses", "Three-way Merge", "Image Compare", "Document Compare"]
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
                    onClicked: {
                        if ((root.leftDirty || root.rightDirty) && root.confirmOnClose)
                            reloadDirtyDialog.open()
                        else
                            root.requestCompare(false)
                    }
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

        StackLayout {
            id: sectionStack
            anchors.fill: parent
            currentIndex: root.activeSection

            ColumnLayout {
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
                        model: ["Text", "Folder", "Table", "Hex"]
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
                        implicitHeight: 36
                        Layout.fillWidth: true
                        text: root.leftPath
                        placeholderText: "Left path"
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
                        // Disabled in 1.1.x: GUI cancellation is not wired
                        // through the bridge yet. Core has cancel hooks
                        // (linsync_core::folder, linsync_core::merge); the
                        // bridge needs a /cancel endpoint plus per-request
                        // tokens. Tracked in PLAN.md Phase 3.
                        enabled: false
                        Controls.ToolTip.text: qsTr("Cancellation is not wired yet — see PLAN.md Phase 3.")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Stop (not implemented)")
                    }

                    Kirigami.Separator {
                        Layout.fillHeight: true
                    }

                    Controls.ToolButton {
                        icon.name: "vcs-merge"
                        icon.color: root.activeText
                        Controls.ToolTip.text: qsTr("Open three-way merge…")
                        Controls.ToolTip.visible: hovered
                        Accessible.name: qsTr("Open three-way merge")
                        onClicked: openMergeDialog.startFlow()
                    }
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

            Controls.SplitView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: Qt.Horizontal

                PaneColumn {
                    id: leftPane

                    Controls.SplitView.fillWidth: true
                    Controls.SplitView.minimumWidth: 320
                    sideName: "Left"
                    sideKey: "left"
                    accentColor: root.activeNeutralText
                    pathText: root.leftPath
                    rows: root.leftRows
                    useBridgeModel: root.hasSessionBridge()
                    modelRevision: root.bridgeModelRevision
                }

                PaneColumn {
                    id: rightPane

                    Controls.SplitView.fillWidth: true
                    Controls.SplitView.minimumWidth: 320
                    sideName: "Right"
                    sideKey: "right"
                    accentColor: root.activePositiveText
                    pathText: root.rightPath
                    rows: root.rightRows
                    useBridgeModel: root.hasSessionBridge()
                    modelRevision: root.bridgeModelRevision
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
                        property int totalRows: root.leftRows.length
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

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 28
                color: root.activeBgAlt
                border.color: root.separatorColor

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 10
                    spacing: 16

                    Controls.Label { text: root.statusText; color: root.activeText }
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
                        text: "Tabs: " + sessionBridgeValue("tabCount", root.sessionState.tabs ? root.sessionState.tabs.length : 0, hasSessionBridge())
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
                Kirigami.Theme.backgroundColor:          root.activeBg
                Kirigami.Theme.alternateBackgroundColor: root.activeBgAlt
                Kirigami.Theme.textColor:                root.activeText
                Kirigami.Theme.disabledTextColor:        root.activeDisabledText
                Kirigami.Theme.highlightColor:           root.activeHighlight
                Kirigami.Theme.highlightedTextColor:     root.activeHighlightedText
                Kirigami.Theme.positiveTextColor:        root.activePositiveText
                Kirigami.Theme.negativeTextColor:        root.activeNegativeText
                Kirigami.Theme.neutralTextColor:         root.activeNeutralText
                sessionState: root.sessionState
                activeTabId: root.activeTabId
                onTabActivated: tabId => root.activateSessionTab(tabId)
                onTabClosed: tabId => root.performCloseTab(tabId)
                onNavigateRequested: section => root.activeSection = section
                onReopenRecentRequested: index => root.reopenRecentSession(index)
                onRefreshRecentRequested: {
                    root.loadRecentSessions(function (items) {
                        sessionsPage.recentSessions = items
                    })
                }
            }

            FiltersPage {
                id: filtersPage
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
                openLastSession:    root.openLastSession
                confirmOnClose:     root.confirmOnClose
                persistRecentPaths: root.persistRecentPaths
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

            AboutPage {
                id: aboutPage
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
            }

            LicensesPage {
                id: licensesPage
            }

            MergePage {
                id: mergePage
                bridgeUrl:      root.bridgeUrl
                basePath:       root.mergeBasePath
                leftPath:       root.mergeLeftPath
                rightPath:      root.mergeRightPath
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
            }

            WebpageComparePage {
                id: webpageComparePage
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
            }

            DocumentComparePage {
                id: documentComparePage
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
            }
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

        function resetRowsModel() {
            contentArea.text = computeJoinedText()
        }

        onRowsChanged: {
            resetRowsModel()
            scrollToTopTimer.restart()
        }
        onUseBridgeModelChanged: resetRowsModel()
        onModelRevisionChanged: resetRowsModel()
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

                                width: rowBackgrounds.width
                                height: paneStack.lineHeight
                                color: {
                                    var st = _state
                                    if (st === "left_only") return root.stateColors["left_only"]
                                    if (st === "right_only") return root.stateColors["right_only"]
                                    if (st === "changed") return root.stateColors["changed"]
                                    if (st === "skipped" || st === "aborted") return root.stateColors["skipped"]
                                    if (st === "error") return root.stateColors["error"]
                                    return root['zebra' + (index % 2)]
                                }
                                border.color: index === root.currentDiffRow ? root.activeHighlight : "transparent"
                                border.width: index === root.currentDiffRow ? 1 : 0
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
                        readOnly: false
                        font.family: root.paneFontFamily
                        font.pixelSize: root.paneFontSize
                        textFormat: Controls.TextArea.PlainText
                        color: root.activeText
                        wrapMode: Controls.TextArea.NoWrap
                        selectByMouse: true
                        selectByKeyboard: true
                        persistentSelection: true
                        leftPadding: paneStack.textLeftPadding
                        rightPadding: 4
                        topPadding: 0
                        bottomPadding: 0
                        verticalAlignment: Controls.TextArea.AlignTop

                        background: Rectangle { color: "transparent" }

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

                // Layer 3 (z=2): line-number gutter + separator.
                Item {
                    anchors.fill: parent
                    clip: true
                    z: 2
                    visible: root.showLineNumbers

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
}
