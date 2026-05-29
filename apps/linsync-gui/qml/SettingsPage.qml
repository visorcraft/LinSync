// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    readonly property color themeBg: Kirigami.Theme.backgroundColor
    readonly property color themeBgAlt: Kirigami.Theme.alternateBackgroundColor
    readonly property color themeText: Kirigami.Theme.textColor
    readonly property color themeHighlight: Kirigami.Theme.highlightColor
    readonly property color themeHighlightedText: Kirigami.Theme.highlightedTextColor

    property int themePreference: 0
    property var themeValues: themeTokens.themeValues
    property var themeLabels: themeTokens.themeLabels
    property int fontSize: 12
    property string fontFamily: "monospace"
    property int tabWidth: 4
    property bool showLineNumbers: true
    property bool showWhitespace: false
    property bool wordWrap: false
    property bool ignoreCase: false
    property bool ignoreWhitespace: false
    property bool ignoreBlankLines: false
    property bool ignoreEol: true
    property bool detectMoves: false
    property string eolNormalization: "auto"
    property string defaultCompareMode: "Text"
    property bool openLastSession: true
    property bool confirmOnClose: true
    property bool persistRecentPaths: true
    property int maxRecentPaths: 20
    property bool bridgeConnected: false

    signal settingChanged(string key, var value)
    signal openConfigFolderRequested()

    padding: 0
    titleDelegate: Item {}
    globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

    background: Rectangle { color: page.themeBg }

    // The instantiation site in Main.qml is responsible for binding
    // Kirigami.Theme.* to the live LinSync palette (root.active*).
    // We keep inherit:false here so descendants of this page use those
    // explicit values rather than whatever a deeper ancestor scope sets.
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.Window

    readonly property color separator: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.2)

    DesignTokens { id: themeTokens }

    function emit(key, value) {
        page.settingChanged(key, value)
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
                    text: qsTr("Settings")
                    font.pixelSize: 22
                    font.bold: true
                    font.letterSpacing: 0
                }
                Controls.Label {
                    text: qsTr("Stored on disk per the XDG base directory spec — see docs/settings-storage-decision.md.")
                    opacity: 0.6
                    font.pixelSize: 12
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
                title: qsTr("Appearance")
                subtitle: qsTr("Theme, fonts, and pane decoration.")

                Kirigami.FormLayout {
                    Layout.fillWidth: true

                    AppComboBox {
                        indicator: Kirigami.Icon {
                            x: parent.width - width - 8
                            y: (parent.height - height) / 2
                            width: 16
                            height: 16
                            source: "arrow-down"
                            color: page.themeText !== undefined ? page.themeText : (root.activeText !== undefined ? root.activeText : Kirigami.Theme.textColor)
                            isMask: true
                        }
                        Layout.preferredWidth: 240
                        implicitWidth: 240
                        implicitHeight: 36
                        contentItem: Controls.Label {
                            text: parent.displayText
                            leftPadding: 8
                            rightPadding: 28
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                            elide: Text.ElideRight
                        }
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        palette.text: page.themeText
                        palette.windowText: page.themeText
                        palette.buttonText: page.themeText
                        Kirigami.FormData.label: qsTr("Color scheme")
                        model: page.themeLabels
                        currentIndex: Math.max(0, page.themeValues.indexOf(page.themePreference))
                        onActivated: {
                            const value = page.themeValues[currentIndex]
                            page.themePreference = value
                            page.emit("themePreference", value)
                        }
                    }

                    AppComboBox {
                        indicator: Kirigami.Icon {
                            x: parent.width - width - 8
                            y: (parent.height - height) / 2
                            width: 16
                            height: 16
                            source: "arrow-down"
                            color: page.themeText !== undefined ? page.themeText : (root.activeText !== undefined ? root.activeText : Kirigami.Theme.textColor)
                            isMask: true
                        }
                        Layout.preferredWidth: 200
                        implicitWidth: 200
                        implicitHeight: 36
                        contentItem: Controls.Label {
                            text: parent.displayText
                            leftPadding: 8
                            rightPadding: 28
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                            elide: Text.ElideRight
                        }
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        palette.text: page.themeText
                        palette.windowText: page.themeText
                        palette.buttonText: page.themeText
                        Kirigami.FormData.label: qsTr("Pane font family")
                        model: ["monospace", "Inconsolata", "DejaVu Sans Mono", "Fira Code", "JetBrains Mono", "Source Code Pro"]
                        currentIndex: Math.max(0, model.indexOf(page.fontFamily))
                        onActivated: {
                            page.fontFamily = model[currentIndex]
                            page.emit("fontFamily", page.fontFamily)
                        }
                    }

                    AppSpinBox {
                        implicitHeight: 36
implicitWidth: 140
leftPadding: 36
rightPadding: 36
down.indicator: Rectangle {
    x: 0
    width: 32
    height: parent.height
    radius: 4
    color: parent.down.pressed
        ? Qt.darker(page.themeBgAlt, 1.15)
        : (parent.down.hovered ? page.themeBgAlt : "transparent")
    Rectangle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.topMargin: 5
        anchors.bottomMargin: 5
        width: 1
        color: page.separator
    }
    Kirigami.Icon {
        anchors.centerIn: parent
        width: 14
        height: 14
        source: "arrow-down"
        color: page.themeText
        isMask: true
    }
    MouseArea {
        anchors.fill: parent
        onClicked: parent.parent.decrease()
    }
}
up.indicator: Rectangle {
    x: parent.width - width
    width: 32
    height: parent.height
    radius: 4
    color: parent.up.pressed
        ? Qt.darker(page.themeBgAlt, 1.15)
        : (parent.up.hovered ? page.themeBgAlt : "transparent")
    Rectangle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.topMargin: 5
        anchors.bottomMargin: 5
        width: 1
        color: page.separator
    }
    Kirigami.Icon {
        anchors.centerIn: parent
        width: 14
        height: 14
        source: "arrow-up"
        color: page.themeText
        isMask: true
    }
    MouseArea {
        anchors.fill: parent
        onClicked: parent.parent.increase()
    }
}
                        contentItem: TextInput {
                            text: parent.displayText
                            horizontalAlignment: Qt.AlignHCenter
                            verticalAlignment: Qt.AlignVCenter
                            color: page.themeText
                            readOnly: !parent.editable
                            validator: parent.validator
                            inputMethodHints: Qt.ImhFormattedNumbersOnly
                        }
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        palette.text: page.themeText
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Pane font size")
                        from: 8
                        to: 28
                        value: page.fontSize
                        onValueModified: {
                            page.fontSize = value
                            page.emit("fontSize", value)
                        }
                    }

                    AppSpinBox {
                        implicitHeight: 36
implicitWidth: 140
leftPadding: 36
rightPadding: 36
down.indicator: Rectangle {
    x: 0
    width: 32
    height: parent.height
    radius: 4
    color: parent.down.pressed
        ? Qt.darker(page.themeBgAlt, 1.15)
        : (parent.down.hovered ? page.themeBgAlt : "transparent")
    Rectangle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.topMargin: 5
        anchors.bottomMargin: 5
        width: 1
        color: page.separator
    }
    Kirigami.Icon {
        anchors.centerIn: parent
        width: 14
        height: 14
        source: "arrow-down"
        color: page.themeText
        isMask: true
    }
    MouseArea {
        anchors.fill: parent
        onClicked: parent.parent.decrease()
    }
}
up.indicator: Rectangle {
    x: parent.width - width
    width: 32
    height: parent.height
    radius: 4
    color: parent.up.pressed
        ? Qt.darker(page.themeBgAlt, 1.15)
        : (parent.up.hovered ? page.themeBgAlt : "transparent")
    Rectangle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.topMargin: 5
        anchors.bottomMargin: 5
        width: 1
        color: page.separator
    }
    Kirigami.Icon {
        anchors.centerIn: parent
        width: 14
        height: 14
        source: "arrow-up"
        color: page.themeText
        isMask: true
    }
    MouseArea {
        anchors.fill: parent
        onClicked: parent.parent.increase()
    }
}
                        contentItem: TextInput {
                            text: parent.displayText
                            horizontalAlignment: Qt.AlignHCenter
                            verticalAlignment: Qt.AlignVCenter
                            color: page.themeText
                            readOnly: !parent.editable
                            validator: parent.validator
                            inputMethodHints: Qt.ImhFormattedNumbersOnly
                        }
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        palette.text: page.themeText
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Tab width")
                        from: 1
                        to: 12
                        value: page.tabWidth
                        onValueModified: {
                            page.tabWidth = value
                            page.emit("tabWidth", value)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Line numbers")
                        text: qsTr("Show on both panes")
                        checked: page.showLineNumbers
                        onToggled: {
                            page.showLineNumbers = checked
                            page.emit("showLineNumbers", checked)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Whitespace")
                        text: qsTr("Render spaces and tabs")
                        checked: page.showWhitespace
                        onToggled: {
                            page.showWhitespace = checked
                            page.emit("showWhitespace", checked)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Long lines")
                        text: qsTr("Wrap instead of horizontal scroll")
                        checked: page.wordWrap
                        onToggled: {
                            page.wordWrap = checked
                            page.emit("wordWrap", checked)
                        }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Comparison behavior")
                subtitle: qsTr("Defaults applied when a new comparison starts.")

                Kirigami.FormLayout {
                    Layout.fillWidth: true

                    AppComboBox {
                        indicator: Kirigami.Icon {
                            x: parent.width - width - 8
                            y: (parent.height - height) / 2
                            width: 16
                            height: 16
                            source: "arrow-down"
                            color: page.themeText !== undefined ? page.themeText : (root.activeText !== undefined ? root.activeText : Kirigami.Theme.textColor)
                            isMask: true
                        }
                        Layout.preferredWidth: 200
                        implicitWidth: 200
                        implicitHeight: 36
                        contentItem: Controls.Label {
                            text: parent.displayText
                            leftPadding: 8
                            rightPadding: 28
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                            elide: Text.ElideRight
                        }
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        palette.text: page.themeText
                        palette.windowText: page.themeText
                        palette.buttonText: page.themeText
                        Kirigami.FormData.label: qsTr("Default mode")
                        model: ["Text", "Folder", "Table", "Hex"]
                        currentIndex: Math.max(0, model.indexOf(page.defaultCompareMode))
                        onActivated: {
                            page.defaultCompareMode = model[currentIndex]
                            page.emit("defaultCompareMode", page.defaultCompareMode)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Case")
                        text: qsTr("Ignore case differences")
                        checked: page.ignoreCase
                        onToggled: {
                            page.ignoreCase = checked
                            page.emit("ignoreCase", checked)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Whitespace")
                        text: qsTr("Ignore leading + trailing whitespace")
                        checked: page.ignoreWhitespace
                        onToggled: {
                            page.ignoreWhitespace = checked
                            page.emit("ignoreWhitespace", checked)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Blank lines")
                        text: qsTr("Treat empty lines as equal")
                        checked: page.ignoreBlankLines
                        onToggled: {
                            page.ignoreBlankLines = checked
                            page.emit("ignoreBlankLines", checked)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Line endings")
                        text: qsTr("Treat CR / LF / CRLF as equal")
                        checked: page.ignoreEol
                        onToggled: {
                            page.ignoreEol = checked
                            page.emit("ignoreEol", checked)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Moved blocks")
                        text: qsTr("Detect reordered sections")
                        checked: page.detectMoves
                        onToggled: {
                            page.detectMoves = checked
                            page.emit("detectMoves", checked)
                        }
                    }

                    AppComboBox {
                        indicator: Kirigami.Icon {
                            x: parent.width - width - 8
                            y: (parent.height - height) / 2
                            width: 16
                            height: 16
                            source: "arrow-down"
                            color: page.themeText !== undefined ? page.themeText : (root.activeText !== undefined ? root.activeText : Kirigami.Theme.textColor)
                            isMask: true
                        }
                        Layout.preferredWidth: 200
                        implicitWidth: 200
                        implicitHeight: 36
                        contentItem: Controls.Label {
                            text: parent.displayText
                            leftPadding: 8
                            rightPadding: 28
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                            elide: Text.ElideRight
                        }
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        palette.text: page.themeText
                        palette.windowText: page.themeText
                        palette.buttonText: page.themeText
                        Kirigami.FormData.label: qsTr("EOL on save")
                        model: [qsTr("Auto-detect"), qsTr("LF (Unix)"), qsTr("CRLF (Windows)"), qsTr("CR (Classic)")]
                        currentIndex: Math.max(0, ["auto", "lf", "crlf", "cr"].indexOf(page.eolNormalization))
                        onActivated: {
                            const value = ["auto", "lf", "crlf", "cr"][currentIndex]
                            page.eolNormalization = value
                            page.emit("eolNormalization", value)
                        }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Session")
                subtitle: qsTr("How LinSync remembers and reopens your work between launches.")

                Kirigami.FormLayout {
                    Layout.fillWidth: true

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("On launch")
                        text: qsTr("Restore last session")
                        checked: page.openLastSession
                        onToggled: {
                            page.openLastSession = checked
                            page.emit("openLastSession", checked)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Unsaved changes")
                        text: qsTr("Prompt before closing a dirty tab")
                        checked: page.confirmOnClose
                        onToggled: {
                            page.confirmOnClose = checked
                            page.emit("confirmOnClose", checked)
                        }
                    }

                    AppCheckBox {
                        indicator: Rectangle {
                            implicitWidth: 18
                            implicitHeight: 18
                            x: parent.leftPadding
                            y: (parent.height - height) / 2
                            radius: 3
                            color: parent.checked ? page.themeHighlight : page.themeBg
                            border.color: parent.checked ? page.themeHighlight : page.separator
                            border.width: 1
                            Controls.Label {
                                anchors.centerIn: parent
                                visible: parent.parent.checked
                                text: "\u2713"
                                font.pixelSize: 14
                                font.bold: true
                                color: page.themeHighlightedText
                            }
                        }
                        contentItem: Controls.Label {
                            text: parent.text
                            leftPadding: parent.indicator.width + 8
                            verticalAlignment: Text.AlignVCenter
                            color: page.themeText
                        }
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Recent paths")
                        text: qsTr("Remember between launches")
                        checked: page.persistRecentPaths
                        onToggled: {
                            page.persistRecentPaths = checked
                            page.emit("persistRecentPaths", checked)
                        }
                    }

                    AppSpinBox {
                        implicitHeight: 36
implicitWidth: 140
leftPadding: 36
rightPadding: 36
down.indicator: Rectangle {
    x: 0
    width: 32
    height: parent.height
    radius: 4
    color: parent.down.pressed
        ? Qt.darker(page.themeBgAlt, 1.15)
        : (parent.down.hovered ? page.themeBgAlt : "transparent")
    Rectangle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.topMargin: 5
        anchors.bottomMargin: 5
        width: 1
        color: page.separator
    }
    Kirigami.Icon {
        anchors.centerIn: parent
        width: 14
        height: 14
        source: "arrow-down"
        color: page.themeText
        isMask: true
    }
    MouseArea {
        anchors.fill: parent
        onClicked: parent.parent.decrease()
    }
}
up.indicator: Rectangle {
    x: parent.width - width
    width: 32
    height: parent.height
    radius: 4
    color: parent.up.pressed
        ? Qt.darker(page.themeBgAlt, 1.15)
        : (parent.up.hovered ? page.themeBgAlt : "transparent")
    Rectangle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.topMargin: 5
        anchors.bottomMargin: 5
        width: 1
        color: page.separator
    }
    Kirigami.Icon {
        anchors.centerIn: parent
        width: 14
        height: 14
        source: "arrow-up"
        color: page.themeText
        isMask: true
    }
    MouseArea {
        anchors.fill: parent
        onClicked: parent.parent.increase()
    }
}
                        contentItem: TextInput {
                            text: parent.displayText
                            horizontalAlignment: Qt.AlignHCenter
                            verticalAlignment: Qt.AlignVCenter
                            color: page.themeText
                            readOnly: !parent.editable
                            validator: parent.validator
                            inputMethodHints: Qt.ImhFormattedNumbersOnly
                        }
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        palette.text: page.themeText
                        palette.windowText: page.themeText
                        Kirigami.FormData.label: qsTr("Max recent paths")
                        from: 5
                        to: 100
                        value: page.maxRecentPaths
                        enabled: page.persistRecentPaths
                        onValueModified: {
                            page.maxRecentPaths = value
                            page.emit("maxRecentPaths", value)
                        }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Storage")
                subtitle: qsTr("Configuration lives under $XDG_CONFIG_HOME/linsync/. Reset wipes only LinSync's own files.")

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    Controls.Button {
                        id: openConfigBtn
                        icon.name: "folder-open"
                        icon.color: page.themeText
                        text: qsTr("Open config folder")
                        display: Controls.AbstractButton.TextBesideIcon
                        enabled: page.bridgeConnected
                        Controls.ToolTip.text: page.bridgeConnected
                            ? qsTr("Opens $XDG_CONFIG_HOME/linsync/ in your file manager")
                            : qsTr("Settings bridge is not connected in this build")
                        Controls.ToolTip.visible: hovered
                        contentItem: RowLayout {
                            spacing: 6
                            Kirigami.Icon {
                                source: openConfigBtn.icon.name
                                color: page.themeText
                                isMask: true
                                opacity: openConfigBtn.enabled ? 0.85 : 0.4
                                implicitWidth: 16
                                implicitHeight: 16
                            }
                            Controls.Label {
                                text: openConfigBtn.text
                                color: page.themeText
                                opacity: openConfigBtn.enabled ? 1.0 : 0.4
                            }
                        }
                        background: Rectangle {
                            color: openConfigBtn.hovered ? Qt.darker(page.themeBgAlt, 1.05) : page.themeBgAlt
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        // The actual $XDG_CONFIG_HOME path lives in the Rust
                        // AppPaths; bubble up so the host can open it via xdg-open
                        // (or equivalent) instead of building a broken file:// URL.
                        onClicked: page.openConfigFolderRequested()
                    }
                    Controls.Button {
                        id: resetBtn
                        icon.name: "edit-undo"
                        text: qsTr("Reset to defaults")
                        display: Controls.AbstractButton.TextBesideIcon
                        contentItem: RowLayout {
                            spacing: 6
                            Kirigami.Icon {
                                source: resetBtn.icon.name
                                color: page.themeText
                                isMask: true
                                implicitWidth: 16
                                implicitHeight: 16
                            }
                            Controls.Label {
                                text: resetBtn.text
                                color: page.themeText
                            }
                        }
                        background: Rectangle {
                            color: resetBtn.hovered ? Qt.darker(page.themeBgAlt, 1.05) : page.themeBgAlt
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        onClicked: resetConfirm.open()
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                Layout.bottomMargin: 24
                wrapMode: Text.WordWrap
                opacity: 0.55
                font.pixelSize: 11
                text: qsTr("The storage backend (JSON on disk under XDG paths) is described in docs/settings-storage-decision.md.")
            }
        }
    }

    Kirigami.PromptDialog {
        id: resetConfirm
        title: qsTr("Reset settings?")
        subtitle: qsTr("This restores every option on this page to its default. Filters and plugin state are unaffected.")
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
        onAccepted: {
            page.emit("__reset", true)
        }
    }
}
