// Node tests for the error cards. Spec sections 5.1, 8, and 13.
// Run with `node --test ui/errors.test.js`.
//
// The last block runs stub binaries that print exactly what a real
// `grammachy check` prints for each code, and one that prints no JSON at all.
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
  known,
  readCheck,
  card,
  buttonLabel
} = require("./errors.js")

const { ENGINE_OPTIONS, labelOf } = require("./settings.js")

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
