// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Window
import org.kde.kirigami as Kirigami

Controls.CheckBox {
    id: control

    Kirigami.Theme.inherit: true
    Kirigami.Theme.colorSet: Kirigami.Theme.View

    palette.window: Kirigami.Theme.backgroundColor
    palette.base: Kirigami.Theme.backgroundColor
    palette.text: Kirigami.Theme.textColor
    palette.windowText: Kirigami.Theme.textColor
    palette.highlight: Kirigami.Theme.highlightColor
    palette.highlightedText: Kirigami.Theme.highlightedTextColor
}
