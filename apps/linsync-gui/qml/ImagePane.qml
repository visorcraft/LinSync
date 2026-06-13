// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls

// Reusable zoom/pan image pane.
Flickable {
    id: root

    property alias source: image.source
    property alias imageItem: image
    property real zoom: 1.0
    property bool active: true

    clip: true
    contentWidth: Math.max(width, image.zoomedWidth)
    contentHeight: Math.max(height, image.zoomedHeight)

    Image {
        id: image
        property real zoomedWidth: status === Image.Ready && sourceSize.width > 0 ? sourceSize.width * root.zoom : root.width
        property real zoomedHeight: status === Image.Ready && sourceSize.height > 0 ? sourceSize.height * root.zoom : root.height
        width: zoomedWidth
        height: zoomedHeight
        x: Math.max(0, (root.width - zoomedWidth) / 2)
        y: Math.max(0, (root.height - zoomedHeight) / 2)
        fillMode: Image.Stretch
        smooth: true
        asynchronous: true
        cache: false
        visible: source !== "" && status !== Image.Error
    }

    Controls.ScrollBar.vertical: Controls.ScrollBar {
        policy: root.active ? Controls.ScrollBar.AsNeeded : Controls.ScrollBar.AlwaysOff
    }
    Controls.ScrollBar.horizontal: Controls.ScrollBar {
        policy: root.active ? Controls.ScrollBar.AsNeeded : Controls.ScrollBar.AlwaysOff
    }
}
