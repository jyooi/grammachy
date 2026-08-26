// Node tests for the Models list. Spec sections 5.3, 7, and 13.
// Run with `node --test ui/models.test.js`.
//
// The last block runs stub binaries that print exactly what `grammachy model`
// prints for each verb and for each failure. A stub is the only safe seam here:
// a test must never fetch a weights file, write the real models directory, or
// stop the llama.cpp unit the live shell uses.

const test = require("node:test")
const assert = require("node:assert/strict")
const fs = require("node:fs")
const os = require("node:os")
const path = require("node:path")
const { spawnSync } = require("node:child_process")

const {
  ABSENT,
  PARTIAL,
  READY,
  STATES,
  CANCELLED,
  DOWNLOAD_FAILED,
  BAD_ARGUMENTS,
  DOWNLOAD,
  CANCEL,
  USE,
  REMOVE,
  ACTION_ICONS,
  stateOf,
  read,
  rows,
  merged,
  bytes,
  share,
  hint,
  actions,
  isBlocked,
  actionIcon,
  actionTooltip,
  note
} = require("./models.js")

// The three catalogue rows, as `grammachy model list` prints them.
const GEMMA = {
  name: "gemma-4-e4b-it",
  fileName: "gemma-4-E4B-it-Q4_K_M.gguf",
  state: "ready",
  partialBytes: 0,
  sizeBytes: 4977171584,
  licence: "Gemma Terms of Use"
}
const QWEN = {
  name: "qwen3-4b-instruct",
  fileName: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
  state: "partial",
  partialBytes: 1248640560,
  sizeBytes: 2497281120,
  licence: "Apache-2.0"
}
const PHI = {
  name: "phi-4-mini-instruct",
  fileName: "Phi-4-mini-instruct-Q4_K_M.gguf",
  state: "absent",
  partialBytes: 0,
  sizeBytes: 2491874272,
  licence: "MIT"
}

function envelope(verb, models) {
  return JSON.stringify({
    contractVersion: 1,
    verb: verb,
    directory: "/home/u/.local/share/grammachy/models",
    freeBytes: 700000000000,
    models: models
  })
}

// ------------------------------------------------------------- reading stdout

test("a report envelope is not an error and carries every row", () => {
  const answer = read(envelope("list", [GEMMA, QWEN, PHI]))

  assert.equal(answer.error, null)
  assert.equal(answer.report.verb, "list")
  assert.equal(answer.report.directory, "/home/u/.local/share/grammachy/models")
  assert.equal(answer.report.freeBytes, 700000000000)
  assert.deepEqual(answer.report.models.map(row => row.name),
    ["gemma-4-e4b-it", "qwen3-4b-instruct", "phi-4-mini-instruct"])
  assert.deepEqual(answer.report.models.map(row => row.state), [READY, PARTIAL, ABSENT])
  assert.equal(answer.report.models[1].partialBytes, 1248640560)
  assert.equal(answer.report.models[1].licence, "Apache-2.0")
})

test("an error envelope carries its code and message through", () => {
  const stdout = JSON.stringify({
    contractVersion: 1,
    error: { code: "download_failed", message: "The downloaded file does not match the pinned digest." }
  })
  assert.deepEqual(read(stdout).error, {
    code: DOWNLOAD_FAILED,
    message: "The downloaded file does not match the pinned digest."
  })
  assert.equal(read(stdout).report, null)
})

// Spec section 5.3, the same rule section 5.1 fixes for a Check: no JSON on
// stdout is the companion tool being missing or out of date.
test("no JSON on stdout reads as bad_arguments", () => {
  for (const stdout of ["", "\n", "grammachy: command not found", "not json at all", "[]", "null"]) {
    assert.deepEqual(read(stdout).error, { code: BAD_ARGUMENTS, message: "" })
  }
})

test("a missing or mismatched contractVersion reads as bad_arguments", () => {
  const missing = JSON.stringify({ verb: "list", models: [] })
  const mismatched = JSON.stringify({ contractVersion: 2, verb: "list", models: [] })
  for (const stdout of [missing, mismatched]) {
    assert.deepEqual(read(stdout).error, { code: BAD_ARGUMENTS, message: "" })
  }
})

test("a report whose models are not a list reads as bad_arguments", () => {
  const stdout = JSON.stringify({ contractVersion: 1, verb: "list", models: "three" })
  assert.deepEqual(read(stdout).error, { code: BAD_ARGUMENTS, message: "" })
})

test("an empty catalogue is a report with no rows, not an error", () => {
  const answer = read(envelope("list", []))
  assert.equal(answer.error, null)
  assert.deepEqual(answer.report.models, [])
})

// ------------------------------------------------------------------- the rows

test("a row without a name is dropped, because nothing can be done with it", () => {
  const built = rows([GEMMA, { state: "ready" }, { name: "", state: "ready" }, PHI])
  assert.deepEqual(built.map(row => row.name), ["gemma-4-e4b-it", "phi-4-mini-instruct"])
})

test("an unknown state reads as absent", () => {
  for (const state of ["downloading", "", undefined, 3, null]) {
    assert.equal(stateOf({ name: "x", state: state }), ABSENT)
  }
  for (const state of STATES) assert.equal(stateOf({ name: "x", state: state }), state)
  assert.equal(stateOf(null), ABSENT)
})

test("a missing count reads as zero rather than as NaN", () => {
  const built = rows([{ name: "x", state: "partial" }])
  assert.equal(built[0].partialBytes, 0)
  assert.equal(built[0].sizeBytes, 0)
  assert.equal(built[0].licence, "")
})

// A `download` or a `remove` answers with the one row it acted on, so the list
// on screen keeps every other row.
test("one answered row replaces its own row and leaves the rest alone", () => {
  const list = rows([GEMMA, QWEN, PHI])
  const answered = rows([{ ...QWEN, state: "ready", partialBytes: 0 }])

  const next = merged(list, answered)

  assert.equal(next.length, 3)
  assert.deepEqual(next.map(row => row.name), list.map(row => row.name))
  assert.equal(next[1].state, READY)
  assert.equal(next[0].state, READY)
  assert.equal(next[2].state, ABSENT)
})

test("a name the list does not carry is appended", () => {
  const next = merged(rows([GEMMA]), rows([PHI]))
  assert.deepEqual(next.map(row => row.name), ["gemma-4-e4b-it", "phi-4-mini-instruct"])
})

// ---------------------------------------------------------------- the numbers

test("a byte count is the size a reader compares at a glance", () => {
  assert.equal(bytes(0), "0 B")
  assert.equal(bytes(512), "512 B")
  assert.equal(bytes(1024), "1.0 KB")
  assert.equal(bytes(2491874272), "2.3 GB")
  assert.equal(bytes(4977171584), "4.6 GB")
})

test("a byte count is never negative and never NaN", () => {
  assert.equal(bytes(-5), "0 B")
  assert.equal(bytes(undefined), "0 B")
  assert.equal(bytes("nonsense"), "0 B")
})

test("the progress share is what the shell polls model list for", () => {
  assert.equal(share(QWEN), 0.5)
  assert.equal(share(GEMMA), 1)
  assert.equal(share(PHI), 0)
  // A row with no pinned size cannot be measured.
  assert.equal(share({ name: "x", state: "partial", partialBytes: 10, sizeBytes: 0 }), 0)
  // A part file larger than the pin is still a full bar rather than an overrun.
  assert.equal(share({ name: "x", state: "partial", partialBytes: 30, sizeBytes: 10 }), 1)
})

// ----------------------------------------------------------------- the hints

test("a hint names the licence and the size, which is what the reader chooses between", () => {
  assert.equal(hint(GEMMA, false), "Ready, 4.6 GB, Gemma Terms of Use")
  assert.equal(hint(PHI, false), "Not downloaded, 2.3 GB, MIT")
  assert.equal(hint(QWEN, false), "Part downloaded, 1.2 GB of 2.3 GB, Apache-2.0")
})

test("the row a download is running on names the progress instead", () => {
  assert.equal(hint(QWEN, true), "Downloading 1.2 GB of 2.3 GB, 50%")
})

test("a row with no licence says so rather than saying nothing", () => {
  assert.equal(hint({ name: "x", state: "absent", sizeBytes: 1024 }, false),
    "Not downloaded, 1.0 KB, unknown licence")
})

// --------------------------------------------------------------- the buttons

test("an absent row offers Download alone", () => {
  assert.deepEqual(actions(PHI, { busy: "", setting: "gemma-4-e4b-it" }), [DOWNLOAD])
})

test("a part downloaded row offers Download and Remove", () => {
  assert.deepEqual(actions(QWEN, { busy: "", setting: "gemma-4-e4b-it" }), [DOWNLOAD, REMOVE])
})

test("a ready row the setting does not name offers Use and Remove", () => {
  assert.deepEqual(actions(GEMMA, { busy: "", setting: "qwen3-4b-instruct" }), [USE, REMOVE])
})

// Picking the model that is already picked does nothing, so the button is not
// offered at all.
test("the ready row the setting already names offers Remove alone", () => {
  assert.deepEqual(actions(GEMMA, { busy: "", setting: "gemma-4-e4b-it" }), [REMOVE])
})

// One download at a time, spec section 7.
test("the row in flight offers Cancel alone", () => {
  assert.deepEqual(actions(QWEN, { busy: "qwen3-4b-instruct", setting: "gemma-4-e4b-it" }), [CANCEL])
})

test("every other row's Download is off while one download runs", () => {
  assert.deepEqual(actions(PHI, { busy: "qwen3-4b-instruct", setting: "gemma-4-e4b-it" }), [])
  assert.equal(isBlocked(PHI, { busy: "qwen3-4b-instruct" }), true)
  assert.equal(isBlocked(PHI, { busy: "" }), false)
  // A ready row has no download to wait for, so its buttons stay live.
  assert.deepEqual(actions(GEMMA, { busy: "qwen3-4b-instruct", setting: "qwen3-4b-instruct" }),
    [USE, REMOVE])
  assert.equal(isBlocked(GEMMA, { busy: "qwen3-4b-instruct" }), false)
})

// Spec section 7: the row buttons are icons only, so the verb is the tooltip
// and the name is in the hint line.
test("every action has an icon and a tooltip that names the verb", () => {
  for (const action of [DOWNLOAD, CANCEL, USE, REMOVE]) {
    assert.ok(actionIcon(action).length > 0, action + " has an icon")
    assert.ok(actionTooltip(action).length > 0, action + " has a tooltip")
  }
  assert.equal(actionIcon("nothing"), "")
  assert.equal(actionTooltip("nothing"), "")
  assert.equal(actionTooltip(DOWNLOAD, "phi-4-mini-instruct"), "Download phi-4-mini-instruct")
  assert.equal(Object.keys(ACTION_ICONS).length, 4)
})

// ----------------------------------------------------------------- the notes

test("a cancel says the part file is kept, because a cancel is not a failure", () => {
  const line = note(CANCELLED, "The download of qwen3-4b-instruct stopped.", "qwen3-4b-instruct")
  assert.equal(line.title, "Download of qwen3-4b-instruct stopped")
  assert.ok(line.body.includes("kept"))
  assert.equal(line.message, "")
})

test("a failed download names what the CLI said", () => {
  const line = note(DOWNLOAD_FAILED, "curl could not fetch it", "phi-4-mini-instruct")
  assert.equal(line.title, "phi-4-mini-instruct could not be downloaded")
  assert.equal(line.message, "curl could not fetch it")
})

test("no JSON at all is the companion tool being missing or out of date", () => {
  const line = note(BAD_ARGUMENTS, "", "")
  assert.ok(line.title.includes("model list"))
  assert.ok(line.body.includes("out of date"))
})

test("any other failure says the models on disk did not change", () => {
  const line = note(BAD_ARGUMENTS, "no such model", "no-such-model")
  assert.ok(line.body.includes("did not change"))
  assert.equal(line.message, "no such model")
})

// ------------------------------------------- a whole run against a stub binary

let stubDirectory = ""

test.before(() => {
  stubDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "grammachy-models-"))
})

test.after(() => {
  if (stubDirectory) fs.rmSync(stubDirectory, { recursive: true, force: true })
})

// A stub that answers all three verbs off one JSON file, the way the real
// binary answers off the models directory. `download` walks the part file
// forward one step per call, which is what the shell polls `list` to see.
function stub(name) {
  const state = path.join(stubDirectory, name + ".json")
  fs.writeFileSync(state, JSON.stringify([GEMMA, { ...QWEN, state: "absent", partialBytes: 0 }, PHI]))
  const file = path.join(stubDirectory, name)
  fs.writeFileSync(file, [
    "#!/usr/bin/env node",
    'const fs = require("fs")',
    "const STATE = " + JSON.stringify(state),
    'const models = JSON.parse(fs.readFileSync(STATE, "utf8"))',
    'const verb = process.argv[3]',
    'const wanted = process.argv[4]',
    "function report(verb, rows) {",
    '  process.stdout.write(JSON.stringify({ contractVersion: 1, verb: verb,',
    '    directory: "/home/u/.local/share/grammachy/models", freeBytes: 700000000000, models: rows }))',
    "  fs.writeFileSync(STATE, JSON.stringify(models))",
    "  process.exit(0)",
    "}",
    'if (verb === "list") report("list", models)',
    "const row = models.find(function (m) { return m.name === wanted })",
    "if (!row) {",
    '  process.stdout.write(JSON.stringify({ contractVersion: 1, error: { code: "bad_arguments", message: wanted + " is not a model." } }))',
    "  process.exit(1)",
    "}",
    'if (verb === "remove") {',
    '  row.state = "absent"',
    "  row.partialBytes = 0",
    '  report("remove", [row])',
    "}",
    "// One step of the transfer per call, so the poll sees it move.",
    "row.partialBytes = Math.min(row.sizeBytes, row.partialBytes + row.sizeBytes / 4)",
    'row.state = row.partialBytes >= row.sizeBytes ? "ready" : "partial"',
    'if (row.state === "ready") row.partialBytes = 0',
    'report("download", [row])',
    ""
  ].join("\n"))
  fs.chmodSync(file, 0o755)
  return file
}

// One verb, read back the way Overlay.qml reads it.
function runModel(binary, args) {
  const run = spawnSync(binary, ["model"].concat(args), { encoding: "utf8" })
  assert.equal(run.error, undefined)
  return read(run.stdout)
}

test("a list from a real process becomes the rows the view draws", () => {
  const binary = stub("list")

  const answer = runModel(binary, ["list"])

  assert.equal(answer.error, null)
  assert.deepEqual(answer.report.models.map(row => row.state), [READY, ABSENT, ABSENT])
  assert.equal(hint(answer.report.models[0], false), "Ready, 4.6 GB, Gemma Terms of Use")
})

// The acceptance criterion of this ticket: the bar moves while a download runs,
// and the row is Ready only once the whole file is there.
test("a download run to the end moves the bar and lands on Ready", () => {
  const binary = stub("download")
  const seen = []
  let list = runModel(binary, ["list"]).report.models

  for (let step = 0; step < 4; step++) {
    const answer = runModel(binary, ["download", "qwen3-4b-instruct"])
    assert.equal(answer.error, null)
    list = merged(list, answer.report.models)
    const row = list.find(row => row.name === "qwen3-4b-instruct")
    seen.push(share(row))
    // Every poll of `list` sees the same row the download answered.
    const polled = runModel(binary, ["list"]).report.models
      .find(row => row.name === "qwen3-4b-instruct")
    assert.equal(polled.state, row.state)
    assert.equal(polled.partialBytes, row.partialBytes)
  }

  assert.deepEqual(seen, [0.25, 0.5, 0.75, 1])
  const row = list.find(row => row.name === "qwen3-4b-instruct")
  assert.equal(row.state, READY)
  assert.equal(row.partialBytes, 0)
  // The other two rows were never touched by the download.
  assert.equal(list.length, 3)
  assert.equal(list[0].state, READY)
  assert.equal(list[2].state, ABSENT)
})

test("a part downloaded row offers Download again, and it resumes", () => {
  const binary = stub("resume")
  runModel(binary, ["download", "qwen3-4b-instruct"])
  const row = runModel(binary, ["list"]).report.models
    .find(row => row.name === "qwen3-4b-instruct")

  assert.equal(row.state, PARTIAL)
  assert.deepEqual(actions(row, { busy: "", setting: "gemma-4-e4b-it" }), [DOWNLOAD, REMOVE])

  const resumed = runModel(binary, ["download", "qwen3-4b-instruct"]).report.models[0]
  assert.ok(resumed.partialBytes > row.partialBytes, "the second run carried on")
})

test("a remove from a real process leaves the row absent", () => {
  const binary = stub("remove")

  const answer = runModel(binary, ["remove", "gemma-4-e4b-it"])

  assert.equal(answer.error, null)
  assert.equal(answer.report.verb, "remove")
  assert.equal(answer.report.models[0].state, ABSENT)
  const list = runModel(binary, ["list"]).report.models
  assert.equal(list[0].state, ABSENT)
  assert.deepEqual(actions(list[0], { busy: "", setting: "gemma-4-e4b-it" }), [DOWNLOAD])
})

test("a name the catalogue does not carry comes back as a note", () => {
  const binary = stub("unknown")

  const answer = runModel(binary, ["download", "no-such-model"])

  assert.equal(answer.report, null)
  assert.equal(answer.error.code, BAD_ARGUMENTS)
  const line = note(answer.error.code, answer.error.message, "no-such-model")
  assert.equal(line.message, "no-such-model is not a model.")
})

test("a binary that is not there at all is one note and no rows", () => {
  const run = spawnSync(path.join(stubDirectory, "no-such-binary"), ["model", "list"],
    { encoding: "utf8" })
  const answer = read(run.stdout || "")

  assert.equal(answer.report, null)
  assert.equal(answer.error.code, BAD_ARGUMENTS)
  assert.ok(note(answer.error.code, answer.error.message, "").body.includes("out of date"))
})
