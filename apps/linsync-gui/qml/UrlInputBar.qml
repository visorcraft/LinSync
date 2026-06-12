// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Layouts

// Two URL fields plus an action button, shared by WebpageComparePage.
RowLayout {
    id: ui

    property string leftLabel: qsTr("Left URL")
    property string rightLabel: qsTr("Right URL")
    property string leftText: ""
    property string rightText: ""
    property string actionText: qsTr("Compare…")
    property string actionAccessibleName: actionText
    property string actionIcon: "internet-web-browser-symbolic"
    property bool actionEnabled: true

    required property color textColor
    required property color disabledTextColor
    required property color fieldColor
    required property color borderColor

    signal leftTextEdited(string text)
    signal rightTextEdited(string text)
    signal actionActivated()

    spacing: 8

    AppTextField {
        id: leftField
        Layout.fillWidth: true
        implicitHeight: 36
        text: ui.leftText
        placeholderText: ui.leftLabel
        color: ui.textColor
        placeholderTextColor: ui.disabledTextColor
        Accessible.name: ui.leftLabel
        onTextChanged: ui.leftTextEdited(text)
        background: Rectangle {
            color: ui.fieldColor
            border.color: ui.borderColor
            border.width: 1
            radius: 4
        }
    }

    AppTextField {
        id: rightField
        Layout.fillWidth: true
        implicitHeight: 36
        text: ui.rightText
        placeholderText: ui.rightLabel
        color: ui.textColor
        placeholderTextColor: ui.disabledTextColor
        Accessible.name: ui.rightLabel
        onTextChanged: ui.rightTextEdited(text)
        background: Rectangle {
            color: ui.fieldColor
            border.color: ui.borderColor
            border.width: 1
            radius: 4
        }
    }

    AppButton {
        Layout.preferredHeight: 30
        text: ui.actionText
        enabled: ui.actionEnabled
        icon.name: ui.actionIcon
        Accessible.name: ui.actionAccessibleName
        onClicked: ui.actionActivated()
    }
}
