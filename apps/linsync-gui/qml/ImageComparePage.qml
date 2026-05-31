// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami

Controls.Pane {
    id: root
    padding: 0
    background: Rectangle { color: root.activeBg }

    component ImagePane: Rectangle {
        id: pane

        property string heading: ""
        property color accent: root.activeHighlight
        property string imageSource: ""
        property string emptyIconName: "image-x-generic"
        property string emptyPrimary: ""
        property string emptySecondary: ""
        property size sourceImageSize: Qt.size(0, 0)

        color: root.activeBgAlt
        clip: true

        Rectangle {
            id: paneHeader
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 28
            color: root.activeBg
            border.color: root.separatorColor
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 8

                Rectangle {
                    Layout.preferredWidth: 3
                    Layout.preferredHeight: 14
                    color: pane.accent
                    radius: 2
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: pane.heading
                    color: root.activeText
                    font.bold: true
                    font.pixelSize: 12
                    elide: Text.ElideRight
                }
            }
        }

        Flickable {
            id: paneFlickable
            anchors.top: paneHeader.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 4
            clip: true
            contentWidth: Math.max(width, paneImage.zoomedWidth)
            contentHeight: Math.max(height, paneImage.zoomedHeight)

            Image {
                id: paneImage
                property real zoomedWidth: status === Image.Ready && sourceSize.width > 0 ? sourceSize.width * root.imageZoom : paneFlickable.width
                property real zoomedHeight: status === Image.Ready && sourceSize.height > 0 ? sourceSize.height * root.imageZoom : paneFlickable.height
                width: zoomedWidth
                height: zoomedHeight
                x: Math.max(0, (paneFlickable.width - zoomedWidth) / 2)
                y: Math.max(0, (paneFlickable.height - zoomedHeight) / 2)
                source: pane.imageSource
                fillMode: Image.Stretch
                smooth: true
                asynchronous: true
                visible: pane.imageSource !== "" && status !== Image.Error
                onStatusChanged: {
                    if (status === Image.Ready)
                        pane.sourceImageSize = Qt.size(sourceSize.width, sourceSize.height)
                }
            }
        }

        ColumnLayout {
            anchors.centerIn: parent
            visible: pane.imageSource === "" || paneImage.status === Image.Error
            spacing: 12

            Kirigami.Icon {
                source: pane.emptyIconName
                Layout.preferredWidth: 56
                Layout.preferredHeight: 56
                Layout.alignment: Qt.AlignHCenter
                color: root.activeDisabledText
                isMask: true
                opacity: 0.55
            }
            Controls.Label {
                Layout.alignment: Qt.AlignHCenter
                horizontalAlignment: Text.AlignHCenter
                text: paneImage.status === Image.Error
                    ? qsTr("Could not load image")
                    : pane.emptyPrimary
                color: root.activeText
                font.pixelSize: 14
                font.bold: true
            }
            Controls.Label {
                Layout.alignment: Qt.AlignHCenter
                horizontalAlignment: Text.AlignHCenter
                text: paneImage.status === Image.Error
                    ? pane.imageSource.replace(/^file:\/\//, "")
                    : pane.emptySecondary
                color: root.activeDisabledText
                font.pixelSize: 12
                wrapMode: Text.NoWrap
                elide: Text.ElideMiddle
                Layout.maximumWidth: pane.width - 24
            }
        }
    }

    component FilePickerBar: Rectangle {
        id: fp

        property string label: ""
        property string path: ""
        signal browseClicked()

        Layout.fillWidth: true
        Layout.preferredHeight: 40
        color: root.activeBgAlt
        border.color: root.separatorColor
        border.width: 1
        radius: 6

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 10
            anchors.rightMargin: 10
            spacing: 8

            Controls.Label {
                text: fp.label
                color: root.activeDisabledText
                font.pixelSize: 11
                Layout.preferredWidth: 40
            }
            AppTextField {
                Layout.fillWidth: true
                readOnly: true
                text: fp.path
                placeholderText: qsTr("No file selected — click Browse…")
                Accessible.name: fp.label + " image path"
            }
            AppButton {
                text: qsTr("Browse…")
                icon.name: "document-open"
                onClicked: fp.browseClicked()
            }
        }
    }

    function urlToLocalPath(u) {
        return u.toString().replace(/^file:\/\//, "");
    }

    FileDialog {
        id: leftFileDialog
        title: qsTr("Select left image")
        nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.bmp *.gif *.webp *.tif *.tiff)"), qsTr("All files (*)")]
        onAccepted: root.leftPath = root.urlToLocalPath(selectedFile)
    }

    FileDialog {
        id: rightFileDialog
        title: qsTr("Select right image")
        nameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.bmp *.gif *.webp *.tif *.tiff)"), qsTr("All files (*)")]
        onAccepted: root.rightPath = root.urlToLocalPath(selectedFile)
    }

    required property string bridgeUrl
    required property color activeBg
    required property color activeBgAlt
    required property color activeText
    required property color activeDisabledText
    required property color activeHighlight
    required property color separatorColor
    signal sessionUpdated(var context)

    property string leftPath: ""
    property string rightPath: ""

    property string statusText: "Select left and right image paths, then run compare."
    property string overlayUri: ""
    property bool running: false
    property var lastResult: null
    property real imageZoom: 1.0
    property bool splitViewActive: false

    function bridgeGet(path, onLoad) {
        if (root.bridgeUrl === "") {
            if (onLoad)
                onLoad(false, null);
            return;
        }
        const xhr = new XMLHttpRequest();
        xhr.onreadystatechange = function () {
            if (xhr.readyState === XMLHttpRequest.DONE) {
                const ok = xhr.status >= 200 && xhr.status < 300;
                let payload = null;
                try {
                    payload = JSON.parse(xhr.responseText);
                } catch (_) {}
                if (onLoad)
                    onLoad(ok, payload);
            }
        };
        xhr.open("GET", root.bridgeUrl + path);
        xhr.send();
    }

    function runCompare() {
        if (root.leftPath === "" || root.rightPath === "") {
            root.statusText = "Both left and right paths are required.";
            return;
        }
        root.running = true;
        root.overlayUri = "";
        root.lastResult = null;
        root.statusText = "Comparing…";

        const modeStr = modeCombo.currentText.toLowerCase();
        const tol = toleranceSpin.value;
        const deltaE = deltaESpin.value;
        const url = "/compare/image" + "?left=" + encodeURIComponent(root.leftPath) + "&right=" + encodeURIComponent(root.rightPath) + "&mode=" + modeStr + "&tolerance=" + tol + "&delta_e=" + deltaE + "&overlay=true";

        root.bridgeGet(url, function (ok, data) {
            root.running = false;
            if (!ok || !data) {
                root.statusText = "Compare failed — check file paths and format support.";
                return;
            }
            root.lastResult = data;
            if (data.session)
                root.sessionUpdated(data);
            root.overlayUri = data.overlay_path || "";
            if (data.equal) {
                root.statusText = "Images are equal (" + data.total_pixels + " pixels).";
            } else {
                const pct = (data.diff_ratio * 100).toFixed(2);
                root.statusText = data.differing_pixels + " of " + data.total_pixels + " pixels differ (" + pct + "%).";
            }
        });
    }

    function computeFitZoom() {
        var srcPane = root.splitViewActive ? splitLeftPane : leftImagePane
        var w = srcPane.sourceImageSize.width
        var h = srcPane.sourceImageSize.height
        if (w <= 0 || h <= 0) return 1.0
        var availW = srcPane.width - 8
        var availH = srcPane.height - 28 - 8
        if (availW <= 0 || availH <= 0) return 1.0
        return Math.min(availW / w, availH / h)
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            FilePickerBar {
                label: qsTr("Left")
                path: root.leftPath
                onBrowseClicked: leftFileDialog.open()
            }
            FilePickerBar {
                label: qsTr("Right")
                path: root.rightPath
                onBrowseClicked: rightFileDialog.open()
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Rectangle {
                Layout.preferredHeight: 40
                color: root.activeBgAlt
                border.color: root.separatorColor
                border.width: 1
                radius: 6
                Layout.preferredWidth: groupRow.implicitWidth + 20

                RowLayout {
                    id: groupRow
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 10
                    spacing: 10

                    Controls.Label {
                        text: qsTr("Mode")
                        color: root.activeDisabledText
                        font.pixelSize: 11
                    }
                    AppComboBox {
                        id: modeCombo
                        model: ["Exact", "Tolerance", "Perceptual"]
                        currentIndex: 0
                        Layout.preferredWidth: 140
                        implicitHeight: 30
                        Accessible.name: "Compare mode"
                    }

                    Rectangle {
                        Layout.preferredWidth: 1
                        Layout.fillHeight: true
                        Layout.topMargin: 8
                        Layout.bottomMargin: 8
                        color: root.separatorColor
                    }

                    Controls.Label {
                        text: qsTr("Tolerance")
                        color: modeCombo.currentIndex === 1 ? root.activeText : root.activeDisabledText
                        font.pixelSize: 11
                    }
                    AppSpinBox {
                        id: toleranceSpin
                        from: 0
                        to: 255
                        value: 0
                        enabled: modeCombo.currentIndex === 1
                        Layout.preferredWidth: 120
                        frameColor: root.activeBg
                        frameBorderColor: root.separatorColor
                        contentColor: root.activeText
                        stepHoverColor: root.activeBgAlt
                        Accessible.name: "Tolerance value (0-255)"
                    }

                    Rectangle {
                        Layout.preferredWidth: 1
                        Layout.fillHeight: true
                        Layout.topMargin: 8
                        Layout.bottomMargin: 8
                        color: root.separatorColor
                    }

                    Controls.Label {
                        text: qsTr("ΔE")
                        color: modeCombo.currentIndex === 2 ? root.activeText : root.activeDisabledText
                        font.pixelSize: 11
                    }
                    AppSpinBox {
                        id: deltaESpin
                        from: 0
                        to: 100
                        value: 23
                        enabled: modeCombo.currentIndex === 2
                        Layout.preferredWidth: 120
                        frameColor: root.activeBg
                        frameBorderColor: root.separatorColor
                        contentColor: root.activeText
                        stepHoverColor: root.activeBgAlt
                        Accessible.name: "DeltaE threshold (×10)"
                    }
                }
            }

            AppButton {
                Layout.preferredHeight: 40
                Layout.preferredWidth: 150
                text: root.running ? qsTr("Comparing…") : qsTr("Run Compare")
                icon.name: "media-playback-start"
                enabled: !root.running && root.leftPath !== "" && root.rightPath !== ""
                onClicked: root.runCompare()
            }

            Controls.BusyIndicator {
                running: root.running
                visible: root.running
                Layout.preferredWidth: 28
                Layout.preferredHeight: 28
            }

            Item { Layout.fillWidth: true }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: root.activeBgAlt
            border.color: root.separatorColor
            border.width: 1
            radius: 6

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 10
                spacing: 10

                Controls.Label {
                    text: qsTr("Overlay opacity")
                    color: root.activeDisabledText
                    font.pixelSize: 11
                }
                Controls.Slider {
                    id: overlayOpacity
                    Layout.fillWidth: true
                    from: 0.0
                    to: 1.0
                    value: 0.7
                    Accessible.name: "Overlay opacity"
                }
                Controls.Label {
                    text: Math.round(overlayOpacity.value * 100) + "%"
                    color: root.activeText
                    font.pixelSize: 11
                    Layout.preferredWidth: 38
                    horizontalAlignment: Text.AlignRight
                }
                Rectangle {
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                    Layout.topMargin: 8
                    Layout.bottomMargin: 8
                    color: root.separatorColor
                }
                AppButton {
                    Layout.preferredWidth: 140
                    text: qsTr("Save PNG…")
                    icon.name: "document-save"
                    enabled: root.overlayUri !== ""
                    onClicked: Qt.openUrlExternally(root.overlayUri)
                    Controls.ToolTip.text: qsTr("Save the overlay PNG to disk")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("Save Overlay PNG")
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: root.activeBgAlt
            border.color: root.separatorColor
            border.width: 1
            radius: 6

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 10
                spacing: 6

                Controls.ToolButton {
                    icon.name: "zoom-in"
                    icon.color: Kirigami.Theme.textColor
                    onClicked: root.imageZoom = Math.min(root.imageZoom * 1.25, 10.0)
                    Controls.ToolTip.text: qsTr("Zoom in")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("Zoom in")
                }
                Controls.ToolButton {
                    icon.name: "zoom-out"
                    icon.color: Kirigami.Theme.textColor
                    onClicked: root.imageZoom = Math.max(root.imageZoom / 1.25, 0.1)
                    Controls.ToolTip.text: qsTr("Zoom out")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("Zoom out")
                }
                Controls.ToolButton {
                    icon.name: "zoom-fit-best"
                    icon.color: Kirigami.Theme.textColor
                    onClicked: root.imageZoom = root.computeFitZoom()
                    Controls.ToolTip.text: qsTr("Fit to pane")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("Fit to pane")
                }
                Controls.ToolButton {
                    icon.name: "zoom-original"
                    icon.color: Kirigami.Theme.textColor
                    onClicked: root.imageZoom = 1.0
                    Controls.ToolTip.text: qsTr("1:1 (native size)")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("1:1")
                }
                Controls.Label {
                    text: Math.round(root.imageZoom * 100) + "%"
                    color: root.activeText
                    font.pixelSize: 11
                    Layout.preferredWidth: 48
                    horizontalAlignment: Text.AlignHCenter
                }

                Rectangle {
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                    Layout.topMargin: 8
                    Layout.bottomMargin: 8
                    color: root.separatorColor
                }

                Controls.ToolButton {
                    icon.name: "view-split-left-right"
                    icon.color: root.splitViewActive ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                    checkable: true
                    checked: root.splitViewActive
                    onToggled: root.splitViewActive = checked
                    Controls.ToolTip.text: qsTr("Split view")
                    Controls.ToolTip.visible: hovered
                    Accessible.name: qsTr("Toggle split view")
                }

                Item { Layout.fillWidth: true }
            }
        }

        Item {
            id: paneArea
            Layout.fillWidth: true
            Layout.fillHeight: true

            RowLayout {
                id: threePaneLayout
                anchors.fill: parent
                spacing: 2
                visible: !root.splitViewActive

                ImagePane {
                    id: leftImagePane
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    heading: qsTr("Left")
                    accent: Kirigami.Theme.neutralTextColor
                    imageSource: root.leftPath !== "" ? "file://" + root.leftPath : ""
                    emptyIconName: "document-open"
                    emptyPrimary: qsTr("No left image loaded")
                    emptySecondary: qsTr("Pick a file in the toolbar above to compare.")
                }

                Rectangle {
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                    color: root.separatorColor
                }

                ImagePane {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    heading: qsTr("Right")
                    accent: Kirigami.Theme.positiveTextColor
                    imageSource: root.rightPath !== "" ? "file://" + root.rightPath : ""
                    emptyIconName: "document-open"
                    emptyPrimary: qsTr("No right image loaded")
                    emptySecondary: qsTr("Pick a file in the toolbar above to compare.")
                }

                Rectangle {
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                    color: root.separatorColor
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: root.activeBgAlt
                    clip: true

                    Rectangle {
                        id: overlayHeader
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        height: 28
                        color: root.activeBg
                        border.color: root.separatorColor
                        border.width: 1

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 8
                            anchors.rightMargin: 8
                            spacing: 8

                            Rectangle {
                                Layout.preferredWidth: 3
                                Layout.preferredHeight: 14
                                color: root.activeNegativeText !== undefined ? root.activeNegativeText : root.activeHighlight
                                radius: 2
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: qsTr("Diff Overlay")
                                color: root.activeText
                                font.bold: true
                                font.pixelSize: 12
                            }
                        }
                    }

                    Flickable {
                        id: overlayFlickable
                        anchors.top: overlayHeader.bottom
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 4
                        clip: true
                        contentWidth: Math.max(width, overlayContentItem.zoomedWidth)
                        contentHeight: Math.max(height, overlayContentItem.zoomedHeight)

                        Item {
                            id: overlayContentItem
                            property real zoomedWidth: overlayBaseImg.status === Image.Ready && overlayBaseImg.sourceSize.width > 0 ? overlayBaseImg.sourceSize.width * root.imageZoom : overlayFlickable.width
                            property real zoomedHeight: overlayBaseImg.status === Image.Ready && overlayBaseImg.sourceSize.height > 0 ? overlayBaseImg.sourceSize.height * root.imageZoom : overlayFlickable.height
                            width: zoomedWidth
                            height: zoomedHeight
                            x: Math.max(0, (overlayFlickable.width - zoomedWidth) / 2)
                            y: Math.max(0, (overlayFlickable.height - zoomedHeight) / 2)

                            Image {
                                id: overlayBaseImg
                                anchors.fill: parent
                                source: root.rightPath !== "" ? "file://" + root.rightPath : ""
                                fillMode: Image.Stretch
                                smooth: true
                                asynchronous: true
                                visible: root.overlayUri !== "" || root.rightPath !== ""
                            }

                            Image {
                                anchors.fill: parent
                                source: root.overlayUri
                                fillMode: Image.Stretch
                                smooth: false
                                asynchronous: true
                                opacity: overlayOpacity.value
                                visible: root.overlayUri !== ""
                            }
                        }
                    }

                    ColumnLayout {
                        anchors.centerIn: parent
                        visible: root.overlayUri === "" && !root.running
                        spacing: 12

                        Kirigami.Icon {
                            source: "view-visible"
                            Layout.preferredWidth: 56
                            Layout.preferredHeight: 56
                            Layout.alignment: Qt.AlignHCenter
                            color: root.activeDisabledText
                            isMask: true
                            opacity: 0.6
                        }
                        Controls.Label {
                            Layout.alignment: Qt.AlignHCenter
                            horizontalAlignment: Text.AlignHCenter
                            text: root.lastResult ? qsTr("No differences to overlay") : qsTr("No diff overlay yet")
                            color: root.activeText
                            font.pixelSize: 14
                            font.bold: true
                        }
                        Controls.Label {
                            Layout.alignment: Qt.AlignHCenter
                            horizontalAlignment: Text.AlignHCenter
                            text: root.leftPath === "" || root.rightPath === ""
                                ? qsTr("Load both images first.")
                                : qsTr("Click \"Run Compare\" to generate.")
                            color: root.activeDisabledText
                            font.pixelSize: 12
                        }
                    }
                }
            }

            Controls.SplitView {
                id: splitPaneLayout
                anchors.fill: parent
                visible: root.splitViewActive
                orientation: Qt.Horizontal

                ImagePane {
                    id: splitLeftPane
                    Controls.SplitView.fillWidth: true
                    Controls.SplitView.minimumWidth: 120
                    heading: qsTr("Left")
                    accent: Kirigami.Theme.neutralTextColor
                    imageSource: root.leftPath !== "" ? "file://" + root.leftPath : ""
                    emptyIconName: "document-open"
                    emptyPrimary: qsTr("No left image loaded")
                    emptySecondary: qsTr("Pick a file in the toolbar above to compare.")
                }

                ImagePane {
                    id: splitRightPane
                    Controls.SplitView.fillWidth: true
                    Controls.SplitView.minimumWidth: 120
                    heading: qsTr("Right")
                    accent: Kirigami.Theme.positiveTextColor
                    imageSource: root.rightPath !== "" ? "file://" + root.rightPath : ""
                    emptyIconName: "document-open"
                    emptyPrimary: qsTr("No right image loaded")
                    emptySecondary: qsTr("Pick a file in the toolbar above to compare.")
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 24
            color: root.activeBg

            Controls.Label {
                anchors {
                    verticalCenter: parent.verticalCenter
                    left: parent.left
                    leftMargin: 8
                }
                text: root.statusText
                color: root.activeText
                elide: Text.ElideRight
            }
        }
    }
}
