// Corrected text and the span math around it. Spec sections 5.1 and 5.3.
//
// Loaded twice: by the QML overlay through `import "splice.js" as Splice`,
// and by `splice.test.js` under node. Nothing here may touch a QML or a node
// API, because each side only has one of them.
//
// Every index is a UTF-16 code unit, which is what the CLI emits and what
// JavaScript string indices already are.

function isAccepted(accepted, index) {
  return Boolean(accepted) && accepted[index] === true
}

// The Selection with every accepted Fix applied. Last to first, so an earlier
// splice never moves a later Issue's span.
function correctedText(text, issues, accepted) {
  var out = text === undefined || text === null ? "" : String(text)
  var list = issues || []
  for (var i = list.length - 1; i >= 0; i--) {
    if (!isAccepted(accepted, i)) continue
    out = out.slice(0, list[i].start) + list[i].fix + out.slice(list[i].end)
  }
  return out
}

// Where each Issue sits in the text the popup draws, which is the Corrected
// text: an accepted mark shows the Fix, every other mark shows the original.
// Issues arrive sorted and never overlap (spec 5.1), so one left to right walk
// carries the running shift.
function displaySpans(issues, accepted) {
  var spans = []
  var shift = 0
  var list = issues || []
  for (var i = 0; i < list.length; i++) {
    var issue = list[i]
    var shown = isAccepted(accepted, i) ? String(issue.fix) : String(issue.original)
    var start = issue.start + shift
    spans.push({ start: start, end: start + shown.length })
    shift += shown.length - (issue.end - issue.start)
  }
  return spans
}

// An Issue whose slice does not match its original would splice the wrong
// characters, so spec 5.1 drops it. The caller warns about what came back in
// `dropped`.
function verifiedIssues(text, issues) {
  var source = text === undefined || text === null ? "" : String(text)
  var kept = []
  var dropped = []
  var list = issues || []
  for (var i = 0; i < list.length; i++) {
    var issue = list[i]
    if (source.slice(issue.start, issue.end) === issue.original) kept.push(issue)
    else dropped.push(issue)
  }
  return { issues: kept, dropped: dropped }
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = { correctedText: correctedText, displaySpans: displaySpans, verifiedIssues: verifiedIssues }
}
