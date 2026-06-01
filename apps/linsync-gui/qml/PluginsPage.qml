// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Window
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    property var plugins: page.builtinPlugins
    property string filterText: ""
    property bool bridgeConnected: false
    property bool reduceMotion: false
    property var discoveryErrors: []
    property var discoveryRoots: []
    // Cache once per (plugins, filterText) change so the count label and the
    // Repeater don't re-filter the list twice on every keystroke.
    readonly property var filteredPlugins: page.filtered()

    signal pluginToggled(string id, bool enabled)
    signal refreshRequested()
    // Request that the bridge layer (Main.qml) fetch this plugin's option
    // schema+values and then call `openOptionsDialog`.
    signal pluginOptionsRequested(string id, string name)
    signal pluginOptionSaved(string id, string key, bool ok, string error)

    function applyDiscovery(payload) {
        if (!payload)
            return
        const discovered = payload.plugins || []
        const merged = page.builtinPlugins.slice()
        for (let index = 0; index < discovered.length; index++) {
            const entry = discovered[index]
            merged.push({
                id: entry.id,
                name: entry.name,
                version: entry.version,
                author: "(discovered)",
                license: entry.license,
                classes: entry.classes || [],
                extensions: entry.extensions || [],
                enabled: !!entry.enabled,
                builtin: false,
                source: entry.source || "user",
                description: qsTr("Discovered from %1").arg(entry.directory || "")
            })
        }
        page.plugins = merged
        page.discoveryErrors = payload.errors || []
        page.discoveryRoots = payload.roots || []
    }

    padding: 0
    titleDelegate: Item {}
    globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

    readonly property color themeBg: Kirigami.Theme.backgroundColor
    readonly property color themeBgAlt: Kirigami.Theme.alternateBackgroundColor
    readonly property color themeText: Kirigami.Theme.textColor
    readonly property color themeHighlight: Kirigami.Theme.highlightColor

    background: Rectangle { color: page.themeBg }

    // The instantiation site in Main.qml is responsible for binding
    // Kirigami.Theme.* to the live LinSync palette (root.active*).
    // We keep inherit:false here so descendants of this page use those
    // explicit values rather than whatever a deeper ancestor scope sets.
    Kirigami.Theme.inherit: false
    Kirigami.Theme.colorSet: Kirigami.Theme.Window
    readonly property color separator: Kirigami.ColorUtils.tintWithAlpha(themeBg, themeText, 0.2)

    readonly property var builtinPlugins: [
        { id: "linsync.builtin.text",
          name: "Built-in text engine",
          version: "1.0.0",
          author: "VisorCraft",
          license: "GPL-3.0-only",
          classes: ["prediffer"],
          extensions: ["txt", "md", "rs", "py", "js", "ts", "json", "yaml"],
          enabled: true,
          builtin: true,
          source: "core",
          description: "Default Myers-diff line comparison with whitespace + EOL normalization." },

        { id: "linsync.builtin.hex",
          name: "Built-in hex engine",
          version: "1.0.0",
          author: "VisorCraft",
          license: "GPL-3.0-only",
          classes: ["prediffer"],
          extensions: ["*"],
          enabled: true,
          builtin: true,
          source: "core",
          description: "Side-by-side hex view for any binary input." },

        { id: "linsync.builtin.folder",
          name: "Built-in folder walker",
          version: "1.0.0",
          author: "VisorCraft",
          license: "GPL-3.0-only",
          classes: ["folder_virtualizer"],
          extensions: [],
          enabled: true,
          builtin: true,
          source: "core",
          description: "Recursive directory comparison driven by the filter settings." },

        { id: "linsync.example.text-normalizer",
          name: "Example Text Normalizer",
          version: "1.0.0",
          author: "VisorCraft Examples",
          license: "MIT",
          classes: ["prediffer"],
          extensions: ["txt", "log"],
          enabled: false,
          builtin: false,
          source: "user",
          description: "Sample prediffer that demonstrates the JSON helper protocol." },

        { id: "linsync.example.zip-unpacker",
          name: "Example Zip Unpacker",
          version: "1.0.0",
          author: "VisorCraft Examples",
          license: "MIT",
          classes: ["unpacker", "folder_virtualizer"],
          extensions: ["zip", "jar", "apk"],
          enabled: false,
          builtin: false,
          source: "user",
          description: "Example entry for ZIP-family virtual folder helpers." }
    ]

    function filtered() {
        if (page.filterText === "")
            return page.plugins
        const needle = page.filterText.toLowerCase()
        return page.plugins.filter(p =>
            p.name.toLowerCase().indexOf(needle) !== -1
            || p.id.toLowerCase().indexOf(needle) !== -1
            || p.classes.join(" ").toLowerCase().indexOf(needle) !== -1)
    }

    function sourceBadgeColor(source) {
        if (source === "core")
            return Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themeHighlight, 0.18)
        if (source === "system")
            return Kirigami.ColorUtils.tintWithAlpha(page.themeBg, Kirigami.Theme.neutralTextColor, 0.18)
        return Kirigami.ColorUtils.tintWithAlpha(page.themeBg, Kirigami.Theme.positiveTextColor, 0.18)
    }

    function classChipColor() {
        return Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themeText, 0.1)
    }

    function setEnabled(id, value) {
        const next = page.plugins.map(p =>
            p.id === id ? Object.assign({}, p, { enabled: value }) : p)
        page.plugins = next
        page.pluginToggled(id, value)
    }

    // ── Plugin options dialog ─────────────────────────────────────────────────

    property string _optionsPluginId: ""
    property string _optionsPluginName: ""
    property var _optionsSchema: []
    property var _optionsValues: ({})
    property var _optionsDirty: ({})
    property string _optionsError: ""

    Kirigami.Dialog {
        id: optionsDialog
        title: qsTr("Plugin Settings: %1").arg(page._optionsPluginName)
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
        preferredWidth: Kirigami.Units.gridUnit * 28

        onAccepted: {
            // Save every dirty key via the bridge signal.
            const dirty = page._optionsDirty
            for (const key in dirty) {
                page.pluginOptionSaved(page._optionsPluginId, key, true, "")
            }
            page._optionsDirty = {}
        }

        ColumnLayout {
            id: optionsForm
            spacing: Kirigami.Units.smallSpacing * 2
            width: parent ? parent.width : 0

            Controls.Label {
                Layout.fillWidth: true
                text: page._optionsError
                color: Kirigami.Theme.negativeTextColor
                visible: page._optionsError !== ""
                wrapMode: Text.WordWrap
            }

            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("This plugin has no configurable options.")
                opacity: 0.6
                font.pixelSize: 12
                visible: page._optionsSchema.length === 0
            }

            Repeater {
                model: page._optionsSchema
                delegate: ColumnLayout {
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 2

                    Controls.Label {
                        text: modelData.label
                        font.pixelSize: 12
                        opacity: 0.8
                    }

                    // String control
                    AppTextField {
                        visible: modelData.kind === "string"
                        Layout.fillWidth: true
                        implicitHeight: 32
                        color: page.themeText
                        placeholderTextColor: Qt.rgba(page.themeText.r, page.themeText.g, page.themeText.b, 0.5)
                        background: Rectangle {
                            color: page.themeBg
                            border.color: page.separator
                            border.width: 1
                            radius: 4
                        }
                        text: (page._optionsValues[modelData.key] !== undefined)
                            ? String(page._optionsValues[modelData.key]) : ""
                        onTextEdited: {
                            const v = Object.assign({}, page._optionsValues)
                            v[modelData.key] = text
                            page._optionsValues = v
                            const d = Object.assign({}, page._optionsDirty)
                            d[modelData.key] = text
                            page._optionsDirty = d
                        }
                    }

                    // Bool control
                    Controls.Switch {
                        visible: modelData.kind === "bool"
                        Accessible.name: modelData.label
                        checked: {
                            const v = page._optionsValues[modelData.key]
                            return v !== undefined ? !!v : !!(modelData.default)
                        }
                        onToggled: {
                            const v = Object.assign({}, page._optionsValues)
                            v[modelData.key] = checked
                            page._optionsValues = v
                            const d = Object.assign({}, page._optionsDirty)
                            d[modelData.key] = checked
                            page._optionsDirty = d
                        }
                    }

                    // Int control
                    Controls.SpinBox {
                        visible: modelData.kind === "int"
                        Accessible.name: modelData.label
                        from: 0
                        to: 9999
                        value: {
                            const v = page._optionsValues[modelData.key]
                            return v !== undefined ? parseInt(v) : (modelData.default !== null ? parseInt(modelData.default) : 0)
                        }
                        onValueModified: {
                            const v = Object.assign({}, page._optionsValues)
                            v[modelData.key] = value
                            page._optionsValues = v
                            const d = Object.assign({}, page._optionsDirty)
                            d[modelData.key] = value
                            page._optionsDirty = d
                        }
                    }

                    // Enum control
                    Controls.ComboBox {
                        visible: modelData.kind === "enum"
                        Accessible.name: modelData.label
                        model: modelData.choices || []
                        currentIndex: {
                            const cur = page._optionsValues[modelData.key]
                                ?? modelData.default
                            return (modelData.choices || []).indexOf(String(cur))
                        }
                        onActivated: {
                            const v = Object.assign({}, page._optionsValues)
                            v[modelData.key] = currentText
                            page._optionsValues = v
                            const d = Object.assign({}, page._optionsDirty)
                            d[modelData.key] = currentText
                            page._optionsDirty = d
                        }
                    }
                }
            }
        }
    }

    // Handler: the bridge layer (Main.qml) populates schema + values, then opens
    // the dialog.
    function openOptionsDialog(pluginId, pluginName, schema, values) {
        page._optionsPluginId = pluginId
        page._optionsPluginName = pluginName
        page._optionsSchema = schema || []
        page._optionsValues = values || {}
        page._optionsDirty = {}
        page._optionsError = ""
        optionsDialog.open()
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
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 24
                anchors.rightMargin: 24
                spacing: 12

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 1
                    Controls.Label {
                        text: qsTr("Plugins")
                        font.pixelSize: 22
                        font.bold: true
                        font.letterSpacing: 0
                    }
                    Controls.Label {
                        text: qsTr("%1 plugin%2 listed · %3 enabled")
                            .arg(page.plugins.length).arg(page.plugins.length === 1 ? "" : "s")
                            .arg(page.plugins.filter(p => p.enabled).length)
                        opacity: 0.6
                        font.pixelSize: 12
                    }
                }

                Controls.Button {
                    icon.name: "view-refresh"
                    text: qsTr("Rescan")
                    flat: true
                    enabled: page.bridgeConnected
                    Controls.ToolTip.text: page.bridgeConnected
                        ? qsTr("Re-run plugin discovery against the user + system roots")
                        : qsTr("Plugin discovery is not connected in this build")
                    Controls.ToolTip.visible: hovered
                    Accessible.description: page.bridgeConnected
                        ? qsTr("Re-run plugin discovery against the user + system roots")
                        : qsTr("Plugin discovery is not connected in this build")
                    onClicked: page.refreshRequested()
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
                title: qsTr("Discovery paths")
                subtitle: qsTr("Plugin manifests use these install locations when discovery is enabled.")

                Repeater {
                    model: [
                        { label: qsTr("User plugins"),  path: "$XDG_DATA_HOME/linsync/plugins/<id>/" },
                        { label: qsTr("System plugins"), path: "/usr/share/linsync/plugins/<id>/" },
                        { label: qsTr("Local install"),  path: "/usr/local/share/linsync/plugins/<id>/" }
                    ]
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 12
                        Controls.Label {
                            Layout.preferredWidth: 130
                            text: modelData.label
                            opacity: 0.7
                            font.pixelSize: 11
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: modelData.path
                            font.family: "monospace"
                            font.pixelSize: 11
                            elide: Text.ElideMiddle
                        }
                    }
                }
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
                    placeholderText: qsTr("Filter by name, id, or class…")
                    onTextChanged: page.filterText = text
                    Accessible.name: "Filter plugins"
                }
                Controls.Label {
                    text: qsTr("%1 / %2").arg(page.filteredPlugins.length).arg(page.plugins.length)
                    opacity: 0.6
                    font.pixelSize: 11
                    font.family: "monospace"
                }
            }

            Repeater {
                model: page.filteredPlugins
                delegate: Rectangle {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredHeight: detailColumn.implicitHeight + 28
                    radius: 8
                    color: page.themeBg
                    border.color: page.separator
                    border.width: 1

                    Rectangle {
                        anchors.left: parent.left
                        anchors.top: parent.top
                        anchors.bottom: parent.bottom
                        width: 4
                        radius: parent.radius
                        color: modelData.enabled
                            ? Kirigami.Theme.positiveTextColor
                            : Kirigami.Theme.disabledTextColor
                    }

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 18
                        anchors.rightMargin: 14
                        anchors.topMargin: 14
                        anchors.bottomMargin: 14
                        spacing: 14

                        ColumnLayout {
                            id: detailColumn
                            Layout.fillWidth: true
                            spacing: 4

                            RowLayout {
                                spacing: 8
                                Controls.Label {
                                    text: modelData.name
                                    font.pixelSize: 14
                                    font.bold: true
                                }

                                Rectangle {
                                    radius: 10
                                    color: page.sourceBadgeColor(modelData.source)
                                    implicitHeight: 18
                                    implicitWidth: srcLabel.implicitWidth + 14
                                    Controls.Label {
                                        id: srcLabel
                                        anchors.centerIn: parent
                                        // qsTr() must take a literal so lupdate can extract it,
                                        // and modelData.source may be missing on third-party entries.
                                        text: modelData.builtin
                                            ? qsTr("BUILT-IN")
                                            : String(modelData.source || "user").toUpperCase()
                                        font.pixelSize: 9
                                        font.bold: true
                                    }
                                }

                                Controls.Label {
                                    text: "v" + modelData.version
                                    opacity: 0.6
                                    font.pixelSize: 11
                                    font.family: "monospace"
                                }
                                Item { Layout.fillWidth: true }
                            }

                            Controls.Label {
                                text: modelData.id
                                opacity: 0.5
                                font.pixelSize: 10
                                font.family: "monospace"
                            }

                            Controls.Label {
                                Layout.fillWidth: true
                                text: modelData.description
                                opacity: 0.8
                                font.pixelSize: 12
                                wrapMode: Text.WordWrap
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                Layout.topMargin: 4
                                spacing: 6

                                Repeater {
                                    model: modelData.classes
                                    delegate: Rectangle {
                                        required property var modelData
                                        radius: 9
                                        color: page.classChipColor()
                                        implicitHeight: 18
                                        implicitWidth: classLabel.implicitWidth + 14
                                        Controls.Label {
                                            id: classLabel
                                            anchors.centerIn: parent
                                            text: modelData
                                            font.pixelSize: 10
                                            font.family: "monospace"
                                        }
                                    }
                                }
                                Item { Layout.fillWidth: true }
                                Controls.Label {
                                    text: modelData.extensions && modelData.extensions.length > 0
                                        ? qsTr("Extensions: %1").arg(modelData.extensions.slice(0, 6).join(", ") + (modelData.extensions.length > 6 ? "…" : ""))
                                        : ""
                                    opacity: 0.55
                                    font.pixelSize: 10
                                    font.family: "monospace"
                                }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 12
                                Controls.Label {
                                    text: qsTr("by %1").arg(modelData.author)
                                    opacity: 0.55
                                    font.pixelSize: 10
                                }
                                Controls.Label {
                                    text: qsTr("license: %1").arg(modelData.license)
                                    opacity: 0.55
                                    font.pixelSize: 10
                                    font.family: "monospace"
                                }
                                Item { Layout.fillWidth: true }
                            }
                        }

                        Controls.Button {
                            Layout.alignment: Qt.AlignVCenter
                            text: qsTr("Settings…")
                            flat: true
                            // Only show the button when the plugin declares options
                            // (discovered plugins expose has_options; builtins don't).
                            visible: !!modelData.has_options
                            enabled: page.bridgeConnected
                            Controls.ToolTip.text: page.bridgeConnected
                                ? qsTr("Configure plugin options")
                                : qsTr("Plugin options require a bridge connection")
                            Controls.ToolTip.visible: hovered
                            onClicked: page.pluginOptionsRequested(modelData.id, modelData.name)
                        }

                        Controls.Switch {
                            id: pluginSwitch
                            Layout.alignment: Qt.AlignVCenter
                            checked: modelData.enabled
                            enabled: !modelData.builtin
                            Accessible.name: modelData.builtin
                                ? qsTr("%1 (built-in, always enabled)").arg(modelData.name)
                                : (modelData.enabled
                                    ? qsTr("Disable %1").arg(modelData.name)
                                    : qsTr("Enable %1").arg(modelData.name))
                            Controls.ToolTip.text: modelData.builtin
                                ? qsTr("Built-in plugins cannot be disabled")
                                : (modelData.enabled ? qsTr("Disable") : qsTr("Enable"))
                            Controls.ToolTip.visible: hovered
                            onToggled: page.setEnabled(modelData.id, checked)

                            // Custom themed track + handle. Colours come from
                            // the page-level theme properties (read while the
                            // page is enabled), so a disabled-but-checked
                            // built-in switch shows a dimmed highlight track
                            // instead of going black (Kirigami.Theme colours
                            // mute toward black on a disabled control).
                            indicator: Rectangle {
                                implicitWidth: 40
                                implicitHeight: 20
                                x: pluginSwitch.leftPadding
                                y: parent.height / 2 - height / 2
                                radius: height / 2
                                color: pluginSwitch.checked
                                    ? page.themeHighlight
                                    : Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themeText, 0.28)
                                border.color: Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themeText, 0.2)
                                border.width: 1
                                opacity: pluginSwitch.enabled ? 1.0 : 0.55

                                Rectangle {
                                    width: 16
                                    height: 16
                                    radius: 8
                                    y: 2
                                    x: pluginSwitch.checked ? parent.width - width - 2 : 2
                                    color: "#ffffff"
                                    border.color: Kirigami.ColorUtils.tintWithAlpha(page.themeBg, page.themeText, 0.25)
                                    border.width: 1
                                    Behavior on x { NumberAnimation { duration: page.reduceMotion ? 0 : 120; easing.type: Easing.OutCubic } }
                                }
                            }
                        }
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                Layout.topMargin: 6
                Layout.bottomMargin: 24
                wrapMode: Text.WordWrap
                opacity: 0.55
                font.pixelSize: 11
                text: qsTr("Plugins run as external helper processes communicating with LinSync over JSON-on-stdio (see docs/plugin-protocol.md). Windows-only in-process plugin formats are not supported on Linux.")
            }
        }
    }
}
