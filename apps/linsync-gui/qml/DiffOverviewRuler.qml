// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

// Reusable diff-overview ruler — the slim far-right strip from the main
// Compare page. Paints a mark for every differing row (so the whole file's
// diff distribution is visible at a glance) on a single Canvas (O(1) items),
// plus a draggable viewport indicator that doubles as a scrollbar handle.
//
// Wire-up: bind `diffRows`/`totalRows`/`topVisibleRow` from the editor panes
// and handle `jumpRequested(row)` to scroll both panes to that row.

import QtQuick

Rectangle {
    id: ruler

    property var diffRows: []          // row indexes that differ
    property int totalRows: 0
    property color markColor: "#e53935"
    property color highlightColor: "#3daee9"
    property color bgColor: "#f0f0f0"
    property color borderColor: "#cccccc"
    // Current scroll position as a fraction of the scroll range (0 = top,
    // 1 = fully scrolled). Drives the viewport indicator.
    property real scrollFraction: 0

    // Emitted on click/drag with the requested scroll fraction (0..1).
    signal jumpToFraction(real fraction)

    color: bgColor
    border.color: borderColor

    Canvas {
        id: canvas
        anchors.fill: parent
        anchors.margins: 6

        property var diffRows: ruler.diffRows
        property int totalRows: ruler.totalRows

        onDiffRowsChanged: requestPaint()
        onTotalRowsChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()

        onPaint: {
            var ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            if (totalRows < 2)
                return;
            var total = totalRows - 1;
            var step = height / total;
            ctx.fillStyle = ruler.markColor;
            for (var i = 0; i < diffRows.length; i++) {
                var r = Number(diffRows[i]);
                var y = r * step;
                var mh = Math.max(3, step * 0.8);
                y = Math.max(0, Math.min(height - mh, y));
                ctx.fillRect(0, y, width, mh);
            }
        }

        // Click/drag to scroll both panes. Maps so the indicator centres on
        // the cursor (no jump) and reaches the very ends of the track.
        MouseArea {
            anchors.fill: parent
            function jumpToY(y) {
                if (canvas.totalRows < 2)
                    return;
                var track = Math.max(1, height - viewportIndicator.height);
                var f = (y - viewportIndicator.height / 2) / track;
                ruler.jumpToFraction(Math.max(0, Math.min(1, f)));
            }
            onPressed: function (mouse) { jumpToY(mouse.y); }
            onPositionChanged: function (mouse) { if (pressed) jumpToY(mouse.y); }
        }

        // Viewport indicator — tracks the panes' scroll fraction, so it sits
        // at the bottom of the track when the panes are scrolled fully down.
        Rectangle {
            id: viewportIndicator
            x: 0
            width: parent.width
            height: 10
            radius: 2
            color: ruler.highlightColor
            opacity: 0.65
            visible: canvas.totalRows > 1

            readonly property real _track: Math.max(0, parent.height - height)
            y: _track * Math.max(0, Math.min(1, ruler.scrollFraction))
        }
    }
}
