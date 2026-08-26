// Node tests for the per-Engine Check size limit the too-long card of spec
// section 6 fires at. Run with `node --test ui/limits.test.js`.

const test = require("node:test")
const assert = require("node:assert/strict")

const Limits = require("./limits.js")
const Format = require("./format.js")
const Settings = require("./settings.js")

test("the local engine reads a smaller limit than every other engine", () => {
  assert.equal(Limits.checkLimit("openai"), 2000)
  assert.equal(Limits.checkLimit("languagetool"), 5000)
  assert.equal(Limits.checkLimit("harper"), 5000)
})

test("an unknown or missing engine reads the wider limit", () => {
  assert.equal(Limits.checkLimit("gector"), 5000)
  assert.equal(Limits.checkLimit(""), 5000)
  assert.equal(Limits.checkLimit(undefined), 5000)
  assert.equal(Limits.checkLimit(null), 5000)
})

test("every engine the Settings dropdown offers names a limit", () => {
  for (const option of Settings.ENGINE_OPTIONS) {
    const limit = Limits.checkLimit(option.value)
    assert.ok(limit === 2000 || limit === 5000, option.value + " names a spec limit")
  }
})

// The too-long card of spec section 6 shows the limit twice, in the size bar
// and on `Check the first N only`. Both read the one number this file owns.
test("the too-long card words the selected engine limit", () => {
  assert.equal(Format.units(Limits.checkLimit("openai")) + " per check", "2,000 units per check")
  assert.equal(
    "Check the first " + Format.grouped(Limits.checkLimit("openai")) + " only",
    "Check the first 2,000 only"
  )
  assert.equal(
    "Check the first " + Format.grouped(Limits.checkLimit("languagetool")) + " only",
    "Check the first 5,000 only"
  )
})
