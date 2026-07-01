// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// WebpageComparePage — fetch and compare two URLs in default-build sub-modes:
//   html (raw HTML source), text (extracted text), tree (resource tree).
//
// All network fetches are opt-in: the user must click "Compare…" and then
// confirm in the dialog before any request is made.
//
// Root is a Controls.Pane so QQC2 Controls inside inherit the
// ApplicationWindow root's QPalette through the standard inheritance chain.
// A plain Item root broke that chain and the widgets stayed dark in Light
// mode regardless of the palette set on the window root.
Controls.Pane {
    id: root
    padding: 0
    background: Rectangle { color: root.activeBg }

    // ── External interface ────────────────────────────────────────────────────
    required property string bridgeUrl
    required property color activeBg
    required property color activeBgAlt
    required property color activeText
    required property color activeDisabledText
    required property color activeHighlight
    required property color separatorColor
    signal sessionUpdated(var context)

    // ── Internal state ────────────────────────────────────────────────────────
    property string leftUrl: ""
    property string rightUrl: ""
    property string subMode: "html"
    property bool busy: false
    // True when the binary was built with Qt WebEngine (set from /capabilities);
    // gates the rendered/screenshot modes.
    property bool webEngineAvailable: false
    // Which renderer backend rendered/screenshot would use on this host
    // ("qml" | "chromium" | "none"; set from /capabilities web_renderer).
    // "none" means the build supports rendering but no usable renderer binary
    // was found, so the rendered/screenshot modes stay hidden.
    property string webRenderer: "none"
    property string resultSummary: ""
    property var resultRows: []
    property bool resultEqual: false
    property bool resultTruncated: false
    property bool resultError: false
    // Resource-tree entries [{path, state, leftSize, rightSize}] for tree mode,
    // and a free-text path filter over them.
    property var resourceEntries: []
    property string resourceFilter: ""
    // Left/right rows derived from resultRows, in the {text,state,number}
    // shape the DiffEditorPane expects.
    property var leftDiffRows: []
    property var rightDiffRows: []
    property var diffRowIndexes: []    // row indexes that differ (for the ruler)
    property bool _syncingScroll: false
    property int requestCounter: 0
    property string activeRequestId: ""
    property int progressCurrent: 0
    property int progressTotal: 0
    property string progressMessage: ""
    property bool pendingNewTab: false
    // Defensive iteration cap for the progress timer so a compare whose XHR
    // never completes (bridge worker wedged, plugin hang, lost response)
    // cannot poll /progress forever and saturate the bridge connection pool.
    // At 200 ms intervals, 1500 iterations == 5 minutes. Mirrors Main.qml's
    // progressPollMax safety net.
    property int progressPollCount: 0
    readonly property int progressPollMax: 1500

    // Per-row diff tints over the page background (left=red, right=green,
    // changed=amber); "equal" rows are absent → transparent.
    readonly property var diffStateColors: ({
        "left_only": Kirigami.ColorUtils.tintWithAlpha(root.activeBg, Kirigami.Theme.negativeTextColor, 0.22),
        "right_only": Kirigami.ColorUtils.tintWithAlpha(root.activeBg, Kirigami.Theme.positiveTextColor, 0.22),
        "changed": Kirigami.ColorUtils.tintWithAlpha(root.activeBg, Kirigami.Theme.neutralTextColor, 0.22)
    })

    function rebuildDiffRows() {
        var L = [];
        var R = [];
        var idx = [];
        for (var i = 0; i < root.resultRows.length; i++) {
            var r = root.resultRows[i];
            var ls = r.s === "left_only" ? "left_only" : (r.s === "changed" ? "changed" : "equal");
            var rs = r.s === "right_only" ? "right_only" : (r.s === "changed" ? "changed" : "equal");
            L.push({ "text": r.l, "state": ls, "number": r.ln });
            R.push({ "text": r.r, "state": rs, "number": r.rn });
            if (r.s !== "equal")
                idx.push(i);
        }
        root.leftDiffRows = L;
        root.rightDiffRows = R;
        root.diffRowIndexes = idx;
    }

    // ── Bridge helper ─────────────────────────────────────────────────────────
    function bridgeGet(path, onLoad) {
        if (root.bridgeUrl === "") {
            if (onLoad)
                onLoad(false, null);
            return;
        }
        const xhr = new XMLHttpRequest();
        xhr.onreadystatechange = function () {
            if (xhr.readyState === XMLHttpRequest.DONE) {
                const ok = xhr.status >= 200 && xhr.status < 300;
                let payload = null;
                try {
                    payload = JSON.parse(xhr.responseText);
                } catch (_) {
                }
                if (onLoad)
                    onLoad(ok, payload);
            }
        };
        xhr.open("GET", root.bridgeUrl + path);
        xhr.send();
    }

    function runCompare() {
        root.busy = true;
        root.resultSummary = "";
        root.resultRows = [];
        root.leftDiffRows = [];
        root.rightDiffRows = [];
        root.diffRowIndexes = [];
        root.resultEqual = false;
        root.resultTruncated = false;
        root.resultError = false;
        root.requestCounter += 1;
        root.activeRequestId = "web-" + root.requestCounter;
        root.progressCurrent = 0;
        root.progressTotal = 0;
        root.progressMessage = "";
        root.progressPollCount = 0;
        const left = encodeURIComponent(root.leftUrl);
        const right = encodeURIComponent(root.rightUrl);
        const mode = encodeURIComponent(root.subMode);
        var query = "/compare/webpage?left=" + left + "&right=" + right + "&mode=" + mode + "&request_id=" + encodeURIComponent(root.activeRequestId) + "&confirmed=1";
        if (root.pendingNewTab)
            query += "&new_tab=1";
        root.bridgeGet(query, function (ok, payload) {
            root.busy = false;
            root.activeRequestId = "";
            root.pendingNewTab = false;
            if (!ok || !payload) {
                root.resultError = true;
                root.resultSummary = qsTr("Error: bridge request failed");
                return;
            }
            if (payload.error) {
                root.resultError = true;
                root.resultSummary = qsTr("Error: %1").arg(payload.error);
                return;
            }
            root.resultSummary = payload.summary ?? qsTr("Compare complete");
            root.resultEqual = payload.equal === true;
            root.resultTruncated = payload.truncated === true;
            root.resultRows = payload.rows ?? [];
            root.resourceEntries = payload.entries ?? [];
            if (payload.session)
                root.sessionUpdated(payload);
            root.rebuildDiffRows();
        });
    }

    function startFromMain(left, right, newTab) {
        root.leftUrl = left;
        root.rightUrl = right;
        root.pendingNewTab = !!newTab;
        confirmDialog.open();
    }

    // Resource entries filtered by the path search and sorted by path. The diff
    // state ("Different"/"LeftOnly"/"RightOnly"/...) comes straight from the
    // core FolderEntryState serialization.
    function filteredResourceEntries() {
        var needle = root.resourceFilter.toLowerCase();
        var out = (root.resourceEntries || []).filter(function (e) {
            return needle === "" || String(e.path || "").toLowerCase().indexOf(needle) !== -1;
        });
        out.sort(function (a, b) { return String(a.path).localeCompare(String(b.path)); });
        return out;
    }

    function resourceStateColor(state) {
        switch (String(state)) {
            case "LeftOnly":  return Kirigami.Theme.negativeTextColor;
            case "RightOnly": return Kirigami.Theme.positiveTextColor;
            case "Different": return Kirigami.Theme.neutralTextColor;
            default:          return root.activeDisabledText;
        }
    }

    Timer {
        id: progressTimer
        interval: 200
        repeat: true
        running: root.busy && root.activeRequestId !== ""
        onTriggered: {
            if (!root.busy || root.activeRequestId === "") {
                progressTimer.stop()
                return
            }
            // If the compare XHR never reports back, stop polling, cancel the
            // request on the bridge (so its worker slot is freed), and recover
            // the UI instead of polling forever and exhausting the connection
            // pool — the "app gets slow until restarted" failure mode.
            root.progressPollCount += 1
            if (root.progressPollCount > root.progressPollMax) {
                root.bridgeGet("/cancel?id=" + encodeURIComponent(root.activeRequestId), function () {})
                root.busy = false
                root.activeRequestId = ""
                root.pendingNewTab = false
                root.resultError = true
                root.resultSummary = qsTr("Compare timed out — no response from the bridge")
                progressTimer.stop()
                return
            }
            root.bridgeGet("/progress?id=" + encodeURIComponent(root.activeRequestId), function (ok, data) {
                if (!ok || !data)
                    return;
                root.progressCurrent = data.current || 0;
                root.progressTotal = data.total || 0;
                root.progressMessage = data.message || "";
                if (root.progressTotal > 0)
                    root.resultSummary = qsTr("Comparing %1/%2").arg(root.progressCurrent).arg(root.progressTotal);
                else if (root.progressMessage !== "")
                    root.resultSummary = root.progressMessage;
            });
        }
    }

    function clearCache() {
        root.bridgeGet("/compare/webpage/clear-cache", function (ok, payload) {
            if (ok)
                root.resultSummary = qsTr("Webcompare cache cleared");
        });
    }

    // ── Layout ────────────────────────────────────────────────────────────────
    // A label-column helper so every form row lines up regardless of label
    // length. Width is tuned to the longest label on the page.
    component FieldLabel: Controls.Label {
        Layout.preferredWidth: 96
        Layout.alignment: Qt.AlignVCenter
        color: root.activeText
        font.pixelSize: 12
        horizontalAlignment: Text.AlignRight
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Privacy notice — always visible on this page.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            Layout.margins: 8
            type: Kirigami.MessageType.Warning
            text: qsTr("Webpage compare fetches content from the internet. Third-party resources on each page may also be requested.")
            visible: true
        }

        // ── Toolbar: pages to compare ──────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 54
            color: root.activeBg
            border.color: root.separatorColor
            border.width: 1

            UrlInputBar {
                anchors.fill: parent
                anchors.margins: 8
                leftLabel: qsTr("Left URL")
                rightLabel: qsTr("Right URL")
                leftText: root.leftUrl
                rightText: root.rightUrl
                actionText: root.busy ? qsTr("Comparing…") : qsTr("Compare…")
                actionAccessibleName: qsTr("Compare")
                actionIcon: "internet-web-browser-symbolic"
                actionEnabled: root.leftUrl.length > 0 && root.rightUrl.length > 0 && !root.busy
                textColor: root.activeText
                disabledTextColor: root.activeDisabledText
                fieldColor: root.activeBg
                borderColor: root.separatorColor
                onLeftTextEdited: function (text) { root.leftUrl = text }
                onRightTextEdited: function (text) { root.rightUrl = text }
                onActionActivated: confirmDialog.open()
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: root.activeBgAlt
            border.color: root.separatorColor
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 8

                Controls.Label {
                    text: qsTr("Mode:")
                    color: root.activeText
                    opacity: 0.7
                    font.pixelSize: 12
                }

                AppComboBox {
                    id: subModeCombo
                    Layout.preferredWidth: 280
                    implicitHeight: 30
                    // Rendered/screenshot need the web-engine build AND a
                    // usable renderer binary on this host; offered only when
                    // /capabilities reports both (see Main.qml).
                    model: root.webEngineAvailable && root.webRenderer !== "none"
                        ? [
                            { text: qsTr("HTML source"), value: "html" },
                            { text: qsTr("Extracted text"), value: "text" },
                            { text: qsTr("Resource tree"), value: "tree" },
                            { text: qsTr("Rendered (pixels)"), value: "rendered" },
                            { text: qsTr("Screenshot"), value: "screenshot" }
                        ]
                        : [
                            { text: qsTr("HTML source"), value: "html" },
                            { text: qsTr("Extracted text"), value: "text" },
                            { text: qsTr("Resource tree"), value: "tree" }
                        ]
                    textRole: "text"
                    valueRole: "value"
                    onActivated: {
                        root.subMode = currentValue
                    }
                    Accessible.name: qsTr("Compare mode")
                }

                // Backend tag: rendered/screenshot fall back to a headless
                // Chromium browser when no Qt WebEngine QML runner is found.
                Controls.Label {
                    visible: root.webEngineAvailable && root.webRenderer === "chromium"
                    text: qsTr("via Chromium")
                    color: root.activeText
                    opacity: 0.7
                    font.pixelSize: 11
                    Accessible.name: qsTr("Rendered and screenshot modes use a headless Chromium browser")
                    Controls.ToolTip.text: qsTr("Rendered and screenshot modes use a headless Chromium browser on this system; rendered output may differ from Qt WebEngine.")
                    Controls.ToolTip.visible: chromiumTagHover.hovered
                    Controls.ToolTip.delay: 300
                    HoverHandler { id: chromiumTagHover }
                }

                // Renderer-unavailable hint: web-engine build, but neither a
                // QML runner nor a Chromium binary was found on this host.
                Controls.Label {
                    visible: root.webEngineAvailable && root.webRenderer === "none"
                    text: qsTr("Rendered modes unavailable — no QML runner or Chromium found")
                    color: Kirigami.Theme.neutralTextColor
                    font.pixelSize: 11
                    Accessible.name: text
                }

                AppButton {
                    Layout.preferredHeight: 30
                    text: qsTr("Clear webcompare cache")
                    onClicked: root.clearCache()
                    icon.name: "edit-clear-symbolic"
                    Accessible.name: qsTr("Clear webcompare cache")
                }

                Item { Layout.fillWidth: true }

                Controls.BusyIndicator {
                    running: root.busy
                    visible: root.busy
                    Layout.preferredWidth: 24
                    Layout.preferredHeight: 24
                }

                Controls.ProgressBar {
                    visible: root.busy && root.progressTotal > 0
                    from: 0
                    to: root.progressTotal
                    value: root.progressCurrent
                    Layout.preferredWidth: 120
                    Layout.preferredHeight: 16
                }
            }
        }

        // ── Card: result ───────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: 8
            color: root.activeBgAlt
            border.color: root.separatorColor
            border.width: 1
            clip: true

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 14
                spacing: 8

                // Header: title + result summary chip.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Controls.Label {
                        text: qsTr("Result")
                        color: root.activeText
                        font.pixelSize: 14
                        font.bold: true
                    }
                    Item { Layout.fillWidth: true }
                    Controls.Label {
                        visible: !root.busy && root.resultSummary.length > 0
                        text: root.resultSummary
                        font.bold: true
                        color: root.resultError
                            ? Kirigami.Theme.negativeTextColor
                            : (root.resultEqual ? Kirigami.Theme.positiveTextColor : root.activeText)
                        elide: Text.ElideRight
                        Layout.maximumWidth: parent.width * 0.7
                    }
                }

                Controls.Label {
                    visible: root.resultTruncated
                    Layout.fillWidth: true
                    text: qsTr("Output truncated — showing the first 4000 lines.")
                    color: Kirigami.Theme.neutralTextColor
                    font.pixelSize: 11
                }

                // ── Side-by-side editor diff ──────────────────────────────────
                // Two real editor panes (TextArea-based, same as the main
                // Compare page): click-to-focus, multi-row drag-select, arrow
                // keys, Home/End, PageUp/PageDown, Ctrl+C — with diff-colour
                // underlay, line-number gutters, and synchronised scrolling.
                RowLayout {
                    visible: !root.busy && root.leftDiffRows.length > 0
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: 2

                    DiffEditorPane {
                        id: leftDiffPane
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        heading: qsTr("Left")
                        accentColor: Kirigami.Theme.neutralTextColor
                        rows: root.leftDiffRows
                        stateColors: root.diffStateColors
                        activeBg: root.activeBg
                        activeBgAlt: root.activeBgAlt
                        activeText: root.activeText
                        activeDisabledText: root.activeDisabledText
                        separatorColor: root.separatorColor
                        highlightColor: root.activeHighlight
                    }

                    DiffEditorPane {
                        id: rightDiffPane
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        heading: qsTr("Right")
                        accentColor: Kirigami.Theme.positiveTextColor
                        rows: root.rightDiffRows
                        stateColors: root.diffStateColors
                        activeBg: root.activeBg
                        activeBgAlt: root.activeBgAlt
                        activeText: root.activeText
                        activeDisabledText: root.activeDisabledText
                        separatorColor: root.separatorColor
                        highlightColor: root.activeHighlight
                    }

                    // Far-right diff-overview ruler: marks every differing row
                    // and a draggable viewport indicator that scrolls both panes.
                    DiffOverviewRuler {
                        Layout.preferredWidth: 36
                        Layout.fillHeight: true
                        diffRows: root.diffRowIndexes
                        totalRows: root.leftDiffRows.length
                        markColor: Kirigami.Theme.negativeTextColor
                        highlightColor: root.activeHighlight
                        bgColor: root.activeBgAlt
                        borderColor: root.separatorColor
                        scrollFraction: leftDiffPane.scrollFraction
                        onJumpToFraction: function (fraction) {
                            root._syncingScroll = true;
                            leftDiffPane.scrollToFraction(fraction);
                            rightDiffPane.scrollToFraction(fraction);
                            root._syncingScroll = false;
                        }
                    }

                    // Keep the two panes scrolled together.
                    Connections {
                        target: leftDiffPane
                        function onContentYChanged() {
                            if (root._syncingScroll) return;
                            root._syncingScroll = true;
                            rightDiffPane.contentY = leftDiffPane.contentY;
                            root._syncingScroll = false;
                        }
                    }
                    Connections {
                        target: rightDiffPane
                        function onContentYChanged() {
                            if (root._syncingScroll) return;
                            root._syncingScroll = true;
                            leftDiffPane.contentY = rightDiffPane.contentY;
                            root._syncingScroll = false;
                        }
                    }
                }

                // ── Resource-tree view (tree mode with differences) ──────────
                ColumnLayout {
                    visible: !root.busy && root.resourceEntries.length > 0
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: 6

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.margins: 8
                        spacing: 8
                        AppTextField {
                            id: resourceFilterField
                            implicitHeight: 32
                            Layout.preferredWidth: 240
                            text: root.resourceFilter
                            placeholderText: qsTr("Filter resources…")
                            color: root.activeText
                            placeholderTextColor: root.activeDisabledText
                            background: Rectangle {
                                color: root.activeBg
                                border.color: root.separatorColor
                                border.width: 1
                                radius: 4
                            }
                            Accessible.name: qsTr("Filter webpage resources by path")
                            onTextChanged: root.resourceFilter = text
                        }
                        Item { Layout.fillWidth: true }
                        Controls.Label {
                            text: qsTr("%1 resources").arg(root.filteredResourceEntries().length)
                            color: root.activeText
                            opacity: 0.6
                            font.pixelSize: 11
                            font.family: "monospace"
                        }
                    }

                    Controls.ScrollView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        ListView {
                            model: root.filteredResourceEntries()
                            delegate: Rectangle {
                                required property var modelData
                                required property int index
                                width: ListView.view ? ListView.view.width : 0
                                height: 30
                                color: index % 2 === 0 ? root.activeBg : root.activeBgAlt
                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 12
                                    anchors.rightMargin: 12
                                    spacing: 10
                                    Rectangle {
                                        Layout.preferredWidth: 9
                                        Layout.preferredHeight: 9
                                        radius: 4.5
                                        color: root.resourceStateColor(modelData.state)
                                    }
                                    Controls.Label {
                                        Layout.fillWidth: true
                                        text: modelData.path
                                        elide: Text.ElideMiddle
                                        color: root.activeText
                                        font.family: "monospace"
                                        font.pixelSize: 12
                                    }
                                    Controls.Label {
                                        text: String(modelData.state)
                                        color: root.resourceStateColor(modelData.state)
                                        font.pixelSize: 11
                                    }
                                }
                            }
                        }
                    }
                }

                // ── Summary-only fallback (identical, tree mode, or error) ───
                Item {
                    visible: !root.busy && root.resultRows.length === 0
                        && root.resourceEntries.length === 0 && root.resultSummary.length > 0
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    ColumnLayout {
                        anchors.centerIn: parent
                        width: Math.min(parent.width - 24, 420)
                        spacing: 10
                        Kirigami.Icon {
                            source: root.resultError ? "dialog-error" : (root.resultEqual ? "dialog-ok" : "dialog-information")
                            Layout.preferredWidth: 48
                            Layout.preferredHeight: 48
                            Layout.alignment: Qt.AlignHCenter
                            color: root.resultError ? Kirigami.Theme.negativeTextColor
                                : (root.resultEqual ? Kirigami.Theme.positiveTextColor : root.activeDisabledText)
                            isMask: true
                            opacity: 0.8
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            text: root.resultSummary
                            color: root.activeText
                            font.pixelSize: 13
                        }
                    }
                }

                // ── Busy ──────────────────────────────────────────────────────
                Item {
                    visible: root.busy
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Controls.BusyIndicator {
                        anchors.centerIn: parent
                        running: root.busy
                    }
                }

                // ── Empty / idle placeholder ────────────────────────────────
                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: !root.busy && root.resultSummary.length === 0

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: Math.min(parent.width - 24, 360)
                        spacing: 10

                        Kirigami.Icon {
                            source: "internet-web-browser"
                            Layout.preferredWidth: 48
                            Layout.preferredHeight: 48
                            Layout.alignment: Qt.AlignHCenter
                            color: root.activeDisabledText
                            isMask: true
                            opacity: 0.55
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            text: qsTr("No comparison run yet")
                            color: root.activeText
                            font.pixelSize: 14
                            font.bold: true
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                            text: qsTr("Enter two URLs above and click Compare… to begin.")
                            color: root.activeDisabledText
                            font.pixelSize: 12
                        }
                    }
                }
            }
        }
    }

    // ── Confirmation dialog ────────────────────────────────────────────────────
    Controls.Dialog {
        id: confirmDialog
        title: qsTr("Fetch from the internet?")
        modal: true
        anchors.centerIn: parent

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("LinSync will fetch the following URLs:\n\n  Left:  %1\n  Right: %2\n\nThird-party resources linked from these pages may also be requested depending on the compare mode. No cookies or credentials from your personal browser are used.").arg(root.leftUrl).arg(root.rightUrl)
            }
        }

        standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel

        onAccepted: root.runCompare()
        onRejected: {
            root.pendingNewTab = false
        }
    }
}
