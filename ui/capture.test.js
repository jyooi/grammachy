// Node tests for the capture of spec section 3 and the Clear of section 6.
// Run with `node --test ui/capture.test.js`.
//
// The bug these hold shut, HUF-235: the compositor keeps the last primary
// selection, so a summon with nothing highlighted read the text of the summon
// before it and checked it all over again.
//
// `Overlay.qml` owns the processes and cannot be instantiated outside the
// shell's plugin loader, so the route below drives the same steps in the same
// order against a stub `grammachy check`. `cli/tests/overlay_capture.rs` is
// what keeps the overlay on those steps. A stub is the only safe seam: no test
// here reaches a real engine, reads the machine, or touches a real clipboard.

const test = require("node:test")
const assert = require("node:assert/strict")
const fs = require("node:fs")
const os = require("node:os")
const path = require("node:path")
const { spawnSync } = require("node:child_process")

const Capture = require("./capture.js")
const { readCheck } = require("./errors.js")

// ------------------------------------------------------------- the stub CLI

let stubDirectory = ""

test.before(() => {
  stubDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "grammachy-capture-"))
})

test.after(() => {
  if (stubDirectory) fs.rmSync(stubDirectory, { recursive: true, force: true })
})

// A stub that reads stdin the way `grammachy check` does, counts the run, and
// prints one clean result envelope. The count is the whole point: what these
// tests claim is how many Checks a summon started.
function stub(name) {
  const counter = path.join(stubDirectory, name + ".count")
  const file = path.join(stubDirectory, name)
  fs.writeFileSync(file, [
    "#!/usr/bin/env bash",
    "cat > /dev/null",
    `printf '.' >> ${JSON.stringify(counter)}`,
    `printf '{"contractVersion":1,"engine":"harper","elapsedMs":3,"issues":[]}'`,
    ""
  ].join("\n"), { mode: 0o755 })
  fs.writeFileSync(counter, "")
  return file
}

// How many Checks that stub has run, which is the whole claim of this file.
function checkCount(name) {
  try {
    return fs.readFileSync(path.join(stubDirectory, name + ".count"), "utf8").length
  } catch (error) {
    return 0
  }
}

// ------------------------------------------------------- the machine and run

// What the compositor holds. `primary` is the primary selection, `clipboard`
// is the clipboard, and `copies` is what Ctrl + C would put on the clipboard
// in the focused window: "" for a field with nothing selected, which is what
// leaves the clipboard as it was.
function machine(values) {
  const state = values || {}
  return {
    primary: state.primary === undefined ? "" : state.primary,
    clipboard: state.clipboard === undefined ? "" : state.clipboard,
    copies: state.copies === undefined ? "" : state.copies,
    address: state.address === undefined ? "0xaaa" : state.address,
    // The kept record of `Overlay.lastCapturedText` and `lastCapturedWindow`.
    last: Capture.kept("", ""),
    // The Draft of spec section 9, which nothing in this file may touch.
    draft: state.draft === undefined ? "" : state.draft,
    // How many times the primary selection was read, which is what says
    // whether a run captured or reused the kept text.
    reads: 0,
    // A Replace has closed the popup and has still to type.
    replacePending: false
  }
}

// `Overlay.isSelection`: a non-empty result that is not only whitespace.
function isSelection(text) {
  return typeof text === "string" && text.replace(/^\s+|\s+$/g, "").length > 0
}

// `Overlay.showNothingNew`: the empty state of spec section 6, popup open.
function nothingNew(run) {
  run.phase = "empty"
  run.capturedText = ""
  return run
}

// `Overlay.consumeCapture`: the record the next summon is measured against,
// and nothing else. The compositor is not touched here.
function consume(box, text, address) {
  if (typeof text !== "string" || text.length === 0) return
  if (Capture.isStale(text, address, box.last)) return
  box.last = Capture.kept(text, address)
}

// `Overlay.releasePrimary`: the primary selection goes. A Replace still to
// type holds it back, because the source window keeps the highlight it is
// about to paste over.
function release(box, run) {
  if (box.replacePending) return run
  box.primary = ""
  run.released += 1
  return run
}

// `Overlay.close`: a run that captured is over, so what it captured is
// recorded and the primary selection it came from goes. A run that captured
// nothing owns no selection, so it records none and takes none away.
function closePopup(box, run) {
  if (!run.captured) return run
  consume(box, run.capturedText, run.address)
  return release(box, run)
}

// `Overlay.showCompose`: SUPER + SHIFT + G opens Compose on the kept Draft and
// captures nothing at all. `Overlay.resetRun` drops the source window of the
// run before it and leaves that run's text in place, which is why the text
// alone cannot say whether this run captured.
function showCompose(previous) {
  const before = previous || {}
  return {
    phase: "editing",
    surface: "compose",
    capturedText: before.capturedText === undefined ? "" : before.capturedText,
    address: "",
    captured: false,
    issues: null,
    released: 0
  }
}

// `Overlay.applyCorrected` with auto-replace on: the popup closes, the source
// window is asked for, and only then is the keystroke typed.
function replace(box, run) {
  box.replacePending = true
  closePopup(box, run)
  // The paste lands on the highlight the source window still holds.
  run.pasted = box.primary
  box.replacePending = false
  return closePopup(box, run)
}

// One SUPER + G, driven the way `Overlay.startQuick` drives it: the source
// window, then the primary selection of step 1, then the Ctrl + C fallback of
// step 2, then the freshness rule, then one Check.
function summon(binary, box) {
  const run = {
    phase: "capturing",
    surface: "quick",
    capturedText: "",
    address: "",
    captured: false,
    issues: null,
    released: 0
  }

  const address = box.address
  box.reads += 1
  let text = box.primary

  if (!isSelection(text)) {
    // Step 2: save the clipboard, send Ctrl + C, read what landed, put the
    // clipboard back.
    const borrowed = box.clipboard
    if (box.copies.length > 0) box.clipboard = box.copies
    const after = box.clipboard
    box.clipboard = borrowed

    if (Capture.copiedNothing(borrowed, after) || !isSelection(after)) return nothingNew(run)
    text = after
  }

  // Spec section 3: the same text from the same window is what the last Check
  // already ran on.
  if (Capture.isStale(text, address, box.last)) return nothingNew(run)

  run.capturedText = text
  run.address = address
  run.captured = true
  consume(box, text, address)
  return check(binary, text, run)
}

// `Overlay.runCheck`, which is the only thing that runs the binary.
function check(binary, text, run) {
  const answer = readCheck(
    spawnSync(binary, ["check", "--engine", "harper"], { input: text, encoding: "utf8" }).stdout)
  run.phase = answer.error ? "error" : "result"
  run.issues = answer.error ? null : answer.result.issues
  return run
}

// `Overlay.checkLastAgain`: the kept text with no capture at all.
function checkLastAgain(binary, box) {
  const run = {
    phase: "checking",
    surface: "quick",
    capturedText: box.last.text,
    address: "",
    captured: false,
    issues: null,
    released: 0
  }
  if (box.last.text.length === 0) return nothingNew(run)
  return check(binary, box.last.text, run)
}

// `Overlay.clearCapture`: the review goes, the popup stays open on the empty
// state, the kept record stays so the button is still there, and neither the
// Draft nor the compositor is touched.
function clearCapture(box, run) {
  run.issues = null
  run.focusIndex = 0
  run.applied = false
  // Clear ends a quick run that did capture, so the selection it came from
  // goes the same way a close releases it.
  release(box, run)
  return nothingNew(run)
}

// ---------------------------------------------------------------- the tests

// The regression: the same capture twice runs one Check.
test("the same selection from the same window is checked once", () => {
  const binary = stub("same-twice")
  const box = machine({ primary: "Their going to the park." })

  const first = summon(binary, box)
  assert.equal(first.phase, "result")
  assert.equal(first.capturedText, "Their going to the park.")
  assert.equal(checkCount("same-twice"), 1)

  // The compositor answers the same selection again, because nothing else has
  // claimed it. Before the fix this ran a second Check on the same words.
  box.primary = "Their going to the park."
  const second = summon(binary, box)
  assert.equal(second.phase, "empty")
  assert.equal(second.capturedText, "")
  assert.equal(checkCount("same-twice"), 1, "the second summon started no Check")
  // The kept text is still there, so the empty state can offer it.
  assert.equal(box.last.text, "Their going to the park.")
})

// Spec section 3: the capture takes nothing from the compositor. A terminal
// drops its own highlight when it loses primary ownership, and the reader is
// still deciding on the Apply that pastes over it.
test("a capture leaves the primary selection where it is", () => {
  const binary = stub("keeps")
  const box = machine({ primary: "Their going to the park." })

  const run = summon(binary, box)
  assert.equal(run.phase, "result")
  assert.equal(run.released, 0)
  assert.equal(box.primary, "Their going to the park.", "the selection is still there")
})

// The release is the close, whatever ended the run.
test("a popup that closes releases the primary selection", () => {
  const binary = stub("release")
  const box = machine({ primary: "Their going to the park." })

  const run = closePopup(box, summon(binary, box))
  assert.equal(run.released, 1)
  assert.equal(box.primary, "", "the primary selection was cleared")

  // With the selection gone and the clipboard empty, the next summon finds
  // nothing at all and still runs no Check.
  const next = summon(binary, box)
  assert.equal(next.phase, "empty")
  assert.equal(checkCount("release"), 1)
})

// Replace closes the popup and only then types over the highlight the source
// window still holds, so the release waits for that keystroke.
test("a Replace types over the selection before the release takes it", () => {
  const binary = stub("replace")
  const box = machine({ primary: "Their going to the park." })

  const run = replace(box, summon(binary, box))

  assert.equal(run.pasted, "Their going to the park.",
    "the highlight was still there when the keystroke landed")
  assert.equal(run.released, 1)
  assert.equal(box.primary, "", "and it goes once the keystroke is out")
})

// Spec sections 2 and 3: SUPER + SHIFT + G opens Compose and captures nothing.
// A terminal drops its own highlight when it loses primary ownership, so a
// surface that took no Selection must take none away either.
test("a Compose that closes releases nothing and records nothing", () => {
  const box = machine({ primary: "Their going to the park." })

  const run = closePopup(box, showCompose())

  assert.equal(run.released, 0)
  assert.equal(box.primary, "Their going to the park.", "the highlight is still there")
  assert.equal(box.last.text, "", "and nothing was recorded as consumed")
})

// The hero's Compose button opens Compose over a quick run that did capture.
// Compose keeps that run's text and drops its source window, so a close that
// recorded the pair would file the text against no window and lose the
// compare the next summon rests on.
test("a Compose opened after a Check leaves the kept record whole", () => {
  const binary = stub("compose-after-check")
  const box = machine({ primary: "Their going to the park." })

  const first = summon(binary, box)
  assert.equal(first.phase, "result")
  assert.equal(checkCount("compose-after-check"), 1)

  closePopup(box, showCompose(first))

  // The compositor still holds the same selection from the same window.
  box.primary = "Their going to the park."
  const second = summon(binary, box)
  assert.equal(second.phase, "empty", "the same text from the same window is still stale")
  assert.equal(checkCount("compose-after-check"), 1, "and no second Check ran")
})

test("a different selection from the same window is fresh", () => {
  const binary = stub("fresh-text")
  const box = machine({ primary: "Their going to the park." })

  closePopup(box, summon(binary, box))
  box.primary = "Its a nice day."
  const second = summon(binary, box)

  assert.equal(second.phase, "result")
  assert.equal(second.capturedText, "Its a nice day.")
  assert.equal(checkCount("fresh-text"), 2)
})

test("the same words from another window are fresh", () => {
  const binary = stub("fresh-window")
  const box = machine({ primary: "Their going to the park." })

  closePopup(box, summon(binary, box))
  box.primary = "Their going to the park."
  box.address = "0xbbb"
  const second = summon(binary, box)

  assert.equal(second.phase, "result")
  assert.equal(checkCount("fresh-window"), 2)
})

// Step 2 of spec section 3. An Electron field with nothing selected answers
// Ctrl + C by leaving the clipboard as it was, so what comes back is an
// earlier copy rather than a Selection.
test("a Ctrl + C that copies nothing is not a Selection", () => {
  const binary = stub("copied-nothing")
  const box = machine({ primary: "", clipboard: "something copied yesterday", copies: "" })

  const run = summon(binary, box)

  assert.equal(run.phase, "empty")
  assert.equal(checkCount("copied-nothing"), 0)
  assert.equal(box.clipboard, "something copied yesterday", "the borrow went back")
})

test("a Ctrl + C that copies a field checks what it copied", () => {
  const binary = stub("copied-field")
  const box = machine({ primary: "", clipboard: "something copied yesterday", copies: "Their going." })

  const run = summon(binary, box)

  assert.equal(run.phase, "result")
  assert.equal(run.capturedText, "Their going.")
  assert.equal(checkCount("copied-field"), 1)
  assert.equal(box.clipboard, "something copied yesterday", "the borrow went back")

  // The same field summoned again copies the same words, which are the ones
  // the last Check ran on.
  const second = summon(binary, box)
  assert.equal(second.phase, "empty")
  assert.equal(checkCount("copied-field"), 1)
})

// Spec section 6: `Check last text again` runs the kept text with no capture.
test("Check last text again runs no capture", () => {
  const binary = stub("check-again")
  const box = machine({ primary: "Their going to the park." })

  summon(binary, box)
  const reads = box.reads

  const again = checkLastAgain(binary, box)
  assert.equal(again.phase, "result")
  assert.equal(again.capturedText, "Their going to the park.")
  assert.equal(checkCount("check-again"), 2)
  assert.equal(box.reads, reads, "no second capture was taken")
})

test("Check last text again with nothing kept checks nothing", () => {
  const binary = stub("check-again-empty")
  const box = machine({})

  const again = checkLastAgain(binary, box)
  assert.equal(again.phase, "empty")
  assert.equal(checkCount("check-again-empty"), 0)
})

// Spec section 6: Clear lands on the same empty state and keeps the popup
// open. The Draft is the one thing it never touches.
test("Clear lands on the empty state and keeps the Draft", () => {
  const binary = stub("clear")
  const box = machine({ primary: "Their going to the park.", draft: "A draft the reader wrote." })

  const run = summon(binary, box)
  assert.equal(run.phase, "result")

  const cleared = clearCapture(box, run)
  assert.equal(cleared.phase, "empty")
  assert.equal(cleared.capturedText, "")
  assert.equal(cleared.issues, null)
  assert.equal(cleared.focusIndex, 0)
  assert.equal(cleared.applied, false)
  assert.equal(box.draft, "A draft the reader wrote.")
  // The kept text survives Clear, so the empty state still offers it.
  assert.equal(box.last.text, "Their going to the park.")
  assert.equal(checkCount("clear"), 1)

  const again = checkLastAgain(binary, box)
  assert.equal(again.phase, "result")
  assert.equal(checkCount("clear"), 2)
})

// ------------------------------------------------------------- the two rules

test("nothing is stale until a capture has been consumed", () => {
  assert.equal(Capture.isStale("some text", "0xaaa", Capture.kept("", "")), false)
  assert.equal(Capture.isStale("some text", "0xaaa", null), false)
  assert.equal(Capture.isStale("", "0xaaa", Capture.kept("", "0xaaa")), false)
})

test("a capture with no source window compares against one with none", () => {
  const kept = Capture.kept("some text", "")
  assert.equal(Capture.isStale("some text", "", kept), true)
  assert.equal(Capture.isStale("some text", "0xaaa", kept), false)
})

test("a clipboard that did not move copied nothing", () => {
  assert.equal(Capture.copiedNothing("same", "same"), true)
  assert.equal(Capture.copiedNothing("", ""), true)
  assert.equal(Capture.copiedNothing("before", "after"), false)
})

test("the empty state prints one line and names the kept text", () => {
  assert.equal(Capture.NOTHING_NEW,
    "No new selection. Highlight text and press SUPER + G, or paste here.")
  assert.equal(Capture.CHECK_LAST_AGAIN, "Check last text again")
})
