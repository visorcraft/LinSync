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
        { name: "adler2",                  version: "2.0.1",     license: "0BSD OR MIT OR Apache-2.0" },
        { name: "aho-corasick",            version: "1.1.4",     license: "Unlicense OR MIT" },
        { name: "anyhow",                  version: "1.0.102",   license: "MIT OR Apache-2.0" },
        { name: "arrayref",                version: "0.3.9",     license: "BSD-2-Clause" },
        { name: "arrayvec",                version: "0.7.6",     license: "MIT OR Apache-2.0" },
        { name: "autocfg",                 version: "1.5.1",     license: "Apache-2.0 OR MIT" },
        { name: "bit_field",               version: "0.10.3",    license: "Apache-2.0/MIT" },
        { name: "bitflags",                version: "2.11.1",    license: "MIT OR Apache-2.0" },
        { name: "blake3",                  version: "1.8.5",     license: "CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception" },
        { name: "block-buffer",            version: "0.10.4",    license: "MIT OR Apache-2.0" },
        { name: "bytemuck",                version: "1.25.0",    license: "Zlib OR Apache-2.0 OR MIT" },
        { name: "byteorder-lite",          version: "0.1.0",     license: "Unlicense OR MIT" },
        { name: "cc",                      version: "1.2.62",    license: "MIT OR Apache-2.0" },
        { name: "cfg-if",                  version: "1.0.4",     license: "MIT OR Apache-2.0" },
        { name: "clang-format",            version: "0.3.0",     license: "MIT OR Apache-2.0" },
        { name: "codespan-reporting",      version: "0.11.1",    license: "Apache-2.0" },
        { name: "codespan-reporting",      version: "0.13.1",    license: "Apache-2.0" },
        { name: "color_quant",             version: "1.1.0",     license: "MIT" },
        { name: "constant_time_eq",        version: "0.4.2",     license: "CC0-1.0 OR MIT-0 OR Apache-2.0" },
        { name: "convert_case",            version: "0.6.0",     license: "MIT" },
        { name: "cpufeatures",             version: "0.2.17",    license: "MIT OR Apache-2.0" },
        { name: "cpufeatures",             version: "0.3.0",     license: "MIT OR Apache-2.0" },
        { name: "crc32fast",               version: "1.5.0",     license: "MIT OR Apache-2.0" },
        { name: "crypto-common",           version: "0.1.7",     license: "MIT OR Apache-2.0" },
        { name: "cxx",                     version: "1.0.194",   license: "MIT OR Apache-2.0" },
        { name: "cxxbridge-flags",         version: "1.0.194",   license: "MIT OR Apache-2.0" },
        { name: "cxxbridge-macro",         version: "1.0.194",   license: "MIT OR Apache-2.0" },
        { name: "cxx-gen",                 version: "0.7.194",   license: "MIT OR Apache-2.0" },
        { name: "cxx-qt",                  version: "0.8.1",     license: "MIT OR Apache-2.0" },
        { name: "cxx-qt-build",            version: "0.8.1",     license: "MIT OR Apache-2.0" },
        { name: "cxx-qt-gen",              version: "0.8.1",     license: "MIT OR Apache-2.0" },
        { name: "cxx-qt-lib",              version: "0.8.1",     license: "MIT OR Apache-2.0" },
        { name: "cxx-qt-macro",            version: "0.8.1",     license: "MIT OR Apache-2.0" },
        { name: "digest",                  version: "0.10.7",    license: "MIT OR Apache-2.0" },
        { name: "enumflags2",              version: "0.7.12",    license: "MIT OR Apache-2.0" },
        { name: "enumflags2_derive",       version: "0.7.12",    license: "MIT OR Apache-2.0" },
        { name: "equivalent",              version: "1.0.2",     license: "Apache-2.0 OR MIT" },
        { name: "exr",                     version: "1.74.0",    license: "BSD-3-Clause" },
        { name: "fax",                     version: "0.2.7",     license: "MIT" },
        { name: "fdeflate",                version: "0.3.7",     license: "MIT OR Apache-2.0" },
        { name: "find-msvc-tools",         version: "0.1.9",     license: "MIT OR Apache-2.0" },
        { name: "flate2",                  version: "1.1.9",     license: "MIT OR Apache-2.0" },
        { name: "foldhash",                version: "0.2.0",     license: "Zlib" },
        { name: "generic-array",           version: "0.14.7",    license: "MIT" },
        { name: "gif",                     version: "0.14.2",    license: "MIT OR Apache-2.0" },
        { name: "half",                    version: "2.7.1",     license: "MIT OR Apache-2.0" },
        { name: "hashbrown",               version: "0.17.1",    license: "MIT OR Apache-2.0" },
        { name: "image",                   version: "0.25.10",   license: "MIT OR Apache-2.0" },
        { name: "image-webp",              version: "0.2.4",     license: "MIT OR Apache-2.0" },
        { name: "indexmap",                version: "2.14.0",    license: "Apache-2.0 OR MIT" },
        { name: "indoc",                   version: "2.0.7",     license: "MIT OR Apache-2.0" },
        { name: "itoa",                    version: "1.0.18",    license: "MIT OR Apache-2.0" },
        { name: "jobserver",               version: "0.1.34",    license: "MIT OR Apache-2.0" },
        { name: "lab",                     version: "0.11.0",    license: "MIT" },
        { name: "landlock",                version: "0.4.5",     license: "MIT OR Apache-2.0" },
        { name: "lazy_static",             version: "1.5.0",     license: "MIT OR Apache-2.0" },
        { name: "lebe",                    version: "0.5.3",     license: "BSD-3-Clause" },
        { name: "libc",                    version: "0.2.186",   license: "MIT OR Apache-2.0" },
        { name: "link-cplusplus",          version: "1.0.12",    license: "MIT OR Apache-2.0" },
        { name: "log",                     version: "0.4.29",    license: "MIT OR Apache-2.0" },
        { name: "memchr",                  version: "2.8.0",     license: "Unlicense OR MIT" },
        { name: "miniz_oxide",             version: "0.8.9",     license: "MIT OR Zlib OR Apache-2.0" },
        { name: "moxcms",                  version: "0.8.1",     license: "BSD-3-Clause OR Apache-2.0" },
        { name: "nu-ansi-term",            version: "0.50.3",    license: "MIT" },
        { name: "num-traits",              version: "0.2.19",    license: "MIT OR Apache-2.0" },
        { name: "once_cell",               version: "1.21.4",    license: "MIT OR Apache-2.0" },
        { name: "pin-project-lite",        version: "0.2.17",    license: "Apache-2.0 OR MIT" },
        { name: "png",                     version: "0.18.1",    license: "MIT OR Apache-2.0" },
        { name: "proc-macro2",             version: "1.0.106",   license: "MIT OR Apache-2.0" },
        { name: "pxfm",                    version: "0.1.29",    license: "BSD-3-Clause OR Apache-2.0" },
        { name: "qt-build-utils",          version: "0.8.1",     license: "MIT OR Apache-2.0" },
        { name: "quick-error",             version: "2.0.1",     license: "MIT/Apache-2.0" },
        { name: "quote",                   version: "1.0.45",    license: "MIT OR Apache-2.0" },
        { name: "regex",                   version: "1.12.3",    license: "MIT OR Apache-2.0" },
        { name: "regex-automata",          version: "0.4.14",    license: "MIT OR Apache-2.0" },
        { name: "regex-syntax",            version: "0.8.10",    license: "MIT OR Apache-2.0" },
        { name: "rustversion",             version: "1.0.22",    license: "MIT OR Apache-2.0" },
        { name: "seccompiler",             version: "0.4.0",     license: "Apache-2.0 OR BSD-3-Clause" },
        { name: "semver",                  version: "1.0.28",    license: "MIT OR Apache-2.0" },
        { name: "serde",                   version: "1.0.228",   license: "MIT OR Apache-2.0" },
        { name: "serde_core",              version: "1.0.228",   license: "MIT OR Apache-2.0" },
        { name: "serde_derive",            version: "1.0.228",   license: "MIT OR Apache-2.0" },
        { name: "serde_json",              version: "1.0.149",   license: "MIT OR Apache-2.0" },
        { name: "serde_repr",              version: "0.1.20",    license: "MIT OR Apache-2.0" },
        { name: "sha2",                    version: "0.10.9",    license: "MIT OR Apache-2.0" },
        { name: "sharded-slab",            version: "0.1.7",     license: "MIT" },
        { name: "shlex",                   version: "1.3.0",     license: "MIT OR Apache-2.0" },
        { name: "simd-adler32",            version: "0.3.9",     license: "MIT" },
        { name: "smallvec",                version: "1.15.1",    license: "MIT OR Apache-2.0" },
        { name: "static_assertions",       version: "1.1.0",     license: "MIT OR Apache-2.0" },
        { name: "syn",                     version: "2.0.117",   license: "MIT OR Apache-2.0" },
        { name: "termcolor",               version: "1.4.1",     license: "Unlicense OR MIT" },
        { name: "thiserror",               version: "1.0.69",    license: "MIT OR Apache-2.0" },
        { name: "thiserror",               version: "2.0.18",    license: "MIT OR Apache-2.0" },
        { name: "thiserror-impl",          version: "1.0.69",    license: "MIT OR Apache-2.0" },
        { name: "thiserror-impl",          version: "2.0.18",    license: "MIT OR Apache-2.0" },
        { name: "thread_local",            version: "1.1.9",     license: "MIT OR Apache-2.0" },
        { name: "tiff",                    version: "0.11.3",    license: "MIT" },
        { name: "tracing",                 version: "0.1.44",    license: "MIT" },
        { name: "tracing-attributes",      version: "0.1.31",    license: "MIT" },
        { name: "tracing-core",            version: "0.1.36",    license: "MIT" },
        { name: "tracing-log",             version: "0.2.0",     license: "MIT" },
        { name: "tracing-serde",           version: "0.2.0",     license: "MIT" },
        { name: "tracing-subscriber",      version: "0.3.23",    license: "MIT" },
        { name: "typenum",                 version: "1.20.1",    license: "MIT OR Apache-2.0" },
        { name: "unicode-ident",           version: "1.0.24",    license: "(MIT OR Apache-2.0) AND Unicode-3.0" },
        { name: "unicode-segmentation",    version: "1.13.2",    license: "MIT OR Apache-2.0" },
        { name: "unicode-width",           version: "0.1.14",    license: "MIT OR Apache-2.0" },
        { name: "unicode-width",           version: "0.2.2",     license: "MIT OR Apache-2.0" },
        { name: "urlencoding",             version: "2.1.3",     license: "MIT" },
        { name: "version_check",           version: "0.9.5",     license: "MIT/Apache-2.0" },
        { name: "weezl",                   version: "0.1.12",    license: "MIT OR Apache-2.0" },
        { name: "zerocopy",                version: "0.8.48",    license: "BSD-2-Clause OR Apache-2.0 OR MIT" },
        { name: "zerocopy-derive",         version: "0.8.48",    license: "BSD-2-Clause OR Apache-2.0 OR MIT" },
        { name: "zmij",                    version: "1.0.21",    license: "MIT" },
        { name: "zune-core",               version: "0.5.1",     license: "MIT OR Apache-2.0 OR Zlib" },
        { name: "zune-inflate",            version: "0.2.54",    license: "MIT OR Apache-2.0 OR Zlib" },
        { name: "zune-jpeg",               version: "0.5.15",    license: "MIT OR Apache-2.0 OR Zlib" }
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
