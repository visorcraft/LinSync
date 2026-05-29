// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

// Sidebar nav row. 36px tall in expanded mode; collapses to a centred
// icon-only square when the sidebar is collapsed. The active row paints
// a soft accent-tinted pill across its whole width; hover/press give
// lighter washes.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Item {
    id: nav

    property string label: ""
    property string iconName: ""
    property bool active: false
    property bool collapsed: false
    property string tooltipText: label

    signal triggered()

    Layout.fillWidth: true
    Layout.preferredHeight: 36

    // Pull the live palette from the ApplicationWindow root so we react
    // to runtime theme changes. Falling back to Kirigami.Theme for
    // standalone use outside the linsync ApplicationWindow.
    readonly property color themeBg: Kirigami.Theme.backgroundColor
    readonly property color themeText: Kirigami.Theme.textColor
    readonly property color themeHighlight: Kirigami.Theme.highlightColor

    readonly property color accentMute: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeHighlight, 0.18)
    readonly property color hoverWash:  Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.06)
    readonly property color pressWash:  Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.10)

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: nav.collapsed ? 6 : 8
        anchors.rightMargin: nav.collapsed ? 6 : 8
        anchors.topMargin: 1
        anchors.bottomMargin: 1
        radius: 8
        color: {
            if (nav.active) return nav.accentMute
            if (mouseArea.containsPress) return nav.pressWash
            if (mouseArea.containsMouse) return nav.hoverWash
            return "transparent"
        }
        border.color: nav.active
            ? Kirigami.ColorUtils.tintWithAlpha(nav.themeBg, nav.themeHighlight, 0.35)
            : "transparent"
        border.width: 1

        // No color Behavior here on purpose. The 110ms ColorAnimation made
        // the leaving item AND the entering item both fade through their
        // hover wash at the same time, producing a "two-items-flash-darker"
        // glitch when moving the cursor up/down the sidebar. Instant color
        // swap on hover-in/out is crisp and looks intentional.
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: nav.collapsed ? 0 : 16
        anchors.rightMargin: nav.collapsed ? 0 : 16
        spacing: 12

        Item {
            Layout.preferredWidth: nav.collapsed ? nav.width : 18
            Layout.preferredHeight: 18
            Layout.alignment: Qt.AlignVCenter | (nav.collapsed ? Qt.AlignHCenter : Qt.AlignLeft)
            Kirigami.Icon {
                source: nav.iconName
                anchors.centerIn: parent
                width: 18
                height: 18
                color: nav.active ? nav.themeHighlight : nav.themeText
                isMask: true
                opacity: nav.active ? 1.0 : 0.78
                Behavior on color { ColorAnimation { duration: 110 } }
            }
        }

        Controls.Label {
            visible: !nav.collapsed
            Layout.fillWidth: true
            text: nav.label
            font.pixelSize: 13
            font.weight: nav.active ? Font.DemiBold : Font.Normal
            color: nav.active ? nav.themeHighlight : nav.themeText
            opacity: nav.active ? 1.0 : 0.9
            elide: Text.ElideRight
            Behavior on color { ColorAnimation { duration: 110 } }
        }
    }

    // Keyboard + accessibility layer: sits on top of the visual MouseArea so
    // AT users can Tab to each nav item and activate it with Space/Return.
    Controls.AbstractButton {
        id: a11yButton
        anchors.fill: parent
        text: nav.label
        Accessible.name: nav.label
        Accessible.role: Accessible.Button
        Accessible.onPressAction: nav.triggered()
        Keys.onReturnPressed: nav.triggered()
        Keys.onSpacePressed: nav.triggered()
        background: null
        contentItem: null
        onClicked: nav.triggered()
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        // Defer to the AbstractButton for clicks so both paths fire triggered().
        onClicked: nav.triggered()
    }

    Controls.ToolTip {
        visible: nav.collapsed && mouseArea.containsMouse && nav.tooltipText !== ""
        text: nav.tooltipText
        delay: 400
    }
}
