// Node tests for the Engines list. Spec sections 5.4, 7, and 13. HUF-237.
// Run with `node --test ui/engines.test.js`.
//
// The last block runs stub binaries that print exactly what `grammachy engine`
// prints for each verb and for each failure. A stub is the only safe seam here:
// a test must never fetch a 250 MB release, write the real engines directory,
// or stop the LanguageTool unit the live shell uses.

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
  INSTALL,
  CANCEL,
  REMOVE,
  ACTION_ICONS,
  stateOf,
  read,
  rows,
  merged,
  absorbed,
  bytes,
  share,
  partialOf,
  sameRows,
  hint,
  isAvailable,
  unavailable,
  actions,
  isBlocked,
  actionIcon,
  actionTooltip,
  note
} = require("./engines.js")

const { ENGINE_OPTIONS, engineOptions, engineAfterRemoval, BUILT_IN_ENGINE } = require("./settings.js")

// The one catalogue row, as `grammachy engine list` prints it.
const ABSENT_ROW = {
  slug: "languagetool",
  name: "LanguageTool",
  version: "6.6",
  state: "absent",
  partialBytes: 0,
  sizeBytes: 251998221,
  licence: "LGPL-2.1-or-later",
  needsJava: true,
  path: "",
  fromPackage: false
}
const READY_ROW = {
  ...ABSENT_ROW,
  state: "ready",
  path: "/home/u/.local/share/grammachy/engines/languagetool"
}
const PARTIAL_ROW = { ...ABSENT_ROW, state: "partial", partialBytes: 125999110 }
const PACKAGE_ROW = { ...ABSENT_ROW, path: "/usr/bin/languagetool", fromPackage: true }

function envelope(verb, engines) {
  return JSON.stringify({
    contractVersion: 1,
    verb: verb,
    directory: "/home/u/.local/share/grammachy/engines",
    freeBytes: 700000000000,
    engines: engines
  })
}

// ------------------------------------------------------------- reading stdout

test("a report becomes the rows the view draws", () => {
  const answer = read(envelope("list", [ABSENT_ROW]))

  assert.equal(answer.error, null)
  assert.equal(answer.report.verb, "list")
  assert.equal(answer.report.directory, "/home/u/.local/share/grammachy/engines")
  assert.deepEqual(answer.report.engines, [ABSENT_ROW])
})

test("an error envelope becomes a code and a message", () => {
  const answer = read(JSON.stringify({
    contractVersion: 1,
    error: { code: "download_failed", message: "curl could not fetch it." }
  }))

  assert.equal(answer.report, null)
  assert.equal(answer.error.code, DOWNLOAD_FAILED)
  assert.equal(answer.error.message, "curl could not fetch it.")
})

// A companion tool that is missing or out of date leaves nothing the shell can
// trust, so both read as the same refusal.
test("stdout that is not the contract reads as bad arguments", () => {
  for (const stdout of ["", "not json", JSON.stringify({ contractVersion: 2, engines: [] }),
    JSON.stringify({ contractVersion: 1 })]) {
    const answer = read(stdout)
    assert.equal(answer.report, null, stdout)
    assert.equal(answer.error.code, BAD_ARGUMENTS, stdout)
  }
})

test("a row without a slug is dropped, and every field has a type", () => {
  const drawn = rows([{ name: "No slug" }, { slug: "languagetool" }, ABSENT_ROW])

  assert.equal(drawn.length, 2)
  assert.equal(drawn[0].slug, "languagetool")
  assert.equal(drawn[0].name, "languagetool", "a row with no name falls back to its slug")
  assert.equal(drawn[0].state, ABSENT)
  assert.equal(drawn[0].needsJava, false)
  assert.equal(drawn[0].fromPackage, false)
  assert.deepEqual(drawn[1], ABSENT_ROW)
})

test("an unknown state reads as absent, so Install is still offered", () => {
  assert.equal(stateOf({ state: "installing" }), ABSENT)
  assert.equal(stateOf(null), ABSENT)
  assert.deepEqual(STATES, [ABSENT, PARTIAL, READY])
})

// ------------------------------------------------------------------ the list

test("install and remove answer one row, which is merged into the list", () => {
  const settled = absorbed([ABSENT_ROW], read(envelope("install", [READY_ROW])).report)

  assert.equal(settled.engines.length, 1)
  assert.equal(settled.engines[0].state, READY)
  assert.equal(settled.freeBytes, 700000000000)
})

// A poll that fired while the install was hashing lands afterwards still
// calling the row `partial`, so the older run loses.
test("a list answer older than the verb that already spoke is dropped", () => {
  const report = read(envelope("list", [PARTIAL_ROW])).report

  assert.equal(absorbed([READY_ROW], report, 3, 5), null)
  assert.notEqual(absorbed([READY_ROW], report, 6, 5), null)
})

test("the list is only replaced when it says something new", () => {
  assert.ok(sameRows([ABSENT_ROW], [{ ...ABSENT_ROW }]))
  assert.ok(!sameRows([ABSENT_ROW], [READY_ROW]))
  // The one number the poll is there to move rides beside the list, so a
  // change to it alone never rebuilds the row.
  assert.ok(sameRows([PARTIAL_ROW], [{ ...PARTIAL_ROW, partialBytes: 9 }], "languagetool"))
  assert.ok(!sameRows([PARTIAL_ROW], [{ ...PARTIAL_ROW, partialBytes: 9 }], "other"))
})

test("the moving part length is read out of the list by slug", () => {
  assert.equal(partialOf([PARTIAL_ROW], "languagetool"), 125999110)
  assert.equal(partialOf([PARTIAL_ROW], "harper"), 0)
  assert.equal(partialOf([PARTIAL_ROW], ""), 0)
})

// -------------------------------------------------------------- what it says

test("a byte count is the unit a reader compares at a glance", () => {
  assert.equal(bytes(251998221), "240.3 MB")
  assert.equal(bytes(0), "0 B")
  assert.equal(bytes(1024), "1.0 KB")
})

test("progress is what the part file holds against the pinned size", () => {
  assert.equal(share(ABSENT_ROW), 0)
  assert.equal(Math.round(share(PARTIAL_ROW) * 100), 50)
  assert.equal(share(READY_ROW), 1)
  // A live count from the poll wins over the list it came beside.
  assert.equal(share(ABSENT_ROW, 251998221 / 4), 0.25)
})

// The whole cost of the button is on screen before it is pressed: the download
// size and the runtime this install cannot put in place itself.
test("the hint names the size and the Java requirement while it is absent", () => {
  assert.equal(hint(ABSENT_ROW, false), "About 240.3 MB, needs Java, LGPL-2.1-or-later")
  assert.equal(hint(PARTIAL_ROW, false),
    "Part downloaded, 120.2 MB of 240.3 MB, LGPL-2.1-or-later, needs Java")
  assert.equal(hint(READY_ROW, false), "Installed, LGPL-2.1-or-later, needs Java")
  assert.equal(hint(ABSENT_ROW, true, 251998221 / 2), "Downloading 120.2 MB of 240.3 MB, 50%")
})

// A component the pacman package supplies is reachable, and Remove here would
// not take it off the machine, so the line says where it came from.
test("a package supplied row says so rather than offering to fetch it", () => {
  assert.equal(hint(PACKAGE_ROW, false),
    "From the languagetool package, LGPL-2.1-or-later, needs Java")
  assert.deepEqual(actions(PACKAGE_ROW, { busy: "" }), [])
})

// ------------------------------------------------------------- what it offers

test("each state offers the buttons spec section 7 gives it", () => {
  assert.deepEqual(actions(ABSENT_ROW, { busy: "" }), [INSTALL])
  assert.deepEqual(actions(PARTIAL_ROW, { busy: "" }), [INSTALL, REMOVE])
  assert.deepEqual(actions(READY_ROW, { busy: "" }), [REMOVE])
  // The row a transfer is running on offers Cancel alone: Remove is about a
  // tree that is not there yet.
  assert.deepEqual(actions(PARTIAL_ROW, { busy: "languagetool" }), [CANCEL])
})

test("every action has an icon and a tooltip", () => {
  for (const action of [INSTALL, CANCEL, REMOVE]) {
    assert.ok(actionIcon(action).length > 0, action)
    assert.ok(actionTooltip(action, "LanguageTool").includes("LanguageTool"), action)
  }
  assert.equal(actionIcon("nothing"), "")
  assert.equal(ACTION_ICONS[REMOVE], "󰩹")
})

// One verb runs at a time, so every row but the one being installed is drawn
// disabled rather than live over a press that goes nowhere.
test("a verb in flight blocks every row but the one it is about", () => {
  assert.ok(!isBlocked(ABSENT_ROW, { working: false, busy: "" }))
  assert.ok(isBlocked(ABSENT_ROW, { working: true, busy: "" }))
  assert.ok(!isBlocked(ABSENT_ROW, { working: true, busy: "languagetool" }))
})

// ------------------------------------------------- what the dropdown may show

test("an engine that is not on this machine is not offered", () => {
  assert.ok(!isAvailable([ABSENT_ROW], "languagetool"))
  assert.ok(isAvailable([READY_ROW], "languagetool"))
  // The adapter runs the pacman package when no installed tree is there, so
  // that LanguageTool is one the dropdown may offer.
  assert.ok(isAvailable([PACKAGE_ROW], "languagetool"))
  // An engine with nothing to install is never in the list and is always there.
  assert.ok(isAvailable([ABSENT_ROW], "harper"))
})

test("the dropdown drops the engines the list says are absent", () => {
  assert.deepEqual(unavailable([ABSENT_ROW]), ["languagetool"])
  assert.deepEqual(unavailable([READY_ROW]), [])

  const offered = engineOptions(unavailable([ABSENT_ROW]), "harper").map(option => option.value)
  assert.deepEqual(offered, ["harper"])

  const whole = engineOptions(unavailable([READY_ROW]), "harper").map(option => option.value)
  assert.deepEqual(whole, ENGINE_OPTIONS.map(option => option.value))
})

// A dropdown that drops its own value shows a blank box, and the stored value
// is untouched until the reader chooses something else.
test("the engine the reader is on stays in the list whatever the disk says", () => {
  const offered = engineOptions(unavailable([ABSENT_ROW]), "languagetool")
    .map(option => option.value)

  assert.deepEqual(offered, ["languagetool", "harper"])
})

// The acceptance criterion: removing the selected engine leaves Settings
// consistent, and the one engine that cannot go away is where it lands.
test("removing the selected engine falls back to the built in one", () => {
  assert.equal(BUILT_IN_ENGINE, "harper")
  assert.equal(engineAfterRemoval("languagetool", "languagetool"), "harper")
  assert.equal(engineAfterRemoval("harper", "languagetool"), null)
})

// ---------------------------------------------------------------- the notes

test("a cancel is a notice and a failure is not", () => {
  const cancelled = note(CANCELLED, "", "LanguageTool")
  assert.equal(cancelled.kind, NOTICE)
  assert.ok(cancelled.body.includes("resumes"))

  const failed = note(DOWNLOAD_FAILED, "curl said no.", "LanguageTool")
  assert.equal(failed.kind, FAILURE)
  assert.equal(failed.message, "curl said no.")

  const missing = note(BAD_ARGUMENTS, "", "")
  assert.equal(missing.kind, FAILURE)
  assert.ok(missing.body.includes("out of date"))
})

// ------------------------------------------- a whole run against a stub binary

let stubDirectory = ""

test.before(() => {
  stubDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "grammachy-engines-"))
})

test.after(() => {
  if (stubDirectory) fs.rmSync(stubDirectory, { recursive: true, force: true })
})

// A stub that answers all three verbs off one JSON file, the way the real
// binary answers off the directory on disk.
function stub(name) {
  const state = path.join(stubDirectory, name + ".json")
  fs.writeFileSync(state, JSON.stringify([ABSENT_ROW]))
  const file = path.join(stubDirectory, name)
  fs.writeFileSync(file, [
    "#!/usr/bin/env node",
    'const fs = require("fs")',
    "const STATE = " + JSON.stringify(state),
    'const engines = JSON.parse(fs.readFileSync(STATE, "utf8"))',
    "const verb = process.argv[3]",
    "const wanted = process.argv[4]",
    "function report(verb, list) {",
    '  process.stdout.write(JSON.stringify({ contractVersion: 1, verb: verb,',
    '    directory: "/home/u/.local/share/grammachy/engines", freeBytes: 700000000000,',
    "    engines: list }))",
    "  fs.writeFileSync(STATE, JSON.stringify(engines))",
    "  process.exit(0)",
    "}",
    'if (verb === "list") report("list", engines)',
    "const row = engines.find(function (e) { return e.slug === wanted })",
    "if (!row) {",
    '  process.stdout.write(JSON.stringify({ contractVersion: 1, error: { code: "bad_arguments",',
    '    message: wanted + " is not an engine Grammachy can install." } }))',
    "  process.exit(1)",
    "}",
    'if (verb === "remove") {',
    '  row.state = "absent"',
    "  row.partialBytes = 0",
    '  row.path = ""',
    '  report("remove", [row])',
    "}",
    "// One step of the transfer per call, so the poll sees it move.",
    "row.partialBytes = Math.min(row.sizeBytes, row.partialBytes + row.sizeBytes / 4)",
    'row.state = row.partialBytes >= row.sizeBytes ? "ready" : "partial"',
    'if (row.state === "ready") {',
    "  row.partialBytes = 0",
    '  row.path = "/home/u/.local/share/grammachy/engines/languagetool"',
    "}",
    'report("install", [row])',
    ""
  ].join("\n"))
  fs.chmodSync(file, 0o755)
  return file
}

// One verb, read back the way Overlay.qml reads it.
function runEngine(binary, args) {
  const run = spawnSync(binary, ["engine"].concat(args), { encoding: "utf8" })
  assert.equal(run.error, undefined)
  return read(run.stdout)
}

test("a list from a real process becomes the row the view draws", () => {
  const binary = stub("list")

  const answer = runEngine(binary, ["list"])

  assert.equal(answer.error, null)
  assert.deepEqual(answer.report.engines.map(row => row.state), [ABSENT])
  assert.equal(hint(answer.report.engines[0], false),
    "About 240.3 MB, needs Java, LGPL-2.1-or-later")
  // The engine the dropdown must not offer yet.
  assert.deepEqual(unavailable(answer.report.engines), ["languagetool"])
})

// The acceptance criterion of this ticket: the bar moves while the install
// runs, and the row is Ready only once the whole release is unpacked.
test("an install run to the end moves the bar and lands on Ready", () => {
  const binary = stub("install")
  const seen = []
  let list = runEngine(binary, ["list"]).report.engines

  for (let step = 0; step < 4; step++) {
    const answer = runEngine(binary, ["install", "languagetool"])
    assert.equal(answer.error, null)
    list = merged(list, answer.report.engines)
    seen.push(share(list[0]))
    // Every poll of `list` sees the same row the install answered.
    const polled = runEngine(binary, ["list"]).report.engines[0]
    assert.equal(polled.state, list[0].state)
    assert.equal(polled.partialBytes, list[0].partialBytes)
  }

  assert.deepEqual(seen, [0.25, 0.5, 0.75, 1])
  assert.equal(list[0].state, READY)
  assert.equal(list[0].partialBytes, 0)
  // Only now may the dropdown offer it, and only now does it offer Remove.
  assert.deepEqual(unavailable(list), [])
  assert.deepEqual(actions(list[0], { busy: "" }), [REMOVE])
})

test("a part downloaded row offers Install again, and it resumes", () => {
  const binary = stub("resume")
  runEngine(binary, ["install", "languagetool"])
  const row = runEngine(binary, ["list"]).report.engines[0]

  assert.equal(row.state, PARTIAL)
  assert.deepEqual(actions(row, { busy: "" }), [INSTALL, REMOVE])

  const resumed = runEngine(binary, ["install", "languagetool"]).report.engines[0]
  assert.ok(resumed.partialBytes > row.partialBytes, "the second run carried on")
})

// The whole round trip of the acceptance criteria: install, then remove, then
// the dropdown drops the row and the setting falls back to Harper.
test("a remove from a real process leaves the row absent and Settings consistent", () => {
  const binary = stub("remove")
  for (let step = 0; step < 4; step++) runEngine(binary, ["install", "languagetool"])
  assert.equal(runEngine(binary, ["list"]).report.engines[0].state, READY)

  const answer = runEngine(binary, ["remove", "languagetool"])

  assert.equal(answer.error, null)
  assert.equal(answer.report.verb, "remove")
  const list = runEngine(binary, ["list"]).report.engines
  assert.equal(list[0].state, ABSENT)
  assert.deepEqual(actions(list[0], { busy: "" }), [INSTALL])
  assert.deepEqual(unavailable(list), ["languagetool"])
  assert.equal(engineAfterRemoval("languagetool", "languagetool"), BUILT_IN_ENGINE)
})

test("a slug the catalogue does not carry comes back as a note", () => {
  const binary = stub("unknown")

  const answer = runEngine(binary, ["install", "harper"])

  assert.equal(answer.report, null)
  assert.equal(answer.error.code, BAD_ARGUMENTS)
  const line = note(answer.error.code, answer.error.message, "harper")
  assert.ok(line.message.includes("is not an engine Grammachy can install"))
})

test("a binary that is not there at all is one note and no rows", () => {
  const run = spawnSync(path.join(stubDirectory, "no-such-binary"), ["engine", "list"],
    { encoding: "utf8" })
  const answer = read(run.stdout || "")

  assert.equal(answer.report, null)
  assert.equal(answer.error.code, BAD_ARGUMENTS)
  assert.ok(note(answer.error.code, answer.error.message, "").body.includes("out of date"))
})

const EnginesModule = require("./engines.js")
test("a row that needs Java reads the runtime hint until jre-openjdk is present", () => {
  const row = { slug: "languagetool", name: "LanguageTool", state: "absent", needsJava: true }
  assert.equal(EnginesModule.runtimeMissing(row, false), true)
  assert.equal(EnginesModule.runtimeMissing(row, undefined), true)
  assert.equal(EnginesModule.runtimeMissing(row, true), false)
  assert.equal(EnginesModule.runtimeMissing({ slug: "x", needsJava: false }, false), false)
  assert.equal(EnginesModule.RUNTIME_HINT, "Needs a Java runtime")
})

test("a missing runtime blocks Install alone, and never Remove or Cancel", () => {
  const row = { slug: "languagetool", name: "LanguageTool", state: "partial", needsJava: true }
  assert.equal(EnginesModule.actionBlocked(EnginesModule.INSTALL, row, { javaPresent: false }), true)
  assert.equal(EnginesModule.actionBlocked(EnginesModule.REMOVE, row, { javaPresent: false }), false)
  assert.equal(EnginesModule.actionBlocked(EnginesModule.INSTALL, row, { javaPresent: true }), false)
  // A verb in flight still blocks every row but its own.
  assert.equal(EnginesModule.actionBlocked(EnginesModule.INSTALL, row, { javaPresent: true, working: true, busy: "other" }), true)
  assert.equal(EnginesModule.actionBlocked(EnginesModule.CANCEL, row, { javaPresent: false, working: true, busy: "languagetool" }), false)
})
