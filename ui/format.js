// Loaded twice: by the QML cards through `import "format.js" as Format`, and
// by `format.test.js` under node. Nothing here may touch a QML or a node API,
// because each side only has one of them.
//
// Every count is a UTF-16 code unit, which is what the CLI measures and what
// a JavaScript string length already is.

function grouped(count) {
  return String(count).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
}

function units(count) {
  return grouped(count) + (count === 1 ? " unit" : " units")
}

function truncatedNote(checked, selected) {
  return "First " + grouped(checked) + " of " + units(selected) + " checked"
}

function draftRefusal(count, cap) {
  if (count <= 0) return "Type or paste a draft, then check it."
  if (count > cap)
    return "The draft is " + units(count) + ", over the cap of " + grouped(cap) + "."
  return ""
}

function elapsed(milliseconds) {
  var value = Math.max(0, Math.round(Number(milliseconds) || 0))
  if (value < 1000) return value + " ms"
  return (Math.round(value / 100) / 10).toFixed(1) + " s"
}

function chunkProgress(index, total, engine, milliseconds) {
  return "Checking " + index + " of " + total + ", " + engine + ", " + elapsed(milliseconds)
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    grouped: grouped,
    units: units,
    truncatedNote: truncatedNote,
    draftRefusal: draftRefusal,
    elapsed: elapsed,
    chunkProgress: chunkProgress
  }
}
