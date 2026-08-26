// Node tests for the key map of both surfaces. Spec sections 6, 9, and 13.
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

function inMode(mode) {
  return function(key, modifiers) {
    return Keymap.action(press(key, modifiers), CODES, mode)
  }
}

const reviewing = inMode(Keymap.MODE_REVIEW)
const idle = inMode(Keymap.MODE_IDLE)
const editing = inMode(Keymap.MODE_COMPOSE_EDIT)
const composeReview = inMode(Keymap.MODE_COMPOSE_REVIEW)
const confirming = inMode(Keymap.MODE_MODEL_CONFIRM)

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
  assert.equal(idle(CODES.escape), Keymap.CLOSE)
  assert.equal(idle(CODES.returnKey), Keymap.NONE)
  assert.equal(idle(CODES.space), Keymap.NONE)
  assert.equal(idle(CODES.a), Keymap.NONE)
  assert.equal(idle(CODES.c, CODES.control), Keymap.NONE)
})

test("Esc closes even under a foreign modifier, so the overlay is never a trap", () => {
  assert.equal(reviewing(CODES.escape, CODES.alt), Keymap.CLOSE)
})

test("a missing event or code table asks for nothing", () => {
  assert.equal(Keymap.action(null, CODES, Keymap.MODE_REVIEW), Keymap.NONE)
  assert.equal(Keymap.action(press(CODES.escape), null, Keymap.MODE_REVIEW), Keymap.NONE)
})

test("a mode the map does not name reviews nothing, and Esc still closes", () => {
  assert.equal(Keymap.action(press(CODES.escape), CODES, undefined), Keymap.CLOSE)
  assert.equal(Keymap.action(press(CODES.a), CODES, "settings"), Keymap.NONE)
})

// ------------------------------------------------------------------ Compose

test("Compose edit mode keeps Ctrl + Enter for the Check and Esc to close", () => {
  assert.equal(editing(CODES.returnKey, CODES.control), Keymap.CHECK)
  assert.equal(editing(CODES.enter, CODES.control), Keymap.CHECK)
  assert.equal(editing(CODES.escape), Keymap.CLOSE)
})

test("Compose edit mode hands every typing key to the Draft", () => {
  assert.equal(editing(CODES.returnKey), Keymap.NONE)
  assert.equal(editing(CODES.space), Keymap.NONE)
  assert.equal(editing(CODES.a), Keymap.NONE)
  assert.equal(editing(CODES.up), Keymap.NONE)
  assert.equal(editing(CODES.down), Keymap.NONE)
  // Ctrl + C is the text area's own copy while a Draft is being written.
  assert.equal(editing(CODES.c, CODES.control), Keymap.NONE)
})

test("Compose review reviews with the popup keys", () => {
  assert.equal(composeReview(CODES.returnKey), Keymap.ACCEPT)
  assert.equal(composeReview(CODES.space), Keymap.SKIP)
  assert.equal(composeReview(CODES.up), Keymap.FOCUS_PREVIOUS)
  assert.equal(composeReview(CODES.down), Keymap.FOCUS_NEXT)
  assert.equal(composeReview(CODES.a), Keymap.ACCEPT_ALL)
  assert.equal(composeReview(CODES.c, CODES.control), Keymap.COPY)
  assert.equal(composeReview(CODES.returnKey, CODES.control), Keymap.APPLY)
})

test("Esc in Compose review goes back to the Draft rather than closing", () => {
  assert.equal(composeReview(CODES.escape), Keymap.BACK)
  assert.equal(composeReview(CODES.escape, CODES.alt), Keymap.BACK)
})

// Spec section 7: the Remove confirm of the Models list is one question, so
// the mode carries the two answers to it and nothing else.
test("the model confirm answers Remove and Keep and nothing else", () => {
  assert.equal(confirming(CODES.returnKey), Keymap.REMOVE_MODEL)
  assert.equal(confirming(CODES.enter), Keymap.REMOVE_MODEL)
  assert.equal(confirming(CODES.escape), Keymap.KEEP_MODEL)
})

// The confirm sits over the Settings view, so a review key must not reach the
// card behind it: Enter would otherwise accept an Issue nobody can see.
test("no review key reaches the card behind the model confirm", () => {
  for (const key of [CODES.space, CODES.up, CODES.down, CODES.a, CODES.c]) {
    assert.equal(confirming(key), Keymap.NONE)
  }
  assert.equal(confirming(CODES.c, CODES.control), Keymap.NONE)
})

// Ctrl + Enter is Apply everywhere else, and it must not become a Remove that
// the reader did not mean. Only a plain Enter answers the question.
test("a modified Enter does not answer the model confirm", () => {
  assert.equal(confirming(CODES.returnKey, CODES.control), Keymap.NONE)
  assert.equal(confirming(CODES.returnKey, CODES.alt), Keymap.NONE)
  assert.equal(confirming(CODES.returnKey, CODES.meta), Keymap.NONE)
})

// Esc leaves every card, and here it leaves the question rather than the
// overlay, so a mistaken bin press costs one key.
test("Esc answers the confirm rather than closing the overlay", () => {
  assert.notEqual(confirming(CODES.escape), Keymap.CLOSE)
  assert.equal(confirming(CODES.escape, CODES.alt), Keymap.KEEP_MODEL)
})
