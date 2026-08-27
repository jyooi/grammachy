// Node tests for the error cards. Spec sections 5.1, 8, and 13.
// Run with `node --test ui/errors.test.js`.
//
// The last blocks run stub binaries that print exactly what a real
// `grammachy check` prints for each code, one that prints no JSON at all, and
// one that answers a whole chunked Check of spec section 9.
// A stub is the only safe seam here: a test must never reach a real engine,
// and it must never stop or start the LanguageTool unit the live shell uses.

const test = require("node:test")
const assert = require("node:assert/strict")
const fs = require("node:fs")
const os = require("node:os")
const path = require("node:path")
const { spawnSync } = require("node:child_process")

const {
  EMPTY_SELECTION,
  TEXT_TOO_LONG,
  ENGINE_UNAVAILABLE,
  ENGINE_TIMEOUT,
  ENGINE_ERROR,
  BAD_ARGUMENTS,
  CODES,
  CLOSE,
  RETRY,
  SETTINGS,
  SETUP,
  COMPOSE,
  TIMEOUT_SECONDS,
  timeoutSeconds,
  RETRY_REMAINING,
  REVIEW_PARTIAL,
  known,
  readCheck,
  readChunks,
  card,
  chunkCard,
  buttonLabel
} = require("./errors.js")

const { ENGINE_OPTIONS, labelOf } = require("./settings.js")
const { chunkText, shiftIssues, mergeIssues, verifiedIssues } = require("./splice.js")
const Limits = require("./limits.js")

function languageToolCard(code, message) {
  return card(code, {
    engineLabel: labelOf(ENGINE_OPTIONS, "languagetool"),
    engineSlug: "languagetool",
    message: message || ""
  })
}

// ------------------------------------------------------------- reading stdout

test("a result envelope is not an error", () => {
  const stdout = JSON.stringify({ contractVersion: 1, engine: "harper", elapsedMs: 3, issues: [] })
  const answer = readCheck(stdout)
  assert.equal(answer.error, null)
  assert.equal(answer.result.engine, "harper")
})

test("an error envelope carries its code and message through", () => {
  const stdout = JSON.stringify({
    contractVersion: 1,
    error: { code: "engine_timeout", message: "LanguageTool did not answer within 10 s on 127.0.0.1:8081" }
  })
  assert.deepEqual(readCheck(stdout).error, {
    code: "engine_timeout",
    message: "LanguageTool did not answer within 10 s on 127.0.0.1:8081"
  })
})

// Spec section 5.1: no JSON on stdout is the `bad_arguments` card, and a
// missing or mismatched contractVersion is an error state.
test("no JSON on stdout reads as bad_arguments", () => {
  for (const stdout of ["", "\n", "grammachy: command not found", "not json at all"]) {
    assert.deepEqual(readCheck(stdout).error, { code: BAD_ARGUMENTS, message: "" })
  }
})

test("a missing or mismatched contractVersion reads as bad_arguments", () => {
  const missing = JSON.stringify({ engine: "harper", elapsedMs: 1, issues: [] })
  const mismatched = JSON.stringify({ contractVersion: 2, engine: "harper", elapsedMs: 1, issues: [] })
  const errorToo = JSON.stringify({ contractVersion: 2, error: { code: "engine_error", message: "x" } })
  for (const stdout of [missing, mismatched, errorToo]) {
    assert.deepEqual(readCheck(stdout).error, { code: BAD_ARGUMENTS, message: "" })
  }
})

test("a JSON value that is not an object reads as bad_arguments", () => {
  for (const stdout of ["null", "12", '"text"', "[]"]) {
    assert.deepEqual(readCheck(stdout).error, { code: BAD_ARGUMENTS, message: "" })
  }
})

// ---------------------------------------------------------------- the codes

test("an unknown code is treated as engine_error", () => {
  assert.equal(known("engine_on_fire"), ENGINE_ERROR)
  assert.equal(known(""), ENGINE_ERROR)
  assert.equal(known(undefined), ENGINE_ERROR)
})

test("every code of the contract is known", () => {
  for (const code of CODES) assert.equal(known(code), code)
})

// ---------------------------------------------------------------- the cards

test("empty_selection asks for a selection and offers Close and Open Compose", () => {
  const model = languageToolCard(EMPTY_SELECTION)
  assert.equal(model.title, "Nothing selected")
  assert.equal(model.body, "Highlight some text, then press SUPER + G.")
  assert.deepEqual(model.buttons, [CLOSE, COMPOSE])
  assert.equal(model.needsDiagnosis, false)
})

test("engine_unavailable names the engine and asks for the doctor line", () => {
  const model = languageToolCard(ENGINE_UNAVAILABLE, "LanguageTool did not answer on 127.0.0.1:8081")
  assert.equal(model.title, "LanguageTool is not running")
  assert.equal(model.body, "Grammachy could not reach it on this machine.")
  assert.equal(model.message, "LanguageTool did not answer on 127.0.0.1:8081")
  assert.equal(model.needsDiagnosis, true)
  assert.deepEqual(model.buttons, [CLOSE, RETRY, SETTINGS])
  assert.equal(model.primary, RETRY)
})

test("engine_timeout names the timeout of the engine that is set", () => {
  assert.equal(languageToolCard(ENGINE_TIMEOUT).body,
    "No answer within 10 s. A first start can take a moment.")

  const local = card(ENGINE_TIMEOUT, {
    engineLabel: labelOf(ENGINE_OPTIONS, "openai"),
    engineSlug: "openai"
  })
  assert.equal(local.title, "Local LLM took too long")
  assert.equal(local.body, "No answer within 90 s. A first start can take a moment.")
  assert.deepEqual(local.buttons, [CLOSE, RETRY, SETTINGS])
})

test("engine_error offers the same three buttons", () => {
  const model = languageToolCard(ENGINE_ERROR, "LanguageTool answered with HTTP 500 on 127.0.0.1:8081")
  assert.equal(model.title, "LanguageTool returned an error")
  assert.equal(model.body, "The Check did not finish.")
  assert.equal(model.message, "LanguageTool answered with HTTP 500 on 127.0.0.1:8081")
  assert.deepEqual(model.buttons, [CLOSE, RETRY, SETTINGS])
})

test("bad_arguments blames the companion tool and offers Setup", () => {
  const model = languageToolCard(BAD_ARGUMENTS)
  assert.equal(model.title, "Grammachy could not run the check")
  assert.equal(model.body, "The companion tool is missing or out of date.")
  assert.deepEqual(model.buttons, [CLOSE, SETUP])
  assert.equal(model.primary, SETUP)
})

// The too-long card of spec section 6 has a size bar and a `Check the first N
// only` button, so it is the quick popup's own card and not one of these.
test("text_too_long has no card here", () => {
  assert.equal(languageToolCard(TEXT_TOO_LONG), null)
})

test("an unknown code shows the engine_error card", () => {
  const model = languageToolCard("engine_on_fire", "something new")
  assert.equal(model.code, ENGINE_ERROR)
  assert.equal(model.title, "LanguageTool returned an error")
  assert.equal(model.message, "something new")
})

// Spec section 8: `<Engine>` is the display name of the current engine
// setting, which is the name the Settings dropdown shows.
test("every engine setting reaches its card by its display name", () => {
  const expected = { languagetool: "LanguageTool", openai: "Local LLM", harper: "Harper" }
  for (const slug of Object.keys(expected)) {
    const model = card(ENGINE_UNAVAILABLE, {
      engineLabel: labelOf(ENGINE_OPTIONS, slug),
      engineSlug: slug
    })
    assert.equal(model.title, expected[slug] + " is not running")
  }
})

// Spec section 8 and HUF-229: the cloud engine runs on nothing this machine
// installs, so its card never asks for a `doctor` line. The CLI message under
// the body carries the reason word instead.
test("the cloud engine card carries the message and asks for no diagnosis", () => {
  const model = card(ENGINE_UNAVAILABLE, {
    engineLabel: "Cloud LLM",
    engineSlug: "openrouter",
    message: "OpenRouter credits are used up. Add credits on openrouter.ai, then retry. (reason: no_credit)"
  })

  assert.equal(model.title, "Cloud LLM could not run the check")
  assert.equal(model.body, "openrouter.ai refused the Check.")
  assert.equal(model.needsDiagnosis, false)
  assert.ok(model.message.includes("no_credit"))
  assert.deepEqual(model.buttons, languageToolCard(ENGINE_UNAVAILABLE).buttons)
  assert.equal(model.primary, languageToolCard(ENGINE_UNAVAILABLE).primary)
})

// HUF-229 AC4: five reasons reach one code, and only `unreachable` means
// openrouter.ai answered nothing. The card says which one it was, because a
// body that claims the wrong one contradicts the message printed under it.
test("the cloud card says what each reason word actually means", () => {
  // The messages are the ones `cli/src/engines/openrouter/mod.rs` prints.
  const cases = [
    {
      message: "Cloud LLM has no key. Store one: printf '%s' \"$KEY\" | grammachy setup --openrouter-key. (reason: no_key)",
      body: "No key is stored for openrouter.ai.",
      meta: "no cloud key"
    },
    {
      message: "Cloud LLM is not reachable. Grammachy could not reach openrouter.ai. (reason: unreachable)",
      body: "Grammachy could not reach openrouter.ai.",
      meta: "cloud engine not reachable"
    },
    {
      message: "OpenRouter rejected the key. Store a new one: printf '%s' \"$KEY\" | grammachy setup --openrouter-key. (reason: rejected_key)",
      body: "openrouter.ai refused the key.",
      meta: "cloud key refused"
    },
    {
      message: "OpenRouter credits are used up. Add credits on openrouter.ai, then retry. (reason: no_credit)",
      body: "openrouter.ai refused the Check.",
      meta: "cloud engine out of credit"
    },
    {
      message: "OpenRouter is rate limited. Wait a moment, then retry. (reason: rate_limited)",
      body: "openrouter.ai refused the Check.",
      meta: "cloud engine rate limited"
    }
  ]

  for (const one of cases) {
    const model = card(ENGINE_UNAVAILABLE, {
      engineLabel: "Cloud LLM",
      engineSlug: "openrouter",
      message: one.message
    })

    assert.equal(model.body, one.body, one.message)
    assert.equal(model.meta, one.meta, one.message)
    assert.equal(model.message, one.message)
    assert.equal(model.needsDiagnosis, false, one.message)
  }
})

// A message the shell cannot read a reason out of still needs a body that is
// true of all five, because the card is what the reader acts on.
test("a cloud message with no reason word says only what is certain", () => {
  for (const message of ["", "something new went wrong", "(reason: )"]) {
    const model = card(ENGINE_UNAVAILABLE, {
      engineLabel: "Cloud LLM",
      engineSlug: "openrouter",
      message: message
    })

    assert.equal(model.body, "The Check did not run.", message)
    assert.equal(model.meta, "cloud engine failed", message)
  }
})

// Every other engine still gets the `doctor` line, which is what tells the two
// branches apart.
test("only the cloud engine skips the doctor line", () => {
  for (const slug of Object.keys(TIMEOUT_SECONDS)) {
    const model = card(ENGINE_UNAVAILABLE, { engineLabel: "Engine", engineSlug: slug })
    assert.equal(model.needsDiagnosis, slug !== "openrouter", slug)
  }
})

test("the cloud engine waits thirty seconds", () => {
  assert.equal(timeoutSeconds("openrouter"), 30)
  assert.equal(card(ENGINE_TIMEOUT, { engineSlug: "openrouter" }).body,
    "No answer within 30 s. A first start can take a moment.")
})

test("every card carries a title, a body, a meta line, and buttons", () => {
  for (const code of CODES) {
    if (code === TEXT_TOO_LONG) continue
    const model = languageToolCard(code)
    assert.ok(model.title.length > 0, code + " has a title")
    assert.ok(model.body.length > 0, code + " has a body")
    assert.ok(model.meta.length > 0, code + " has a meta line")
    assert.ok(model.buttons.length > 0, code + " has buttons")
    assert.ok(model.buttons.indexOf(CLOSE) !== -1, code + " can be closed")
    assert.ok(model.buttons.indexOf(model.primary) !== -1, code + " leads with a button it has")
    for (const action of model.buttons) {
      assert.ok(buttonLabel(action).length > 0, action + " has a label")
    }
  }
})

test("an engine with no timeout entry still gets a number", () => {
  assert.equal(timeoutSeconds("gector"), 10)
  assert.equal(timeoutSeconds(undefined), 10)
  for (const slug of Object.keys(TIMEOUT_SECONDS)) {
    assert.equal(timeoutSeconds(slug), TIMEOUT_SECONDS[slug])
  }
})

// --------------------------------------------------------- against a stub CLI
//
// The overlay never sees an envelope as a string literal: it sees whatever the
// binary at `bin/grammachy` wrote. These stubs write exactly that, so the route
// from a real process to a real card runs end to end.

let stubDirectory = ""

test.before(() => {
  stubDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "grammachy-errors-"))
})

test.after(() => {
  if (stubDirectory) fs.rmSync(stubDirectory, { recursive: true, force: true })
})

// A stub that reads stdin the way `grammachy check` does, prints `output`, and
// leaves with `exitCode`.
function stub(name, output, exitCode) {
  const file = path.join(stubDirectory, name)
  fs.writeFileSync(file, [
    "#!/bin/sh",
    "cat > /dev/null",
    "printf '%s' " + JSON.stringify(output),
    "exit " + exitCode,
    ""
  ].join("\n"))
  fs.chmodSync(file, 0o755)
  return file
}

// One Check: run the stub with the text on stdin and read the card back the
// way Overlay.qml does.
function checkThrough(binary, text) {
  const run = spawnSync(binary, ["check", "--engine", "languagetool"], { input: text, encoding: "utf8" })
  assert.equal(run.error, undefined)
  const answer = readCheck(run.stdout)
  if (!answer.error) return { stdin: text, card: null, result: answer.result }
  return {
    stdin: text,
    card: card(answer.error.code, {
      engineLabel: labelOf(ENGINE_OPTIONS, "languagetool"),
      engineSlug: "languagetool",
      message: answer.error.message
    })
  }
}

function errorEnvelope(code, message) {
  return JSON.stringify({ contractVersion: 1, error: { code: code, message: message } })
}

test("a stub that emits each envelope shows each card", () => {
  const wanted = [
    { code: EMPTY_SELECTION, message: "The selection is empty.", title: "Nothing selected", buttons: [CLOSE, COMPOSE] },
    {
      code: ENGINE_UNAVAILABLE,
      message: "LanguageTool did not answer on 127.0.0.1:8081",
      title: "LanguageTool is not running",
      buttons: [CLOSE, RETRY, SETTINGS]
    },
    {
      code: ENGINE_TIMEOUT,
      message: "LanguageTool did not answer within 10 s on 127.0.0.1:8081",
      title: "LanguageTool took too long",
      buttons: [CLOSE, RETRY, SETTINGS]
    },
    {
      code: ENGINE_ERROR,
      message: "LanguageTool answered with HTTP 500 on 127.0.0.1:8081",
      title: "LanguageTool returned an error",
      buttons: [CLOSE, RETRY, SETTINGS]
    },
    {
      code: BAD_ARGUMENTS,
      message: "The base URL must stay on this machine.",
      title: "Grammachy could not run the check",
      buttons: [CLOSE, SETUP]
    }
  ]

  for (const want of wanted) {
    const binary = stub("emit-" + want.code, errorEnvelope(want.code, want.message), 1)
    const shown = checkThrough(binary, "I has two book.").card
    assert.equal(shown.code, want.code, want.code + " shows its own card")
    assert.equal(shown.title, want.title)
    assert.deepEqual(shown.buttons, want.buttons)
    // Spec section 8: the CLI message shows under the body.
    assert.equal(shown.message, want.message)
  }
})

test("a stub that emits no JSON shows the bad_arguments card", () => {
  const binary = stub("emit-nothing", "grammachy: not a subcommand\n", 2)
  const shown = checkThrough(binary, "I has two book.").card
  assert.equal(shown.code, BAD_ARGUMENTS)
  assert.equal(shown.title, "Grammachy could not run the check")
  assert.deepEqual(shown.buttons, [CLOSE, SETUP])
  assert.equal(shown.message, "")
})

test("a stub with a silent stdout shows the bad_arguments card", () => {
  const binary = stub("emit-silence", "", 127)
  assert.equal(checkThrough(binary, "I has two book.").card.code, BAD_ARGUMENTS)
})

test("a stub that emits a result envelope shows no card", () => {
  const envelope = JSON.stringify({ contractVersion: 1, engine: "languagetool", elapsedMs: 12, issues: [] })
  const binary = stub("emit-result", envelope, 0)
  const shown = checkThrough(binary, "I have two books.")
  assert.equal(shown.card, null)
  assert.equal(shown.result.engine, "languagetool")
})

// Spec section 8: Retry re-runs the Check with the same Selection and no
// re-capture. `retryCheck()` in Overlay.qml calls `runCheck(root.selectionText)`
// and nothing else, so the text the failed Check ran on is the text that runs
// again, whatever the source window is highlighting by then.
// `cli/tests/overlay_errors.rs` is what holds that call in place.
test("retry sends the text of the failed check, not a fresh selection", () => {
  const binary = stub("emit-unavailable-twice",
    errorEnvelope(ENGINE_UNAVAILABLE, "LanguageTool did not answer on 127.0.0.1:8081"), 1)

  const selection = "I has two book."
  const failed = checkThrough(binary, selection)
  assert.equal(failed.card.code, ENGINE_UNAVAILABLE)

  // The source window now holds something else entirely. The overlay keeps
  // `selectionText`, so Retry hands the stub the same stdin as the first run.
  const retried = checkThrough(binary, failed.stdin)
  assert.equal(retried.stdin, selection)
  assert.notEqual(retried.stdin, "something the user highlighted since")
  assert.deepEqual(retried.card, failed.card)
})

// ------------------------------------------------- reading the Chunk list

test("a Chunk list envelope carries its chunks through", () => {
  const stdout = JSON.stringify({ contractVersion: 1, chunks: [{ start: 0, end: 12 }, { start: 12, end: 20 }] })
  const answer = readChunks(stdout)
  assert.equal(answer.error, null)
  assert.deepEqual(answer.chunks, [{ start: 0, end: 12 }, { start: 12, end: 20 }])
})

test("an error envelope from chunk carries its code and message through", () => {
  const stdout = JSON.stringify({
    contractVersion: 1,
    error: { code: "text_too_long", message: "The Draft is 50001 units long, over the limit of 50000." }
  })
  assert.deepEqual(readChunks(stdout).error, {
    code: "text_too_long",
    message: "The Draft is 50001 units long, over the limit of 50000."
  })
})

// Spec section 5.2: the shell cannot walk a tiling it did not get, and both of
// these say the same thing about the companion tool.
test("no JSON and no chunks array both read as bad_arguments", () => {
  for (const stdout of ["", "not json", JSON.stringify({ contractVersion: 2, chunks: [] }),
    JSON.stringify({ contractVersion: 1, chunks: "all of it" })]) {
    assert.deepEqual(readChunks(stdout).error, { code: BAD_ARGUMENTS, message: "" })
  }
})

// ------------------------------------------- the inline failure of a Chunk

function languageToolChunkCard(code, message, hasPartial) {
  return chunkCard(code, {
    engineLabel: labelOf(ENGINE_OPTIONS, "languagetool"),
    engineSlug: "languagetool",
    message: message || "",
    hasPartial: hasPartial === true
  })
}

// Spec section 9: the failure says what went wrong in the same words section 8
// uses, because the same thing went wrong.
test("a failed Chunk keeps the title, body, and message of its code", () => {
  const inline = languageToolChunkCard(ENGINE_UNAVAILABLE, "LanguageTool did not answer on 127.0.0.1:8081", true)
  const plain = languageToolCard(ENGINE_UNAVAILABLE, "LanguageTool did not answer on 127.0.0.1:8081")
  assert.equal(inline.title, plain.title)
  assert.equal(inline.body, plain.body)
  assert.equal(inline.message, plain.message)
  assert.equal(inline.needsDiagnosis, plain.needsDiagnosis)
})

test("a failed Chunk with finished Chunks behind it offers both recoveries", () => {
  const inline = languageToolChunkCard(ENGINE_TIMEOUT, "no answer in 10 s", true)
  assert.deepEqual(inline.buttons, [RETRY_REMAINING, REVIEW_PARTIAL])
  assert.equal(inline.primary, RETRY_REMAINING)
  assert.equal(buttonLabel(RETRY_REMAINING), "Retry remaining")
  assert.equal(buttonLabel(REVIEW_PARTIAL), "Review what we have")
})

// With nothing behind it there is nothing to review, so the card falls back to
// what section 8 offers around the same resume.
test("a failure with no Chunk behind it offers no review", () => {
  const inline = languageToolChunkCard(ENGINE_UNAVAILABLE, "not running", false)
  assert.deepEqual(inline.buttons, [CLOSE, RETRY_REMAINING, SETTINGS])
  assert.equal(inline.primary, RETRY_REMAINING)
})

// A Chunk is cut to fit, so `text_too_long` from one is the engine failing.
test("text_too_long from a Chunk reads as an engine error, not as a card of its own", () => {
  const inline = languageToolChunkCard(TEXT_TOO_LONG, "too long", true)
  assert.equal(inline.code, ENGINE_ERROR)
  assert.equal(inline.title, "LanguageTool returned an error")
  assert.deepEqual(inline.buttons, [RETRY_REMAINING, REVIEW_PARTIAL])
})

test("every code a Chunk can fail with has an inline card", () => {
  for (const code of CODES) {
    const inline = languageToolChunkCard(code, "", true)
    assert.ok(inline.title.length > 0, code + " has a title")
    assert.deepEqual(inline.buttons, [RETRY_REMAINING, REVIEW_PARTIAL])
  }
})

// ------------------------------------------ a whole chunked run, spec 9
//
// These stubs answer both subcommands of a chunked Check, so the run below is
// the route from two real processes to one merged list of Issues. A stub is the
// only safe seam: a test must never reach a real engine, and it must never stop
// or start the LanguageTool unit the live shell uses.

// A Draft of 20 identical sentences, so a Chunk boundary never splits the word
// the stub finds and every Issue start is known in advance.
const SENTENCE = "I has a cat. "
const DRAFT = SENTENCE.repeat(20)
const CHUNK_UNITS = SENTENCE.length * 5
const CHUNK_COUNT = 4
// The engine a Check runs on when nothing else is said, spec section 7.
const DEFAULT_ENGINE = "languagetool"
// What the stub packs to, one size per Engine. The limit belongs to the Engine
// (spec section 4), so a Chunk list fits only the Engine that sized it. These
// are the shape of that rule rather than its numbers, which `limits.test.js`
// owns.
const LOCAL_CHUNK_UNITS = SENTENCE.length * 2
const LOCAL_CHUNK_COUNT = 10
const CHUNK_UNITS_BY_ENGINE = { languagetool: CHUNK_UNITS, harper: CHUNK_UNITS, openai: LOCAL_CHUNK_UNITS }
// One Issue per sentence, at the same offset in each.
const WANTED_STARTS = Array.from({ length: 20 }, (_, i) => 2 + SENTENCE.length * i)

// A stub that answers `chunk` with a tiling of the named Engine's size and
// `check` with one Issue per "has" in the text it was handed, in that text's
// own coordinates. It refuses a text over that Engine's size with
// `text_too_long`, the way the CLI does before any engine runs. `delayMs` makes
// a run take long enough for a Cancel to be a real decision, and `failOnCall`
// fails the nth `check` once and succeeds on every call after it.
function chunkedStub(name, delayMs, failOnCall) {
  const counter = path.join(stubDirectory, name + ".count")
  const packs = path.join(stubDirectory, name + ".packs")
  const file = path.join(stubDirectory, name)
  fs.writeFileSync(file, [
    "#!/usr/bin/env node",
    'const fs = require("fs")',
    'const input = fs.readFileSync(0, "utf8")',
    "const SIZES = " + JSON.stringify(CHUNK_UNITS_BY_ENGINE),
    "const DELAY = " + Number(delayMs || 0),
    "const FAIL_ON = " + Number(failOnCall || 0),
    "const COUNTER = " + JSON.stringify(counter),
    "const PACKS = " + JSON.stringify(packs),
    'const named = process.argv.indexOf("--engine")',
    'const engine = named === -1 ? ' + JSON.stringify(DEFAULT_ENGINE) + ' : process.argv[named + 1]',
    "const SIZE = SIZES[engine] || SIZES." + DEFAULT_ENGINE,
    "function bump(file) {",
    "  let count = 0",
    '  try { count = Number(fs.readFileSync(file, "utf8")) || 0 } catch (error) { count = 0 }',
    "  count += 1",
    "  fs.writeFileSync(file, String(count))",
    "  return count",
    "}",
    'if (process.argv[2] === "chunk") {',
    "  bump(PACKS)",
    "  const chunks = []",
    "  for (let start = 0; start < input.length; start += SIZE)",
    "    chunks.push({ start: start, end: Math.min(input.length, start + SIZE) })",
    '  process.stdout.write(JSON.stringify({ contractVersion: 1, chunks: chunks }))',
    "  process.exit(0)",
    "}",
    "const calls = bump(COUNTER)",
    "if (DELAY > 0) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, DELAY)",
    "if (calls === FAIL_ON) {",
    '  process.stdout.write(JSON.stringify({ contractVersion: 1, error: { code: "engine_unavailable", message: "LanguageTool did not answer on 127.0.0.1:8081" } }))',
    "  process.exit(1)",
    "}",
    "if (input.length > SIZE) {",
    '  process.stdout.write(JSON.stringify({ contractVersion: 1, error: { code: "text_too_long", message: "The selection is " + input.length + " units long, over the limit of " + SIZE + "." } }))',
    "  process.exit(1)",
    "}",
    "const issues = []",
    'for (let at = input.indexOf("has"); at !== -1; at = input.indexOf("has", at + 3))',
    '  issues.push({ start: at, end: at + 3, original: "has", fix: "have", reason: "Subject and verb do not agree.", category: "grammar" })',
    'process.stdout.write(JSON.stringify({ contractVersion: 1, engine: engine, elapsedMs: 7, issues: issues }))',
    ""
  ].join("\n"))
  fs.chmodSync(file, 0o755)
  return file
}

// How many Chunk lists that stub has packed, which is what says whether a
// retry resumed the list in hand or asked for a new one.
function packCount(name) {
  try {
    return Number(fs.readFileSync(path.join(stubDirectory, name + ".packs"), "utf8")) || 0
  } catch (error) {
    return 0
  }
}

function chunkCardOf(engineSlug, code, message, hasPartial) {
  return chunkCard(code, {
    engineLabel: labelOf(ENGINE_OPTIONS, engineSlug),
    engineSlug: engineSlug,
    message: message || "",
    hasPartial: hasPartial === true
  })
}

// The chunked Check of spec section 9, driven in the order Overlay.qml drives
// it: one `grammachy chunk` for the selected Engine, then one `grammachy check`
// per Chunk in sequence, every Chunk's spans moved by its own start before they
// merge and are verified against the whole Draft. `cli/tests/overlay_chunks.rs`
// is what keeps Overlay.qml on these same steps.
function runChunked(binary, draft, options) {
  const run = {
    issues: [], index: 0, chunks: [], chunkEngine: "",
    elapsedMs: 0, engine: "", card: null, cancelled: false
  }
  return packChunks(binary, draft, run, options || {})
}

// `Overlay.runChunkList`: the Chunks are packed to the selected Engine's limit,
// and the run remembers which Engine that was.
function packChunks(binary, draft, run, options) {
  const settings = options || {}
  const engine = settings.engine || DEFAULT_ENGINE
  run.chunkEngine = engine
  run.card = null

  const listed = readChunks(
    spawnSync(binary, ["chunk", "--engine", engine], { input: draft, encoding: "utf8" }).stdout)
  if (listed.error) {
    run.card = chunkCardOf(engine, listed.error.code, listed.error.message, run.issues.length > 0)
    return run
  }
  run.chunks = listed.chunks
  run.index = 0
  return resumeChunked(binary, draft, run, settings)
}

// The walk itself, resumed at the Chunk that stopped it. Every Check names the
// Engine the list was packed for, the way `Overlay.runChunk` names
// `runEngine()`: the Chunk was cut to a size, and only that Engine reads it.
// `settings.engine` is the live setting, which the reader can move while the
// walk runs.
function resumeChunked(binary, draft, run, options) {
  const settings = options || {}
  run.card = null
  while (run.index < run.chunks.length) {
    const chunk = run.chunks[run.index]
    const answer = readCheck(spawnSync(binary, ["check", "--engine", run.chunkEngine],
      { input: chunkText(draft, chunk), encoding: "utf8" }).stdout)

    if (answer.error) {
      run.card = chunkCardOf(
        run.chunkEngine, answer.error.code, answer.error.message, run.issues.length > 0)
      return run
    }

    const shifted = shiftIssues(answer.result.issues || [], chunk.start)
    run.issues = mergeIssues(run.issues, verifiedIssues(draft, shifted).issues)
    run.engine = String(answer.result.engine || run.engine)
    run.elapsedMs += Number(answer.result.elapsedMs || 0)
    run.index += 1

    // The reader picks another Engine in the Settings view, which stays
    // reachable while the Check runs.
    if (settings.switchAfter && run.index === settings.switchAfter.chunks)
      settings.engine = settings.switchAfter.to

    // Cancel stops the run after the Chunk in flight, spec section 9.
    if (settings.cancelAfter && run.index >= settings.cancelAfter) {
      run.cancelled = true
      return run
    }
  }
  return run
}

// `Retry remaining` as `Overlay.retryRemaining` drives it. The retry is where a
// new Engine takes effect: a list packed to another size cannot be resumed and
// is packed again, and one of the same size resumes on the new Engine with the
// Issues the finished Chunks found still in hand.
function retryRemaining(binary, draft, run, options) {
  const settings = options || {}
  const engine = settings.engine || DEFAULT_ENGINE
  if (Limits.checkLimit(run.chunkEngine) !== Limits.checkLimit(engine)) {
    run.chunks = []
    run.index = 0
    run.chunkEngine = ""
    run.elapsedMs = 0
    run.issues = []
  } else {
    run.chunkEngine = engine
  }
  if (run.chunks.length === 0) return packChunks(binary, draft, run, settings)
  return resumeChunked(binary, draft, run, settings)
}

test("a whole Draft merges into one list whose spans point at the right text", () => {
  const binary = chunkedStub("chunked-clean", 0, 0)
  const run = runChunked(binary, DRAFT)

  assert.equal(run.card, null)
  assert.equal(run.chunks.length, CHUNK_COUNT)
  assert.equal(run.index, CHUNK_COUNT)
  assert.deepEqual(run.issues.map(issue => issue.start), WANTED_STARTS)
  // The acceptance criterion: every span, the ones from the second Chunk on
  // included, slices the original out of the merged view.
  for (const issue of run.issues) assert.equal(DRAFT.slice(issue.start, issue.end), issue.original)
  assert.ok(run.issues.some(issue => issue.start >= run.chunks[1].start),
    "issues come from later Chunks too")
})

// Spec section 9: Cancel stops after the Chunk in flight and keeps what
// finished. The stub delays so the run is long enough for that to be a choice.
test("Cancel after a Chunk keeps every Issue the finished Chunks found", () => {
  const binary = chunkedStub("chunked-slow", 60, 0)
  const started = Date.now()
  const run = runChunked(binary, DRAFT, { cancelAfter: 2 })

  assert.equal(run.cancelled, true)
  assert.equal(run.index, 2)
  assert.ok(run.index < run.chunks.length, "the run stopped before the last Chunk")
  // The Issues of Chunks 1 and 2, and none of Chunk 3.
  assert.deepEqual(run.issues.map(issue => issue.start), WANTED_STARTS.slice(0, 10))
  for (const issue of run.issues) assert.equal(DRAFT.slice(issue.start, issue.end), issue.original)
  assert.ok(run.issues.every(issue => issue.start < run.chunks[2].start))
  // The Chunks really ran one after another rather than being skipped.
  assert.ok(Date.now() - started >= 120, "each checked Chunk waited on the stub")
})

// Spec section 9: a failed Chunk keeps the Issues from the finished ones, shows
// the engine message, and `Retry remaining` resumes at the Chunk that failed.
test("a Chunk that fails once shows both recoveries and Retry remaining finishes the run", () => {
  const binary = chunkedStub("chunked-fails-once", 0, 3)
  const run = runChunked(binary, DRAFT)

  assert.equal(run.card.code, ENGINE_UNAVAILABLE)
  assert.equal(run.card.message, "LanguageTool did not answer on 127.0.0.1:8081")
  assert.deepEqual(run.card.buttons, [RETRY_REMAINING, REVIEW_PARTIAL])
  assert.equal(run.card.primary, RETRY_REMAINING)
  // The two Chunks before it kept everything they found.
  assert.equal(run.index, 2)
  assert.deepEqual(run.issues.map(issue => issue.start), WANTED_STARTS.slice(0, 10))

  // `Retry remaining` resumes at the Chunk that failed, so nothing before it
  // runs again and nothing after it is skipped.
  const resumed = retryRemaining(binary, DRAFT, run, {})
  assert.equal(resumed.card, null)
  assert.equal(resumed.index, CHUNK_COUNT)
  assert.deepEqual(resumed.issues.map(issue => issue.start), WANTED_STARTS)
  for (const issue of resumed.issues) assert.equal(DRAFT.slice(issue.start, issue.end), issue.original)
})

test("a first Chunk that fails has nothing to review", () => {
  const binary = chunkedStub("chunked-fails-first", 0, 1)
  const run = runChunked(binary, DRAFT)

  assert.equal(run.card.code, ENGINE_UNAVAILABLE)
  assert.deepEqual(run.card.buttons, [CLOSE, RETRY_REMAINING, SETTINGS])
  assert.equal(run.issues.length, 0)
  assert.equal(run.index, 0)

  // Retry remaining starts at the same Chunk, which is the first one.
  const resumed = retryRemaining(binary, DRAFT, run, {})
  assert.equal(resumed.card, null)
  assert.deepEqual(resumed.issues.map(issue => issue.start), WANTED_STARTS)
})

// The limit belongs to the Engine (spec section 4), so a Chunk list packed for
// one Engine is the wrong size for a narrower one. The Settings gear stays
// reachable at the failure, so a reader can pick that narrower Engine before
// `Retry remaining`. Resending the Chunks in hand would answer `text_too_long`
// every time, which no button can get out of.
test("a narrower Engine picked at the failure packs the Draft again instead of resending Chunks it cannot read", () => {
  const binary = chunkedStub("chunked-engine-change", 0, 3)
  const run = runChunked(binary, DRAFT, { engine: "languagetool" })

  assert.equal(run.card.code, ENGINE_UNAVAILABLE)
  assert.equal(run.index, 2)
  assert.equal(run.chunkEngine, "languagetool")
  assert.equal(packCount("chunked-engine-change"), 1)

  // The reader opens Settings at the failure and picks the local engine.
  const retried = retryRemaining(binary, DRAFT, run, { engine: "openai" })

  assert.equal(retried.card, null)
  assert.equal(retried.chunkEngine, "openai")
  assert.equal(packCount("chunked-engine-change"), 2)
  assert.equal(retried.chunks.length, LOCAL_CHUNK_COUNT)
  for (const chunk of retried.chunks)
    assert.ok(chunk.end - chunk.start <= LOCAL_CHUNK_UNITS, "every Chunk fits the local engine")

  // The whole Draft is checked once, so no Issue the first Engine found is
  // reported twice and none of them is lost.
  assert.equal(retried.index, LOCAL_CHUNK_COUNT)
  assert.deepEqual(retried.issues.map(issue => issue.start), WANTED_STARTS)
  for (const issue of retried.issues) assert.equal(DRAFT.slice(issue.start, issue.end), issue.original)
})

// The engine that did not change is the normal case, and it must still resume.
test("Retry remaining on the same Engine resumes the Chunk list in hand", () => {
  const binary = chunkedStub("chunked-same-engine", 0, 3)
  const run = runChunked(binary, DRAFT, { engine: "languagetool" })
  assert.equal(run.index, 2)

  const resumed = retryRemaining(binary, DRAFT, run, { engine: "languagetool" })

  assert.equal(resumed.card, null)
  assert.equal(packCount("chunked-same-engine"), 1, "no second Chunk list was asked for")
  assert.equal(resumed.chunks.length, CHUNK_COUNT)
  assert.equal(resumed.index, CHUNK_COUNT)
  assert.deepEqual(resumed.issues.map(issue => issue.start), WANTED_STARTS)
})

// A Chunk is cut to the size one Engine reads (spec section 4), so the Check
// that reads it has to be that Engine. The Settings view stays reachable while
// the run walks, so a reader can pick a narrower Engine mid-run; the Chunks in
// hand are still the old size, and sending one of them to the new Engine would
// answer text_too_long and blame the engine for a Chunk the shell sized.
test("a run finishes on the Engine its Chunks were packed for when the setting moves mid-run", () => {
  const binary = chunkedStub("chunked-switch-mid-run", 0, 0)
  const live = { engine: "languagetool", switchAfter: { chunks: 1, to: "openai" } }
  const run = runChunked(binary, DRAFT, live)

  // The reader really did change the setting while the walk ran.
  assert.equal(live.engine, "openai")
  assert.notEqual(Limits.checkLimit("openai"), Limits.checkLimit("languagetool"))

  assert.equal(run.card, null, "no Chunk was refused for its size")
  assert.equal(run.chunkEngine, "languagetool")
  // The stub names the engine each Check ran on, so this is the whole claim.
  assert.equal(run.engine, "languagetool")
  assert.equal(run.index, CHUNK_COUNT)
  assert.deepEqual(run.issues.map(issue => issue.start), WANTED_STARTS)
})

// Two Engines that read the same number of units share a Chunk list, so the
// size is what decides whether the retry can resume, not the slug. The retry is
// also where the reader's new Engine takes effect, which is what makes the
// Settings view a recovery from `engine_unavailable`.
test("Retry remaining on an Engine of the same size resumes on it and keeps the partial Issues", () => {
  const binary = chunkedStub("chunked-same-size-engine", 0, 3)
  const run = runChunked(binary, DRAFT, { engine: "languagetool" })

  assert.equal(run.card.code, ENGINE_UNAVAILABLE)
  assert.equal(run.index, 2)
  assert.deepEqual(run.issues.map(issue => issue.start), WANTED_STARTS.slice(0, 10))

  // The reader picks Harper, which reads the same number of units.
  assert.equal(Limits.checkLimit("harper"), Limits.checkLimit("languagetool"))
  const resumed = retryRemaining(binary, DRAFT, run, { engine: "harper" })

  assert.equal(resumed.card, null)
  assert.equal(packCount("chunked-same-size-engine"), 1, "no second Chunk list was asked for")
  assert.equal(resumed.index, CHUNK_COUNT)
  // The two Chunks that finished are still in hand, and the rest ran on the
  // Engine the reader picked.
  assert.deepEqual(resumed.issues.map(issue => issue.start), WANTED_STARTS)
  assert.equal(resumed.chunkEngine, "harper")
  assert.equal(resumed.engine, "harper")
})

test("a chunk step that cannot answer stops the run before any Check", () => {
  const binary = stub("chunk-emits-nothing", "grammachy: not a subcommand\n", 2)
  const run = runChunked(binary, DRAFT)
  assert.equal(run.card.code, BAD_ARGUMENTS)
  assert.equal(run.issues.length, 0)
  assert.equal(run.chunks.length, 0)
})
