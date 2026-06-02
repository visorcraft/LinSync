// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// MergePage — three-pane (Left | Base | Right) merge view with output preview
// and conflict navigator.  Launched from the Compare toolbar via the
// "Three-way merge…" button.  Communicates with the Rust bridge over the same
// HTTP/JSON protocol as the rest of the QML shell.
Item {
    id: root

    // ── External interface ────────────────────────────────────────────────────

    required property string bridgeUrl
    required property color activeBg
    required property color activeBgAlt
    required property color activeText
    required property color activeDisabledText
    required property color activeHighlight
    required property color activeNeutralText
    required property color activeNegativeText
    required property color activePositiveText
    required property color separatorColor

    // Paths set before the page becomes visible — changing them triggers a new
    // merge session via start().
    property string basePath: ""
    property string leftPath: ""
    property string rightPath: ""
    // Predetermined output path (Git mergetool). When non-empty, a one-click
    // "Save merge" writes straight here instead of prompting.
    property string outputPath: ""

    // ── Internal state ────────────────────────────────────────────────────────

    property var    conflicts:       []
    property int    currentConflict: -1
    property string outputText:      ""
    property string statusText:      "Ready"
    property bool   syncing:         false
    property string customText:      ""

    // ── Helpers ───────────────────────────────────────────────────────────────

    function bridgeGet(path, onLoad) {
        if (root.bridgeUrl === "") {
            if (onLoad) onLoad(false, null)
            return
        }
        const xhr = new XMLHttpRequest()
        xhr.onreadystatechange = function () {
            if (xhr.readyState === XMLHttpRequest.DONE) {
                if (xhr.status >= 200 && xhr.status < 300) {
                    let payload = null
                    try { payload = JSON.parse(xhr.responseText) } catch (_e) {}
                    if (onLoad) onLoad(true, payload)
                } else {
                    if (onLoad) onLoad(false, null)
                }
            }
        }
        xhr.open("GET", root.bridgeUrl + path)
        xhr.send()
    }

    // Kick off a new three-way merge session from the current paths.
    function start() {
        if (root.basePath === "" || root.leftPath === "" || root.rightPath === "") {
            root.statusText = "Select base, left, and right paths first"
            return
        }
        root.statusText = "Loading…"
        root.conflicts = []
        root.currentConflict = -1
        root.outputText = ""

        const url = "/merge3/start"
            + "?base="  + encodeURIComponent(root.basePath)
            + "&left="  + encodeURIComponent(root.leftPath)
            + "&right=" + encodeURIComponent(root.rightPath)

        root.bridgeGet(url, function (ok, data) {
            if (ok && data && data.ok) {
                root.conflicts       = data.conflicts  || []
                root.outputText      = data.output_text || ""
                root.currentConflict = root.conflicts.length > 0 ? 0 : -1
                root.statusText      = root.conflicts.length > 0
                    ? root.conflicts.length + " conflict(s) — select a choice for each"
                    : "No conflicts — files merged cleanly"
            } else {
                root.statusText = "Merge start failed"
            }
        })
    }

    // Resolve the currently selected conflict with left / right / base.
    function resolveCurrent(choice) {
        if (root.currentConflict < 0 || root.conflicts.length === 0)
            return
        const id = root.conflicts[root.currentConflict].id
        root.bridgeGet(
            "/merge3/resolve?id=" + id + "&choice=" + choice,
            function (ok, data) {
                if (ok && data && data.ok) {
                    root.conflicts  = data.conflicts  || []
                    root.outputText = data.output_text || ""
                    // Advance to next unresolved conflict when available.
                    if (root.currentConflict >= root.conflicts.length)
                        root.currentConflict = Math.max(0, root.conflicts.length - 1)
                    // `conflicts` is the stable full list; the count of
                    // still-unresolved conflicts comes from the bridge.
                    const remaining = data.unresolved_count !== undefined
                        ? data.unresolved_count
                        : root.conflicts.length
                    root.statusText = remaining > 0
                        ? remaining + " conflict(s) remaining"
                        : "All conflicts resolved — ready to save"
                } else {
                    root.statusText = "Resolve failed"
                }
            }
        )
    }

    function resolveCustom(customText) {
        if (root.currentConflict < 0 || root.conflicts.length === 0)
            return
        const id = root.conflicts[root.currentConflict].id
        root.bridgeGet(
            "/merge3/resolve?id=" + id + "&choice=custom&text=" + encodeURIComponent(customText),
            function (ok, data) {
                if (ok && data && data.ok) {
                    root.conflicts  = data.conflicts  || []
                    root.outputText = data.output_text || ""
                    if (root.currentConflict >= root.conflicts.length)
                        root.currentConflict = Math.max(0, root.conflicts.length - 1)
                    // `conflicts` is the stable full list; the count of
                    // still-unresolved conflicts comes from the bridge.
                    const remaining = data.unresolved_count !== undefined
                        ? data.unresolved_count
                        : root.conflicts.length
                    root.statusText = remaining > 0
                        ? remaining + " conflict(s) remaining"
                        : "All conflicts resolved — ready to save"
                } else {
                    root.statusText = "Custom resolve failed"
                }
            }
        )
    }

    function prevConflict() {
        if (root.currentConflict > 0)
            root.currentConflict--
    }

    function nextConflict() {
        if (root.currentConflict < root.conflicts.length - 1)
            root.currentConflict++
    }

    function saveTo(path) {
        root.bridgeGet(
            "/merge3/save?path=" + encodeURIComponent(path),
            function (ok, data) {
                if (ok && data && data.ok)
                    root.statusText = "Saved to " + path
                else
                    root.statusText = "Save failed" + (data && data.error ? ": " + data.error : "")
            }
        )
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ── Top toolbar ───────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: root.activeBgAlt

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

                Controls.Label {
                    text: qsTr("Three-way Merge")
                    font.bold: true
                    color: root.activeText
                }

                Kirigami.Separator { Layout.fillHeight: true }

                Controls.ToolButton {
                    icon.name: "go-previous"
                    icon.color: root.activeText
                    enabled: root.currentConflict > 0
                    Controls.ToolTip.text: qsTr("Previous conflict")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("Previous conflict")
                    onClicked: root.prevConflict()
                }

                Controls.Label {
                    text: root.conflicts.length > 0
                        ? qsTr("Conflict %1 / %2").arg(root.currentConflict + 1).arg(root.conflicts.length)
                        : qsTr("No conflicts")
                    color: root.activeText
                    font.pixelSize: 12
                }

                Controls.ToolButton {
                    icon.name: "go-next"
                    icon.color: root.activeText
                    enabled: root.currentConflict < root.conflicts.length - 1
                    Controls.ToolTip.text: qsTr("Next conflict")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("Next conflict")
                    onClicked: root.nextConflict()
                }

                Kirigami.Separator { Layout.fillHeight: true }

                Controls.Button {
                    text: qsTr("Keep Left")
                    enabled: root.currentConflict >= 0
                    palette.button: root.activeBgAlt
                    palette.buttonText: root.activeText
                    onClicked: root.resolveCurrent("left")
                }

                Controls.Button {
                    text: qsTr("Keep Base")
                    enabled: root.currentConflict >= 0
                    palette.button: root.activeBgAlt
                    palette.buttonText: root.activeText
                    onClicked: root.resolveCurrent("base")
                }

                Controls.Button {
                    text: qsTr("Keep Right")
                    enabled: root.currentConflict >= 0
                    palette.button: root.activeBgAlt
                    palette.buttonText: root.activeText
                    onClicked: root.resolveCurrent("right")
                }

                Item { Layout.fillWidth: true }

                Controls.Button {
                    // Git-mergetool one-click save to the predetermined $MERGED.
                    visible: root.outputPath !== ""
                    text: qsTr("Save merge")
                    palette.button: root.activeBgAlt
                    palette.buttonText: root.activeText
                    onClicked: root.saveTo(root.outputPath)
                }

                Controls.Button {
                    text: qsTr("Save to…")
                    palette.button: root.activeBgAlt
                    palette.buttonText: root.activeText
                    onClicked: savePicker.open()
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 60
            visible: root.currentConflict >= 0
            color: root.activeBg
            border.color: root.separatorColor
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.margins: 6
                spacing: 8

                Controls.Label {
                    text: qsTr("Custom text:")
                    color: root.activeText
                    font.pixelSize: 11
                }

                Controls.TextArea {
                    id: customTextArea
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    placeholderText: qsTr("Enter custom resolution text…")
                    text: root.currentConflict >= 0 && root.conflicts.length > 0
                        ? (root.conflicts[root.currentConflict].left_lines || []).join("\n")
                        : ""
                    font.family: "monospace"
                    font.pixelSize: 11
                    color: root.activeText
                    wrapMode: Controls.TextArea.NoWrap
                    background: Rectangle {
                        color: root.activeBg
                        border.color: root.separatorColor
                        border.width: 1
                        radius: 4
                    }
                }

                Controls.Button {
                    text: qsTr("Use Custom")
                    enabled: root.currentConflict >= 0 && customTextArea.text.length > 0
                    palette.button: root.activeBgAlt
                    palette.buttonText: root.activeText
                    onClicked: root.resolveCustom(customTextArea.text)
                }
            }
        }

        // ── Three-column pane ─────────────────────────────────────────────────
        Controls.SplitView {
            id: threePane
            Layout.fillWidth: true
            Layout.preferredHeight: parent.height * 0.55
            orientation: Qt.Horizontal

            MergeFileColumn {
                id: leftCol
                Controls.SplitView.fillWidth: true
                Controls.SplitView.minimumWidth: 180
                label: qsTr("Left")
                filePath: root.leftPath
                accentColor: root.activeNeutralText
                highlightColor: root.activeHighlight
                activeBg: root.activeBg
                activeBgAlt: root.activeBgAlt
                activeText: root.activeText
                activeDisabledText: root.activeDisabledText
                separatorColor: root.separatorColor
                conflictStart: currentConflictStart("left")
                conflictEnd: currentConflictEnd("left")
            }

            MergeFileColumn {
                id: baseCol
                Controls.SplitView.fillWidth: true
                Controls.SplitView.minimumWidth: 180
                label: qsTr("Base")
                filePath: root.basePath
                accentColor: root.activeDisabledText
                highlightColor: root.activeHighlight
                activeBg: root.activeBg
                activeBgAlt: root.activeBgAlt
                activeText: root.activeText
                activeDisabledText: root.activeDisabledText
                separatorColor: root.separatorColor
                conflictStart: currentConflictStart("base")
                conflictEnd: currentConflictEnd("base")
            }

            MergeFileColumn {
                id: rightCol
                Controls.SplitView.fillWidth: true
                Controls.SplitView.minimumWidth: 180
                label: qsTr("Right")
                filePath: root.rightPath
                accentColor: root.activePositiveText
                highlightColor: root.activeHighlight
                activeBg: root.activeBg
                activeBgAlt: root.activeBgAlt
                activeText: root.activeText
                activeDisabledText: root.activeDisabledText
                separatorColor: root.separatorColor
                conflictStart: currentConflictStart("right")
                conflictEnd: currentConflictEnd("right")
            }
        }

        Connections {
            target: leftCol
            function onListViewContentYChanged() {
                if (!root.syncing && leftCol.listViewMoving) {
                    root.syncing = true
                    baseCol.setListViewContentY(leftCol.listViewContentY)
                    rightCol.setListViewContentY(leftCol.listViewContentY)
                    root.syncing = false
                }
            }
        }
        Connections {
            target: baseCol
            function onListViewContentYChanged() {
                if (!root.syncing && baseCol.listViewMoving) {
                    root.syncing = true
                    leftCol.setListViewContentY(baseCol.listViewContentY)
                    rightCol.setListViewContentY(baseCol.listViewContentY)
                    root.syncing = false
                }
            }
        }
        Connections {
            target: rightCol
            function onListViewContentYChanged() {
                if (!root.syncing && rightCol.listViewMoving) {
                    root.syncing = true
                    leftCol.setListViewContentY(rightCol.listViewContentY)
                    baseCol.setListViewContentY(rightCol.listViewContentY)
                    root.syncing = false
                }
            }
        }

        // ── Separator ─────────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.separatorColor
        }

        // ── Output preview ────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 20
            color: root.activeBgAlt

            Controls.Label {
                anchors.verticalCenter: parent.verticalCenter
                leftPadding: 10
                text: qsTr("Merged output preview")
                font.bold: true
                color: root.activeText
                opacity: 0.75
                font.pixelSize: 11
            }
        }

        Controls.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Controls.TextArea {
                id: outputArea
                text: root.outputText
                readOnly: true
                font.family: "monospace"
                font.pixelSize: 12
                color: root.activeText
                background: Rectangle { color: root.activeBg }
                wrapMode: Controls.TextArea.NoWrap
                Accessible.name: qsTr("Merged output")
                Accessible.role: Accessible.EditableText
            }
        }

        // ── Status bar ────────────────────────────────────────────────────────
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 28
            color: root.activeBgAlt

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                height: 1
                color: root.separatorColor
            }

            Controls.Label {
                anchors.verticalCenter: parent.verticalCenter
                leftPadding: 10
                text: root.statusText
                color: root.activeText
            }
        }
    }

    // ── Save file picker ──────────────────────────────────────────────────────
    Dialogs.FileDialog {
        id: savePicker
        fileMode: Dialogs.FileDialog.SaveFile
        nameFilters: [qsTr("All files (*)")]
        onAccepted: {
            let path = selectedFile.toString()
            if (path.startsWith("file://"))
                path = path.substring(7)
            path = decodeURIComponent(path)
            root.saveTo(path)
        }
    }

    // ── Conflict range helpers ────────────────────────────────────────────────

// Each MergeFileColumn displays one *side's file* (left/base/right), so a
// conflict's highlight/scroll range must be expressed in that side's own line
// numbers. The conflict's `start_line`/`end_line` are positions in the merged
// *marker-rendered* output, not in any single side, so using them here put the
// highlight on the wrong lines (and out of range). Derive the range from the
// side's own line array instead. The current merge model emits one whole-file
// conflict, so each side's conflict region is its entire content.
function currentConflictLines(side) {
    if (root.currentConflict < 0 || root.conflicts.length === 0)
        return null
    const c = root.conflicts[root.currentConflict]
    if (!c) return null
    if (side === "base") return c.base_lines || []
    if (side === "right") return c.right_lines || []
    return c.left_lines || []
}

function currentConflictStart(side) {
    const lines = currentConflictLines(side)
    return lines && lines.length > 0 ? 0 : -1
}

function currentConflictEnd(side) {
    const lines = currentConflictLines(side)
    return lines && lines.length > 0 ? lines.length - 1 : -1
}

    // ── Inner component: a single file column ─────────────────────────────────

    component MergeFileColumn: Rectangle {
        id: col

        required property string label
        required property string filePath
        required property color  accentColor
        required property color  highlightColor
        required property color  activeBg
        required property color  activeBgAlt
        required property color  activeText
        required property color  activeDisabledText
        required property color  separatorColor
        // First and last highlighted row index (-1 = none).
        property int conflictStart: -1
        property int conflictEnd:   -1
        property alias listViewContentY: lineList.contentY
        property alias listViewMoving: lineList.moving

        function setListViewContentY(y) {
            lineList.contentY = y
        }

        color: activeBg
        border.color: separatorColor

        // Lines loaded from the file on demand.
        property var lines: []

        onFilePathChanged: loadFile()
        Component.onCompleted: loadFile()

        function loadFile() {
            if (filePath === "") {
                col.lines = []
                return
            }
            const xhr = new XMLHttpRequest()
            const url = filePath.startsWith("/") ? "file://" + filePath : "file:///" + filePath
            xhr.onreadystatechange = function () {
                if (xhr.readyState === XMLHttpRequest.DONE) {
                    const text = xhr.responseText || ""
                    col.lines = text.split("\n")
                    // Remove the trailing empty string caused by the final newline.
                    if (col.lines.length > 0 && col.lines[col.lines.length - 1] === "")
                        col.lines = col.lines.slice(0, col.lines.length - 1)
                }
            }
            xhr.open("GET", url)
            xhr.send()
        }

        // Scroll to the conflict range when it changes.
        onConflictStartChanged: {
            if (conflictStart >= 0)
                lineList.positionViewAtIndex(conflictStart, ListView.Center)
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            // Column header
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 32
                color: col.activeBgAlt
                border.color: col.separatorColor

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 6

                    Rectangle {
                        Layout.preferredWidth: 4
                        Layout.fillHeight: true
                        color: col.accentColor
                    }

                    Controls.Label {
                        text: col.label
                        font.bold: true
                        color: col.activeText
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        text: col.filePath !== "" ? col.filePath : qsTr("(no file)")
                        elide: Text.ElideLeft
                        color: col.activeDisabledText
                        font.pixelSize: 10
                    }
                }
            }

            // Line list
            ListView {
                id: lineList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: col.lines

                delegate: Rectangle {
                    required property int    index
                    required property string modelData

                    width:  ListView.view.width
                    height: 22

                    color: {
                        if (index >= col.conflictStart && col.conflictStart >= 0 && index <= col.conflictEnd)
                            return Kirigami.ColorUtils.tintWithAlpha(col.activeBg, col.highlightColor, 0.22)
                        return index % 2 === 0 ? col.activeBg : col.activeBgAlt
                    }

                    RowLayout {
                        anchors.fill: parent
                        spacing: 0

                        Controls.Label {
                            Layout.preferredWidth: 44
                            horizontalAlignment: Text.AlignRight
                            rightPadding: 6
                            text: index + 1
                            color: col.activeDisabledText
                            font.family: "monospace"
                            font.pixelSize: 11
                        }

                        Rectangle {
                            Layout.preferredWidth: 1
                            Layout.fillHeight: true
                            color: col.separatorColor
                        }

                        Controls.Label {
                            Layout.fillWidth: true
                            leftPadding: 6
                            text: modelData
                            font.family: "monospace"
                            font.pixelSize: 11
                            color: col.activeText
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                        }
                    }
                }
            }
        }
    }
}
