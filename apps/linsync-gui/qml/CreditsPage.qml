// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    padding: 0
    titleDelegate: Item {}
    globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

    readonly property color themeBg: Kirigami.Theme.backgroundColor
    readonly property color themeBgAlt: Kirigami.Theme.alternateBackgroundColor
    readonly property color themeText: Kirigami.Theme.textColor
    readonly property color themeHighlight: Kirigami.Theme.highlightColor
    readonly property color themeNeutral: Kirigami.Theme.neutralTextColor
    readonly property color themePositive: Kirigami.Theme.positiveTextColor
    readonly property color themeNegative: Kirigami.Theme.negativeTextColor

    background: Rectangle { color: page.themeBg }

    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    readonly property color separator: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.2)

    readonly property var runtimeComponents: [
        { name: "Qt 6 (Core, Qml, Gui, Quick)", license: "LGPL-3.0 / GPL-3.0 / commercial", url: "https://www.qt.io" },
        { name: "KDE Frameworks 6 — Kirigami", license: "LGPL-2.1+", url: "https://invent.kde.org/frameworks/kirigami" },
        { name: "FreeDesktop / Portal services", license: "various (MIT / LGPL / Apache-2.0)", url: "https://www.freedesktop.org" }
    ]

    readonly property var crates: [
        { name: "aho-corasick",       version: "1.1.4",   license: "Unlicense OR MIT" },
        { name: "arrayref",           version: "0.3.9",   license: "BSD-2-Clause" },
        { name: "arrayvec",           version: "0.7.6",   license: "MIT OR Apache-2.0" },
        { name: "bit_field",          version: "0.10.3",  license: "MIT" },
        { name: "blake3",             version: "1.8.5",   license: "CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception" },
        { name: "cc",                 version: "1.2.62",  license: "MIT OR Apache-2.0" },
        { name: "cfg-if",             version: "1.0.4",   license: "MIT OR Apache-2.0" },
        { name: "color_quant",        version: "1.1.0",   license: "MIT" },
        { name: "constant_time_eq",   version: "0.4.2",   license: "CC0-1.0 OR MIT-0 OR Apache-2.0" },
        { name: "cpufeatures",        version: "0.3.0",   license: "MIT OR Apache-2.0" },
        { name: "exr",                version: "1.74.0",  license: "BSD-3-Clause" },
        { name: "find-msvc-tools",    version: "0.1.9",   license: "MIT OR Apache-2.0" },
        { name: "gif",                version: "0.14.2",  license: "MIT OR Apache-2.0" },
        { name: "itoa",               version: "1.0.18",  license: "MIT OR Apache-2.0" },
        { name: "lazy_static",        version: "1.5.0",   license: "MIT OR Apache-2.0" },
        { name: "lebe",               version: "0.5.3",   license: "BSD-3-Clause" },
        { name: "libc",               version: "0.2.186", license: "MIT OR Apache-2.0" },
        { name: "log",                version: "0.4.29",  license: "MIT OR Apache-2.0" },
        { name: "memchr",             version: "2.8.0",   license: "Unlicense OR MIT" },
        { name: "nu-ansi-term",       version: "0.50.3",  license: "MIT" },
        { name: "once_cell",          version: "1.21.4",  license: "MIT OR Apache-2.0" },
        { name: "pin-project-lite",   version: "0.2.17",  license: "Apache-2.0 OR MIT" },
        { name: "proc-macro2",        version: "1.0.106", license: "MIT OR Apache-2.0" },
        { name: "quote",              version: "1.0.45",  license: "MIT OR Apache-2.0" },
        { name: "regex",              version: "1.12.3",  license: "MIT OR Apache-2.0" },
        { name: "regex-automata",     version: "0.4.14",  license: "MIT OR Apache-2.0" },
        { name: "regex-syntax",       version: "0.8.10",  license: "MIT OR Apache-2.0" },
        { name: "serde",              version: "1.0.228", license: "MIT OR Apache-2.0" },
        { name: "serde_core",         version: "1.0.228", license: "MIT OR Apache-2.0" },
        { name: "serde_derive",       version: "1.0.228", license: "MIT OR Apache-2.0" },
        { name: "serde_json",         version: "1.0.149", license: "MIT OR Apache-2.0" },
        { name: "serde_repr",         version: "0.1.20",  license: "MIT OR Apache-2.0" },
        { name: "sharded-slab",       version: "0.1.7",   license: "MIT" },
        { name: "shlex",              version: "1.3.0",   license: "MIT OR Apache-2.0" },
        { name: "smallvec",           version: "1.15.1",  license: "MIT OR Apache-2.0" },
        { name: "syn",                version: "2.0.117", license: "MIT OR Apache-2.0" },
        { name: "thread_local",       version: "1.1.9",   license: "MIT OR Apache-2.0" },
        { name: "tracing",            version: "0.1.44",  license: "MIT" },
        { name: "tracing-attributes", version: "0.1.31",  license: "MIT" },
        { name: "tracing-core",       version: "0.1.36",  license: "MIT" },
        { name: "tracing-log",        version: "0.2.0",   license: "MIT" },
        { name: "tracing-serde",      version: "0.2.0",   license: "MIT" },
        { name: "tracing-subscriber", version: "0.3.23",  license: "MIT" },
        { name: "unicode-ident",      version: "1.0.24",  license: "(MIT OR Apache-2.0) AND Unicode-3.0" },
        { name: "zmij",               version: "1.0.21",  license: "MIT" },
        { name: "zune-inflate",       version: "0.2.54",  license: "MIT OR Apache-2.0" }
    ]

    property string filterText: ""
    // Computed once per filterText change; referenced from the count label, the
    // table size binding, and the Repeater model so we don't filter the crate
    // list three times on every keystroke.
    readonly property var filteredCrateList: page.filteredCrates()

    function licenseTint(expr) {
        const e = String(expr)
        if (e.indexOf("GPL") !== -1)
            return Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themeNegative, 0.2)
        if (e.indexOf("LGPL") !== -1)
            return Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themeNeutral, 0.2)
        return Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themePositive, 0.18)
    }

    function filteredCrates() {
        if (page.filterText === "")
            return page.crates
        const needle = page.filterText.toLowerCase()
        return page.crates.filter(c => c.name.toLowerCase().indexOf(needle) !== -1
                                    || c.license.toLowerCase().indexOf(needle) !== -1)
    }

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
                    text: qsTr("Credits")
                    font.pixelSize: 22
                    font.bold: true
                    font.letterSpacing: 0
                }
                Controls.Label {
                    text: qsTr("%1 Cargo crates · %2 runtime components").arg(page.crates.length).arg(page.runtimeComponents.length)
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

            Card {
                Layout.fillWidth: true
                title: qsTr("Runtime components")
                subtitle: qsTr("System libraries LinSync links against at execution. None are bundled — downstream packagers handle redistribution.")
                Repeater {
                    model: page.runtimeComponents
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 12
                        Controls.Label {
                            Layout.preferredWidth: 240
                            text: modelData.name
                            font.bold: true
                            font.pixelSize: 12
                            elide: Text.ElideRight
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: modelData.license
                            font.family: "monospace"
                            font.pixelSize: 11
                            opacity: 0.85
                        }
                        Controls.ToolButton {
                            icon.name: "globe"
                            text: qsTr("Visit %1 website").arg(modelData.name)
                            display: Controls.AbstractButton.IconOnly
                            Accessible.name: qsTr("Visit %1 website").arg(modelData.name)
                            Controls.ToolTip.text: modelData.url
                            Controls.ToolTip.visible: hovered
                            onClicked: Qt.openUrlExternally(modelData.url)
                        }
                    }
                }
            }

            Controls.Label {
                text: qsTr("CARGO CRATES")
                font.pixelSize: 10
                font.bold: true
                font.letterSpacing: 0
                opacity: 0.5
                Layout.topMargin: 8
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                AppTextField {
                    implicitHeight: 36
                    color: page.themeText
                    placeholderTextColor: Qt.rgba(page.themeText.r, page.themeText.g, page.themeText.b, 0.55)
                    background: Rectangle {
                        color: page.themeBg
                        border.color: page.separator
                        border.width: 1
                        radius: 4
                    }
                    Layout.fillWidth: true
                    placeholderText: qsTr("Filter by crate name or license…")
                    onTextChanged: page.filterText = text
                    Accessible.name: "Filter licenses"
                }
                Controls.Label {
                    text: qsTr("%1 / %2").arg(page.filteredCrateList.length).arg(page.crates.length)
                    opacity: 0.6
                    font.pixelSize: 11
                    font.family: "monospace"
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 30 + 28 * page.filteredCrateList.length
                radius: 8
                color: page.themeBg
                border.color: page.separator
                border.width: 1

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 0
                    spacing: 0

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 30
                        color: page.themeBgAlt
                        radius: 8
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 14
                            anchors.rightMargin: 14
                            spacing: 12
                            Controls.Label {
                                Layout.preferredWidth: 200
                                text: qsTr("Crate")
                                font.bold: true
                                font.pixelSize: 11
                                opacity: 0.7
                            }
                            Controls.Label {
                                Layout.preferredWidth: 88
                                text: qsTr("Version")
                                font.bold: true
                                font.pixelSize: 11
                                opacity: 0.7
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: qsTr("License expression")
                                font.bold: true
                                font.pixelSize: 11
                                opacity: 0.7
                            }
                            Item { Layout.preferredWidth: 24 }
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 1
                        color: page.separator
                    }

                    Repeater {
                        model: page.filteredCrateList
                        delegate: Rectangle {
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            Layout.preferredHeight: 28
                            color: index % 2 === 0 ? page.themeBg
                                                   : Qt.rgba(page.themeBgAlt.r,
                                                             page.themeBgAlt.g,
                                                             page.themeBgAlt.b, 0.5)
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 14
                                anchors.rightMargin: 14
                                spacing: 12
                                Controls.Label {
                                    Layout.preferredWidth: 200
                                    text: modelData.name
                                    font.family: "monospace"
                                    font.pixelSize: 11
                                    elide: Text.ElideRight
                                }
                                Controls.Label {
                                    Layout.preferredWidth: 88
                                    text: modelData.version
                                    font.family: "monospace"
                                    font.pixelSize: 11
                                    opacity: 0.7
                                }
                                Rectangle {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 20
                                    Layout.alignment: Qt.AlignVCenter
                                    radius: 10
                                    color: page.licenseTint(modelData.license)
                                    Controls.Label {
                                        anchors.left: parent.left
                                        anchors.leftMargin: 8
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: modelData.license
                                        font.pixelSize: 10
                                        font.family: "monospace"
                                    }
                                }
                                Controls.ToolButton {
                                    Layout.preferredWidth: 24
                                    icon.name: "globe"
                                    text: qsTr("Open %1 on crates.io").arg(modelData.name)
                                    display: Controls.AbstractButton.IconOnly
                                    Accessible.name: qsTr("Open %1 on crates.io").arg(modelData.name)
                                    Controls.ToolTip.text: qsTr("Open on crates.io")
                                    Controls.ToolTip.visible: hovered
                                    onClicked: Qt.openUrlExternally("https://crates.io/crates/" + modelData.name)
                                }
                            }
                        }
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                Layout.topMargin: 4
                Layout.bottomMargin: 24
                wrapMode: Text.WordWrap
                opacity: 0.55
                font.pixelSize: 11
                text: qsTr("Where a crate offers multiple licenses (e.g. MIT OR Apache-2.0), LinSync selects the option compatible with GPL-3.0-only. No copyleft Cargo crates are currently linked. The authoritative crate list lives in docs/third-party-notices.md and is regenerated before each release.")
            }
        }
    }
}
