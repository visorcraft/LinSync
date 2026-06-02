// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    property var sessionState: ({})
    property int activeTabId: 0
    property var recentSessions: []

    signal tabActivated(int tabId)
    signal tabClosed(int tabId)
    signal navigateRequested(int section)
    signal reopenRecentRequested(int index)
    signal refreshRecentRequested()
    signal saveCurrentSessionRequested()
    // Ask Main.qml to pick a path and save the open tabs as a project file, or
    // open a project file and restore its comparisons as tabs.
    signal saveProjectRequested()
    signal openProjectRequested()
    // Reopen a recent workspace by its project-file path.
    signal openRecentProjectRequested(string path)
    // Transient status line for project save/open outcomes.
    property string projectStatus: ""
    // Recent workspaces: [{path, name}], most-recent first.
    property var recentProjects: []

    required property string bridgeUrl

    Component.onCompleted: refreshRecentRequested()

    padding: 0
    titleDelegate: Item {}
    globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

    readonly property color themeBg: Kirigami.Theme.backgroundColor
    readonly property color themeBgAlt: Kirigami.Theme.alternateBackgroundColor
    readonly property color themeText: Kirigami.Theme.textColor
    readonly property color themeHighlight: Kirigami.Theme.highlightColor
    readonly property color themeHighlightedText: Kirigami.Theme.highlightedTextColor

    background: Rectangle { color: page.themeBg }

    // The instantiation site in Main.qml is responsible for binding
    // Kirigami.Theme.* to the live LinSync palette (root.active*).
    // We keep inherit:false here so descendants of this page use those
    // explicit values rather than whatever a deeper ancestor scope sets.
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    readonly property color separator: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.2)

    readonly property var tabs: page.sessionState && page.sessionState.tabs ? page.sessionState.tabs : []
    readonly property var recentPaths: page.sessionState && page.sessionState.recent_paths ? page.sessionState.recent_paths : []

    function pathLabel(p) {
        if (!p || p === "")
            return qsTr("(not set)")
        const parts = String(p).split("/")
        return parts[parts.length - 1] || String(p)
    }

    function modeIcon(mode) {
        switch (String(mode || "Text")) {
            case "Folder": return "folder"
            case "Table":  return "view-table"
            case "Hex":    return "format-text-code"
            case "Image":  return "image-x-generic"
            case "Document": return "x-office-document"
            case "Webpage": return "internet-web-browser"
            default:       return "text-x-generic"
        }
    }

    ColumnLayout {
        width: page.width
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 76
            color: page.themeBgAlt
            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: 1
                color: page.separator
            }
            ColumnLayout {
                anchors.fill: parent
                anchors.leftMargin: 24
                anchors.rightMargin: 24
                spacing: 1
                Controls.Label {
                    text: qsTr("Sessions")
                    font.pixelSize: 22
                    font.bold: true
                    font.letterSpacing: 0
                }
                Controls.Label {
                    text: qsTr("%1 open tab%2 · %3 recent path%4")
                        .arg(page.tabs.length).arg(page.tabs.length === 1 ? "" : "s")
                        .arg(page.recentPaths.length).arg(page.recentPaths.length === 1 ? "" : "s")
                    opacity: 0.6
                    font.pixelSize: 12
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    AppButton {
                        visible: page.tabs.length > 0
                        icon.name: "document-save"
                        text: qsTr("Save session")
                        onClicked: {
                            var req = new XMLHttpRequest()
                            var title = "Session " + new Date().toLocaleString()
                            req.onreadystatechange = function () {
                                if (req.readyState === XMLHttpRequest.DONE && req.status === 200)
                                    page.refreshRecentRequested()
                            }
                            req.open("GET", page.bridgeUrl + "/sessions/save?title=" + encodeURIComponent(title))
                            req.send()
                        }
                    }
                    AppButton {
                        visible: page.tabs.length > 0
                        icon.name: "project-development-new-template"
                        text: qsTr("Save project…")
                        onClicked: page.saveProjectRequested()
                    }
                    AppButton {
                        icon.name: "document-open"
                        text: qsTr("Open project…")
                        onClicked: page.openProjectRequested()
                    }
                    Item { Layout.fillWidth: true }
                    Controls.Label {
                        visible: page.projectStatus.length > 0
                        text: page.projectStatus
                        opacity: 0.75
                        font.pixelSize: 12
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 24
            Layout.rightMargin: 24
            Layout.topMargin: 20
            spacing: 16

            Card {
                Layout.fillWidth: true
                title: qsTr("Open tabs")
                subtitle: page.tabs.length === 0
                    ? qsTr("No active comparison tabs. Open files from the Compare page or pick a recent pair below.")
                    : qsTr("Switch between active comparisons or close tabs you're done with.")

                Repeater {
                    model: page.tabs

                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.preferredHeight: 56
                        radius: 6
                        color: modelData.id === page.activeTabId
                            ? Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themeHighlight, 0.12)
                            : page.themeBgAlt
                        border.color: modelData.id === page.activeTabId
                            ? page.themeHighlight
                            : page.separator
                        border.width: 1

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 8
                            spacing: 10

                            Kirigami.Icon {
                                Layout.preferredWidth: 24
                                Layout.preferredHeight: 24
                                source: page.modeIcon(modelData.mode)
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 1

                                RowLayout {
                                    spacing: 6
                                    Controls.Label {
                                        text: (modelData.left_dirty || modelData.right_dirty ? "● " : "") + (modelData.title || qsTr("Compare"))
                                        font.bold: true
                                        font.pixelSize: 13
                                    }
                                    Rectangle {
                                        visible: modelData.id === page.activeTabId
                                        Layout.preferredHeight: 18
                                        Layout.preferredWidth: activeBadge.implicitWidth + 14
                                        radius: 9
                                        color: page.themeHighlight
                                        Controls.Label {
                                            id: activeBadge
                                            anchors.centerIn: parent
                                            text: qsTr("active")
                                            color: page.themeHighlightedText
                                            font.pixelSize: 9
                                            font.bold: true
                                        }
                                    }
                                }

                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: (page.pathLabel(modelData.left_path)) + "  ↔  " + (page.pathLabel(modelData.right_path))
                                    opacity: 0.7
                                    font.pixelSize: 11
                                    font.family: "monospace"
                                    elide: Text.ElideMiddle
                                }
                            }

                            AppButton {
                                visible: modelData.id !== page.activeTabId
                                flat: true
                                icon.name: "go-jump"
                                text: qsTr("Switch")
                                display: Controls.AbstractButton.IconOnly
                                Controls.ToolTip.text: qsTr("Activate this tab")
                                Controls.ToolTip.visible: hovered
                                onClicked: page.tabActivated(modelData.id)
                            }
                            AppButton {
                                flat: true
                                icon.name: "window-close"
                                text: qsTr("Close")
                                display: Controls.AbstractButton.IconOnly
                                Controls.ToolTip.text: qsTr("Close tab")
                                Controls.ToolTip.visible: hovered
                                onClicked: page.tabClosed(modelData.id)
                            }
                        }
                    }
                }

                Item {
                    visible: page.tabs.length === 0
                    Layout.fillWidth: true
                    Layout.preferredHeight: 150
                    Layout.bottomMargin: 24

                    ColumnLayout {
                        anchors.centerIn: parent
                        width: parent.width - 20
                        spacing: 12

                        Kirigami.PlaceholderMessage {
                            Layout.fillWidth: true
                            text: qsTr("No active tabs")
                            explanation: qsTr("Comparisons you open from the Compare page will appear here.")
                        }
                        // A themed AppButton instead of PlaceholderMessage's
                        // built-in helpfulAction button, which rendered with
                        // the unthemed Fusion palette.
                        AppButton {
                            Layout.alignment: Qt.AlignHCenter
                            icon.name: "view-split-left-right"
                            text: qsTr("Go to Compare")
                            onClicked: page.navigateRequested(0)
                        }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Recent comparisons")
                subtitle: page.recentSessions.length === 0
                    ? qsTr("Comparisons you've opened recently will appear here. Click Reopen to launch a remembered pair as a fresh tab.")
                    : qsTr("Reopen a past comparison as a new tab.")

                Repeater {
                    model: page.recentSessions
                    delegate: Rectangle {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        Layout.preferredHeight: 56
                        radius: 6
                        color: index % 2 === 0 ? page.themeBg : page.themeBgAlt
                        border.color: page.separator
                        border.width: 1

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 8
                            spacing: 10

                            Kirigami.Icon {
                                Layout.preferredWidth: 24
                                Layout.preferredHeight: 24
                                source: page.modeIcon(modelData.mode)
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 1

                                Controls.Label {
                                    text: modelData.title && modelData.title !== ""
                                        ? modelData.title
                                        : (page.pathLabel(modelData.left) + "  ↔  " + page.pathLabel(modelData.right))
                                    font.bold: true
                                    font.pixelSize: 13
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: modelData.left + "  ↔  " + modelData.right
                                    opacity: 0.6
                                    font.family: "monospace"
                                    font.pixelSize: 10
                                    elide: Text.ElideMiddle
                                }
                            }

                            // Last-known comparison outcome, when recorded.
                            Controls.Label {
                                visible: !!modelData.lastResult
                                text: modelData.lastResult
                                    ? (modelData.lastResult.equal
                                        ? qsTr("equal")
                                        : qsTr("%1 diff").arg(modelData.lastResult.differenceCount))
                                    : ""
                                color: (modelData.lastResult && modelData.lastResult.equal)
                                    ? Kirigami.Theme.positiveTextColor
                                    : Kirigami.Theme.neutralTextColor
                                font.pixelSize: 11
                            }

                            AppButton {
                                flat: true
                                icon.name: "document-open-recent"
                                text: qsTr("Reopen")
                                onClicked: page.reopenRecentRequested(modelData.index !== undefined ? modelData.index : index)
                            }
                        }
                    }
                }

                Item {
                    visible: page.recentSessions.length === 0
                    Layout.fillWidth: true
                    Layout.preferredHeight: 60
                    Kirigami.PlaceholderMessage {
                        anchors.centerIn: parent
                        width: parent.width - 20
                        text: qsTr("No remembered comparisons yet")
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                visible: page.recentProjects.length > 0
                title: qsTr("Recent workspaces")
                subtitle: qsTr("Reopen a saved project to restore its comparisons as tabs.")

                Repeater {
                    model: page.recentProjects
                    delegate: Rectangle {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        Layout.preferredHeight: 36
                        radius: 4
                        color: index % 2 === 0 ? page.themeBg : page.themeBgAlt

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 8
                            spacing: 10

                            Kirigami.Icon {
                                Layout.preferredWidth: 18
                                Layout.preferredHeight: 18
                                source: "project-development"
                                opacity: 0.7
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: modelData.name || page.pathLabel(modelData.path)
                                elide: Text.ElideRight
                                color: page.themeText
                            }
                            Controls.Label {
                                text: page.pathLabel(modelData.path)
                                opacity: 0.5
                                font.pixelSize: 10
                                font.family: "monospace"
                            }
                            AppButton {
                                text: qsTr("Open")
                                onClicked: page.openRecentProjectRequested(modelData.path)
                            }
                        }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Recent paths")
                subtitle: page.recentPaths.length === 0
                    ? qsTr("Files and folders you've recently compared will be remembered here.")
                    : qsTr("Use the copy button to reuse a path in the Compare bar.")

                Repeater {
                    model: page.recentPaths
                    delegate: Rectangle {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        Layout.preferredHeight: 36
                        radius: 4
                        color: index % 2 === 0 ? page.themeBg : page.themeBgAlt

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 8
                            spacing: 10

                            Kirigami.Icon {
                                Layout.preferredWidth: 18
                                Layout.preferredHeight: 18
                                source: "document-open-recent"
                                opacity: 0.7
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: String(modelData)
                                font.family: "monospace"
                                font.pixelSize: 11
                                elide: Text.ElideMiddle
                            }
                            Controls.ToolButton {
                                icon.name: "edit-copy"
                                text: qsTr("Copy path")
                                display: Controls.AbstractButton.IconOnly
                                Accessible.name: qsTr("Copy path")
                                Controls.ToolTip.text: qsTr("Copy path")
                                Controls.ToolTip.visible: hovered
                                onClicked: pathClipboard.text = String(modelData)
                            }
                        }
                    }
                }

                Item {
                    visible: page.recentPaths.length === 0
                    Layout.fillWidth: true
                    Layout.preferredHeight: 60
                    Kirigami.PlaceholderMessage {
                        anchors.centerIn: parent
                        width: parent.width - 20
                        text: qsTr("No recent paths yet")
                    }
                }
            }
        }
    }

    TextEdit {
        id: pathClipboard
        visible: false
        onTextChanged: {
            if (text === "") return
            selectAll()
            copy()
            text = ""
        }
    }
}
