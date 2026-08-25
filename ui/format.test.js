// Node tests for the counted sizes the cards print and for the Compose
// refusal. Spec sections 6, 8, 9, and 13.
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
  assert.equal(Format.draftRefusal(1, CHECK_LIMIT, CAP), "")
  assert.equal(Format.draftRefusal(CHECK_LIMIT, CHECK_LIMIT, CAP), "")
})

test("an empty Draft asks for a Draft rather than naming a limit", () => {
  const refusal = Format.draftRefusal(0, CHECK_LIMIT, CAP)
  assert.match(refusal, /Type or paste a draft/)
  assert.doesNotMatch(refusal, /50,000/)
})

test("a Draft over the cap is refused with its count and the cap", () => {
  const refusal = Format.draftRefusal(CAP + 1, CHECK_LIMIT, CAP)
  assert.match(refusal, /50,001 units/)
  assert.match(refusal, /cap of 50,000/)
})

test("a Draft over one Chunk is refused until chunked checking lands", () => {
  const refusal = Format.draftRefusal(CHECK_LIMIT + 1, CHECK_LIMIT, CAP)
  assert.match(refusal, /5,001 units/)
  assert.match(refusal, /chunks/)
  assert.doesNotMatch(refusal, /cap/)
})
