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

    component ImageCard: Rectangle {
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

        ImagePane {
            id: paneImage
            anchors.top: paneHeader.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 4
            source: pane.imageSource
            zoom: root.imageZoom
            active: true
        }

        ColumnLayout {
            anchors.centerIn: parent
            visible: pane.imageSource === "" || paneImage.imageItem.status === Image.Error
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
                text: paneImage.imageItem.status === Image.Error
                    ? qsTr("Could not load image")
                    : pane.emptyPrimary
                color: root.activeText
                font.pixelSize: 14
                font.bold: true
            }
            Controls.Label {
                Layout.alignment: Qt.AlignHCenter
                horizontalAlignment: Text.AlignHCenter
                text: paneImage.imageItem.status === Image.Error
                    ? pane.imageSource.replace(/^file:\/\//, "")
                    : pane.emptySecondary
                color: root.activeDisabledText
                font.pixelSize: 12
                wrapMode: Text.NoWrap
                elide: Text.ElideMiddle
                Layout.maximumWidth: pane.width - 24
            }
        }

        Binding {
            target: pane
            property: "sourceImageSize"
            value: paneImage.imageItem.status === Image.Ready
                ? Qt.size(paneImage.imageItem.sourceSize.width, paneImage.imageItem.sourceSize.height)
                : Qt.size(0, 0)
            when: paneImage.imageItem.status === Image.Ready || paneImage.imageItem.status === Image.Null
        }
    }

    function urlToLocalPath(u) {
        var path = u.toString().replace(/^file:\/\//, "");
        return decodeURIComponent(path);
    }

    property var imageNameFilters: [qsTr("Images (*.png *.jpg *.jpeg *.webp *.tif *.tiff)"), qsTr("All files (*)")]
    property string supportedImageFormatsText: qsTr("PNG, JPEG, WebP, TIFF")

    FileDialog {
        id: saveOverlayDialog
        title: qsTr("Save diff overlay")
        fileMode: FileDialog.SaveFile
        nameFilters: [qsTr("PNG image (*.png)"), qsTr("All files (*)")]
        onAccepted: root.saveOverlayTo(root.urlToLocalPath(selectedFile))
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
    // Announce status/error changes to assistive technology as they happen.
    // Accessible.announce() exists on Qt 6.8+; guarded so older Qt is a no-op.
    onStatusTextChanged: {
        if (typeof statusBarLabel !== "undefined" && statusBarLabel.Accessible
                && typeof statusBarLabel.Accessible.announce === "function")
            statusBarLabel.Accessible.announce(root.statusText)
    }
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

    function refreshImageFormats() {
        root.bridgeGet("/compare/image/formats", function (ok, payload) {
            if (!ok || !payload || !payload.formats)
                return;

            var labels = [];
            for (var i = 0; i < payload.formats.length; i++) {
                if (payload.formats[i] && payload.formats[i].name)
                    labels.push(payload.formats[i].name);
            }
            if (labels.length > 0)
                root.supportedImageFormatsText = labels.join(", ");

            if (payload.extension_globs && payload.extension_globs.length > 0)
                root.imageNameFilters = [qsTr("Images (%1)").arg(payload.extension_globs.join(" ")), qsTr("All files (*)")];
        });
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
        // The spinbox is an integer in tenths of a ΔE unit (its label reads
        // "×10"), so the default 23 means ΔE 2.3. Scale to the real value the
        // bridge expects; sending it unscaled made the threshold 10× too lenient.
        const deltaE = deltaESpin.value / 10;
        const frameMode = frameCombo.currentIndex === 1 ? "all" : "first";
        const url = "/compare/image" + "?left=" + encodeURIComponent(root.leftPath) + "&right=" + encodeURIComponent(root.rightPath) + "&mode=" + modeStr + "&tolerance=" + tol + "&delta_e=" + deltaE + "&frames=" + frameMode + "&overlay=true";

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

    function saveOverlayTo(path) {
        if (path === "" || root.overlayUri === "") {
            root.statusText = "No overlay is available to save.";
            return;
        }

        root.bridgeGet("/compare/image/save-overlay?path=" + encodeURIComponent(path), function (ok, data) {
            if (ok && data && data.ok) {
                root.statusText = qsTr("Saved overlay to %1").arg(data.path || path);
            } else if (data && data.error) {
                root.statusText = qsTr("Save overlay failed: %1").arg(data.error);
            } else {
                root.statusText = qsTr("Save overlay failed.");
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

    Component.onCompleted: root.refreshImageFormats()

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 54
            color: root.activeBg
            border.color: root.separatorColor
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.margins: 8
                spacing: 8

                FilePickerBar {
                    label: qsTr("Left")
                    kind: qsTr("image")
                    path: root.leftPath
                    nameFilters: root.imageNameFilters
                    textColor: root.activeText
                    disabledTextColor: root.activeDisabledText
                    fieldColor: root.activeBg
                    borderColor: root.separatorColor
                    onPathPicked: function (pickedPath) { root.leftPath = pickedPath }
                }
                FilePickerBar {
                    label: qsTr("Right")
                    kind: qsTr("image")
                    path: root.rightPath
                    nameFilters: root.imageNameFilters
                    textColor: root.activeText
                    disabledTextColor: root.activeDisabledText
                    fieldColor: root.activeBg
                    borderColor: root.separatorColor
                    onPathPicked: function (pickedPath) { root.rightPath = pickedPath }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: root.activeBgAlt
            border.color: root.separatorColor
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 8

                Controls.Label {
                    text: qsTr("Mode:")
                    color: root.activeText
                    opacity: 0.7
                    font.pixelSize: 12
                }
                AppComboBox {
                    id: modeCombo
                    model: ["Exact", "Tolerance", "Perceptual"]
                    currentIndex: 0
                    Layout.preferredWidth: 140
                    implicitHeight: 30
                    Accessible.name: "Compare mode"
                }

                Kirigami.Separator { Layout.fillHeight: true }

                Controls.Label {
                    text: qsTr("Tolerance:")
                    color: modeCombo.currentIndex === 1 ? root.activeText : root.activeDisabledText
                    opacity: modeCombo.currentIndex === 1 ? 0.7 : 1.0
                    font.pixelSize: 12
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

                Kirigami.Separator { Layout.fillHeight: true }

                Controls.Label {
                    text: qsTr("ΔE:")
                    color: modeCombo.currentIndex === 2 ? root.activeText : root.activeDisabledText
                    opacity: modeCombo.currentIndex === 2 ? 0.7 : 1.0
                    font.pixelSize: 12
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

                Kirigami.Separator { Layout.fillHeight: true }

                Controls.Label {
                    text: qsTr("Frames:")
                    color: root.activeText
                    opacity: 0.7
                    font.pixelSize: 12
                }
                AppComboBox {
                    id: frameCombo
                    model: [qsTr("First frame"), qsTr("All frames")]
                    Layout.preferredWidth: 130
                    implicitHeight: 30
                    Accessible.name: qsTr("Frame compare mode")
                    Controls.ToolTip.text: qsTr("Compare only the first frame, or every frame in animated images (GIF, APNG, WebP)")
                    Controls.ToolTip.visible: hovered
                }

                AppButton {
                    Layout.preferredHeight: 30
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

                Controls.Label {
                    Layout.maximumWidth: 280
                    text: qsTr("Formats: %1").arg(root.supportedImageFormatsText)
                    color: root.activeDisabledText
                    font.pixelSize: 11
                    elide: Text.ElideRight
                    Accessible.name: qsTr("Supported image formats")
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: root.activeBgAlt
            border.color: root.separatorColor
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 8

                Controls.Label {
                    text: qsTr("Overlay opacity:")
                    color: root.activeText
                    opacity: 0.7
                    font.pixelSize: 12
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
                Kirigami.Separator { Layout.fillHeight: true }
                AppButton {
                    Layout.preferredWidth: 140
                    text: qsTr("Save PNG…")
                    icon.name: "document-save"
                    enabled: root.overlayUri !== ""
                    onClicked: saveOverlayDialog.open()
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

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
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

                Kirigami.Separator { Layout.fillHeight: true }

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

                ImageCard {
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

                ImageCard {
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

                    ImagePane {
                        id: overlayPane
                        anchors.top: overlayHeader.bottom
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 4
                        source: root.rightPath !== "" ? "file://" + root.rightPath : ""
                        zoom: root.imageZoom
                        active: true

                        Image {
                            x: overlayPane.imageItem.x
                            y: overlayPane.imageItem.y
                            width: overlayPane.imageItem.width
                            height: overlayPane.imageItem.height
                            source: root.overlayUri
                            fillMode: Image.Stretch
                            smooth: false
                            asynchronous: true
                            opacity: overlayOpacity.value
                            visible: root.overlayUri !== ""
                            cache: false
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

                ImageCard {
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

                ImageCard {
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
                id: statusBarLabel
                anchors {
                    verticalCenter: parent.verticalCenter
                    left: parent.left
                    leftMargin: 8
                }
                text: root.statusText
                color: root.activeText
                elide: Text.ElideRight
                // Expose the status line as a live region so screen
                // readers announce status/error changes as they happen.
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Status: %1").arg(root.statusText)
            }
        }
    }
}
