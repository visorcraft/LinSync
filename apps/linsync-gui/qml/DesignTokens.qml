// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick

QtObject {
    id: tokens

    readonly property int spaceXS:   4
    readonly property int spaceS:    8
    readonly property int spaceM:   12
    readonly property int spaceL:   16
    readonly property int spaceXL:  24
    readonly property int spaceXXL: 32
    readonly property int spaceXXXL: 48

    readonly property int radiusButton: 6
    readonly property int radiusCard:   8
    readonly property int radiusPill:   999
    readonly property int radiusInput:  6

    readonly property int textCaption:      11
    readonly property int textBody:         13
    readonly property int textBodyEmphasis: 14
    readonly property int textSubheading:   16
    readonly property int textHeading:      22
    readonly property int textDisplay:      30

    readonly property var themeValues: [
        0,
        1,
        2,
        12,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11
    ]

    readonly property var themeKeys: [
        "system",
        "light",
        "dark",
        "oled-black",
        "gentle-gecko",
        "black-knight",
        "diamond",
        "dreams",
        "paranoid",
        "red-velvet",
        "subspace",
        "tiefling",
        "vibes"
    ]

    readonly property var themeLabels: [
        qsTr("Follow system"),
        qsTr("Light"),
        qsTr("Dark"),
        qsTr("OLED Black"),
        qsTr("Gentle Gecko"),
        qsTr("Black Knight"),
        qsTr("Diamond"),
        qsTr("Dreams"),
        qsTr("Paranoid"),
        qsTr("Red Velvet"),
        qsTr("Subspace"),
        qsTr("Tiefling"),
        qsTr("Vibes")
    ]

    readonly property var schemePalettes: ({
        "light": {
            background:          "#F5F5F5",
            alternateBackground: "#ECEEF2",
            tertiaryBackground:  "#E3E6EC",
            text:                "#1A1A1A",
            disabledText:        "#777D88",
            highlight:           "#2D7FF9",
            highlightedText:     "#FFFFFF",
            positiveText:        "#1FA862",
            negativeText:        "#D93B3B",
            neutralText:         "#E08319"
        },
        "dark": {
            background:          "#181818",
            alternateBackground: "#292929",
            tertiaryBackground:  "#343434",
            text:                "#F5F5F5",
            disabledText:        "#8C8C8C",
            highlight:           "#2D7FF9",
            highlightedText:     "#FFFFFF",
            positiveText:        "#2DBE7A",
            negativeText:        "#F05656",
            neutralText:         "#FFA948"
        },
        "oled-black": {
            background:          "#000000",
            alternateBackground: "#050505",
            tertiaryBackground:  "#111111",
            text:                "#F5F5F5",
            disabledText:        "#767676",
            highlight:           "#2D7FF9",
            highlightedText:     "#FFFFFF",
            positiveText:        "#2DBE7A",
            negativeText:        "#F05656",
            neutralText:         "#FFA948"
        },
        "gentle-gecko": {
            background:          "#000000",
            alternateBackground: "#003322",
            tertiaryBackground:  "#00593D",
            text:                "#FFFFFF",
            disabledText:        "#B8D6CA",
            highlight:           "#00B86B",
            highlightedText:     "#FFFFFF",
            positiveText:        "#00FF7F",
            negativeText:        "#FF5050",
            neutralText:         "#FFAA00"
        },
        "black-knight": {
            background:          "#000000",
            alternateBackground: "#003366",
            tertiaryBackground:  "#00478F",
            text:                "#FFFFFF",
            disabledText:        "#B8CCE0",
            highlight:           "#0078D4",
            highlightedText:     "#FFFFFF",
            positiveText:        "#00FF7F",
            negativeText:        "#FF5050",
            neutralText:         "#FFAA00"
        },
        "diamond": {
            background:          "#2D5B67",
            alternateBackground: "#4F7F8C",
            tertiaryBackground:  "#7CA2B1",
            text:                "#B9DAE9",
            disabledText:        "#91B0BC",
            highlight:           "#A5C5D5",
            highlightedText:     "#1A2D34",
            positiveText:        "#C7F7D6",
            negativeText:        "#FFD2D2",
            neutralText:         "#FFE2A8"
        },
        "dreams": {
            background:          "#210B4B",
            alternateBackground: "#3F1C6D",
            tertiaryBackground:  "#6A2A98",
            text:                "#FF3D94",
            disabledText:        "#B95D91",
            highlight:           "#B5307E",
            highlightedText:     "#FFFFFF",
            positiveText:        "#8DFFB0",
            negativeText:        "#FF8AB5",
            neutralText:         "#FFD166"
        },
        "paranoid": {
            background:          "#1D1D4E",
            alternateBackground: "#3F3F88",
            tertiaryBackground:  "#5F5FBF",
            text:                "#D2D2F4",
            disabledText:        "#A2A2C8",
            highlight:           "#9A9AE0",
            highlightedText:     "#17173A",
            positiveText:        "#BFF6D0",
            negativeText:        "#FFD2D2",
            neutralText:         "#FFE0A3"
        },
        "red-velvet": {
            background:          "#1A0F0F",
            alternateBackground: "#3C1414",
            tertiaryBackground:  "#8B2323",
            text:                "#FFDCDC",
            disabledText:        "#C99B9B",
            highlight:           "#DC3C3C",
            highlightedText:     "#FFFFFF",
            positiveText:        "#8DFFB0",
            negativeText:        "#FF8A8A",
            neutralText:         "#FFD166"
        },
        "subspace": {
            background:          "#2E1A47",
            alternateBackground: "#4A2A6A",
            tertiaryBackground:  "#794B8B",
            text:                "#E2C7E6",
            disabledText:        "#B69CBC",
            highlight:           "#B77BB4",
            highlightedText:     "#241338",
            positiveText:        "#BAF4CB",
            negativeText:        "#FFD2D2",
            neutralText:         "#FFE0A3"
        },
        "tiefling": {
            background:          "#3A0A4D",
            alternateBackground: "#711D9A",
            tertiaryBackground:  "#A42DB4",
            text:                "#F9C54E",
            disabledText:        "#BD9440",
            highlight:           "#FF5C8A",
            highlightedText:     "#FFFFFF",
            positiveText:        "#9DFFC0",
            negativeText:        "#FF9BB5",
            neutralText:         "#F9C54E"
        },
        "vibes": {
            background:          "#0F0F1E",
            alternateBackground: "#1E1E3C",
            tertiaryBackground:  "#CC00FF",
            text:                "#00FFCC",
            disabledText:        "#66A89A",
            highlight:           "#FFCC00",
            highlightedText:     "#111111",
            positiveText:        "#00FF7F",
            negativeText:        "#FF5050",
            neutralText:         "#FFCC00"
        }
    })

    function keyForTheme(value) {
        switch (value) {
            case 0: return "system"
            case 1: return "light"
            case 2: return "dark"
            case 3: return "gentle-gecko"
            case 4: return "black-knight"
            case 5: return "diamond"
            case 6: return "dreams"
            case 7: return "paranoid"
            case 8: return "red-velvet"
            case 9: return "subspace"
            case 10: return "tiefling"
            case 11: return "vibes"
            case 12: return "oled-black"
            default: return "system"
        }
    }

    function normalizeTheme(value) {
        const parsed = Number(value)
        return themeValues.indexOf(parsed) >= 0 ? parsed : 0
    }

    function themeForKey(key) {
        if (key === "high-contrast")
            return 4
        const index = themeKeys.indexOf(key)
        return index >= 0 ? themeValues[index] : 0
    }

    function paletteForTheme(value) {
        const key = keyForTheme(normalizeTheme(value))
        return schemePalettes[key] || schemePalettes["dark"]
    }
}
