// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Dialogs as Dialogs

// Read-only path field + browse button + file dialog, shared by the Image and
// Document compare pages. `kind` is the noun used in placeholder, tooltip, and
// dialog text ("image", "document"); `pathPicked` fires with the selected
// local path when the dialog is accepted.
RowLayout {
    id: fp

    property string label: ""
    property string kind: qsTr("file")
    property string path: ""
    property var nameFilters: [qsTr("All files (*)")]
    required property color textColor
    required property color disabledTextColor
    required property color fieldColor
    required property color borderColor
    signal pathPicked(string pickedPath)

    function urlToLocalPath(u) {
        var path = u.toString().replace(/^file:\/\//, "");
        return decodeURIComponent(path);
    }

    Layout.fillWidth: true
    Layout.preferredHeight: 36
    spacing: 6

    AppTextField {
        Layout.fillWidth: true
        implicitHeight: 36
        readOnly: true
        text: fp.path
        placeholderText: fp.label + " " + fp.kind + qsTr(" path")
        Accessible.name: fp.label + " " + fp.kind + " path"
        color: fp.textColor
        placeholderTextColor: fp.disabledTextColor
        background: Rectangle {
            color: fp.fieldColor
            border.color: fp.borderColor
            border.width: 1
            radius: 4
        }
    }
    Controls.ToolButton {
        icon.name: "document-open-folder"
        icon.color: fp.textColor
        Controls.ToolTip.text: qsTr("Browse %1 %2").arg(fp.label.toLowerCase()).arg(fp.kind)
        Controls.ToolTip.visible: hovered
        Accessible.name: qsTr("Browse %1 %2").arg(fp.label).arg(fp.kind)
        onClicked: pickDialog.open()
    }

    Dialogs.FileDialog {
        id: pickDialog
        title: qsTr("Select %1 %2").arg(fp.label.toLowerCase()).arg(fp.kind)
        nameFilters: fp.nameFilters
        onAccepted: fp.pathPicked(fp.urlToLocalPath(selectedFile))
    }
}
