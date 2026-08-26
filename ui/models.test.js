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
  NOTICE,
  FAILURE,
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
  partialOf,
  sameRows,
  hint,
  resolvedName,
  resolves,
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

// The three rows as one list, which is what the view and the overlay hand the
// shared functions so a setting can be resolved against the files on disk.
const CATALOGUE = [GEMMA, QWEN, PHI]

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

// ------------------------------------------------- what a poll answer changes

// A QML Repeater rebuilds every delegate the moment its array is replaced, and
// the poll answers once a second. So the list may only be replaced when it
// really says something new: otherwise the progress bar restarts its animation
// instead of advancing, an open tooltip goes, and a press whose release lands
// after the rebuild never becomes a click.
test("two reads of the same stdout say the same thing", () => {
  const stdout = envelope("list", [GEMMA, QWEN, PHI])

  const first = read(stdout).report.models
  const second = read(stdout).report.models

  // Fresh objects every time, so identity cannot be the test.
  assert.notEqual(first, second)
  assert.equal(sameRows(first, second), true)
  assert.equal(sameRows(first, second, "qwen3-4b-instruct"), true)
})

test("a poll whose part file grew says something new", () => {
  const before = read(envelope("list", [GEMMA, QWEN, PHI])).report.models
  const after = read(envelope("list", [
    GEMMA,
    { ...QWEN, partialBytes: QWEN.partialBytes + 4096 },
    PHI
  ])).report.models

  assert.equal(sameRows(before, after), false)
})

// That one number is the only thing the poll is there to move, and the bar
// reads it from its own property, so it is not a reason to rebuild the list.
test("the row in flight may move its part length without rebuilding the list", () => {
  const before = read(envelope("list", [GEMMA, QWEN, PHI])).report.models
  const after = read(envelope("list", [
    GEMMA,
    { ...QWEN, partialBytes: QWEN.partialBytes + 4096 },
    PHI
  ])).report.models

  assert.equal(sameRows(before, after, "qwen3-4b-instruct"), true)
  // Another row moving is still a rebuild, because no bar is reading it.
  assert.equal(sameRows(before, after, "phi-4-mini-instruct"), false)
})

test("a row that finished, appeared, or went is always something new", () => {
  const list = read(envelope("list", [GEMMA, QWEN, PHI])).report.models
  const moving = "qwen3-4b-instruct"

  const finished = read(envelope("list", [GEMMA, { ...QWEN, state: "ready" }, PHI])).report.models
  assert.equal(sameRows(list, finished, moving), false)

  const shorter = read(envelope("list", [GEMMA, QWEN])).report.models
  assert.equal(sameRows(list, shorter, moving), false)

  const renamed = read(envelope("list", [GEMMA, QWEN, { ...PHI, name: "other" }])).report.models
  assert.equal(sameRows(list, renamed, moving), false)
})

// The moving byte count lives beside the list, so the overlay has to be able to
// pick it out of one answer.
test("the part length of the row in flight is read off the answer", () => {
  const list = read(envelope("list", [GEMMA, QWEN, PHI])).report.models

  assert.equal(partialOf(list, "qwen3-4b-instruct"), QWEN.partialBytes)
  assert.equal(partialOf(list, "phi-4-mini-instruct"), 0)
  assert.equal(partialOf(list, "no-such-model"), 0)
  assert.equal(partialOf(list, ""), 0)
  assert.equal(partialOf(null, "qwen3-4b-instruct"), 0)
})

// The bar and the hint of the running row read that live count rather than the
// list, which is what keeps the list still while the number moves.
test("a live byte count overrides the one the list carries", () => {
  const half = QWEN.sizeBytes / 2

  assert.equal(share(QWEN, half + QWEN.sizeBytes / 4), 0.75)
  assert.equal(hint(QWEN, true, QWEN.sizeBytes / 4), "Downloading 595.4 MB of 2.3 GB, 25%")

  // No live count, or a negative one, leaves the list as the only answer.
  assert.equal(share(QWEN, -1), 0.5)
  assert.equal(share(QWEN, undefined), 0.5)
  assert.equal(hint(QWEN, true, -1), "Downloading 1.2 GB of 2.3 GB, 50%")
})

// A Download pressed on a part downloaded row resumes it, so the bar has to
// carry on from where the earlier cancel left it. The running row reads the
// live count rather than the list, so the live count starts at what the list
// already holds: seeding it with 0 would animate the bar down to empty and
// back up again a second later.
test("a resumed download starts its bar where the part file already is", () => {
  const half = { ...QWEN, state: "partial", partialBytes: QWEN.sizeBytes / 2 }
  const list = read(envelope("list", [GEMMA, half, PHI])).report.models
  const row = list.find(row => row.name === "qwen3-4b-instruct")

  // Before the press the row is not running, so the list is the only answer.
  assert.equal(share(row, -1), 0.5)

  // Pressing Download seeds the live count off the row the user pressed.
  const seeded = partialOf(list, "qwen3-4b-instruct")
  assert.equal(seeded, QWEN.sizeBytes / 2)
  assert.equal(share(row, seeded), 0.5)
  assert.equal(hint(row, true, seeded), "Downloading 1.2 GB of 2.3 GB, 50%")

  // The first poll lands a second later, and the bar only ever moves forward.
  const polled = partialOf(
    read(envelope("list", [
      GEMMA,
      { ...half, partialBytes: half.partialBytes + 4096 },
      PHI
    ])).report.models,
    "qwen3-4b-instruct"
  )
  assert.ok(share(row, polled) >= share(row, seeded), "the share never goes backwards")
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
  assert.deepEqual(actions(GEMMA, { busy: "", setting: "qwen3-4b-instruct", models: CATALOGUE }),
    [USE, REMOVE])
})

// Picking the model that is already picked does nothing, so the button is not
// offered at all.
test("the ready row the setting already names offers Remove alone", () => {
  assert.deepEqual(actions(GEMMA, { busy: "", setting: "gemma-4-e4b-it", models: CATALOGUE }),
    [REMOVE])
})

// One download at a time, spec section 7.
test("the row in flight offers Cancel alone", () => {
  assert.deepEqual(actions(QWEN, { busy: "qwen3-4b-instruct", setting: "gemma-4-e4b-it" }), [CANCEL])
})

// Only one verb of `grammachy model` runs at a time, so a button on any other
// row would be a dead click. It stays drawn and goes dim rather than vanishing,
// which is what keeps the list from shifting under the pointer.
test("every other row's buttons are blocked while one download runs", () => {
  const busy = {
    working: true,
    busy: "qwen3-4b-instruct",
    setting: "gemma-4-e4b-it",
    models: CATALOGUE
  }

  assert.deepEqual(actions(PHI, busy), [DOWNLOAD])
  assert.equal(isBlocked(PHI, busy), true)

  // A Ready row's Remove and Use are just as dead, so they are dimmed too.
  assert.deepEqual(actions(GEMMA, busy), [REMOVE])
  assert.equal(isBlocked(GEMMA, busy), true)

  // The row the download belongs to keeps its Cancel live.
  assert.deepEqual(actions(QWEN, busy), [CANCEL])
  assert.equal(isBlocked(QWEN, busy), false)

  // Nothing is blocked when nothing is running.
  for (const row of CATALOGUE) assert.equal(isBlocked(row, { working: false, busy: "" }), false)
})

// A remove names no row the way a download does: it takes the one process with
// `busy` still empty. Keying the rule on the states rather than on the one fact
// is what left every row drawn live while every press on it was dropped.
test("every row is blocked while a remove is in flight, not only a download", () => {
  const removing = { working: true, busy: "", setting: "gemma-4-e4b-it", models: CATALOGUE }

  for (const row of CATALOGUE) {
    assert.equal(isBlocked(row, removing), true, row.name + " waits for the verb")
  }
  // The buttons stay drawn, so the list does not shift while the verb runs.
  assert.deepEqual(actions(PHI, removing), [DOWNLOAD])
  assert.deepEqual(actions(GEMMA, removing), [REMOVE])

  // The verb finishing brings every row back.
  const done = { ...removing, working: false }
  for (const row of CATALOGUE) assert.equal(isBlocked(row, done), false)
  assert.deepEqual(actions(PHI, done), [DOWNLOAD])
})

// One question at a time. A Download started under an open Remove confirm would
// take the one process the answer needs, so the answer would vanish with no
// note and nothing deleted. Refusing to start it is the whole rule, and the
// confirm reaches this rule through the same `working` fact a verb does.
test("an open Remove confirm blocks every row until it is answered", () => {
  const asking = { working: true, busy: "", setting: "gemma-4-e4b-it", models: CATALOGUE }

  for (const row of CATALOGUE) {
    assert.equal(isBlocked(row, asking), true, row.name + " waits for the answer")
  }
  // The button stays drawn, so the list does not shift while the question is up.
  assert.deepEqual(actions(PHI, asking), [DOWNLOAD])

  // Answering the question closes it, and the same row is offered Download again.
  const answered = { ...asking, working: false }
  assert.equal(isBlocked(PHI, answered), false)
  assert.deepEqual(actions(PHI, answered), [DOWNLOAD])
})

// ------------------------------------------------- which row the setting names

// The CLI is the authority: `unit::model_file` takes the exact `<name>.gguf`
// first and then any `.gguf` that begins with the name, ignoring case. The
// shell has to agree, or it marks the wrong row and skips the Remove confirm.
const READY_CATALOGUE = [
  GEMMA,
  { ...QWEN, state: "ready", partialBytes: 0 },
  { ...PHI, state: "ready" }
]

test("a setting that reaches a file by prefix resolves to that row", () => {
  assert.equal(resolvedName(READY_CATALOGUE, "qwen3-4b"), "qwen3-4b-instruct")
  assert.equal(resolvedName(READY_CATALOGUE, "Qwen3-4B-Instruct-2507-Q4_K_M"), "qwen3-4b-instruct")
  assert.equal(resolves(READY_CATALOGUE[1], "qwen3-4b", READY_CATALOGUE), true)
})

test("the setting is matched without regard to case, the way the CLI matches it", () => {
  assert.equal(resolvedName(READY_CATALOGUE, "Gemma-4-E4B-it"), "gemma-4-e4b-it")
  assert.equal(resolves(GEMMA, "Gemma-4-E4B-it", READY_CATALOGUE), true)
  assert.equal(resolves(GEMMA, "GEMMA-4-E4B-IT", READY_CATALOGUE), true)
})

test("a setting that reaches nothing resolves to no row at all", () => {
  assert.equal(resolvedName(READY_CATALOGUE, "llama-3-8b"), "")
  assert.equal(resolvedName(READY_CATALOGUE, ""), "")
  assert.equal(resolvedName([], "gemma-4-e4b-it"), "")
  for (const row of READY_CATALOGUE) assert.equal(resolves(row, "llama-3-8b", READY_CATALOGUE), false)
})

// Only a `ready` row has a `.gguf` on disk, so a part downloaded row is not
// what a Check would load and the Remove confirm must not claim it is.
test("a row that is not ready is never the model in use", () => {
  assert.equal(resolvedName([QWEN], "qwen3-4b-instruct"), "")
  assert.equal(resolves(QWEN, "qwen3-4b-instruct", [QWEN]), false)
})

// The CLI sorts the matching files and takes the first, so exactly one row is
// ever marked even when several could match.
test("only one row is marked when several files begin with the setting", () => {
  const shared = [
    { ...QWEN, name: "qwen3-4b-thinking", fileName: "Qwen3-4B-Thinking-Q4_K_M.gguf", state: "ready" },
    { ...QWEN, name: "qwen3-4b-instruct", state: "ready" }
  ]

  const marked = shared.filter(row => resolves(row, "qwen3-4b", shared))

  assert.equal(marked.length, 1)
  assert.equal(marked[0].name, "qwen3-4b-instruct")
})

// The exact file wins over any prefix, which is the first thing the CLI checks.
test("the exact file name wins over a row that only begins with the setting", () => {
  const both = [
    { ...QWEN, name: "qwen3-4b-instruct", state: "ready" },
    { ...PHI, name: "hand-placed", fileName: "Qwen3-4B.gguf", state: "ready" }
  ]

  assert.equal(resolvedName(both, "Qwen3-4B"), "hand-placed")
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
  // A cancel is the reader's own decision, so it is not drawn as a failure.
  assert.equal(line.kind, NOTICE)
})

test("a failed download names what the CLI said", () => {
  const line = note(DOWNLOAD_FAILED, "curl could not fetch it", "phi-4-mini-instruct")
  assert.equal(line.title, "phi-4-mini-instruct could not be downloaded")
  assert.equal(line.message, "curl could not fetch it")
  assert.equal(line.kind, FAILURE)
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
  assert.equal(line.kind, FAILURE)
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
