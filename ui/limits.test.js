// Node tests for the Check size limit the too-long card of spec section 6
// fires at. Run with `node --test ui/limits.test.js`.

const test = require("node:test")
const assert = require("node:assert/strict")

const Limits = require("./limits.js")
const Format = require("./format.js")
const Settings = require("./settings.js")

test("every engine reads the same limit", () => {
  assert.equal(Limits.checkLimit("languagetool"), 5000)
  assert.equal(Limits.checkLimit("harper"), 5000)
})

test("an unknown or missing engine reads the same limit too", () => {
  assert.equal(Limits.checkLimit("gector"), 5000)
  assert.equal(Limits.checkLimit(""), 5000)
  assert.equal(Limits.checkLimit(undefined), 5000)
  assert.equal(Limits.checkLimit(null), 5000)
})

test("every engine the Settings dropdown offers names the limit", () => {
  for (const option of Settings.ENGINE_OPTIONS) {
    assert.equal(Limits.checkLimit(option.value), 5000, option.value + " names the spec limit")
  }
})

// The too-long card of spec section 6 shows the limit twice, in the size bar
// and on `Check the first N only`. Both read the one number this file owns.
test("the too-long card words the check limit", () => {
  assert.equal(Format.units(Limits.checkLimit("languagetool")) + " per check", "5,000 units per check")
  assert.equal(
    "Check the first " + Format.grouped(Limits.checkLimit("languagetool")) + " only",
    "Check the first 5,000 only"
  )
})
