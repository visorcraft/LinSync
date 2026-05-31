// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// WebpageComparePage — fetch and compare two URLs in one of five sub-modes:
//   html (raw HTML source), text (extracted text), tree (resource tree),
//   rendered (Qt WebEngine, feature-gated), screenshot (Qt WebEngine, feature-gated).
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

    // ── Internal state ────────────────────────────────────────────────────────
    property string leftUrl: ""
    property string rightUrl: ""
    property string subMode: "html"
    property bool busy: false
    property string resultSummary: ""
    property var resultRows: []
    property bool resultEqual: false
    property bool resultTruncated: false
    property bool resultError: false
    // Left/right rows derived from resultRows, in the {text,state,number}
    // shape the DiffEditorPane expects.
    property var leftDiffRows: []
    property var rightDiffRows: []
    property var diffRowIndexes: []    // row indexes that differ (for the ruler)
    property bool _syncingScroll: false

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
        const left = encodeURIComponent(root.leftUrl);
        const right = encodeURIComponent(root.rightUrl);
        const mode = encodeURIComponent(root.subMode);
        root.bridgeGet("/compare/webpage?left=" + left + "&right=" + right + "&mode=" + mode, function (ok, payload) {
            root.busy = false;
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
            root.rebuildDiffRows();
        });
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
        anchors.margins: 12
        spacing: 12

        // Privacy notice — always visible on this page.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            text: qsTr("Webpage compare fetches content from the internet. Third-party resources on each page may also be requested.")
            visible: true
        }

        // ── Card: pages to compare ─────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: setupCard.implicitHeight + 28
            radius: 8
            color: root.activeBgAlt
            border.color: root.separatorColor
            border.width: 1

            ColumnLayout {
                id: setupCard
                anchors.fill: parent
                anchors.margins: 14
                spacing: 10

                Controls.Label {
                    text: qsTr("Pages to compare")
                    color: root.activeText
                    font.pixelSize: 14
                    font.bold: true
                }

                GridLayout {
                    Layout.fillWidth: true
                    columns: 2
                    rowSpacing: 8
                    columnSpacing: 10

                    FieldLabel { text: qsTr("Left URL") }
                    AppTextField {
                        id: leftUrlField
                        Layout.fillWidth: true
                        placeholderText: "https://example.com/"
                        text: root.leftUrl
                        onTextChanged: root.leftUrl = text
                        Accessible.name: qsTr("Left URL")
                    }

                    FieldLabel { text: qsTr("Right URL") }
                    AppTextField {
                        id: rightUrlField
                        Layout.fillWidth: true
                        placeholderText: "https://example.com/"
                        text: root.rightUrl
                        onTextChanged: root.rightUrl = text
                        Accessible.name: qsTr("Right URL")
                    }

                    FieldLabel { text: qsTr("Compare mode") }
                    AppComboBox {
                        id: subModeCombo
                        Layout.preferredWidth: 280
                        implicitHeight: 30
                        // Rendered / screenshot modes are not yet implemented:
                        // linsync-webengine returns NotImplemented even when the
                        // feature is built. Tracked in PLAN.md Phase 5 "Webpage".
                        model: [
                            { text: qsTr("HTML source"), value: "html" },
                            { text: qsTr("Extracted text"), value: "text" },
                            { text: qsTr("Resource tree"), value: "tree" },
                            { text: qsTr("Rendered (not implemented yet)"), value: "rendered" },
                            { text: qsTr("Screenshot (not implemented yet)"), value: "screenshot" }
                        ]
                        textRole: "text"
                        valueRole: "value"
                        property string _previousValidValue: "html"
                        onActivated: {
                            if (currentValue === "rendered" || currentValue === "screenshot") {
                                root.resultSummary = qsTr("%1 mode is not implemented yet").arg(currentValue)
                                var prevIdx = -1
                                for (var i = 0; i < model.length; i++) {
                                    if (model[i].value === subModeCombo._previousValidValue) {
                                        prevIdx = i
                                        break
                                    }
                                }
                                if (prevIdx >= 0)
                                    currentIndex = prevIdx
                                return
                            }
                            subModeCombo._previousValidValue = currentValue
                            root.subMode = currentValue
                        }
                        Accessible.name: qsTr("Compare mode")
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.topMargin: 2
                    height: 1
                    color: root.separatorColor
                }

                // Action row.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10

                    AppButton {
                        text: qsTr("Compare…")
                        enabled: root.leftUrl.length > 0 && root.rightUrl.length > 0 && !root.busy
                        onClicked: confirmDialog.open()
                        icon.name: "internet-web-browser-symbolic"
                        Accessible.name: qsTr("Compare")
                    }

                    AppButton {
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

                // ── Summary-only fallback (identical, tree mode, or error) ───
                Item {
                    visible: !root.busy && root.resultRows.length === 0 && root.resultSummary.length > 0
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
        }
    }
}
