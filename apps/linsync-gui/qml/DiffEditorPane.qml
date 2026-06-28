// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

// Reusable diff editor pane — the same editor architecture as the main
// Compare page's PaneColumn, packaged so other pages (Webpage compare) get
// identical behaviour: a single TextArea that owns all keyboard/mouse input
// (click-to-place cursor, multi-row drag-select, arrow keys, Home/End,
// PageUp/PageDown, Ctrl+C), with a per-row diff-colour underlay and a
// line-number gutter scrolling in lockstep.
//
// Layered exactly like PaneColumn:
//   z=0  per-row diff-colour rectangles (mouse-transparent)
//   z=1  ScrollView { TextArea } — owns input
//   z=2  line-number gutter + separator (mouse-transparent)
//   z=9  focus-forwarding MouseArea (Breeze-on-Qt-6.11 click-focus fix)

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Rectangle {
    id: pane

    // ── Inputs ────────────────────────────────────────────────────────────
    property string heading: ""
    property color accentColor: pane.highlightColor
    property var rows: []                 // [{ text, state, number }]

    property color activeBg: "#ffffff"
    property color activeBgAlt: "#f0f0f0"
    property color activeText: "#000000"
    property color activeDisabledText: "#888888"
    property color separatorColor: "#cccccc"
    property color highlightColor: "#3daee9"
    property var stateColors: ({})        // { left_only, right_only, changed }
    property string fontFamily: "monospace"
    property int fontSize: 12
    property bool showLineNumbers: true

    // Scroll-position mirror for sibling-pane sync. External code sets
    // `contentY`; internally we proxy to the ScrollView's inner Flickable and
    // mirror its scrolling back out so a parent can keep two panes in sync.
    property real contentY: 0
    onContentYChanged: {
        if (lineScroll && lineScroll.contentItem
            && lineScroll.contentItem.contentY !== contentY)
            lineScroll.contentItem.contentY = contentY;
    }

    readonly property real lineHeight: paneStack.lineHeight

    // Index of the topmost visible row, for an overview ruler's viewport
    // indicator. Derived from the scroll offset and the real rendered line
    // height (same metric the gutter uses).
    readonly property real topVisibleRow:
        lineScroll && lineScroll.contentItem && paneStack && paneStack.lineHeight > 0
            ? lineScroll.contentItem.contentY / paneStack.lineHeight
            : 0

    // Scroll so that `row` is centred in the viewport (used by the overview
    // ruler when clicked/dragged).
    function positionAtRow(row) {
        var inner = lineScroll ? lineScroll.contentItem : null;
        if (!inner)
            return;
        var lh = paneStack.lineHeight;
        var y = Math.max(0, row) * lh;
        var maxY = Math.max(0, inner.contentHeight - inner.height);
        inner.contentY = Math.max(0, Math.min(maxY, y - (inner.height - lh) / 2));
    }

    // Fraction of the way through the scroll range (0 = top, 1 = fully
    // scrolled). Used by the overview ruler's viewport indicator so it reaches
    // the very bottom at max scroll and tracks the handle 1:1 on click/drag.
    readonly property real scrollFraction: {
        if (!lineScroll || !lineScroll.contentItem)
            return 0;
        var inner = lineScroll.contentItem;
        var maxY = inner.contentHeight - inner.height;
        return maxY > 0 ? Math.max(0, Math.min(1, inner.contentY / maxY)) : 0;
    }

    function scrollToFraction(f) {
        var inner = lineScroll ? lineScroll.contentItem : null;
        if (!inner)
            return;
        var maxY = Math.max(0, inner.contentHeight - inner.height);
        inner.contentY = Math.max(0, Math.min(1, f)) * maxY;
    }

    color: activeBg
    border.color: separatorColor

    FontMetrics {
        id: paneFontMetrics
        font.family: pane.fontFamily
        font.pixelSize: pane.fontSize
    }

    function computeJoinedText() {
        if (!pane.rows || pane.rows.length === 0)
            return "";
        var parts = [];
        for (var i = 0; i < pane.rows.length; i++) {
            var r = pane.rows[i];
            parts.push(r && r.text !== undefined && r.text !== null ? String(r.text) : "");
        }
        return parts.join("\n");
    }
    function resetText() {
        contentArea.text = computeJoinedText();
    }
    // Assigning TextArea.text moves the caret to the end, which scrolls the
    // ScrollView to the bottom. Snap back to the top once layout settles
    // (same fix PaneColumn uses) so a fresh compare starts at row 0.
    Timer {
        id: scrollToTopTimer
        interval: 50
        repeat: false
        onTriggered: {
            if (lineScroll && lineScroll.contentItem)
                lineScroll.contentItem.contentY = 0;
        }
    }
    onRowsChanged: {
        resetText();
        scrollToTopTimer.restart();
    }
    Component.onCompleted: resetText()

    // Mirror inner Flickable scrolling back onto `contentY`.
    Connections {
        target: lineScroll && lineScroll.contentItem ? lineScroll.contentItem : null
        ignoreUnknownSignals: true
        function onContentYChanged() {
            var cy = lineScroll.contentItem.contentY;
            if (pane.contentY !== cy)
                pane.contentY = cy;
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Header bar: accent stripe + heading.
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 28
            color: pane.activeBgAlt
            border.color: pane.separatorColor

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 8

                Rectangle {
                    Layout.preferredWidth: 3
                    Layout.preferredHeight: 14
                    radius: 2
                    color: pane.accentColor
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: pane.heading
                    font.bold: true
                    font.pixelSize: 12
                    color: pane.activeText
                    elide: Text.ElideRight
                }
            }
        }

        Item {
            id: paneStack
            Layout.fillWidth: true
            Layout.fillHeight: true

            property real lineHeight: contentArea && contentArea.cursorRectangle.height > 0
                ? contentArea.cursorRectangle.height
                : paneFontMetrics.height
            property int gutterWidth: pane.showLineNumbers ? 48 : 0
            property int separatorWidth: pane.showLineNumbers ? 1 : 0
            property int textLeftPadding: gutterWidth + separatorWidth + 4
            property real scrollY: lineScroll && lineScroll.contentItem ? lineScroll.contentItem.contentY : 0

            // z=0: per-row diff backgrounds.
            Item {
                anchors.fill: parent
                clip: true
                z: 0

                Column {
                    id: rowBackgrounds
                    x: paneStack.gutterWidth + paneStack.separatorWidth
                    y: -paneStack.scrollY
                    width: Math.max(paneStack.width - x, 0)
                    spacing: 0

                    Repeater {
                        model: pane.rows
                        delegate: Rectangle {
                            required property int index
                            required property var modelData
                            property string _state: modelData && modelData.state ? String(modelData.state) : "equal"
                            width: rowBackgrounds.width
                            height: paneStack.lineHeight
                            color: pane.stateColors[_state] !== undefined ? pane.stateColors[_state] : "transparent"
                        }
                    }
                }
            }

            // z=1: the text (owns all input).
            Controls.ScrollView {
                id: lineScroll
                anchors.fill: parent
                clip: true
                z: 1

                Controls.TextArea {
                    id: contentArea
                    readOnly: false
                    font.family: pane.fontFamily
                    font.pixelSize: pane.fontSize
                    textFormat: Controls.TextArea.PlainText
                    color: pane.activeText
                    wrapMode: Controls.TextArea.NoWrap
                    selectByMouse: true
                    selectByKeyboard: true
                    persistentSelection: true
                    leftPadding: paneStack.textLeftPadding
                    rightPadding: 4
                    topPadding: 0
                    bottomPadding: 0
                    verticalAlignment: Controls.TextArea.AlignTop
                    background: Rectangle { color: "transparent" }

                    // TextArea inside ScrollView doesn't translate PageUp/Down
                    // into scrolling on its own — move the cursor one viewport
                    // and the ScrollView follows.
                    Keys.onPressed: function(event) {
                        if (event.key !== Qt.Key_PageUp && event.key !== Qt.Key_PageDown)
                            return;
                        var inner = lineScroll.contentItem;
                        if (!inner)
                            return;
                        var dir = event.key === Qt.Key_PageDown ? 1 : -1;
                        var pageDist = Math.max(paneStack.lineHeight, inner.height - paneStack.lineHeight);
                        var curRect = contentArea.cursorRectangle;
                        var targetY = curRect.y + dir * pageDist;
                        targetY = Math.max(0, Math.min(contentArea.contentHeight - 1, targetY));
                        contentArea.cursorPosition = contentArea.positionAt(curRect.x, targetY);
                        event.accepted = true;
                    }
                }
            }

            // z=2: line-number gutter + separator.
            Item {
                anchors.fill: parent
                clip: true
                z: 2
                visible: pane.showLineNumbers

                Column {
                    id: gutterColumn
                    x: 0
                    y: -paneStack.scrollY
                    width: paneStack.gutterWidth
                    spacing: 0

                    Repeater {
                        model: pane.rows
                        delegate: Rectangle {
                            required property int index
                            required property var modelData
                            width: gutterColumn.width
                            height: paneStack.lineHeight
                            color: pane.activeBg

                            Controls.Label {
                                anchors.fill: parent
                                text: modelData && modelData.number !== undefined && modelData.number !== null ? String(modelData.number) : ""
                                color: pane.activeDisabledText
                                font.family: pane.fontFamily
                                font.pixelSize: pane.fontSize
                                horizontalAlignment: Text.AlignRight
                                verticalAlignment: Text.AlignVCenter
                                rightPadding: 8
                            }
                        }
                    }
                }

                Rectangle {
                    x: paneStack.gutterWidth
                    y: 0
                    width: paneStack.separatorWidth
                    height: parent.height
                    color: pane.separatorColor
                }
            }

            // z=9: focus-forwarding overlay (Breeze-on-Qt-6.11 click-focus fix).
            MouseArea {
                anchors.fill: parent
                z: 9
                propagateComposedEvents: true
                onPressed: function(mouse) {
                    contentArea.forceActiveFocus();
                    mouse.accepted = false;
                }
            }
        }
    }
}
