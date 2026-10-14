function isAccepted(accepted, index) {
  return Boolean(accepted) && accepted[index] === true
}

function correctedText(text, issues, accepted) {
  var out = text === undefined || text === null ? "" : String(text)
  var list = issues || []
  for (var i = list.length - 1; i >= 0; i--) {
    if (!isAccepted(accepted, i)) continue
    out = out.slice(0, list[i].start) + list[i].fix + out.slice(list[i].end)
  }
  return out
}

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

function firstUnits(text, limit) {
  var source = text === undefined || text === null ? "" : String(text)
  var end = Math.max(0, Math.floor(limit))
  if (source.length <= end) return source
  var last = source.charCodeAt(end - 1)
  if (last >= 0xD800 && last <= 0xDBFF) end -= 1
  return source.slice(0, end)
}

function chunkText(text, chunk) {
  var source = text === undefined || text === null ? "" : String(text)
  if (!chunk) return source
  return source.slice(chunk.start, chunk.end)
}

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
