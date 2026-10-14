import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui

ColumnLayout {
  id: root

  property string metaText: ""
  property string noteText: ""

  property bool showsAutoReplace: false
  property bool autoReplace: false
  property bool settingsOpen: false

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
        textFormat: Text.PlainText
        text: root.metaText
        color: Color.muted
        elide: Text.ElideRight
        font.family: Style.font.family
        font.pixelSize: Style.font.bodySmall
      }

      Text {
        Layout.fillWidth: true
        visible: root.noteText.length > 0
        textFormat: Text.PlainText
        text: root.noteText
        color: Color.accent
        elide: Text.ElideRight
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
    }

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
