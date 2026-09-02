import QtQuick
import qs.Commons

// The three counts on the leading edge of the footer, spec section 6. Compose
// keeps the same footer, spec section 9, so the counts live here rather than
// in either card.
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
