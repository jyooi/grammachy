// Corrected text and the span math around it. Spec sections 5.1 and 5.4.
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

// The head of a text that one Check can take, for the `Check the first N only`
// button of the too-long card (spec sections 6 and 8).
//
// A plain slice at the limit can cut a surrogate pair in half, and a lone
// surrogate is not UTF-8, so the CLI would never see the character the user
// typed. Backing off one unit keeps the pair whole and costs one character.
function firstUnits(text, limit) {
  var source = text === undefined || text === null ? "" : String(text)
  var end = Math.max(0, Math.floor(limit))
  if (source.length <= end) return source
  var last = source.charCodeAt(end - 1)
  if (last >= 0xD800 && last <= 0xDBFF) end -= 1
  return source.slice(0, end)
}

// The text of one Chunk, spec section 5.2. The CLI cuts a Chunk on a character
// boundary, so a plain slice never halves a surrogate pair the way `firstUnits`
// above has to guard against.
function chunkText(text, chunk) {
  var source = text === undefined || text === null ? "" : String(text)
  if (!chunk) return source
  return source.slice(chunk.start, chunk.end)
}

// The Issues of one Chunk in the coordinates of the whole Draft, spec section
// 9. A Chunk is checked on its own text, so every span the CLI answers is an
// offset into that Chunk and the Chunk's own `start` is what makes it an offset
// into the Draft.
//
// The Issue is copied rather than moved in place, so the envelope the caller
// parsed stays what the CLI said.
function shiftIssues(issues, start) {
  var offset = Math.round(Number(start) || 0)
  var list = issues || []
  var out = []
  for (var i = 0; i < list.length; i++) {
    var issue = list[i]
    var moved = {}
    for (var key in issue) {
      if (Object.prototype.hasOwnProperty.call(issue, key)) moved[key] = issue[key]
    }
    moved.start = issue.start + offset
    moved.end = issue.end + offset
    out.push(moved)
  }
  return out
}

// One Chunk's answer merged into the list the review will show. Chunks tile the
// Draft in order and never overlap (spec 5.2), so appending keeps the sort by
// `start` and the no-overlap guarantee of spec 5.1 that the review relies on.
function mergeIssues(merged, more) {
  var out = (merged || []).slice()
  var list = more || []
  for (var i = 0; i < list.length; i++) out.push(list[i])
  return out
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    correctedText: correctedText,
    displaySpans: displaySpans,
    verifiedIssues: verifiedIssues,
    firstUnits: firstUnits,
    chunkText: chunkText,
    shiftIssues: shiftIssues,
    mergeIssues: mergeIssues
  }
}
