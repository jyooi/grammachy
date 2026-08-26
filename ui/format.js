// Counted sizes as the cards word them, the one rule that decides whether a
// Draft may be checked at all, and the progress line of a chunked Check.
// Spec sections 6, 8, and 9.
//
// Loaded twice: by the QML cards through `import "format.js" as Format`, and
// by `format.test.js` under node. Nothing here may touch a QML or a node API,
// because each side only has one of them.
//
// Every count is a UTF-16 code unit, which is what the CLI measures and what
// a JavaScript string length already is.

// Thousands separators, because every card that prints a size is about a
// number the reader has to compare at a glance.
function grouped(count) {
  return String(count).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
}

function units(count) {
  return grouped(count) + (count === 1 ? " unit" : " units")
}

// The note a first-N Check leaves on the hero, spec sections 6 and 8.
//
// `checked` is the size of the text the Check actually ran on, never the limit
// it was cut to: the cut backs off a unit to keep a surrogate pair whole, and
// the Engine that named the limit can be changed after the answer is on screen.
function truncatedNote(checked, selected) {
  return "First " + grouped(checked) + " of " + units(selected) + " checked"
}

// Why Compose will not run a Check on this Draft, or "" when it will.
//
// `cap` is the whole Draft limit of spec section 5.2. Anything under it that
// needs more than one `grammachy check` is checked in Chunks (spec section 9),
// so the size of one Check is not a refusal here: only an empty Draft and one
// over the cap are.
function draftRefusal(count, cap) {
  if (count <= 0) return "Type or paste a draft, then check it."
  if (count > cap)
    return "The draft is " + units(count) + ", over the cap of " + grouped(cap) + "."
  return ""
}

// How long a run has taken, as the hero says it. Milliseconds while a Check is
// still a moment, and seconds once a chunked run has become a wait, because a
// six-digit millisecond count is not a number anyone reads at a glance.
function elapsed(milliseconds) {
  var value = Math.max(0, Math.round(Number(milliseconds) || 0))
  if (value < 1000) return value + " ms"
  return (Math.round(value / 100) / 10).toFixed(1) + " s"
}

// The hero meta line of a chunked Check, spec section 9. `index` is the Chunk
// being checked, counted from one, so the line reads as progress rather than
// as how many are already behind it.
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
