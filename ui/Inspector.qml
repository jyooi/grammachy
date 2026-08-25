import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui

// The inspector strip of spec section 6, which spec section 9 gives to Compose
// unchanged: the focused Issue in full, with Accept and Skip.
//
// The strip draws one Issue and reports intent. Which Issue is focused, and
// what a decision does to the Corrected text, belong to Overlay.qml.
BorderSurface {
  id: root

  // The focused Issue of spec section 5.1, or null.
  property var issue: null
  property int focusIndex: 0
  property int issueCount: 0
  // MarkedText owns the accepted green, so the fix reads the same in both.
  property color acceptedColor: "#9ece6a"

  signal accepted(int index)
  signal skipped(int index)

  color: "transparent"
  radius: Style.cornerRadius
  padding: Style.spacing.lg
  borderSpec: Border.controlSpec("normal", Color.popups.text, Color.accent)
  implicitHeight: strip.implicitHeight + topPadding + bottomPadding + borderTop + borderBottom

  RowLayout {
    id: strip

    anchors.fill: parent
    anchors.margins: Style.spacing.lg
    spacing: Style.spacing.xxl

    ColumnLayout {
      Layout.fillWidth: true
      spacing: Style.spacing.sm

      RowLayout {
        Layout.fillWidth: true
        spacing: Style.spacing.lg

        Text {
          text: root.issue ? root.issue.original : ""
          color: Color.urgent
          font.family: Style.font.family
          font.pixelSize: Style.font.subtitle
          font.strikeout: true
        }

        Text {
          text: "→"
          color: Color.muted
          font.family: Style.font.family
          font.pixelSize: Style.font.subtitle
        }

        Text {
          text: root.issue ? root.issue.fix : ""
          color: root.acceptedColor
          font.family: Style.font.family
          font.pixelSize: Style.font.subtitle
          font.bold: true
        }

        BorderSurface {
          Layout.alignment: Qt.AlignVCenter
          color: "transparent"
          radius: Style.cornerRadius
          padding: Style.spacing.xs
          borderSpec: Border.controlSpec("normal", Color.popups.text, Color.accent)
          implicitWidth: category.implicitWidth + padding * 2 + borderLeft + borderRight
          implicitHeight: category.implicitHeight + padding * 2 + borderTop + borderBottom

          Text {
            id: category
            anchors.centerIn: parent
            text: root.issue ? root.issue.category : ""
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }

        Item { Layout.fillWidth: true }
      }

      Text {
        Layout.fillWidth: true
        text: root.issue ? root.issue.reason : ""
        color: Color.muted
        wrapMode: Text.Wrap
        font.family: Style.font.family
        font.pixelSize: Style.font.bodySmall
      }

      Text {
        Layout.fillWidth: true
        text: "Issue " + (root.focusIndex + 1) + " of " + root.issueCount
          + ". Enter accepts, Space skips, Up and Down move, A accepts all."
        color: Color.muted
        wrapMode: Text.Wrap
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }
    }

    RowLayout {
      Layout.alignment: Qt.AlignTop
      spacing: Style.spacing.lg

      Button {
        text: "Accept"
        tooltipText: "Enter"
        bordered: true
        foreground: Color.popups.text
        fontFamily: Style.font.family
        onClicked: root.accepted(root.focusIndex)
      }

      Button {
        text: "Skip"
        tooltipText: "Space"
        bordered: true
        foreground: Color.popups.text
        fontFamily: Style.font.family
        onClicked: root.skipped(root.focusIndex)
      }
    }
  }
}
