import QtQuick
import qs.Commons
import "splice.js" as Splice

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
  readonly property var spans: Splice.displaySpans(issues, decisions)
  readonly property var lines: root.buildLines(displayText, spans)
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

  // One entry per line of the drawn text. A blank line is the paragraph break
  // the Selection already carries, so nothing normalises the text.
  function buildLines(text, spans) {
    var pieces = String(text).split("\n")
    var out = []
    var offset = 0
    for (var i = 0; i < pieces.length; i++) {
      out.push(root.buildLine(pieces[i], offset, spans))
      offset += pieces[i].length + 1
    }
    return out
  }

  function buildLine(piece, offset, spans) {
    var end = offset + piece.length
    var runs = []
    var cursor = offset
    for (var i = 0; i < spans.length; i++) {
      var span = spans[i]
      if (span.end <= cursor || span.start >= end) continue
      var from = Math.max(span.start, cursor)
      var to = Math.min(span.end, end)
      if (from > cursor) runs.push({ text: piece.slice(cursor - offset, from - offset), issue: -1 })
      runs.push({ text: piece.slice(from - offset, to - offset), issue: i })
      cursor = to
    }
    if (cursor < end) runs.push({ text: piece.slice(cursor - offset, end - offset), issue: -1 })
    return { blank: piece.length === 0, tokens: root.tokenize(runs) }
  }

  // A word plus the blanks that follow it is one Flow cell, so the Flow wraps
  // where a reader expects and the underline stops at the word.
  function tokenize(runs) {
    var tokens = []
    for (var r = 0; r < runs.length; r++) {
      var chunks = runs[r].text.match(/\S+[ \t]*|[ \t]+/g)
      if (!chunks) continue
      for (var c = 0; c < chunks.length; c++) {
        var word = chunks[c].replace(/[ \t]+$/, "")
        tokens.push({ word: word, blanks: chunks[c].slice(word.length), issue: runs[r].issue })
      }
    }
    return tokens
  }

  function lineOfFocus() {
    if (focusIndex < 0 || focusIndex >= spans.length) return -1
    var start = spans[focusIndex].start
    var offset = 0
    var pieces = String(displayText).split("\n")
    for (var i = 0; i < pieces.length; i++) {
      var end = offset + pieces[i].length
      if (start <= end) return i
      offset = end + 1
    }
    return pieces.length - 1
  }

  function showFocus() {
    var index = root.lineOfFocus()
    if (index < 0) return
    var item = lineRepeater.itemAt(index)
    if (!item) return
    var top = item.y
    var bottom = top + item.height
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
