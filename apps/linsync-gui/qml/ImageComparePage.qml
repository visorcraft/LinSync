// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami

// ImageComparePage — three-pane image compare: left | right | diff overlay.
// The diff overlay is a red-tinted mask (rgba(255,40,40,200) for each differing
// pixel) composited over the right image at adjustable opacity.
//
// Root is a Controls.Pane (not a plain Item) so QQC2 Controls (ComboBox /
// SpinBox / Button / Slider) inside inherit the ApplicationWindow root's
// QPalette through the standard QQC2 palette-inheritance chain. With a
// plain Item root that chain was broken — leaving the Fusion-style widgets
// rendering with whatever Qt thought the system theme was (typically dark
// on KDE) regardless of the LinSync palette set on the window root.
Controls.Pane {
    id: root
    padding: 0
    background: Rectangle { color: root.activeBg }

    // Reusable left/right image pane component. Renders a heading bar with
    // an accent stripe, the loaded image, and a centered icon + text empty
    // state when no path is set.
    component ImagePane: Rectangle {
        id: pane

        property string heading: ""
        property color accent: root.activeHighlight
        property string imageSource: ""
        property string emptyIconName: "image-x-generic"
        property string emptyPrimary: ""
        property string emptySecondary: ""

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

        Image {
            id: paneImage
            anchors.top: paneHeader.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 4
            source: pane.imageSource
            fillMode: Image.PreserveAspectFit
            smooth: true
            asynchronous: true
            visible: pane.imageSource !== "" && status !== Image.Error
        }

        // Empty state — only visible when no image is set or loading failed.
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

    // Source-file picker bar: a label, a read-only path field, and a Browse
    // button. Emits browseClicked(); the parent owns the FileDialog and
    // writes the chosen path back into `path`.
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

    // Convert a file:// URL (as returned by FileDialog) to a bare local path,
    // which is what the bridge /compare/image endpoint and the Image source
    // ("file://" + path) expect.
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

    // ── External interface ────────────────────────────────────────────────────
    required property string bridgeUrl
    required property color activeBg
    required property color activeBgAlt
    required property color activeText
    required property color activeDisabledText
    required property color activeHighlight
    required property color separatorColor

    property string leftPath: ""
    property string rightPath: ""

    // ── Internal state ────────────────────────────────────────────────────────
    property string statusText: "Select left and right image paths, then run compare."
    property string overlayUri: ""
    property bool running: false
    property var lastResult: null

    // ── Bridge helper ─────────────────────────────────────────────────────────
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
            root.overlayUri = data.overlay_path || "";
            if (data.equal) {
                root.statusText = "Images are equal (" + data.total_pixels + " pixels).";
            } else {
                const pct = (data.diff_ratio * 100).toFixed(2);
                root.statusText = data.differing_pixels + " of " + data.total_pixels + " pixels differ (" + pct + "%).";
            }
        });
    }

    // ── Layout ────────────────────────────────────────────────────────────────
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 8

        // ── Toolbar ──────────────────────────────────────────────────────────
        // Three rows so nothing has to elide on narrower windows.
        //   Row 0: [ Left image picker ............ ][ Right image picker .... ]
        //   Row 1: [ Comparison settings ] [ Run Compare ]
        //   Row 2: [ Overlay opacity slider .................... Save PNG ]
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

            // Comparison-settings group
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
                        // Stable page colours so the field stays themed (not
                        // black) when disabled — Kirigami.Theme mutes on disable.
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
                        // Represents 2.3 default stored as integer 23 to avoid float SpinBox.
                        enabled: modeCombo.currentIndex === 2
                        Layout.preferredWidth: 120
                        // Stable page colours so the field stays themed (not
                        // black) when disabled — Kirigami.Theme mutes on disable.
                        frameColor: root.activeBg
                        frameBorderColor: root.separatorColor
                        contentColor: root.activeText
                        stepHoverColor: root.activeBgAlt
                        Accessible.name: "DeltaE threshold (×10)"
                    }
                }
            }

            // Primary action: Run Compare
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

            // Flex spacer keeps the settings card + Run button left-aligned.
            Item { Layout.fillWidth: true }
        }

        // Row 2: overlay controls. The slider fills the row so it can shrink
        // freely; the Save PNG button on the right has a fixed slot so it
        // never gets clipped no matter how narrow the window is.
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

        // ── Three image panes ─────────────────────────────────────────────────
        // Each pane is a `imagePane` reused via `component` declaration so
        // header, empty-state, and image rendering stay consistent.
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 2

            // Left image pane
            ImagePane {
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

            // Right image pane
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

            // Diff overlay pane: right image + red mask composited at slider opacity
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: root.activeBgAlt
                clip: true

                // Header bar with accent stripe + label
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

                // Base: right image (background of the overlay pane)
                Image {
                    id: overlayBase
                    anchors.top: overlayHeader.bottom
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 4
                    source: root.rightPath !== "" ? "file://" + root.rightPath : ""
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    asynchronous: true
                    visible: root.overlayUri !== "" || root.rightPath !== ""
                }

                // Red diff mask on top
                Image {
                    anchors.fill: overlayBase
                    source: root.overlayUri
                    fillMode: Image.PreserveAspectFit
                    smooth: false
                    asynchronous: true
                    opacity: overlayOpacity.value
                    visible: root.overlayUri !== ""
                }

                // Placeholder when no overlay has been generated yet
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
                            : qsTr("Click “Run Compare” to generate.")
                        color: root.activeDisabledText
                        font.pixelSize: 12
                    }
                }
            }
        }

        // ── Status bar ────────────────────────────────────────────────────────
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
