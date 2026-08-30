import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "setupCard.js" as Setup

// The setup card of spec section 10, drawn when bin/grammachy is missing.
//
// The model comes from `ui/setupCard.js`, which owns the title, the body,
// and which of Install, the streamed log, and Retry the card shows for the
// current state. This file draws that model and reports which button was
// pressed; running bin/bootstrap.sh and reading cli.lock both live in
// Overlay.qml.
ColumnLayout {
  id: root

  // A card model from `Setup.card({...})`.
  property var card: null

  signal actionRequested(string action)

  readonly property string cardState: root.card ? String(root.card.state) : ""
  readonly property string title: root.card ? String(root.card.title) : ""
  readonly property string body: root.card ? String(root.card.body) : ""
  readonly property string log: root.card ? String(root.card.log) : ""
  readonly property bool showsInstall: Boolean(root.card) && root.card.showsInstall === true
  readonly property bool installEnabled: Boolean(root.card) && root.card.installEnabled === true
  readonly property bool showsLog: Boolean(root.card) && root.card.showsLog === true
  readonly property bool showsRetry: Boolean(root.card) && root.card.showsRetry === true
  readonly property string installReason: root.card ? String(root.card.installReason) : ""
  readonly property var missingDependencies: root.card && Array.isArray(root.card.missingDependencies)
    ? root.card.missingDependencies : []
  readonly property bool showsDependencies: Boolean(root.card) && root.card.showsDependencies === true
  readonly property bool running: root.cardState === Setup.RUNNING

  spacing: Style.spacing.md

  Text {
    Layout.fillWidth: true
    text: root.title
    color: root.cardState === Setup.DONE ? Color.popups.text : Color.urgent
    wrapMode: Text.Wrap
    font.family: Style.font.family
    font.pixelSize: Style.font.title
    font.bold: true
  }

  Text {
    Layout.fillWidth: true
    text: root.body
    color: Color.popups.text
    wrapMode: Text.Wrap
    font.family: Style.font.family
    font.pixelSize: Style.font.body
  }

  // Spec section 10: the system packages this machine lacks that the
  // bootstrap needs, each with its purpose. The card names them. It does not
  // install them. Add them through Omarchy Install.
  ColumnLayout {
    Layout.fillWidth: true
    visible: root.showsDependencies
    spacing: Style.spacing.xs

    Text {
      Layout.fillWidth: true
      text: "Missing system packages"
      color: Color.urgent
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.bodySmall
      font.bold: true
    }

    Repeater {
      model: root.missingDependencies

      RowLayout {
        required property var modelData

        Layout.fillWidth: true
        spacing: Style.spacing.sm

        Text {
          text: String(modelData.package)
          color: Color.popups.text
          font.family: "monospace"
          font.pixelSize: Style.font.caption
        }

        Text {
          Layout.fillWidth: true
          text: String(modelData.purpose)
          color: Color.muted
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }
      }
    }

    Text {
      Layout.fillWidth: true
      text: "Add each named package through Omarchy Install. Open SUPER+SPACE, then Install, then Package."
      color: Color.muted
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
  }

  // What bin/bootstrap.sh has printed so far, streamed line by line while it
  // runs and kept once it finishes, so a failure shows the reason.
  Flickable {
    Layout.fillWidth: true
    Layout.preferredHeight: Style.space(120)
    visible: root.showsLog
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    contentWidth: width
    contentHeight: logText.implicitHeight

    Text {
      id: logText

      width: parent.width
      text: root.log
      color: root.cardState === Setup.FAILED ? Color.urgent : Color.muted
      wrapMode: Text.Wrap
      font.family: "monospace"
      font.pixelSize: Style.font.caption
    }
  }

  RowLayout {
    Layout.fillWidth: true
    Layout.topMargin: Style.spacing.sm
    spacing: Style.spacing.lg

    // Why the bootstrap Install cannot be pressed, when a package it needs
    // is still missing.
    Text {
      Layout.fillWidth: true
      visible: root.installReason.length > 0
      text: root.installReason
      color: Color.muted
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }

    Item { Layout.fillWidth: true; visible: root.installReason.length === 0 }

    Button {
      text: "Close"
      bordered: true
      foreground: Color.popups.text
      fontFamily: Style.font.family
      tooltipText: "Esc"
      onClicked: root.actionRequested(Setup.CLOSE)
    }

    Button {
      visible: root.showsRetry
      text: "Retry the check"
      bordered: true
      foreground: Color.accent
      fontFamily: Style.font.family
      onClicked: root.actionRequested(Setup.RETRY)
    }

    Button {
      visible: root.showsInstall
      text: root.running ? "Installing..." : "Install"
      enabled: root.installEnabled
      opacity: root.installEnabled ? 1 : 0.6
      tooltipText: root.installReason
      bordered: true
      foreground: Color.accent
      fontFamily: Style.font.family
      onClicked: root.actionRequested(Setup.INSTALL)
    }
  }
}
