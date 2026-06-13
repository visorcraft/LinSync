// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Rectangle {
    id: card

    property string title: ""
    property string subtitle: ""
    default property alias contentChildren: contentColumn.children

    readonly property color themeBg: Kirigami.Theme.backgroundColor
    readonly property color themeText: Kirigami.Theme.textColor
    readonly property color separator: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.2)

    Layout.fillWidth: true
    Layout.preferredHeight: cardLayout.implicitHeight + 28
    radius: 8
    color: themeBg
    border.color: separator
    border.width: 1

    ColumnLayout {
        id: cardLayout
        anchors.fill: parent
        anchors.margins: 14
        spacing: 6

        Controls.Label {
            visible: card.title !== ""
            text: card.title
            font.pixelSize: 14
            font.bold: true
        }

        Controls.Label {
            visible: card.subtitle !== ""
            Layout.fillWidth: true
            text: card.subtitle
            wrapMode: Text.WordWrap
            opacity: 0.65
            font.pixelSize: 12
        }

        ColumnLayout {
            id: contentColumn
            Layout.fillWidth: true
            Layout.topMargin: (card.title !== "" || card.subtitle !== "") ? 6 : 0
            spacing: 6
        }
    }
}
