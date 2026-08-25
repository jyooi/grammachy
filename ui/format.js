// Counted sizes as the cards word them, and the one rule that decides whether
// a Draft may be checked at all. Spec sections 6, 8, and 9.
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

// Why Compose will not run a Check on this Draft, or "" when it will.
//
// `checkLimit` is what one `grammachy check` takes and `cap` is the whole
// Draft limit of spec section 5.2. Between the two lies the chunked path of
// spec section 9, which is its own ticket: until it lands, a Draft that needs
// more than one Chunk is refused here rather than sent to a Check that would
// answer `text_too_long`.
function draftRefusal(count, checkLimit, cap) {
  if (count <= 0) return "Type or paste a draft, then check it."
  if (count > cap)
    return "The draft is " + units(count) + ", over the cap of " + grouped(cap) + "."
  if (count > checkLimit)
    return "The draft is " + units(count) + ", over the " + grouped(checkLimit)
      + " one check takes. Checking in chunks arrives in a later milestone."
  return ""
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    grouped: grouped,
    units: units,
    draftRefusal: draftRefusal
  }
}
