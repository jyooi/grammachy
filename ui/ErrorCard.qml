import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "errors.js" as Errors

// The error cards of spec section 8, drawn from one card model.
//
// The model comes from `ui/errors.js`, which owns the title, the body, and the
// buttons of every code. This file draws that model and reports which button
// was pressed; the routing itself lives in Overlay.qml.
ColumnLayout {
  id: root

  // A card model from `Errors.card(code, options)`, or null while there is
  // no error to show.
  property var card: null
  // The one-line `grammachy doctor` answer, spec section 8. It shows on the
  // `engine_unavailable` card only, and only once doctor has answered.
  property string diagnosis: ""

  signal actionRequested(string action)

  readonly property string title: root.card ? String(root.card.title) : ""
  readonly property string body: root.card ? String(root.card.body) : ""
  readonly property string message: root.card ? String(root.card.message) : ""
  readonly property var buttons: root.card ? root.card.buttons : []
  readonly property string primary: root.card ? String(root.card.primary) : ""
  readonly property bool showsDiagnosis: Boolean(root.card) && root.card.needsDiagnosis === true
    && root.diagnosis.length > 0

  spacing: Style.spacing.md

  Text {
    Layout.fillWidth: true
    textFormat: Text.PlainText
    text: root.title
    color: Color.urgent
    wrapMode: Text.Wrap
    font.family: Style.font.family
    font.pixelSize: Style.font.title
    font.bold: true
  }

  Text {
    Layout.fillWidth: true
    textFormat: Text.PlainText
    text: root.body
    color: Color.popups.text
    wrapMode: Text.Wrap
    font.family: Style.font.family
    font.pixelSize: Style.font.body
  }

  // The doctor line names the missing piece and the exact command that
  // installs it, so it reads as the next step rather than as more prose.
  Text {
    Layout.fillWidth: true
    visible: root.showsDiagnosis
    textFormat: Text.PlainText
    text: root.diagnosis
    color: Color.accent
    wrapMode: Text.Wrap
    font.family: Style.font.family
    font.pixelSize: Style.font.bodySmall
  }

  // Spec section 8: what the CLI said shows under the body, in monospace.
  Text {
    Layout.fillWidth: true
    visible: root.message.length > 0
    textFormat: Text.PlainText
    text: root.message
    color: Color.muted
    wrapMode: Text.Wrap
    font.family: "monospace"
    font.pixelSize: Style.font.caption
  }

  RowLayout {
    Layout.fillWidth: true
    Layout.topMargin: Style.spacing.sm
    spacing: Style.spacing.lg

    Item { Layout.fillWidth: true }

    Repeater {
      model: root.buttons

      Button {
        required property string modelData

        text: Errors.buttonLabel(modelData)
        bordered: true
        foreground: modelData === root.primary ? Color.accent : Color.popups.text
        fontFamily: Style.font.family
        tooltipText: modelData === Errors.CLOSE ? "Esc" : ""
        onClicked: root.actionRequested(modelData)
      }
    }
  }
}
