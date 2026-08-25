import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui

// The hero both surfaces wear, spec sections 6 and 9: the mark, the name, one
// meta line, whatever actions the caller hangs on the trailing edge, and the
// gear that flips the card to Settings.
//
// The caller words its own meta line, because what it says is the caller's
// state, and names its own actions for the same reason. The auto-replace
// toggle is the popup's alone: spec section 9 says auto-replace never applies
// in Compose, so a toggle there would promise something the Apply button does
// not do.
ColumnLayout {
  id: root

  // The one line under the name: the counts and the engine, or what the card
  // is waiting on.
  property string metaText: ""
  // A second line in the accent colour, for a caveat about the meta line
  // above it. Empty hides it.
  property string noteText: ""

  property bool showsAutoReplace: false
  property bool autoReplace: false
  property bool settingsOpen: false

  // The actions on the trailing edge, before the gear. Each entry is
  // `{ id, text, tooltip, primary }`. The popup puts its Compose button here
  // (spec section 6) and Compose puts the Cancel of a chunked Check here (spec
  // section 9), so the hero carries either without knowing what it does.
  property var actions: []

  signal autoReplaceToggled()
  signal settingsToggled()
  signal actionRequested(string id)

  spacing: Style.spacing.lg

  RowLayout {
    Layout.fillWidth: true
    spacing: Style.spacing.xxl

    Text {
      text: "G"
      color: Color.accent
      font.family: Style.font.family
      font.pixelSize: Style.font.heading
      font.bold: true
    }

    ColumnLayout {
      Layout.fillWidth: true
      spacing: Style.spacing.xxs

      Text {
        text: "Grammachy"
        color: Color.popups.text
        font.family: Style.font.family
        font.pixelSize: Style.font.title
        font.bold: true
      }

      Text {
        Layout.fillWidth: true
        text: root.metaText
        color: Color.muted
        elide: Text.ElideRight
        font.family: Style.font.family
        font.pixelSize: Style.font.bodySmall
      }

      Text {
        Layout.fillWidth: true
        visible: root.noteText.length > 0
        text: root.noteText
        color: Color.accent
        elide: Text.ElideRight
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
    }

    // The auto-replace toggle, spec sections 6 and 7. The Settings view
    // persists it; the hero holds a session-local override beside that.
    RowLayout {
      Layout.alignment: Qt.AlignVCenter
      visible: root.showsAutoReplace
      spacing: Style.spacing.lg

      Text {
        text: "Auto-replace"
        color: Color.popups.text
        font.family: Style.font.family
        font.pixelSize: Style.font.bodySmall
      }

      ToggleSwitch {
        checked: root.autoReplace
        foreground: Color.popups.text
        onToggled: root.autoReplaceToggled()
      }
    }

    Repeater {
      model: root.actions

      Button {
        required property var modelData

        Layout.alignment: Qt.AlignVCenter
        text: String(modelData.text)
        tooltipText: modelData.tooltip ? String(modelData.tooltip) : ""
        bordered: true
        foreground: modelData.primary === true ? Color.accent : Color.popups.text
        fontFamily: Style.font.family
        onClicked: root.actionRequested(String(modelData.id))
      }
    }

    // The one control that flips the body, spec section 7. It sits in the
    // hero so it is reachable from every phase, the notice card included,
    // and it stays a gear both ways so the hero never shifts under a click.
    Button {
      Layout.alignment: Qt.AlignVCenter
      iconText: "󰒓"
      tooltipText: root.settingsOpen ? "Back to the check" : "Settings"
      bordered: true
      selected: root.settingsOpen
      foreground: Color.popups.text
      fontFamily: Style.font.family
      onClicked: root.settingsToggled()
    }
  }

  // The hint spans the hero rather than riding beside the switch, so it
  // stays one line at any theme font and never squeezes the meta line.
  Text {
    Layout.fillWidth: true
    Layout.topMargin: -Style.spacing.sm
    visible: root.showsAutoReplace
    text: "Replaces the highlighted text by pasting over it"
    color: Color.muted
    elide: Text.ElideRight
    horizontalAlignment: Text.AlignRight
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }

  Rectangle {
    Layout.fillWidth: true
    implicitHeight: Style.spacing.hairline
    color: Style.normalBorderColor
  }
}
