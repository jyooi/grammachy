// Word tokens for the marked text. Spec section 6.
//
// Loaded twice: by MarkedText.qml through `import "tokens.js" as Tokens`,
// and by `tokens.test.js` under node. Nothing here may touch a QML or a node
// API, because each side only has one of them.
//
// Every index is a UTF-16 code unit.
// A word plus the blanks that follow it is one Flow cell.
// The Flow wraps where a reader expects. The underline stops at the word.

function isBlank(ch) {
  return ch !== "\n" && ch !== "\r" && /\s/.test(ch)
}

function tokenize(runs) {
  var tokens = []
  var list = runs || []
  for (var r = 0; r < list.length; r++) {
    var text = list[r].text === undefined || list[r].text === null ? "" : String(list[r].text)
    var issue = list[r].issue
    var i = 0
    while (i < text.length) {
      if (isBlank(text.charAt(i))) {
        var blankEnd = i + 1
        while (blankEnd < text.length && isBlank(text.charAt(blankEnd))) blankEnd += 1
        tokens.push({ word: "", blanks: text.slice(i, blankEnd), issue: issue })
        i = blankEnd
        continue
      }
      var wordEnd = i + 1
      while (wordEnd < text.length && !isBlank(text.charAt(wordEnd))) wordEnd += 1
      var spaceEnd = wordEnd
      while (spaceEnd < text.length && isBlank(text.charAt(spaceEnd))) spaceEnd += 1
      tokens.push({
        word: text.slice(i, wordEnd),
        blanks: text.slice(wordEnd, spaceEnd),
        issue: issue
      })
      i = spaceEnd
    }
  }
  return tokens
}

function buildLine(piece, offset, spans) {
  var end = offset + piece.length
  var runs = []
  var cursor = offset
  var list = spans || []
  for (var i = 0; i < list.length; i++) {
    var span = list[i]
    if (span.end <= cursor || span.start >= end) continue
    var from = Math.max(span.start, cursor)
    var to = Math.min(span.end, end)
    if (from > cursor) runs.push({ text: piece.slice(cursor - offset, from - offset), issue: -1 })
    runs.push({ text: piece.slice(from - offset, to - offset), issue: i })
    cursor = to
  }
  if (cursor < end) runs.push({ text: piece.slice(cursor - offset, end - offset), issue: -1 })
  return { blank: piece.length === 0, tokens: tokenize(runs) }
}

function buildLines(text, spans) {
  var source = text === undefined || text === null ? "" : String(text)
  var pieces = source.split("\n")
  var out = []
  var offset = 0
  for (var i = 0; i < pieces.length; i++) {
    out.push(buildLine(pieces[i], offset, spans))
    offset += pieces[i].length + 1
  }
  return out
}

function joinedTokens(tokens) {
  var out = ""
  var list = tokens || []
  for (var i = 0; i < list.length; i++) out += list[i].word + list[i].blanks
  return out
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    tokenize: tokenize,
    buildLine: buildLine,
    buildLines: buildLines,
    joinedTokens: joinedTokens
  }
}
