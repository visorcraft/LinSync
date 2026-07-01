// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// DocumentComparePage — two-pane text-diff view for OCR/document compare.
// Sends a GET to /compare/document and renders the extracted text diff.
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

    property string leftPath: ""
    property string rightPath: ""

    // ── Internal state ────────────────────────────────────────────────────────
    property string statusText: "Select left and right document paths, then run compare."
    // Announce status/error changes to assistive technology as they happen.
    // Accessible.announce() exists on Qt 6.8+; guarded so older Qt is a no-op.
    onStatusTextChanged: {
        if (typeof statusBarLabel !== "undefined" && statusBarLabel.Accessible
                && typeof statusBarLabel.Accessible.announce === "function")
            statusBarLabel.Accessible.announce(root.statusText)
    }
    property bool running: false
    property var lastResult: null
    property int requestCounter: 0
    property string activeRequestId: ""
    property int progressCurrent: 0
    property int progressTotal: 0
    property string progressMessage: ""
    // Defensive iteration cap for the progress timer so a compare whose XHR
    // never completes (bridge worker wedged, OCR plugin hang, lost response)
    // cannot poll /progress forever and saturate the bridge connection pool.
    // At 200 ms intervals, 1500 iterations == 5 minutes. Mirrors Main.qml's
    // progressPollMax safety net.
    property int progressPollCount: 0
    readonly property int progressPollMax: 1500
    property int selectedRenderedPageIndex: -1
    readonly property bool isRenderedMode: modeCombo.currentText === "Rendered"
    property real renderedZoom: 1.0
    function renderedPage(index) {
        if (!root.lastResult || !Array.isArray(root.lastResult.rendered_pages))
            return null;
        const idx = (index < 0 || index >= root.lastResult.rendered_pages.length) ? 0 : index;
        return root.lastResult.rendered_pages[idx];
    }

    property var documentNameFilters: [
        qsTr("Documents (*.pdf *.odt *.docx *.txt *.rtf)"),
        qsTr("All files (*)")
    ]

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
                } catch (_) {}
                if (onLoad)
                    onLoad(ok, payload);
            }
        };
        xhr.open("GET", root.bridgeUrl + path);
        xhr.send();
    }

    function runCompare() {
        if (root.leftPath === "" || root.rightPath === "") {
            root.statusText = "Both left and right paths are required.";
            return;
        }
        root.running = true;
        root.lastResult = null;
        root.statusText = "Extracting text and comparing…";
        root.requestCounter += 1;
        root.activeRequestId = "doc-" + root.requestCounter;
        root.progressCurrent = 0;
        root.progressTotal = 0;
        root.progressMessage = "";
        root.progressPollCount = 0;

        const modeStr = {
            "OCR Text": "ocr_text",
            "Rendered": "rendered"
        }[modeCombo.currentText] || "text";
        const lang = ocrLangField.text.trim() || "eng";
        const url = "/compare/document"
            + "?left=" + encodeURIComponent(root.leftPath)
            + "&right=" + encodeURIComponent(root.rightPath)
            + "&mode=" + modeStr
            + "&ocr_language=" + encodeURIComponent(lang)
            + "&request_id=" + encodeURIComponent(root.activeRequestId);

        root.bridgeGet(url, function (ok, data) {
            root.running = false;
            root.activeRequestId = "";
            if (!ok || !data || data.error) {
                const msg = data && data.error ? data.error : "Compare failed — check file paths and plugin availability.";
                root.statusText = msg;
                return;
            }
            root.lastResult = data;
            if (data.session)
                root.sessionUpdated(data);
            if (Array.isArray(data.rendered_pages) && data.rendered_pages.length > 0) {
                const differing = data.rendered_pages.filter(function (p) { return !p.equal }).length;
                root.selectedRenderedPageIndex = 0;
                root.statusText = differing === 0
                    ? qsTr("Rendered pages are identical (%1 pages).").arg(data.rendered_pages.length)
                    : qsTr("%1 of %2 rendered pages differ.").arg(differing).arg(data.rendered_pages.length);
            } else if (data.equal) {
                root.statusText = "Documents are equal (extracted via " + data.left_extractor + ").";
            } else {
                root.statusText = data.differing_lines + " differing lines (extracted via " + data.left_extractor + ").";
            }
        });
    }

    Timer {
        id: progressTimer
        interval: 200
        repeat: true
        running: root.running && root.activeRequestId !== ""
        onTriggered: {
            if (!root.running || root.activeRequestId === "") {
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
                root.running = false
                root.activeRequestId = ""
                root.statusText = qsTr("Compare timed out — no response from the bridge")
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
                    root.statusText = qsTr("Comparing %1/%2").arg(root.progressCurrent).arg(root.progressTotal);
                else if (root.progressMessage !== "")
                    root.statusText = root.progressMessage;
            });
        }
    }

    // Reusable extracted-text pane: an accent-striped header bar plus a
    // scrollable read-only monospace body. Mirrors ImageComparePage's
    // ImagePane so the two compare pages share a visual language.
    component TextPane: Rectangle {
        id: pane

        property string heading: ""
        property color accent: root.activeHighlight
        property string bodyText: ""
        property bool placeholder: false

        color: root.activeBgAlt
        clip: true

        Rectangle {
            id: paneHeader
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 28
            color: root.activeBg
            border.color: root.separatorColor
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 8

                Rectangle {
                    Layout.preferredWidth: 3
                    Layout.preferredHeight: 14
                    color: pane.accent
                    radius: 2
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: pane.heading
                    color: root.activeText
                    font.bold: true
                    font.pixelSize: 12
                    elide: Text.ElideRight
                }
            }
        }

        Controls.ScrollView {
            anchors.top: paneHeader.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 4
            clip: true

            Controls.TextArea {
                text: pane.bodyText
                readOnly: true
                font.family: "monospace"
                font.pointSize: 10
                color: pane.placeholder ? root.activeDisabledText : root.activeText
                background: null
                wrapMode: Controls.TextArea.Wrap
            }
        }
    }

    // ── Layout ────────────────────────────────────────────────────────────────
    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 54
            color: root.activeBg
            border.color: root.separatorColor
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.margins: 8
                spacing: 8

                FilePickerBar {
                    label: qsTr("Left")
                    kind: qsTr("document")
                    path: root.leftPath
                    nameFilters: root.documentNameFilters
                    textColor: root.activeText
                    disabledTextColor: root.activeDisabledText
                    fieldColor: root.activeBg
                    borderColor: root.separatorColor
                    onPathPicked: function (pickedPath) { root.leftPath = pickedPath }
                }
                FilePickerBar {
                    label: qsTr("Right")
                    kind: qsTr("document")
                    path: root.rightPath
                    nameFilters: root.documentNameFilters
                    textColor: root.activeText
                    disabledTextColor: root.activeDisabledText
                    fieldColor: root.activeBg
                    borderColor: root.separatorColor
                    onPathPicked: function (pickedPath) { root.rightPath = pickedPath }
                }
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
                    id: modeCombo
                    model: ["Text", "OCR Text", "Rendered"]
                    currentIndex: 0
                    Layout.preferredWidth: 130
                    implicitHeight: 30
                    Accessible.name: "Document extraction mode"
                }

                Kirigami.Separator { Layout.fillHeight: true }

                Controls.Label {
                    text: qsTr("OCR Language:")
                    color: modeCombo.currentIndex === 1 ? root.activeText : root.activeDisabledText
                    opacity: modeCombo.currentIndex === 1 ? 0.7 : 1.0
                    font.pixelSize: 12
                }
                AppTextField {
                    id: ocrLangField
                    text: "eng"
                    Layout.preferredWidth: 80
                    enabled: modeCombo.currentIndex === 1
                    opacity: modeCombo.currentIndex === 1 ? 1.0 : 0.4
                    Accessible.name: "OCR language code"
                }

                AppButton {
                    Layout.preferredHeight: 30
                    Layout.preferredWidth: 150
                    text: root.running ? qsTr("Comparing…") : qsTr("Run Compare")
                    icon.name: "media-playback-start"
                    enabled: !root.running && root.leftPath !== "" && root.rightPath !== ""
                    onClicked: root.runCompare()
                }

                Controls.BusyIndicator {
                    running: root.running
                    visible: root.running
                    Layout.preferredWidth: 28
                    Layout.preferredHeight: 28
                }

                Controls.ProgressBar {
                    visible: root.running && root.progressTotal > 0
                    from: 0
                    to: root.progressTotal
                    value: root.progressCurrent
                    Layout.preferredWidth: 120
                    Layout.preferredHeight: 16
                }

                Item { Layout.fillWidth: true }

                Controls.Label {
                    text: root.lastResult !== null ? (root.lastResult.equal ? qsTr("Equal") : root.lastResult.differing_lines + qsTr(" diffs")) : ""
                    color: root.lastResult !== null && !root.lastResult.equal ? "#e53935" : root.activeText
                    font.bold: true
                }
            }
        }

        // ── Result summary card ───────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: root.lastResult !== null ? resultSummary.implicitHeight + 24 : 0
            visible: root.lastResult !== null
            color: root.activeBgAlt
            border.color: root.separatorColor
            border.width: 1
            radius: 6

            ColumnLayout {
                id: resultSummary
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    margins: 12
                }
                spacing: 2

                Controls.Label {
                    text: root.lastResult !== null ? (root.lastResult.equal ? "Documents are identical." : "Documents differ.") : ""
                    font.bold: true
                    color: root.lastResult !== null && !root.lastResult.equal ? "#e53935" : root.activeText
                }
                Controls.Label {
                    visible: root.lastResult !== null && !root.lastResult.equal && !root.isRenderedMode
                    text: root.lastResult !== null ? "Differing lines: " + root.lastResult.differing_lines : ""
                    color: root.activeText
                }
                Controls.Label {
                    visible: root.lastResult !== null && root.isRenderedMode
                          && Array.isArray(root.lastResult.rendered_pages)
                    text: root.lastResult !== null
                        ? qsTr("Rendered pages: %1, differing: %2")
                            .arg(root.lastResult.rendered_pages.length)
                            .arg(root.lastResult.rendered_pages.filter(function (p) { return !p.equal }).length)
                        : ""
                    color: root.activeText
                }
                Controls.Label {
                    visible: root.lastResult !== null && !root.isRenderedMode
                    text: root.lastResult !== null ? "Extracted via: " + root.lastResult.left_extractor : ""
                    color: root.activeDisabledText
                    font.pointSize: 9
                }
                // OCR word-position summary. The page shows extracted text (not
                // the rendered source image), so these are surfaced as a count;
                // the boxes themselves are carried in the result for callers and
                // a future rendered-page overlay.
                Controls.Label {
                    function wordCount(lines) {
                        if (!lines) return 0
                        var n = 0
                        for (var i = 0; i < lines.length; ++i)
                            n += (lines[i] ? lines[i].length : 0)
                        return n
                    }
                    visible: root.lastResult !== null
                        && (root.lastResult.left_word_positions !== undefined
                            || root.lastResult.right_word_positions !== undefined)
                    text: root.lastResult !== null
                        ? qsTr("OCR word positions: %1 left / %2 right")
                            .arg(wordCount(root.lastResult.left_word_positions))
                            .arg(wordCount(root.lastResult.right_word_positions))
                        : ""
                    color: root.activeDisabledText
                    font.pointSize: 9
                }
            }
        }

        // ── Two text panes (left / right extracted text) ──────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 2
            visible: !root.isRenderedMode

            TextPane {
                Layout.fillWidth: true
                Layout.fillHeight: true
                heading: qsTr("Left")
                accent: Kirigami.Theme.neutralTextColor
                placeholder: !(root.lastResult !== null && root.lastResult.left_text)
                bodyText: root.lastResult !== null && root.lastResult.left_text
                    ? root.lastResult.left_text
                    : (root.running ? "" : qsTr("(run compare to see extracted text)"))
            }

            Rectangle {
                Layout.preferredWidth: 1
                Layout.fillHeight: true
                color: root.separatorColor
            }

            TextPane {
                Layout.fillWidth: true
                Layout.fillHeight: true
                heading: qsTr("Right")
                accent: Kirigami.Theme.positiveTextColor
                placeholder: !(root.lastResult !== null && root.lastResult.right_text)
                bodyText: root.lastResult !== null && root.lastResult.right_text
                    ? root.lastResult.right_text
                    : (root.running ? "" : qsTr("(run compare to see extracted text)"))
            }
        }

        // ── Rendered-page navigator (Rendered mode only) ──────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.isRenderedMode
            color: root.activeBgAlt
            clip: true

            RowLayout {
                anchors.fill: parent
                spacing: 2

                // Thumbnail strip
                Rectangle {
                    Layout.preferredWidth: 120
                    Layout.fillHeight: true
                    color: root.activeBg
                    border.color: root.separatorColor
                    border.width: 1

                    ColumnLayout {
                        anchors.fill: parent
                        spacing: 0

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 28
                            color: root.activeBgAlt

                            Controls.Label {
                                anchors {
                                    verticalCenter: parent.verticalCenter
                                    left: parent.left
                                    leftMargin: 8
                                }
                                text: qsTr("Pages")
                                color: root.activeText
                                font.bold: true
                                font.pixelSize: 12
                            }
                        }

                        ListView {
                            id: pageList
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            model: root.lastResult !== null && Array.isArray(root.lastResult.rendered_pages)
                                   ? root.lastResult.rendered_pages : []
                            currentIndex: root.selectedRenderedPageIndex
                            highlight: Rectangle { color: root.activeHighlight; opacity: 0.3 }
                            highlightMoveDuration: 0

                            delegate: Rectangle {
                                required property var modelData
                                required property int index
                                width: pageList.width
                                height: 36
                                color: pageList.currentIndex === index
                                    ? "transparent"
                                    : (modelData.equal ? root.activeBg : Kirigami.Theme.negativeBackgroundColor)
                                border.color: root.separatorColor
                                border.width: 1

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: 8
                                    anchors.rightMargin: 8
                                    spacing: 6

                                    Controls.Label {
                                        text: modelData.page + 1
                                        color: root.activeText
                                        font.bold: true
                                    }
                                    Controls.Label {
                                        Layout.fillWidth: true
                                        text: modelData.equal ? qsTr("equal") : qsTr("diff")
                                        color: modelData.equal ? root.activeDisabledText : "#e53935"
                                        font.pixelSize: 10
                                        horizontalAlignment: Text.AlignRight
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: root.selectedRenderedPageIndex = index
                                }
                            }
                        }
                    }
                }

                Rectangle {
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                    color: root.separatorColor
                }

                // Page viewer
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: 2

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 40
                        color: root.activeBg
                        border.color: root.separatorColor
                        border.width: 1

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 8
                            anchors.rightMargin: 8
                            spacing: 8

                            Controls.Label {
                                text: {
                                    const p = root.renderedPage(root.selectedRenderedPageIndex);
                                    return p ? qsTr("Page %1  •  diff %2%").arg(p.page + 1).arg(Math.round(p.diff_ratio * 100)) : "";
                                }
                                color: root.activeText
                                font.bold: true
                            }

                            Item { Layout.fillWidth: true }

                            Controls.Label {
                                text: qsTr("Zoom:")
                                color: root.activeText
                                opacity: 0.7
                                font.pixelSize: 12
                            }
                            Controls.Slider {
                                id: zoomSlider
                                from: 0.25
                                to: 4.0
                                value: root.renderedZoom
                                stepSize: 0.25
                                Layout.preferredWidth: 160
                                onMoved: root.renderedZoom = value
                            }
                            Controls.Label {
                                text: Math.round(root.renderedZoom * 100) + "%"
                                color: root.activeText
                                Layout.preferredWidth: 44
                            }
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: 2

                        ImagePane {
                            id: renderedLeftPane
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            source: {
                                const p = root.renderedPage(root.selectedRenderedPageIndex);
                                return p && p.left_uri ? p.left_uri : "";
                            }
                            zoom: root.renderedZoom
                            active: true
                        }

                        Rectangle {
                            Layout.preferredWidth: 1
                            Layout.fillHeight: true
                            color: root.separatorColor
                        }

                        ImagePane {
                            id: renderedRightPane
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            source: {
                                const p = root.renderedPage(root.selectedRenderedPageIndex);
                                return p && p.right_uri ? p.right_uri : "";
                            }
                            zoom: root.renderedZoom
                            active: true
                        }
                    }
                }
            }

            ColumnLayout {
                anchors.centerIn: parent
                visible: !root.running
                    && root.isRenderedMode
                    && (!root.lastResult || !Array.isArray(root.lastResult.rendered_pages)
                        || root.lastResult.rendered_pages.length === 0)
                spacing: 12

                Kirigami.Icon {
                    source: "view-pages"
                    Layout.preferredWidth: 56
                    Layout.preferredHeight: 56
                    Layout.alignment: Qt.AlignHCenter
                    color: root.activeDisabledText
                    isMask: true
                    opacity: 0.6
                }
                Controls.Label {
                    Layout.alignment: Qt.AlignHCenter
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("No rendered pages")
                    color: root.activeText
                    font.pixelSize: 14
                    font.bold: true
                }
                Controls.Label {
                    Layout.alignment: Qt.AlignHCenter
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("Select two documents and click Run Compare in Rendered mode.")
                    color: root.activeDisabledText
                    font.pixelSize: 12
                }
            }
        }

        // ── Status bar ────────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 24
            color: root.activeBg

            Controls.Label {
                id: statusBarLabel
                anchors {
                    verticalCenter: parent.verticalCenter
                    left: parent.left
                    leftMargin: 8
                }
                text: root.statusText
                color: root.activeText
                elide: Text.ElideRight
                // Expose the status line as a live region so screen
                // readers announce status/error changes as they happen.
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Status: %1").arg(root.statusText)
            }
        }
    }
}
