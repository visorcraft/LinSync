// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Window
import org.kde.kirigami as Kirigami

// SpinBox styled to match the Settings page "Pane font size" field: a themed
// frame with flat 32px stepper buttons on each side (arrow-down on the left,
// arrow-up on the right) and a centred numeric display.
//
// The four frame*/content*/step* colours default to Kirigami.Theme. NOTE:
// Kirigami.Theme colours MUTE toward the background when the control is
// `enabled: false`, which turns a disabled field near-black. Call sites whose
// spin boxes can be disabled (e.g. Image Compare's mode-gated Tolerance/ΔE)
// should pass STABLE plain colours (the page's active* properties) into these
// so the field stays themed while disabled.
Controls.SpinBox {
    id: control

    Kirigami.Theme.inherit: true
    Kirigami.Theme.colorSet: Kirigami.Theme.View

    property color frameColor: Kirigami.Theme.backgroundColor
    property color frameBorderColor: Kirigami.ColorUtils.tintWithAlpha(
        Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)
    property color contentColor: Kirigami.Theme.textColor
    property color stepHoverColor: Kirigami.Theme.alternateBackgroundColor

    palette.window: Kirigami.Theme.backgroundColor
    palette.base: Kirigami.Theme.backgroundColor
    palette.button: Kirigami.Theme.alternateBackgroundColor
    palette.text: Kirigami.Theme.textColor
    palette.windowText: Kirigami.Theme.textColor
    palette.buttonText: Kirigami.Theme.textColor
    palette.highlight: Kirigami.Theme.highlightColor
    palette.highlightedText: Kirigami.Theme.highlightedTextColor

    implicitHeight: 36
    implicitWidth: 140
    leftPadding: 36
    rightPadding: 36

    down.indicator: Rectangle {
        x: 0
        width: 32
        height: parent.height
        radius: 4
        color: control.down.pressed
            ? Qt.darker(control.stepHoverColor, 1.15)
            : (control.down.hovered ? control.stepHoverColor : "transparent")
        Rectangle {
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: parent.right
            anchors.topMargin: 5
            anchors.bottomMargin: 5
            width: 1
            color: control.frameBorderColor
        }
        Kirigami.Icon {
            anchors.centerIn: parent
            width: 14
            height: 14
            source: "arrow-down"
            color: control.contentColor
            isMask: true
            opacity: control.enabled ? 1.0 : 0.5
        }
        MouseArea {
            anchors.fill: parent
            onClicked: control.decrease()
        }
    }

    up.indicator: Rectangle {
        x: parent.width - width
        width: 32
        height: parent.height
        radius: 4
        color: control.up.pressed
            ? Qt.darker(control.stepHoverColor, 1.15)
            : (control.up.hovered ? control.stepHoverColor : "transparent")
        Rectangle {
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.topMargin: 5
            anchors.bottomMargin: 5
            width: 1
            color: control.frameBorderColor
        }
        Kirigami.Icon {
            anchors.centerIn: parent
            width: 14
            height: 14
            source: "arrow-up"
            color: control.contentColor
            isMask: true
            opacity: control.enabled ? 1.0 : 0.5
        }
        MouseArea {
            anchors.fill: parent
            onClicked: control.increase()
        }
    }

    contentItem: TextInput {
        text: control.displayText
        horizontalAlignment: Qt.AlignHCenter
        verticalAlignment: Qt.AlignVCenter
        color: control.contentColor
        opacity: control.enabled ? 1.0 : 0.5
        readOnly: !control.editable
        validator: control.validator
        inputMethodHints: Qt.ImhFormattedNumbersOnly
    }

    background: Rectangle {
        color: control.frameColor
        border.color: control.frameBorderColor
        border.width: 1
        radius: 4
    }
}
