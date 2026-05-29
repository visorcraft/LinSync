// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Window
import org.kde.kirigami as Kirigami

Controls.ComboBox {
    id: control

    Kirigami.Theme.inherit: true
    Kirigami.Theme.colorSet: Kirigami.Theme.View

    palette.window: Kirigami.Theme.backgroundColor
    palette.base: Kirigami.Theme.backgroundColor
    palette.button: Kirigami.Theme.alternateBackgroundColor
    palette.text: Kirigami.Theme.textColor
    palette.windowText: Kirigami.Theme.textColor
    palette.buttonText: Kirigami.Theme.textColor
    palette.highlight: Kirigami.Theme.highlightColor
    palette.highlightedText: Kirigami.Theme.highlightedTextColor

    // Themed default display + frame so bare usages don't fall back to the
    // Fusion style's black border. Call sites that set their own
    // `contentItem`/`background` override these (Settings, the main Compare
    // dialog, etc.). rightPadding leaves room for the drop-down indicator.
    contentItem: Controls.Label {
        text: control.displayText
        leftPadding: 8
        rightPadding: 28
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        color: Kirigami.Theme.textColor
    }

    background: Rectangle {
        color: Kirigami.Theme.backgroundColor
        border.color: Kirigami.ColorUtils.tintWithAlpha(
            Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)
        border.width: 1
        radius: 4
    }
}
