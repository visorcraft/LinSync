// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    property var includeRules: []
    property var excludeRules: []
    property bool respectGitignore: true
    property bool followSymlinks: false
    property int maxDepth: 0
    property bool bridgeConnected: false
    property var savedFilters: []
    property var validationResult: ({})

    signal includesEdited(var rules)
    signal excludesEdited(var rules)
    signal gitignoreToggled(bool value)
    signal followSymlinksToggled(bool value)
    signal maxDepthEdited(int value)
    signal validateRequested(string body)
    signal saveFilterRequested(string body)
    signal deleteFilterRequested(string name)
    signal openMigratePickerRequested()

    property var migrateResult: null

    padding: 0
    titleDelegate: Item {}
    globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

    readonly property color themeBg: Kirigami.Theme.backgroundColor
    readonly property color themeBgAlt: Kirigami.Theme.alternateBackgroundColor
    readonly property color themeText: Kirigami.Theme.textColor
    readonly property color themePositive: Kirigami.Theme.positiveTextColor
    readonly property color themeNegative: Kirigami.Theme.negativeTextColor

    background: Rectangle { color: page.themeBg }

    // The instantiation site in Main.qml is responsible for binding
    // Kirigami.Theme.* to the live LinSync palette (root.active*).
    // We keep inherit:false here so descendants of this page use those
    // explicit values rather than whatever a deeper ancestor scope sets.
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    readonly property color separator: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.2)

    function addRule(list, pattern) {
        const trimmed = String(pattern || "").trim()
        if (trimmed === "")
            return list
        const updated = list.slice()
        if (updated.indexOf(trimmed) === -1)
            updated.push(trimmed)
        return updated
    }

    function removeRule(list, pattern) {
        return list.filter(p => p !== pattern)
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
                    text: qsTr("Filters")
                    font.pixelSize: 22
                    font.bold: true
                    font.letterSpacing: 0
                }
                Controls.Label {
                    text: qsTr("Glob rules and walk options for folder comparisons.")
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
                title: qsTr("Include patterns")
                subtitle: qsTr("When filter wiring is enabled, only files matching at least one include pattern are compared.")

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
                        id: includeInput
                        Layout.fillWidth: true
                        placeholderText: qsTr("e.g. **/*.rs, src/**/*.toml, *.md")
                        Accessible.name: "Add include glob"
                        onAccepted: {
                            const next = page.addRule(page.includeRules, text)
                            page.includeRules = next
                            page.includesEdited(next)
                            text = ""
                        }
                    }
                    AppButton {
                        icon.name: "list-add"
                        text: qsTr("Add")
                        onClicked: {
                            const next = page.addRule(page.includeRules, includeInput.text)
                            page.includeRules = next
                            page.includesEdited(next)
                            includeInput.text = ""
                        }
                    }
                }

                Flow {
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    spacing: 6
                    visible: page.includeRules.length > 0

                    Repeater {
                        model: page.includeRules
                        delegate: Rectangle {
                            required property var modelData
                            implicitHeight: 26
                            implicitWidth: includeChipRow.implicitWidth + 14
                            radius: 13
                            color: Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themePositive, 0.18)
                            border.color: Qt.rgba(page.themePositive.r,
                                                  page.themePositive.g,
                                                  page.themePositive.b, 0.4)
                            border.width: 1

                            RowLayout {
                                id: includeChipRow
                                anchors.centerIn: parent
                                spacing: 4
                                Controls.Label {
                                    text: modelData
                                    font.family: "monospace"
                                    font.pixelSize: 11
                                }
                                Controls.ToolButton {
                                    icon.name: "edit-delete-remove"
                                    Layout.preferredWidth: 18
                                    Layout.preferredHeight: 18
                                    text: qsTr("Remove include pattern %1").arg(modelData)
                                    display: Controls.AbstractButton.IconOnly
                                    Accessible.name: qsTr("Remove include pattern %1").arg(modelData)
                                    onClicked: {
                                        const next = page.removeRule(page.includeRules, modelData)
                                        page.includeRules = next
                                        page.includesEdited(next)
                                    }
                                }
                            }
                        }
                    }
                }

                Controls.Label {
                    visible: page.includeRules.length === 0
                    text: qsTr("No include patterns. All files will be considered.")
                    opacity: 0.55
                    font.italic: true
                    font.pixelSize: 11
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Exclude patterns")
                subtitle: qsTr("When filter wiring is enabled, files matching any exclude pattern are skipped.")

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
                        id: excludeInput
                        Layout.fillWidth: true
                        placeholderText: qsTr("e.g. target/**, node_modules/**, *.log")
                        Accessible.name: "Add exclude glob"
                        onAccepted: {
                            const next = page.addRule(page.excludeRules, text)
                            page.excludeRules = next
                            page.excludesEdited(next)
                            text = ""
                        }
                    }
                    AppButton {
                        icon.name: "list-add"
                        text: qsTr("Add")
                        onClicked: {
                            const next = page.addRule(page.excludeRules, excludeInput.text)
                            page.excludeRules = next
                            page.excludesEdited(next)
                            excludeInput.text = ""
                        }
                    }
                }

                Flow {
                    Layout.fillWidth: true
                    Layout.topMargin: 6
                    spacing: 6
                    visible: page.excludeRules.length > 0

                    Repeater {
                        model: page.excludeRules
                        delegate: Rectangle {
                            required property var modelData
                            implicitHeight: 26
                            implicitWidth: excludeChipRow.implicitWidth + 14
                            radius: 13
                            color: Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themeNegative, 0.18)
                            border.color: Qt.rgba(page.themeNegative.r,
                                                  page.themeNegative.g,
                                                  page.themeNegative.b, 0.4)
                            border.width: 1

                            RowLayout {
                                id: excludeChipRow
                                anchors.centerIn: parent
                                spacing: 4
                                Controls.Label {
                                    text: modelData
                                    font.family: "monospace"
                                    font.pixelSize: 11
                                }
                                Controls.ToolButton {
                                    icon.name: "edit-delete-remove"
                                    Layout.preferredWidth: 18
                                    Layout.preferredHeight: 18
                                    text: qsTr("Remove exclude pattern %1").arg(modelData)
                                    display: Controls.AbstractButton.IconOnly
                                    Accessible.name: qsTr("Remove exclude pattern %1").arg(modelData)
                                    onClicked: {
                                        const next = page.removeRule(page.excludeRules, modelData)
                                        page.excludeRules = next
                                        page.excludesEdited(next)
                                    }
                                }
                            }
                        }
                    }
                }

                Controls.Label {
                    visible: page.excludeRules.length === 0
                    text: qsTr("No exclude patterns.")
                    opacity: 0.55
                    font.italic: true
                    font.pixelSize: 11
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: 10
                    spacing: 8
                    Controls.Label {
                        text: qsTr("Quick add:")
                        opacity: 0.6
                        font.pixelSize: 11
                    }
                    Repeater {
                        model: [".git/**", "target/**", "node_modules/**", "*.lock", "*.tmp"]
                        delegate: AppButton {
                            required property var modelData
                            flat: true
                            text: modelData
                            font.family: "monospace"
                            font.pixelSize: 10
                            Accessible.name: qsTr("Add %1 to exclude patterns").arg(modelData)
                            Accessible.description: qsTr("Quickly add a common exclude pattern")
                            onClicked: {
                                const next = page.addRule(page.excludeRules, modelData)
                                page.excludeRules = next
                                page.excludesEdited(next)
                            }
                        }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Walk options")

                AppCheckBox {
                    indicator: Rectangle {
                        implicitWidth: 18
                        implicitHeight: 18
                        x: parent.leftPadding
                        y: (parent.height - height) / 2
                        radius: 3
                        color: parent.checked ? (page.themeHighlight !== undefined ? page.themeHighlight : Kirigami.Theme.highlightColor) : page.themeBg
                        border.color: parent.checked ? (page.themeHighlight !== undefined ? page.themeHighlight : Kirigami.Theme.highlightColor) : page.separator
                        border.width: 1
                        Controls.Label {
                            anchors.centerIn: parent
                            visible: parent.parent.checked
                            text: "\u2713"
                            font.pixelSize: 14
                            font.bold: true
                            color: "white"
                        }
                    }
                    contentItem: Controls.Label {
                        text: parent.text
                        leftPadding: parent.indicator.width + 8
                        verticalAlignment: Text.AlignVCenter
                        color: page.themeText
                    }
                    checked: page.respectGitignore
                    text: qsTr("Respect .gitignore (and parents)")
                    onToggled: {
                        page.respectGitignore = checked
                        page.gitignoreToggled(checked)
                    }
                }

                AppCheckBox {
                    indicator: Rectangle {
                        implicitWidth: 18
                        implicitHeight: 18
                        x: parent.leftPadding
                        y: (parent.height - height) / 2
                        radius: 3
                        color: parent.checked ? (page.themeHighlight !== undefined ? page.themeHighlight : Kirigami.Theme.highlightColor) : page.themeBg
                        border.color: parent.checked ? (page.themeHighlight !== undefined ? page.themeHighlight : Kirigami.Theme.highlightColor) : page.separator
                        border.width: 1
                        Controls.Label {
                            anchors.centerIn: parent
                            visible: parent.parent.checked
                            text: "\u2713"
                            font.pixelSize: 14
                            font.bold: true
                            color: "white"
                        }
                    }
                    contentItem: Controls.Label {
                        text: parent.text
                        leftPadding: parent.indicator.width + 8
                        verticalAlignment: Text.AlignVCenter
                        color: page.themeText
                    }
                    checked: page.followSymlinks
                    text: qsTr("Follow symbolic links")
                    onToggled: {
                        page.followSymlinks = checked
                        page.followSymlinksToggled(checked)
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    Controls.Label {
                        text: qsTr("Maximum depth")
                    }
                    AppSpinBox {
                        implicitHeight: 36
implicitWidth: 140
leftPadding: 36
rightPadding: 36
down.indicator: Rectangle {
    x: 0
    width: 32
    height: parent.height
    radius: 4
    color: parent.down.pressed
        ? Qt.darker(page.themeBgAlt, 1.15)
        : (parent.down.hovered ? page.themeBgAlt : "transparent")
    Rectangle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        anchors.topMargin: 5
        anchors.bottomMargin: 5
        width: 1
        color: page.separator
    }
    Kirigami.Icon {
        anchors.centerIn: parent
        width: 14
        height: 14
        source: "arrow-down"
        color: page.themeText
        isMask: true
    }
    MouseArea {
        anchors.fill: parent
        onClicked: parent.parent.decrease()
    }
}
up.indicator: Rectangle {
    x: parent.width - width
    width: 32
    height: parent.height
    radius: 4
    color: parent.up.pressed
        ? Qt.darker(page.themeBgAlt, 1.15)
        : (parent.up.hovered ? page.themeBgAlt : "transparent")
    Rectangle {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.topMargin: 5
        anchors.bottomMargin: 5
        width: 1
        color: page.separator
    }
    Kirigami.Icon {
        anchors.centerIn: parent
        width: 14
        height: 14
        source: "arrow-up"
        color: page.themeText
        isMask: true
    }
    MouseArea {
        anchors.fill: parent
        onClicked: parent.parent.increase()
    }
}
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        contentItem: TextInput {
                            text: parent.displayText
                            horizontalAlignment: Qt.AlignHCenter
                            verticalAlignment: Qt.AlignVCenter
                            color: page.themeText
                            readOnly: !parent.editable
                            validator: parent.validator
                            inputMethodHints: Qt.ImhFormattedNumbersOnly
                        }
                        Accessible.name: qsTr("Maximum walk depth")
                        from: 0
                        to: 64
                        value: page.maxDepth
                        onValueModified: {
                            page.maxDepth = value
                            page.maxDepthEdited(value)
                        }
                    }
                    Controls.Label {
                        text: page.maxDepth === 0 ? qsTr("unlimited") : qsTr("levels")
                        opacity: 0.6
                        font.pixelSize: 11
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Saved filters")
                subtitle: page.bridgeConnected
                    ? qsTr("Validate a filter rule before saving. Saved filters can be referenced by name from the CLI's --filter-name flag.")
                    : qsTr("Connect the bridge to save and load named filters from $XDG_CONFIG_HOME/linsync/filters.json.")

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    AppTextField {
                        id: namedFilterInput
                        Layout.fillWidth: true
                        implicitHeight: 36
                        color: page.themeText
                        placeholderText: qsTr("name: My filter\\nwf:*.rs\\nwd!:target")
                        Accessible.name: qsTr("Filter name")
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                    }
                    AppButton {
                        text: qsTr("Validate")
                        enabled: namedFilterInput.text.length > 0 && page.bridgeConnected
                        onClicked: page.validateRequested(namedFilterInput.text)
                    }
                    AppButton {
                        text: qsTr("Save")
                        enabled: namedFilterInput.text.length > 0 && page.bridgeConnected
                        onClicked: {
                            page.saveFilterRequested(namedFilterInput.text)
                            namedFilterInput.text = ""
                        }
                    }
                }

                Controls.Label {
                    visible: page.validationResult && page.validationResult.message !== undefined
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: page.validationResult && page.validationResult.ok
                        ? page.themePositive : page.themeNegative
                    font.pixelSize: 11
                    font.family: "monospace"
                    text: {
                        const v = page.validationResult
                        if (!v) return ""
                        if (v.ok)
                            return qsTr("ok: parsed '%1' with %2 rule(s)")
                                .arg(v.name || qsTr("(unnamed)"))
                                .arg(v.rules || 0)
                        const prefix = v.migration_hint ? qsTr("migration") : qsTr("error")
                        return qsTr("%1: line %2: %3").arg(prefix).arg(v.line || 0).arg(v.message || "")
                    }
                }

                Repeater {
                    model: page.savedFilters
                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.preferredHeight: 36
                        radius: 4
                        color: page.themeBgAlt

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 8
                            spacing: 10

                            Controls.Label {
                                Layout.fillWidth: true
                                text: modelData.name + "  ·  " + (modelData.rules ? modelData.rules.length : 0) + " rule(s)"
                                font.family: "monospace"
                                font.pixelSize: 11
                            }
                            Controls.ToolButton {
                                icon.name: "edit-delete"
                                text: qsTr("Delete filter %1").arg(modelData.name)
                                display: Controls.AbstractButton.IconOnly
                                Accessible.name: qsTr("Delete filter %1").arg(modelData.name)
                                enabled: page.bridgeConnected
                                Controls.ToolTip.text: qsTr("Delete filter")
                                Controls.ToolTip.visible: hovered
                                onClicked: page.deleteFilterRequested(modelData.name)
                            }
                        }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                title: qsTr("Legacy .flt migration")
                subtitle: page.bridgeConnected
                    ? qsTr("Convert a WinMerge/ExamDiff .flt file to LinSync filter syntax.")
                    : qsTr("Connect the bridge to migrate legacy .flt files.")

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    AppButton {
                        text: qsTr("Migrate legacy .flt…")
                        icon.name: "document-open"
                        enabled: page.bridgeConnected
                        onClicked: page.openMigratePickerRequested()
                    }
                    Item { Layout.fillWidth: true }
                    AppButton {
                        text: qsTr("Clear")
                        flat: true
                        visible: page.migrateResult !== null
                        onClicked: page.migrateResult = null
                    }
                }

                Rectangle {
                    visible: page.migrateResult !== null
                    Layout.fillWidth: true
                    Layout.topMargin: 8
                    Layout.preferredHeight: migrateText.implicitHeight + 20
                    radius: 4
                    color: page.themeBgAlt
                    border.color: page.separator
                    border.width: 1

                    Controls.Label {
                        id: migrateText
                        anchors {
                            left: parent.left
                            right: parent.right
                            top: parent.top
                            margins: 10
                        }
                        wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                        font.family: "monospace"
                        font.pixelSize: 10
                        color: page.themeText
                        text: {
                            const r = page.migrateResult
                            if (!r) return ""
                            if (!r.ok) return qsTr("Error: %1").arg(r.error || qsTr("unknown error"))
                            let out = r.migrated || ""
                            if (r.warnings && r.warnings.length > 0)
                                out += "\n\n" + qsTr("Warnings (%1):").arg(r.warnings.length) + "\n"
                                    + r.warnings.join("\n")
                            return out
                        }
                    }
                }

                Controls.Label {
                    visible: page.migrateResult !== null && page.migrateResult.ok
                        && page.migrateResult.warnings && page.migrateResult.warnings.length > 0
                    text: qsTr("%1 line(s) could not be fully migrated (see warnings above).")
                        .arg(page.migrateResult ? (page.migrateResult.warnings ? page.migrateResult.warnings.length : 0) : 0)
                    color: page.themeNegative
                    font.pixelSize: 11
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                Controls.Label {
                    visible: page.migrateResult !== null && page.migrateResult.ok
                        && !(page.migrateResult.warnings && page.migrateResult.warnings.length > 0)
                    text: qsTr("Migration complete — all lines translated successfully.")
                    color: page.themePositive
                    font.pixelSize: 11
                    Layout.fillWidth: true
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                Layout.bottomMargin: 24
                wrapMode: Text.WordWrap
                opacity: 0.55
                font.pixelSize: 11
                text: qsTr("Glob syntax follows the gitignore spec: ** matches any number of directories, * matches within a path segment, and a leading / anchors the rule to the comparison root.")
            }
        }
    }
}
