// Node tests for the Corrected text splice. Spec sections 5.3 and 13.
// Run with `node --test ui/`.

const test = require("node:test")
const assert = require("node:assert/strict")

const { correctedText, displaySpans, verifiedIssues } = require("./splice.js")

// One Selection reused across the tests. The spans are UTF-16 code units into
// exactly this string.
const TEXT = "I has two book. She go home."
const ISSUES = [
  { start: 2, end: 5, original: "has", fix: "have", reason: "Subject and verb do not agree.", category: "grammar" },
  { start: 10, end: 14, original: "book", fix: "books", reason: "The noun is countable.", category: "grammar" },
  { start: 20, end: 22, original: "go", fix: "goes", reason: "The subject is third person.", category: "grammar" }
]

function decisions(values) {
  return ISSUES.map((_, i) => (values[i] === undefined ? null : values[i]))
}

test("no accepted Issue leaves the Selection alone", () => {
  assert.equal(correctedText(TEXT, ISSUES, decisions([])), TEXT)
})

test("several accepted Issues all splice, and the later spans stay right", () => {
  assert.equal(
    correctedText(TEXT, ISSUES, decisions([true, true, true])),
    "I have two books. She goes home."
  )
})

test("a skipped Issue keeps its original while its neighbours splice", () => {
  assert.equal(
    correctedText(TEXT, ISSUES, decisions([true, false, true])),
    "I have two book. She goes home."
  )
})

test("an open Issue counts as skipped until the user decides", () => {
  assert.equal(correctedText(TEXT, ISSUES, decisions([null, true, null])), "I has two books. She go home.")
})

test("an astral character before an Issue shifts its span by two code units", () => {
  // The emoji is one code point but two UTF-16 code units, so "teh" starts at
  // index 3, not 2. The CLI counts the same way, so plain slicing is correct.
  const text = "\u{1F600} teh cat"
  assert.equal(text.slice(2, 3), " ")
  const issues = [{ start: 3, end: 6, original: "teh", fix: "the", category: "spelling", reason: "Spelling." }]

  assert.equal(correctedText(text, issues, [true]), "\u{1F600} the cat")
  assert.deepEqual(displaySpans(issues, [true]), [{ start: 3, end: 6 }])
  assert.deepEqual(verifiedIssues(text, issues).issues, issues)
})

test("an astral character inside an accepted Fix widens the following span", () => {
  const issues = [
    { start: 0, end: 2, original: "hi", fix: "hi \u{1F600}", category: "grammar", reason: "Greeting." },
    { start: 3, end: 6, original: "teh", fix: "the", category: "spelling", reason: "Spelling." }
  ]

  assert.deepEqual(displaySpans(issues, [true, false]), [{ start: 0, end: 5 }, { start: 6, end: 9 }])
})

test("display spans follow the accept and skip decisions", () => {
  assert.deepEqual(displaySpans(ISSUES, decisions([])), [
    { start: 2, end: 5 },
    { start: 10, end: 14 },
    { start: 20, end: 22 }
  ])

  // "have" is one unit longer than "has", so every later mark moves by one.
  assert.deepEqual(displaySpans(ISSUES, decisions([true, false, true])), [
    { start: 2, end: 6 },
    { start: 11, end: 15 },
    { start: 21, end: 25 }
  ])
})

test("an Issue whose slice does not match its original is dropped", () => {
  const issues = [ISSUES[0], { start: 10, end: 14, original: "cake", fix: "cakes", category: "grammar", reason: "Wrong." }]
  const verified = verifiedIssues(TEXT, issues)

  assert.deepEqual(verified.issues, [ISSUES[0]])
  assert.deepEqual(verified.dropped, [issues[1]])
})

test("an empty Issue list is a no-op everywhere", () => {
  assert.equal(correctedText(TEXT, [], []), TEXT)
  assert.deepEqual(displaySpans([], []), [])
  assert.deepEqual(verifiedIssues(TEXT, []), { issues: [], dropped: [] })
})
