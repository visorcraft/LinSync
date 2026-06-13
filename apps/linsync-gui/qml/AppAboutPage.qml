// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    property string appVersion: "1.13.1"

    signal navigateRequested(int section)
    signal creditsRequested()
    signal licensesRequested()

    padding: 0
    titleDelegate: Item {}
    globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

    readonly property color themeBg: Kirigami.Theme.backgroundColor
    readonly property color themeBgAlt: Kirigami.Theme.alternateBackgroundColor
    readonly property color themeText: Kirigami.Theme.textColor
    readonly property color themeHighlight: Kirigami.Theme.highlightColor
    readonly property string appIconSource: Window.window && Window.window.appIconSource !== undefined
        ? Window.window.appIconSource : Qt.resolvedUrl("assets/com.visorcraft.LinSync.png")

    background: Rectangle { color: page.themeBg }

    // The instantiation site in Main.qml is responsible for binding
    // Kirigami.Theme.* to the live LinSync palette (root.active*).
    // We keep inherit:false here so descendants of this page use those
    // explicit values rather than whatever a deeper ancestor scope sets.
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    readonly property color separator: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.2)
    readonly property color accentMute: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeHighlight, 0.18)

    readonly property var features: [
        { icon: "view-split-left-right", title: qsTr("Side-by-side compare"),
          body: qsTr("Synchronised scrolling, inline diff highlights, three-way merge support.") },
        { icon: "folder-sync",            title: qsTr("Folder diff"),
          body: qsTr("Recursive directory comparison with .gitignore-aware include + exclude globs.") },
        { icon: "format-text-code",       title: qsTr("Plugin-driven engines"),
          body: qsTr("Built-in text, folder, table, and hex comparison with protocol hooks for helper engines.") },
        { icon: "preferences-system",     title: qsTr("Native Linux integration"),
          body: qsTr("File-manager open-with/reveal, desktop entries, XDG storage, and documented sandbox constraints.") }
    ]

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
                    text: qsTr("About")
                    font.pixelSize: 22
                    font.bold: true
                    font.letterSpacing: 0
                }
                Controls.Label {
                    text: qsTr("Built on Rust + Qt 6 / Kirigami.")
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

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 168
                radius: 10
                color: page.themeBg
                border.color: page.separator
                border.width: 1

                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: 240
                    radius: parent.radius
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0.0; color: page.accentMute }
                        GradientStop { position: 1.0; color: "transparent" }
                    }
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 24
                    anchors.rightMargin: 24
                    spacing: 20

                    Image {
                        Layout.preferredWidth: 112
                        Layout.preferredHeight: 112
                        Layout.alignment: Qt.AlignVCenter
                        source: page.appIconSource
                        sourceSize.width: 224
                        sourceSize.height: 224
                        fillMode: Image.PreserveAspectFit
                        smooth: true
                        mipmap: true
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 4
                        Controls.Label {
                            text: "LinSync"
                            font.pixelSize: 32
                            font.bold: true
                            font.letterSpacing: 0
                        }
                        Controls.Label {
                            text: qsTr("Visual file and folder comparison for Linux with a native KDE feel.")
                            opacity: 0.7
                            font.pixelSize: 13
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                        }
                        RowLayout {
                            spacing: 6
                            Layout.topMargin: 8

                            Rectangle {
                                radius: 12
                                color: page.accentMute
                                border.color: page.themeHighlight
                                border.width: 1
                                implicitHeight: 26
                                implicitWidth: versionLabel.implicitWidth + 24
                                Controls.Label {
                                    id: versionLabel
                                    anchors.centerIn: parent
                                    text: qsTr("v%1").arg(page.appVersion)
                                    font.pixelSize: 11
                                    font.bold: true
                                    color: page.themeHighlight
                                    font.family: "monospace"
                                }
                            }

                            Rectangle {
                                radius: 12
                                color: page.themeBgAlt
                                border.color: page.separator
                                border.width: 1
                                implicitHeight: 26
                                implicitWidth: gplLabel.implicitWidth + 24
                                Controls.Label {
                                    id: gplLabel
                                    anchors.centerIn: parent
                                    text: qsTr("GPL v3")
                                    font.pixelSize: 11
                                    opacity: 0.85
                                }
                            }

                            Rectangle {
                                radius: 12
                                color: page.themeBgAlt
                                border.color: page.separator
                                border.width: 1
                                implicitHeight: 26
                                implicitWidth: platformLabel.implicitWidth + 24
                                Controls.Label {
                                    id: platformLabel
                                    anchors.centerIn: parent
                                    text: qsTr("Linux · Qt 6")
                                    font.pixelSize: 11
                                    font.family: "monospace"
                                    opacity: 0.85
                                }
                            }
                        }
                    }
                }
            }

            Controls.Label {
                text: qsTr("WHAT'S INSIDE")
                font.pixelSize: 10
                font.bold: true
                font.letterSpacing: 0
                opacity: 0.5
                Layout.topMargin: 12
            }

            GridLayout {
                Layout.fillWidth: true
                columns: 2
                columnSpacing: 12
                rowSpacing: 12

                Repeater {
                    model: page.features
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.preferredHeight: 84
                        radius: 8
                        color: page.themeBg
                        border.color: page.separator
                        border.width: 1
                        RowLayout {
                            anchors.fill: parent
                            anchors.margins: 12
                            spacing: 12
                            Rectangle {
                                Layout.preferredWidth: 44
                                Layout.preferredHeight: 44
                                Layout.alignment: Qt.AlignVCenter
                                radius: 8
                                color: page.accentMute
                                border.color: Qt.rgba(page.themeHighlight.r,
                                                      page.themeHighlight.g,
                                                      page.themeHighlight.b, 0.35)
                                border.width: 1
                                Kirigami.Icon {
                                    anchors.centerIn: parent
                                    source: modelData.icon
                                    implicitWidth: 22
                                    implicitHeight: 22
                                    color: page.themeHighlight
                                    isMask: true
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignVCenter
                                spacing: 2
                                Controls.Label {
                                    text: modelData.title
                                    font.pixelSize: 13
                                    font.bold: true
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: modelData.body
                                    font.pixelSize: 11
                                    opacity: 0.65
                                    wrapMode: Text.WordWrap
                                    elide: Text.ElideRight
                                    maximumLineCount: 2
                                }
                            }
                        }
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.topMargin: 12
                Layout.preferredHeight: 96
                radius: 8
                color: page.themeBg
                border.color: page.separator
                border.width: 1

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 14
                    anchors.rightMargin: 14
                    spacing: 12

                    Rectangle {
                        Layout.preferredWidth: 56
                        Layout.preferredHeight: 56
                        Layout.alignment: Qt.AlignVCenter
                        radius: 8
                        color: page.themeBgAlt
                        border.color: page.separator
                        border.width: 1
                        Kirigami.Icon {
                            anchors.centerIn: parent
                            source: page.appIconSource
                            implicitWidth: 36
                            implicitHeight: 36
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        spacing: 2
                        Controls.Label {
                            text: qsTr("LinSync is built for focused comparison work.")
                            font.pixelSize: 13
                            font.bold: true
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            text: qsTr("Compare files and folders, review changes, and merge text edits through a Rust core with a Qt/Kirigami shell.")
                            font.pixelSize: 12
                            opacity: 0.75
                        }
                    }

                    Controls.Button {
                        id: visitBtn
                        Layout.alignment: Qt.AlignVCenter
                        icon.name: "go-next-symbolic"
                        text: qsTr("Visit LinSync")
                        display: Controls.AbstractButton.TextBesideIcon
                        contentItem: RowLayout {
                            spacing: 6
                            Kirigami.Icon {
                                source: visitBtn.icon.name
                                color: page.themeText
                                isMask: true
                                implicitWidth: 16
                                implicitHeight: 16
                            }
                            Controls.Label { text: visitBtn.text; color: page.themeText }
                        }
                        background: Rectangle {
                            color: visitBtn.hovered ? Qt.darker(page.themeBgAlt, 1.05) : page.themeBgAlt
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        onClicked: Qt.openUrlExternally("https://github.com/visorcraft/LinSync")
                    }
                }
            }

            AppCard {
                Layout.fillWidth: true
                title: qsTr("Licenses & Credits")
                subtitle: qsTr("Every direct + transitive crate, with version and license expression, is documented in docs/third-party-notices.md.")
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    Controls.Button {
                        id: creditsBtn
                        icon.name: "view-list-details"
                        text: qsTr("Credits")
                        display: Controls.AbstractButton.TextBesideIcon
                        contentItem: RowLayout {
                            spacing: 6
                            Kirigami.Icon {
                                source: creditsBtn.icon.name
                                color: page.themeText
                                isMask: true
                                implicitWidth: 16
                                implicitHeight: 16
                            }
                            Controls.Label { text: creditsBtn.text; color: page.themeText }
                        }
                        background: Rectangle {
                            color: creditsBtn.hovered ? Qt.darker(page.themeBgAlt, 1.05) : page.themeBgAlt
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        onClicked: page.creditsRequested()
                    }
                    Controls.Button {
                        id: viewLicBtn
                        icon.name: "view-list-text"
                        text: qsTr("Licenses")
                        display: Controls.AbstractButton.TextBesideIcon
                        contentItem: RowLayout {
                            spacing: 6
                            Kirigami.Icon {
                                source: viewLicBtn.icon.name
                                color: page.themeText
                                isMask: true
                                implicitWidth: 16
                                implicitHeight: 16
                            }
                            Controls.Label { text: viewLicBtn.text; color: page.themeText }
                        }
                        background: Rectangle {
                            color: viewLicBtn.hovered ? Qt.darker(page.themeBgAlt, 1.05) : page.themeBgAlt
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        onClicked: page.licensesRequested()
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            Controls.Label {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 14
                Layout.bottomMargin: 24
                textFormat: Text.RichText
                text: qsTr("Built by <b>VisorCraft</b>") + "  ·  " + qsTr("Powered by Rust, Qt 6, and Kirigami")
                font.pixelSize: 11
                opacity: 0.55
            }
        }
    }
}
