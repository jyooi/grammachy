import QtQuick
import qs.Commons
import "splice.js" as Splice
import "tokens.js" as Tokens

// The whole Selection with every Issue drawn as a mark, spec section 6.
//
// The text is laid out as one Flow of word tokens per line rather than as one
// Text item, because Qt rich text can underline but cannot draw a dotted
// underline, and spelling has to read differently from grammar. A token owns
// its own colour, its own underline, and its own click target.
Flickable {
  id: root

  // The exact Selection the Check ran on. Every span indexes into it.
  property string sourceText: ""
  property var issues: []
  // One entry per Issue: true accepted, false skipped, null still open.
  property var decisions: []
  property int focusIndex: -1

  property color textColor: Color.popups.text
  // Themes carry no green role, so the accepted colour is the one constant in
  // the card. Grammachy states the Fix is applied; borrowing the accent would
  // read as "focused" instead.
  property color acceptedColor: "#9ece6a"
  property color openColor: Color.urgent
  property color skippedColor: Color.muted
  property color focusFill: Style.selectionFill
  property string fontFamily: Style.font.family
  property int fontSize: Style.font.subtitle

  signal markActivated(int index)

  // The drawn text is the Corrected text: an accepted mark shows its Fix.
  readonly property string displayText: Splice.correctedText(sourceText, issues, decisions)
  readonly property var lines: Tokens.buildLines(displayText, Splice.displaySpans(issues, decisions))
  readonly property int lineHeight: Math.round(fontSize * 1.8)
  readonly property int markThickness: Math.max(1, Style.space(2))

  function decisionAt(index) {
    var value = decisions ? decisions[index] : undefined
    return value === undefined ? null : value
  }

  function categoryAt(index) {
    var issue = issues ? issues[index] : null
    return issue ? String(issue.category) : "grammar"
  }

  // Scroll the focused mark into view. A paragraph wraps into many rows inside
  // one Flow, so the row the mark sits on is what has to be found, not the
  // paragraph: on a first-N Check of 5,000 units a whole paragraph is taller
  // than the card, and revealing all of it lands at the wrong end of the text.
  function showFocus() {
    if (root.focusIndex < 0) return
    for (var i = 0; i < lineRepeater.count; i++) {
      var item = lineRepeater.itemAt(i)
      if (!item || typeof item.focusOffset !== "function") continue
      var offset = item.focusOffset()
      if (offset < 0) continue
      root.reveal(item.y + offset, root.lineHeight)
      return
    }
  }

  function reveal(top, height) {
    var bottom = top + height
    if (top < root.contentY) root.contentY = Math.max(0, top)
    else if (bottom > root.contentY + root.height)
      root.contentY = Math.max(0, Math.min(bottom - root.height, root.contentHeight - root.height))
  }

  onFocusIndexChanged: Qt.callLater(root.showFocus)

  clip: true
  contentWidth: width
  contentHeight: column.implicitHeight
  boundsBehavior: Flickable.StopAtBounds
  flickableDirection: Flickable.VerticalFlick

  Column {
    id: column
    width: root.width

    Repeater {
      id: lineRepeater
      model: root.lines

      delegate: Flow {
        id: line
        required property var modelData

        // The y of the first token of the focused Issue on this line, or -1.
        // The Flow lays its children out in token order, so the first hit is
        // the topmost one.
        function focusOffset() {
          for (var i = 0; i < line.children.length; i++) {
            if (line.children[i].focused === true) return line.children[i].y
          }
          return -1
        }

        width: root.width
        spacing: 0
        // A blank line is half a line tall: enough to read as a paragraph
        // break without wasting the card's height.
        height: line.modelData.blank ? Math.round(root.lineHeight / 2) : implicitHeight

        Repeater {
          model: line.modelData.tokens

          delegate: Item {
            id: token
            required property var modelData

            readonly property int issueIndex: token.modelData.issue
            readonly property bool marked: token.issueIndex >= 0
            readonly property var decision: token.marked ? root.decisionAt(token.issueIndex) : null
            readonly property bool focused: token.marked && token.issueIndex === root.focusIndex
            readonly property bool dotted: token.marked && root.categoryAt(token.issueIndex) === "spelling"
            readonly property color wordColor: !token.marked ? root.textColor
              : token.decision === true ? root.acceptedColor
              : token.decision === false ? root.skippedColor
              : root.textColor
            readonly property color markColor: token.decision === true ? root.acceptedColor : root.openColor
            // A skipped mark keeps its dim text and loses its underline, so
            // the eye passes over it.
            readonly property bool underlined: token.marked && token.decision !== false && word.implicitWidth > 0

            implicitWidth: word.implicitWidth + blanks.implicitWidth
            implicitHeight: root.lineHeight

            Rectangle {
              width: word.implicitWidth
              height: parent.height
              color: token.focused ? root.focusFill : "transparent"
            }

            Text {
              id: word
              text: token.modelData.word
              color: token.wordColor
              font.family: root.fontFamily
              font.pixelSize: root.fontSize
              anchors.verticalCenter: parent.verticalCenter
            }

            Text {
              id: blanks
              x: word.implicitWidth
              text: token.modelData.blanks
              color: root.textColor
              font.family: root.fontFamily
              font.pixelSize: root.fontSize
              anchors.verticalCenter: parent.verticalCenter
            }

            Rectangle {
              visible: token.underlined && !token.dotted
              y: word.y + word.implicitHeight
              width: word.implicitWidth
              height: root.markThickness
              color: token.markColor
            }

            Row {
              visible: token.underlined && token.dotted
              y: word.y + word.implicitHeight
              spacing: root.markThickness

              Repeater {
                model: Math.floor(word.implicitWidth / (root.markThickness * 2))

                delegate: Rectangle {
                  width: root.markThickness
                  height: root.markThickness
                  color: token.markColor
                }
              }
            }

            MouseArea {
              enabled: token.marked
              width: word.implicitWidth
              height: parent.height
              cursorShape: Qt.PointingHandCursor
              onClicked: root.markActivated(token.issueIndex)
            }
          }
        }
      }
    }
  }
}
