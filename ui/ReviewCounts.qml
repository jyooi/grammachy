import QtQuick
import qs.Commons

Row {
  id: root

  property int acceptedCount: 0
  property int skippedCount: 0
  property int openCount: 0

  spacing: Style.spacing.xxl

  Text {
    textFormat: Text.PlainText
    text: root.acceptedCount + " accepted"
    color: Color.muted
    font.family: Style.font.family
    font.pixelSize: Style.font.bodySmall
  }

  Text {
    textFormat: Text.PlainText
    text: root.skippedCount + " skipped"
    color: Color.muted
    font.family: Style.font.family
    font.pixelSize: Style.font.bodySmall
  }

  Text {
    textFormat: Text.PlainText
    text: root.openCount + " open"
    color: Color.muted
    font.family: Style.font.family
    font.pixelSize: Style.font.bodySmall
  }
}
