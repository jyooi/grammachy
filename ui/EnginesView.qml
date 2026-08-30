import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "engines.js" as Engines
import "deps.js" as Deps

// The Engines list of the Settings view, spec sections 5.4 and 7.
//
// It sits under the Engine dropdown and is drawn whatever engine is selected,
// because the whole point is to add an engine the dropdown cannot offer yet
// (HUF-237). One row per optional component: the name, one hint line, a bar
// while a download runs, and the icon button that row offers.
//
// The row button carries an icon and no text label: the verb is the tooltip
// and the cost is in the hint line above.
//
// What a row offers and what its hint says both come from `ui/engines.js`,
// which a node test can run and this file cannot. This view draws that answer
// and reports which button was pressed; the processes live in Overlay.qml.
ColumnLayout {
  id: root

  // The rows from `Engines.read`, already merged into the list on screen.
  property var engines: []
  // The slug an install is running on, or "" when none is.
  property string busy: ""
  // The `.part` length of that row, moved by every poll answer.
  property double busyBytes: 0

  // Whether any verb is in flight.
  property bool working: false
  // The slug awaiting a Remove confirm, spec section 7, or "" when none is.
  property string confirmSlug: ""
  // The engine a Check would run on, so removing it can say what happens next.
  property string selected: ""

  // The engines directory and free disk space, from the same envelope.
  property string directory: ""
  property double freeBytes: 0
  // One failure, from `Engines.note`, or null when the last verb was fine.
  property var note: null
  // The dependency table of spec section 10, from `Deps.fromDoctor`.
  property var dependencies: []


  signal install(string slug)
  signal cancel()
  signal remove(string slug)
  signal confirmRemove(string slug)
  signal keepEngine()

  spacing: Style.spacing.labelGap

  Text {
    text: "Engines"
    color: Qt.darker(Color.popups.text, 1.4)
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
    font.bold: true
  }

  Text {
    Layout.fillWidth: true
    text: "Harper is built in. Add another engine here and it appears in the dropdown above."
    color: Color.muted
    wrapMode: Text.Wrap
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }

  Repeater {
    model: root.engines

    ColumnLayout {
      id: row

      required property var modelData

      readonly property string slug: String(row.modelData.slug)
      readonly property string name: String(row.modelData.name)
      readonly property bool running: root.busy === row.slug
      // Below zero means the list is the only answer there is.
      readonly property double live: row.running ? root.busyBytes : -1
      readonly property bool blocked: Engines.isBlocked(row.modelData, {
        busy: root.busy,
        working: root.working
      })
      readonly property bool asking: root.confirmSlug === row.slug
      // Spec section 7: a row that needs a system package this machine lacks
      // says so and names the package. Its own Install waits until they are
      // there.
      readonly property var missingPackages: Deps.absentFor(root.dependencies, row.slug)
      readonly property bool runtimeMissing: Engines.runtimeMissing(row.modelData, row.missingPackages)

      Layout.fillWidth: true
      Layout.topMargin: Style.spacing.sm
      // A row that draws a bar or a confirm under it must not run into what
      // follows, so the gap belongs to the row rather than to the list.
      Layout.bottomMargin: row.running || row.asking
        || Engines.stateOf(row.modelData) === Engines.PARTIAL ? Style.spacing.sm : 0
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
              text: row.name + " " + String(row.modelData.version)
              color: Color.popups.text
              elide: Text.ElideRight
              font.family: Style.font.family
              font.pixelSize: Style.font.bodySmall
              font.bold: Engines.stateOf(row.modelData) === Engines.READY
            }

            Text {
              visible: root.selected === row.slug
              text: "in use"
              color: Color.accent
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }

            Text {
              visible: row.runtimeMissing
              text: Deps.needsHint(row.missingPackages)
              color: Color.urgent
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }

            Item { Layout.fillWidth: true }
          }

          Text {
            visible: row.runtimeMissing
            Layout.fillWidth: true
            text: Deps.installHint(Deps.packagesOf(row.missingPackages))
            color: Color.muted
            wrapMode: Text.Wrap
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }

          Text {
            Layout.fillWidth: true
            text: Engines.hint(row.modelData, row.running, row.live)
            color: Color.muted
            elide: Text.ElideRight
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }

        // The buttons this row offers, spec section 7. A component the pacman
        // package supplies offers none, because Remove here would delete a
        // directory this plugin never wrote and leave the package in place.
        Repeater {
          model: Engines.actions(row.modelData, { busy: root.busy })

          Button {
            required property string modelData

            readonly property bool blocked: Engines.actionBlocked(modelData, row.modelData, {
              busy: root.busy,
              working: root.working,
              missing: row.missingPackages
            })

            Layout.alignment: Qt.AlignVCenter
            enabled: !blocked
            opacity: blocked ? 0.4 : 1
            iconText: Engines.actionIcon(modelData)
            tooltipText: Engines.actionTooltip(modelData, row.name)
            bordered: true
            foreground: modelData === Engines.REMOVE ? Color.urgent : Color.popups.text
            fontFamily: Style.font.family
            onClicked: {
              if (modelData === Engines.INSTALL) root.install(row.slug)
              else if (modelData === Engines.CANCEL) root.cancel()
              else if (modelData === Engines.REMOVE) root.remove(row.slug)
            }
          }
        }
      }

      // The progress bar of spec section 5.4. The whole track is the pinned
      // archive size and the filled part is what the `.part` file already
      // holds, which is what the one-second poll of `engine list` moves.
      Item {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.xxs
        visible: row.running || Engines.stateOf(row.modelData) === Engines.PARTIAL
        implicitHeight: Style.space(6)

        Rectangle {
          anchors.fill: parent
          radius: Style.cornerRadius > 0 ? height / 2 : 0
          color: Style.normalFill
          border.width: Math.max(1, Style.normalBorderWidth)
          border.color: Style.normalBorderColor
        }

        Rectangle {
          width: Math.round(parent.width * Engines.share(row.modelData, row.live))
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

      // Spec section 7: removing the engine a Check would run on asks once,
      // because the next Check would have no engine to reach.
      ColumnLayout {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.xs
        visible: row.asking
        spacing: Style.spacing.xs

        Text {
          Layout.fillWidth: true
          text: "Remove the engine this check uses? Checks go back to Harper, which is built in."
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
            onClicked: root.keepEngine()
          }

          Button {
            text: "Remove"
            bordered: true
            foreground: Color.urgent
            fontFamily: Style.font.family
            tooltipText: "Enter"
            onClicked: root.confirmRemove(row.slug)
          }
        }
      }
    }
  }

  // What the last verb said when it did not work, spec section 5.4. A cancel
  // lands here too, because "what arrived is kept" is what the reader wants to
  // know before they press Install again.
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
      color: root.note && root.note.kind === Engines.NOTICE ? Color.popups.text : Color.urgent
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

  // The disk an install would land on, so the cost is on screen before the
  // button is pressed rather than in the refusal after it.
  Text {
    Layout.fillWidth: true
    Layout.topMargin: Style.spacing.xs
    visible: root.directory.length > 0
    text: Engines.bytes(root.freeBytes) + " free in " + root.directory
    color: Color.muted
    elide: Text.ElideMiddle
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }
}
