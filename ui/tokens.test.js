// Node tests for marked-text tokens. Spec section 6.
// Run with `node --test ui/`.

const test = require("node:test")
const assert = require("node:assert/strict")

const { tokenize, buildLines, joinedTokens } = require("./tokens.js")

function lineTokens(text, spans) {
  const lines = buildLines(text, spans)
  const tokens = []
  for (const line of lines) {
    for (const token of line.tokens) tokens.push(token)
  }
  return tokens
}

function offsets(tokens) {
  const out = []
  let cursor = 0
  for (const token of tokens) {
    const start = cursor
    cursor += token.word.length + token.blanks.length
    out.push({ start: start, end: cursor, word: token.word, blanks: token.blanks, issue: token.issue })
  }
  return out
}

test("a non-breaking space between words is kept", () => {
  const text = "I has\u00a0two book."
  const tokens = lineTokens(text, [])
  assert.equal(joinedTokens(tokens), text)
  assert.ok(tokens.some((token) => token.blanks.includes("\u00a0")))
})

test("a mark on a word before a non-breaking space still lines up", () => {
  const text = "I has\u00a0two book."
  const spans = [{ start: 2, end: 5 }]
  const tokens = lineTokens(text, spans)
  assert.equal(joinedTokens(tokens), text)

  const marked = offsets(tokens).filter((token) => token.issue === 0)
  assert.ok(marked.length > 0)
  assert.equal(marked[0].start, 2)
  assert.equal(marked[0].word, "has")
  assert.equal(text.slice(2, 5), "has")
  assert.ok(marked.every((token) => token.start >= 2 && token.end <= 5))
})

test("trailing blanks stay off the word so the underline stops there", () => {
  const tokens = tokenize([{ text: "has  ", issue: 0 }])
  assert.deepEqual(tokens, [{ word: "has", blanks: "  ", issue: 0 }])
})

test("a word plus its blanks is one token so Flow wraps at the word", () => {
  const tokens = tokenize([{ text: "two book.", issue: -1 }])
  assert.deepEqual(tokens, [
    { word: "two", blanks: " ", issue: -1 },
    { word: "book.", blanks: "", issue: -1 }
  ])
})

test("every whitespace run is kept with the word before it", () => {
  const text = "a\tb\u00a0c\u3000d"
  const tokens = lineTokens(text, [])
  assert.equal(joinedTokens(tokens), text)
  assert.deepEqual(
    tokens.map((token) => token.word),
    ["a", "b", "c", "d"]
  )
})
