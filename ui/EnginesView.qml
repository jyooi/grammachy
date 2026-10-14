import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "engines.js" as Engines
import "deps.js" as Deps

ColumnLayout {
  id: root

  property var engines: []
  property string busy: ""
  property double busyBytes: 0

  property bool working: false
  property string confirmSlug: ""
  property string selected: ""

  property string directory: ""
  property double freeBytes: 0
  property var note: null
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
      readonly property double live: row.running ? root.busyBytes : -1
      readonly property bool blocked: Engines.isBlocked(row.modelData, {
        busy: root.busy,
        working: root.working
      })
      readonly property bool asking: root.confirmSlug === row.slug
      readonly property var missingPackages: Deps.absentFor(root.dependencies, row.slug)
      readonly property bool runtimeMissing: Engines.runtimeMissing(row.modelData, row.missingPackages)

      Layout.fillWidth: true
      Layout.topMargin: Style.spacing.sm
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
              textFormat: Text.PlainText
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
              textFormat: Text.PlainText
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
            textFormat: Text.PlainText
            text: Deps.installHint(Deps.packagesOf(row.missingPackages))
            color: Color.muted
            wrapMode: Text.Wrap
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }

          Text {
            Layout.fillWidth: true
            textFormat: Text.PlainText
            text: Engines.hint(row.modelData, row.running, row.live)
            color: Color.muted
            elide: Text.ElideRight
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }

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

          Behavior on width {
            NumberAnimation { duration: 900; easing.type: Easing.Linear }
          }
        }
      }

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

  ColumnLayout {
    Layout.fillWidth: true
    Layout.topMargin: Style.spacing.sm
    visible: Boolean(root.note)
    spacing: Style.spacing.xxs

    Text {
      Layout.fillWidth: true
      textFormat: Text.PlainText
      text: root.note ? String(root.note.title) : ""
      color: root.note && root.note.kind === Engines.NOTICE ? Color.popups.text : Color.urgent
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
      font.bold: true
    }

    Text {
      Layout.fillWidth: true
      textFormat: Text.PlainText
      text: root.note ? String(root.note.body) : ""
      color: Color.muted
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }

    Text {
      Layout.fillWidth: true
      visible: Boolean(root.note) && String(root.note.message).length > 0
      textFormat: Text.PlainText
      text: root.note ? String(root.note.message) : ""
      color: Color.muted
      wrapMode: Text.Wrap
      font.family: "monospace"
      font.pixelSize: Style.font.caption
    }
  }

  Text {
    Layout.fillWidth: true
    Layout.topMargin: Style.spacing.xs
    visible: root.directory.length > 0
    textFormat: Text.PlainText
    text: Engines.bytes(root.freeBytes) + " free in " + root.directory
    color: Color.muted
    elide: Text.ElideMiddle
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
  }
}
