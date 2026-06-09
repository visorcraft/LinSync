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
    property bool running: false
    property var lastResult: null
    property int requestCounter: 0
    property string activeRequestId: ""
    property int progressCurrent: 0
    property int progressTotal: 0
    property string progressMessage: ""

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

        const modeStr = modeCombo.currentText === "OCR Text" ? "ocr_text" : "text";
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
            if (data.equal) {
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
                    model: ["Text", "OCR Text"]
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
                    visible: root.lastResult !== null && !root.lastResult.equal
                    text: root.lastResult !== null ? "Differing lines: " + root.lastResult.differing_lines : ""
                    color: root.activeText
                }
                Controls.Label {
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

        // ── Status bar ────────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 24
            color: root.activeBg

            Controls.Label {
                anchors {
                    verticalCenter: parent.verticalCenter
                    left: parent.left
                    leftMargin: 8
                }
                text: root.statusText
                color: root.activeText
                elide: Text.ElideRight
            }
        }
    }
}
