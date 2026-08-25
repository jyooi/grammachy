// Node tests for the counted sizes the cards print, for the Compose refusal,
// and for the progress line of a chunked Check. Spec sections 6, 8, 9, and 13.
// Run with `node --test ui/`.

const test = require("node:test")
const assert = require("node:assert/strict")

const Format = require("./format.js")

// The two limits of the CLI. `cli/tests/overlay_limit.rs` keeps the QML copies
// of these equal to `check::MAX_UTF16_UNITS` and `chunk::MAX_DRAFT_UTF16_UNITS`.
const CHECK_LIMIT = 5000
const CAP = 50000

test("a size over a thousand is grouped, so it can be read at a glance", () => {
  assert.equal(Format.grouped(0), "0")
  assert.equal(Format.grouped(999), "999")
  assert.equal(Format.grouped(1000), "1,000")
  assert.equal(Format.grouped(50001), "50,001")
})

test("one unit is singular and every other count is plural", () => {
  assert.equal(Format.units(1), "1 unit")
  assert.equal(Format.units(0), "0 units")
  assert.equal(Format.units(5000), "5,000 units")
})

test("a Draft that fits one Check is not refused", () => {
  assert.equal(Format.draftRefusal(1, CAP), "")
  assert.equal(Format.draftRefusal(CHECK_LIMIT, CAP), "")
})

test("an empty Draft asks for a Draft rather than naming a limit", () => {
  const refusal = Format.draftRefusal(0, CAP)
  assert.match(refusal, /Type or paste a draft/)
  assert.doesNotMatch(refusal, /50,000/)
})

test("a Draft over the cap is refused with its count and the cap", () => {
  const refusal = Format.draftRefusal(CAP + 1, CAP)
  assert.match(refusal, /50,001 units/)
  assert.match(refusal, /cap of 50,000/)
})

// Spec section 9: a Draft over what one Check takes is checked in Chunks, so
// the size of one Check is no longer a refusal. Only the cap is.
test("a Draft over one Chunk is checked in chunks rather than refused", () => {
  assert.equal(Format.draftRefusal(CHECK_LIMIT + 1, CAP), "")
  assert.equal(Format.draftRefusal(20000, CAP), "")
  assert.equal(Format.draftRefusal(CAP, CAP), "")
})

// ------------------------------------------------------- the progress line

test("a run under a second is counted in milliseconds", () => {
  assert.equal(Format.elapsed(0), "0 ms")
  assert.equal(Format.elapsed(23), "23 ms")
  assert.equal(Format.elapsed(999), "999 ms")
})

test("a run of a second or more is counted in seconds, to one decimal", () => {
  assert.equal(Format.elapsed(1000), "1.0 s")
  assert.equal(Format.elapsed(1240), "1.2 s")
  assert.equal(Format.elapsed(12000), "12.0 s")
  assert.equal(Format.elapsed(95500), "95.5 s")
})

test("a negative or unreadable elapsed reads as none at all", () => {
  assert.equal(Format.elapsed(-5), "0 ms")
  assert.equal(Format.elapsed(undefined), "0 ms")
  assert.equal(Format.elapsed("x"), "0 ms")
})

// Spec section 9 fixes this wording: `Checking k of n, <engine>, <elapsed>`.
test("the progress line names the chunk, the total, the engine, and the wait", () => {
  assert.equal(Format.chunkProgress(1, 4, "languagetool", 320),
    "Checking 1 of 4, languagetool, 320 ms")
  assert.equal(Format.chunkProgress(3, 4, "harper", 4200),
    "Checking 3 of 4, harper, 4.2 s")
})
