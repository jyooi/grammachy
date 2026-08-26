import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "models.js" as Models

// The Models list of the Settings view, spec sections 5.3 and 7.
//
// It sits under the Local LLM fields and is drawn only when the engine is
// `openai`, because no other engine has weights. One row per catalogue model:
// the name, one hint line, a bar while a download runs, and the icon buttons
// that row offers.
//
// The row buttons carry an icon and no text label: the verb is the tooltip and
// the name is in the hint line above, so a row of four models stays one column
// of names rather than a wall of words.
//
// What a row offers and what its hint says both come from `ui/models.js`, which
// a node test can run and this file cannot. This view draws that answer and
// reports which button was pressed; the processes live in Overlay.qml.
ColumnLayout {
  id: root

  // The rows from `Models.read`, already merged into the list on screen.
  property var models: []
  // The catalogue name a download is running on, or "" when none is.
  property string busy: ""
  // The stored `openaiModel`, so the row it names is marked and offers no Use.
  property string setting: ""
  // The name awaiting a Remove confirm, spec section 7, or "" when none is.
  property string confirmName: ""
  // The models directory and what the disk has left, from the same envelope.
  property string directory: ""
  property double freeBytes: 0
  // One failure, from `Models.note`, or null when the last verb was fine.
  property var note: null

  signal download(string name)
  signal cancel()
  signal use(string name)
  signal remove(string name)
  signal confirmRemove(string name)
  signal keepModel()

  spacing: Style.spacing.labelGap

  Text {
    text: "Models"
    color: Qt.darker(Color.popups.text, 1.4)
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
    font.bold: true
  }

  Repeater {
    model: root.models

    ColumnLayout {
      id: row

      required property var modelData

      readonly property string name: String(row.modelData.name)
      readonly property bool running: root.busy === row.name
      readonly property bool blocked: Models.isBlocked(row.modelData, {
        busy: root.busy,
        confirm: root.confirmName
      })
      // The model a Check would load, resolved the way the CLI resolves it, so
      // a setting that names the file by a prefix still marks the right row.
      readonly property bool chosen: Models.resolves(row.modelData, root.setting, root.models)
      readonly property bool asking: root.confirmName === row.name

      Layout.fillWidth: true
      Layout.topMargin: Style.spacing.sm
      // A row that draws a bar or a confirm under it must not run into the
      // name of the next one, so the gap belongs to the row rather than to the
      // list.
      Layout.bottomMargin: row.running || row.asking
        || Models.stateOf(row.modelData) === Models.PARTIAL ? Style.spacing.sm : 0
      spacing: Style.spacing.xxs

      RowLayout {
        Layout.fillWidth: true
        spacing: Style.spacing.lg

        ColumnLayout {
          Layout.fillWidth: true
          spacing: Style.spacing.xxs

          RowLayout {
            Layout.fillWidth: true
            spacing: Style.spacing.sm

            Text {
              text: row.name
              color: Color.popups.text
              elide: Text.ElideRight
              font.family: Style.font.family
              font.pixelSize: Style.font.bodySmall
              font.bold: row.chosen
            }

            // The model a Check would run on, so the reader can tell at a
            // glance which of several Ready rows the setting names.
            Text {
              visible: row.chosen
              text: "in use"
              color: Color.accent
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }

            Item { Layout.fillWidth: true }
          }

          Text {
            Layout.fillWidth: true
            text: Models.hint(row.modelData, row.running)
            color: Color.muted
            elide: Text.ElideRight
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }

        // The buttons this row offers, spec section 7. A row waiting on another
        // row's download keeps every button in place and disabled, so the list
        // never shifts under a click and no press is silently dropped.
        Repeater {
          model: Models.actions(row.modelData, {
            busy: root.busy,
            setting: root.setting,
            models: root.models
          })

          Button {
            required property string modelData

            Layout.alignment: Qt.AlignVCenter
            enabled: !row.blocked
            opacity: row.blocked ? 0.4 : 1
            iconText: Models.actionIcon(modelData)
            tooltipText: Models.actionTooltip(modelData, row.name)
            bordered: true
            foreground: modelData === Models.REMOVE ? Color.urgent : Color.popups.text
            fontFamily: Style.font.family
            onClicked: {
              if (modelData === Models.DOWNLOAD) root.download(row.name)
              else if (modelData === Models.CANCEL) root.cancel()
              else if (modelData === Models.USE) root.use(row.name)
              else if (modelData === Models.REMOVE) root.remove(row.name)
            }
          }
        }
      }

      // The progress bar of spec section 5.3. The whole track is the pinned
      // size and the filled part is what the `.part` file already holds, which
      // is what the one-second poll of `model list` moves.
      Item {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.xxs
        visible: row.running || Models.stateOf(row.modelData) === Models.PARTIAL
        implicitHeight: Style.space(6)

        Rectangle {
          anchors.fill: parent
          radius: Style.cornerRadius > 0 ? height / 2 : 0
          color: Style.normalFill
          border.width: Math.max(1, Style.normalBorderWidth)
          border.color: Style.normalBorderColor
        }

        Rectangle {
          width: Math.round(parent.width * Models.share(row.modelData))
          height: parent.height
          radius: Style.cornerRadius > 0 ? height / 2 : 0
          color: Color.accent

          // The bar steps once a second with the poll, so it moves rather
          // than jumps.
          Behavior on width {
            NumberAnimation { duration: 900; easing.type: Easing.Linear }
          }
        }
      }

      // Spec section 7: removing the model a Check would run on asks once,
      // because the next Check would have nothing to load.
      ColumnLayout {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.xs
        visible: row.asking
        spacing: Style.spacing.xs

        Text {
          Layout.fillWidth: true
          text: "Remove the model this engine uses? The next check has nothing to load until another one is here."
          color: Color.urgent
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.caption
        }

        RowLayout {
          Layout.fillWidth: true
          spacing: Style.spacing.lg

          Item { Layout.fillWidth: true }

          Button {
            text: "Keep"
            bordered: true
            foreground: Color.popups.text
            fontFamily: Style.font.family
            tooltipText: "Esc"
            onClicked: root.keepModel()
          }

          Button {
            text: "Remove"
            bordered: true
            foreground: Color.urgent
            fontFamily: Style.font.family
            tooltipText: "Enter"
            onClicked: root.confirmRemove(row.name)
          }
        }
      }
    }
  }

  // What the last verb said when it did not work, spec section 5.3. A cancel
  // lands here too, because "the part file is kept" is what the reader wants to
  // know before they press Download again.
  ColumnLayout {
    Layout.fillWidth: true
    Layout.topMargin: Style.spacing.sm
    visible: Boolean(root.note)
    spacing: Style.spacing.xxs

    Text {
      Layout.fillWidth: true
      text: root.note ? String(root.note.title) : ""
      // A cancel is what the reader asked for, so it never wears the colour
      // of something that went wrong.
      color: root.note && root.note.kind === Models.NOTICE ? Color.popups.text : Color.urgent
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
      font.bold: true
    }

    Text {
      Layout.fillWidth: true
      text: root.note ? String(root.note.body) : ""
      color: Color.muted
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }

    Text {
      Layout.fillWidth: true
      visible: Boolean(root.note) && String(root.note.message).length > 0
      text: root.note ? String(root.note.message) : ""
      color: Color.muted
      wrapMode: Text.Wrap
      font.family: "monospace"
      font.pixelSize: Style.font.caption
    }
  }

  Rectangle {
    Layout.fillWidth: true
    Layout.topMargin: Style.spacing.md
    visible: root.directory.length > 0
    implicitHeight: Style.spacing.hairline
    color: Style.normalBorderColor
  }

  // The disk a download would land on, so the cost is on screen before the
  // button is pressed rather than in the refusal after it.
  Text {
    Layout.fillWidth: true
    Layout.topMargin: Style.spacing.xs
    visible: root.directory.length > 0
    text: Models.bytes(root.freeBytes) + " free in " + root.directory
    color: Color.muted
    elide: Text.ElideMiddle
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }
}
