// Node tests for the quick popup key map. Spec sections 6 and 13.
// Run with `node --test ui/`.

const test = require("node:test")
const assert = require("node:assert/strict")

const Keymap = require("./keymap.js")

// The real table is Qt's; these stand-ins only have to be distinct, because
// the map compares codes and never reads a number's meaning.
const CODES = {
  escape: 101,
  returnKey: 102,
  enter: 103,
  space: 104,
  up: 105,
  down: 106,
  a: 107,
  c: 108,
  control: 1,
  shift: 2,
  alt: 4,
  meta: 8
}

function press(key, modifiers) {
  return { key: key, modifiers: modifiers === undefined ? 0 : modifiers }
}

function reviewing(key, modifiers) {
  return Keymap.action(press(key, modifiers), CODES, true)
}

test("every key of the map answers its own action", () => {
  assert.equal(reviewing(CODES.returnKey), Keymap.ACCEPT)
  assert.equal(reviewing(CODES.space), Keymap.SKIP)
  assert.equal(reviewing(CODES.up), Keymap.FOCUS_PREVIOUS)
  assert.equal(reviewing(CODES.down), Keymap.FOCUS_NEXT)
  assert.equal(reviewing(CODES.a), Keymap.ACCEPT_ALL)
  assert.equal(reviewing(CODES.escape), Keymap.CLOSE)
  assert.equal(reviewing(CODES.c, CODES.control), Keymap.COPY)
  assert.equal(reviewing(CODES.returnKey, CODES.control), Keymap.APPLY)
})

test("the keypad Enter accepts and replaces like the main Return", () => {
  assert.equal(reviewing(CODES.enter), Keymap.ACCEPT)
  assert.equal(reviewing(CODES.enter, CODES.control), Keymap.APPLY)
})

test("Ctrl turns Enter into Apply rather than Accept", () => {
  assert.equal(reviewing(CODES.returnKey), Keymap.ACCEPT)
  assert.notEqual(reviewing(CODES.returnKey, CODES.control), Keymap.ACCEPT)
})

test("a bare C is not the copy key", () => {
  assert.equal(reviewing(CODES.c), Keymap.NONE)
})

test("Ctrl on a key the map does not name asks for nothing", () => {
  assert.equal(reviewing(CODES.a, CODES.control), Keymap.NONE)
  assert.equal(reviewing(CODES.space, CODES.control), Keymap.NONE)
})

test("Shift still reaches the map, because a reader of `A` may hold it", () => {
  assert.equal(reviewing(CODES.a, CODES.shift), Keymap.ACCEPT_ALL)
})

test("Alt and Meta belong to the compositor, so the map declines them", () => {
  assert.equal(reviewing(CODES.a, CODES.alt), Keymap.NONE)
  assert.equal(reviewing(CODES.returnKey, CODES.meta), Keymap.NONE)
  assert.equal(reviewing(CODES.c, CODES.control | CODES.alt), Keymap.NONE)
})

test("Esc closes a card with nothing to review, and nothing else does", () => {
  assert.equal(Keymap.action(press(CODES.escape), CODES, false), Keymap.CLOSE)
  assert.equal(Keymap.action(press(CODES.returnKey), CODES, false), Keymap.NONE)
  assert.equal(Keymap.action(press(CODES.space), CODES, false), Keymap.NONE)
  assert.equal(Keymap.action(press(CODES.a), CODES, false), Keymap.NONE)
  assert.equal(Keymap.action(press(CODES.c, CODES.control), CODES, false), Keymap.NONE)
})

test("Esc closes even under a foreign modifier, so the overlay is never a trap", () => {
  assert.equal(reviewing(CODES.escape, CODES.alt), Keymap.CLOSE)
})

test("a missing event or code table asks for nothing", () => {
  assert.equal(Keymap.action(null, CODES, true), Keymap.NONE)
  assert.equal(Keymap.action(press(CODES.escape), null, true), Keymap.NONE)
})
