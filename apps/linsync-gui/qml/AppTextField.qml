// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Window
import org.kde.kirigami as Kirigami

Controls.TextField {
    id: control

    Kirigami.Theme.inherit: true
    Kirigami.Theme.colorSet: Kirigami.Theme.View

    palette.window: Kirigami.Theme.backgroundColor
    palette.base: Kirigami.Theme.backgroundColor
    palette.text: Kirigami.Theme.textColor
    palette.windowText: Kirigami.Theme.textColor
    palette.highlight: Kirigami.Theme.highlightColor
    palette.highlightedText: Kirigami.Theme.highlightedTextColor

    color: Kirigami.Theme.textColor

    // Themed default frame so bare usages don't fall back to the Fusion
    // style's black border. Call sites that set their own `background`
    // override this (Settings, the main Compare dialog, etc.).
    background: Rectangle {
        color: Kirigami.Theme.backgroundColor
        border.color: Kirigami.ColorUtils.tintWithAlpha(
            Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)
        border.width: 1
        radius: 4
    }
}
