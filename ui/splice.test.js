// Node tests for the Corrected text splice and the Chunk span math.
// Spec sections 5.2, 5.3, 9, and 13.
// Run with `node --test ui/`.

const test = require("node:test")
const assert = require("node:assert/strict")

const {
  correctedText,
  displaySpans,
  verifiedIssues,
  firstUnits,
  chunkText,
  shiftIssues,
  mergeIssues
} = require("./splice.js")

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

test("firstUnits keeps a text that already fits", () => {
  assert.equal(firstUnits(TEXT, 5000), TEXT)
  assert.equal(firstUnits(TEXT, TEXT.length), TEXT)
})

test("firstUnits cuts at the limit in UTF-16 code units", () => {
  assert.equal(firstUnits(TEXT, 5), "I has")
  assert.equal(firstUnits(TEXT, 0), "")
})

test("firstUnits never splits a surrogate pair", () => {
  // Two code units per emoji, so a limit of 3 lands inside the second pair.
  const pairs = "\u{1F600}\u{1F600}"
  assert.equal(firstUnits(pairs, 3), "\u{1F600}")
  assert.equal(firstUnits(pairs, 2), "\u{1F600}")
  assert.equal(firstUnits(pairs, 4), pairs)
})

test("firstUnits treats a missing text as empty", () => {
  assert.equal(firstUnits(undefined, 10), "")
  assert.equal(firstUnits(null, 10), "")
})

// ------------------------------------------------ the chunked Check, spec 9
//
// A Chunk is checked on its own text, so every span the CLI answers is an
// offset into that Chunk. The merged review indexes into the whole Draft, so
// the Chunk's own start is what makes the two agree.

// A Draft with an Issue right after a Chunk boundary, which is the one place
// a missing shift still looks plausible.
const DRAFT = "She go home.\n\nThey was late. I has two book.\n\nHe walk fast."
const CHUNKS = [
  { start: 0, end: 14 },
  { start: 14, end: 45 },
  { start: 45, end: DRAFT.length }
]

test("the chunks tile the whole Draft with no gap and no overlap", () => {
  let joined = ""
  for (const chunk of CHUNKS) joined += chunkText(DRAFT, chunk)
  assert.equal(joined, DRAFT)
})

test("a Chunk's Issues move by the Chunk start and nothing else changes", () => {
  const found = [
    { start: 5, end: 8, original: "was", fix: "were", reason: "Plural subject.", category: "grammar", ruleId: "X" }
  ]
  const moved = shiftIssues(found, CHUNKS[1].start)
  assert.deepEqual(moved, [
    { start: 19, end: 22, original: "was", fix: "were", reason: "Plural subject.", category: "grammar", ruleId: "X" }
  ])
  // The envelope the caller parsed stays what the CLI said.
  assert.equal(found[0].start, 5)
})

test("a shifted span points at the same text in the whole Draft", () => {
  const chunk = CHUNKS[1]
  const body = chunkText(DRAFT, chunk)
  const found = [
    { start: body.indexOf("was"), end: body.indexOf("was") + 3, original: "was", fix: "were" },
    { start: body.indexOf("has"), end: body.indexOf("has") + 3, original: "has", fix: "have" }
  ]
  for (const issue of shiftIssues(found, chunk.start)) {
    assert.equal(DRAFT.slice(issue.start, issue.end), issue.original)
  }
})

// The acceptance criterion of this ticket: an Issue at the very first unit of
// the second Chunk, which a missing shift would put at the top of the Draft.
test("an Issue on a Chunk boundary lands on the boundary, not on the Draft start", () => {
  const chunk = CHUNKS[1]
  assert.equal(chunkText(DRAFT, chunk).slice(0, 4), "They")
  const moved = shiftIssues([{ start: 0, end: 4, original: "They", fix: "They" }], chunk.start)
  assert.equal(moved[0].start, 14)
  assert.equal(DRAFT.slice(moved[0].start, moved[0].end), "They")
  assert.notEqual(DRAFT.slice(0, 4), "They")
})

test("an unshifted Issue from a later Chunk is dropped by the verify", () => {
  const chunk = CHUNKS[1]
  const found = [{ start: 5, end: 8, original: "was", fix: "were" }]
  assert.equal(verifiedIssues(DRAFT, found).issues.length, 0)
  assert.equal(verifiedIssues(DRAFT, shiftIssues(found, chunk.start)).issues.length, 1)
})

test("merging keeps every Issue in Chunk order, which is span order", () => {
  const merged = mergeIssues(
    mergeIssues([], shiftIssues([{ start: 4, end: 6, original: "go", fix: "goes" }], CHUNKS[0].start)),
    shiftIssues([{ start: 5, end: 8, original: "was", fix: "were" }], CHUNKS[1].start))

  assert.deepEqual(merged.map(issue => issue.start), [4, 19])
  for (let i = 1; i < merged.length; i++) assert.ok(merged[i].start >= merged[i - 1].end)
})

test("merging leaves the list it was handed alone", () => {
  const first = [{ start: 0, end: 1, original: "a", fix: "b" }]
  mergeIssues(first, [{ start: 5, end: 6, original: "c", fix: "d" }])
  assert.equal(first.length, 1)
})

test("the merged list splices the whole Draft correctly", () => {
  const merged = mergeIssues(
    shiftIssues([{ start: 4, end: 6, original: "go", fix: "goes" }], CHUNKS[0].start),
    shiftIssues([{ start: 5, end: 8, original: "was", fix: "were" }], CHUNKS[1].start))

  assert.equal(correctedText(DRAFT, merged, [true, true]),
    "She goes home.\n\nThey were late. I has two book.\n\nHe walk fast.")
})

test("an empty Chunk answer and a missing one both merge to nothing", () => {
  assert.deepEqual(shiftIssues(undefined, 10), [])
  assert.deepEqual(shiftIssues([], 10), [])
  assert.deepEqual(mergeIssues(undefined, undefined), [])
})

test("chunkText with no chunk is the whole text", () => {
  assert.equal(chunkText(DRAFT, null), DRAFT)
  assert.equal(chunkText(undefined, CHUNKS[0]), "")
})
