// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Real grid view for table (CSV/TSV) compare. Shows column headers, row numbers,
// and per-cell state highlighting (Equal / Changed / LeftOnly / RightOnly).
// Changed cells render both values with left in negative and right in positive.
Rectangle {
    id: root

    property var headers: []
    property var rows: []

    signal loadMoreRequested()

    color: Kirigami.Theme.backgroundColor

    readonly property color separator: Kirigami.ColorUtils.tintWithAlpha(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)

    function columnCount() {
        if (root.headers && root.headers.length > 0)
            return root.headers.length
        if (root.rows && root.rows.length > 0 && root.rows[0].cells)
            return root.rows[0].cells.length
        return 1
    }

    function cellWidth() {
        return Math.max(96, (root.width - rowNumberWidth) / columnCount())
    }

    readonly property int rowNumberWidth: 56
    readonly property int rowHeight: 34
    readonly property int headerHeight: 34

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Header row
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: root.headerHeight
            color: Kirigami.Theme.alternateBackgroundColor
            border.color: root.separator

            Row {
                anchors.fill: parent

                Rectangle {
                    width: root.rowNumberWidth
                    height: parent.height
                    color: Kirigami.Theme.alternateBackgroundColor
                    border.color: root.separator

                    Controls.Label {
                        anchors.centerIn: parent
                        text: "#"
                        color: Kirigami.Theme.textColor
                        font.bold: true
                    }
                }

                Repeater {
                    model: root.headers

                    Rectangle {
                        required property var modelData

                        width: root.cellWidth()
                        height: parent.height
                        color: Kirigami.Theme.alternateBackgroundColor
                        border.color: root.separator

                        Controls.Label {
                            anchors.fill: parent
                            anchors.margins: 4
                            text: modelData || ""
                            color: Kirigami.Theme.textColor
                            font.bold: true
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }

        // Body rows
        ListView {
            id: tableBody

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.rows
            boundsBehavior: Flickable.StopAtBounds

            onContentYChanged: {
                if (contentHeight - (contentY + height) < height)
                    root.loadMoreRequested()
            }

            delegate: Rectangle {
                required property var modelData
                required property int index

                width: tableBody.width
                height: root.rowHeight
                color: index % 2 === 0 ? Kirigami.Theme.backgroundColor : Kirigami.Theme.alternateBackgroundColor

                Row {
                    anchors.fill: parent

                    Rectangle {
                        width: root.rowNumberWidth
                        height: parent.height
                        color: Kirigami.Theme.alternateBackgroundColor
                        border.color: root.separator

                        Controls.Label {
                            anchors.centerIn: parent
                            text: modelData.row_index + 1
                            color: Kirigami.Theme.textColor
                        }
                    }

                    Repeater {
                        model: modelData.cells

                        Rectangle {
                            required property var modelData

                            width: root.cellWidth()
                            height: parent.height
                            color: cellBackground(modelData.state)
                            border.color: root.separator

                            function cellBackground(state) {
                                const bg = Kirigami.Theme.backgroundColor
                                if (state === "Changed")
                                    return Kirigami.ColorUtils.tintWithAlpha(bg, Kirigami.Theme.neutralTextColor, 0.16)
                                if (state === "LeftOnly")
                                    return Kirigami.ColorUtils.tintWithAlpha(bg, Kirigami.Theme.negativeTextColor, 0.14)
                                if (state === "RightOnly")
                                    return Kirigami.ColorUtils.tintWithAlpha(bg, Kirigami.Theme.positiveTextColor, 0.14)
                                return bg
                            }

                            Row {
                                anchors.fill: parent
                                anchors.margins: 4
                                spacing: 4

                                readonly property bool showBoth: modelData.state === "Changed"
                                readonly property real valueWidth: showBoth
                                    ? (parent.width - arrowLabel.implicitWidth - parent.spacing) / 2
                                    : parent.width

                                Controls.Label {
                                    visible: modelData.state === "Changed" || modelData.state === "LeftOnly" || modelData.state === "Equal"
                                    width: parent.valueWidth
                                    text: modelData.left || ""
                                    color: modelData.state === "Changed" ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
                                    elide: Text.ElideRight
                                }
                                Controls.Label {
                                    id: arrowLabel
                                    visible: modelData.state === "Changed"
                                    text: "→"
                                    color: Kirigami.Theme.textColor
                                }
                                Controls.Label {
                                    visible: modelData.state === "Changed" || modelData.state === "RightOnly"
                                    width: parent.valueWidth
                                    text: modelData.right || ""
                                    color: modelData.state === "Changed" ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
