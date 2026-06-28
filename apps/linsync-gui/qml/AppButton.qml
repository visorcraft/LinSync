// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

// AppButton — a flat, fully theme-token-driven push button. Mirrors the
// hand-styled buttons on the Settings page (alternate-background fill,
// separator border, 4px radius) but derives every colour from
// Kirigami.Theme so it renders correctly under every app theme instead of
// falling back to the Fusion QPalette defaults (which stayed dark in Light
// mode on pages that used a plain Controls.Button).
//
// Drop-in for Controls.Button: set `text` and optionally `icon.name`. Set
// `highlighted: true` for the accent/primary variant (highlight-coloured
// fill, highlighted text) used for the main action on a page.

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Controls.Button {
    id: control

    Kirigami.Theme.inherit: true
    Kirigami.Theme.colorSet: Kirigami.Theme.View

    readonly property color _sep: Kirigami.ColorUtils.tintWithAlpha(
        Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)
    readonly property color _fg: control.highlighted
        ? Kirigami.Theme.highlightedTextColor
        : Kirigami.Theme.textColor

    palette.buttonText: _fg
    palette.windowText: _fg

    contentItem: RowLayout {
        spacing: 6
        Kirigami.Icon {
            visible: control.icon.name !== "" && control.display !== Controls.AbstractButton.TextOnly
            source: control.icon.name
            color: control._fg
            isMask: true
            opacity: control.enabled ? 0.85 : 0.4
            implicitWidth: 16
            implicitHeight: 16
            Layout.alignment: Qt.AlignVCenter
        }
        Controls.Label {
            visible: control.text !== "" && control.display !== Controls.AbstractButton.IconOnly
            text: control.text
            font: control.font
            color: control._fg
            opacity: control.enabled ? 1.0 : 0.4
            elide: Text.ElideRight
            horizontalAlignment: Text.AlignHCenter
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
        }
    }

    background: Rectangle {
        radius: 4
        border.width: control.flat ? 0 : 1
        color: {
            if (control.flat)
                return (control.down || control.hovered)
                    ? Kirigami.ColorUtils.tintWithAlpha(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.08)
                    : "transparent"
            if (control.highlighted) {
                var base = Kirigami.Theme.highlightColor
                return (control.down || control.hovered) ? Qt.darker(base, 1.1) : base
            }
            var alt = Kirigami.Theme.alternateBackgroundColor
            return (control.down || control.hovered) ? Qt.darker(alt, 1.05) : alt
        }
        border.color: control.highlighted
            ? Kirigami.ColorUtils.tintWithAlpha(Kirigami.Theme.highlightColor, "#000000", 0.15)
            : control._sep
        opacity: control.enabled ? 1.0 : 0.5
    }
}
