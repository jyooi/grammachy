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
const Limits = require("./limits.js")
const { readCheck } = require("./errors.js")

let stubDirectory = ""

test.before(() => {
  stubDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "grammachy-capture-"))
})

test.after(() => {
  if (stubDirectory) fs.rmSync(stubDirectory, { recursive: true, force: true })
})

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

function checkCount(name) {
  try {
    return fs.readFileSync(path.join(stubDirectory, name + ".count"), "utf8").length
  } catch (error) {
    return 0
  }
}

function machine(values) {
  const state = values || {}
  return {
    primary: state.primary === undefined ? "" : state.primary,
    clipboard: state.clipboard === undefined ? "" : state.clipboard,
    copies: state.copies === undefined ? "" : state.copies,
    address: state.address === undefined ? "0xaaa" : state.address,
    last: Capture.kept("", ""),
    draft: state.draft === undefined ? "" : state.draft,
    reads: 0,
    replacePending: false,
    typed: []
  }
}

function isSelection(text) {
  return typeof text === "string" && text.replace(/^\s+|\s+$/g, "").length > 0
}

function nothingNew(run) {
  run.phase = "empty"
  run.capturedText = ""
  return run
}

function consume(box, text, address) {
  if (typeof text !== "string" || text.length === 0) return
  if (Capture.isStale(text, address, box.last)) return
  box.last = Capture.kept(text, address)
}

// `Overlay.releasePrimary`: the one place that runs `wl-copy --primary
// --clear`. Only a run that took a Selection releases one, and it releases at
// most once, whichever exit it takes. A Replace still to type holds it back,
// because the source window keeps the highlight it is about to paste over, so
// the claim outlives the close that armed the wait.
function release(box, run) {
  if (!run.captured) return run
  if (box.replacePending) return run
  run.captured = false
  box.primary = ""
  run.released += 1
  return run
}

function closePopup(box, run) {
  if (run.captured) consume(box, run.capturedText, run.address)
  return release(box, run)
}

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

const CORRECTED = "They are going to the park."

function replace(box, run) {
  box.replacePending = true
  closePopup(box, run)
  run.pasted = box.primary
  box.typed.push({ text: box.clipboard, window: box.address })
  box.replacePending = false
  return release(box, run)
}

function replaces(run, autoReplace) {
  return run.surface === "quick" && autoReplace === true && run.captured === true
}

function applyCorrected(box, run, autoReplace) {
  box.clipboard = CORRECTED
  run.applyLabel = replaces(run, autoReplace) ? "Replace selection" : "Copy corrected text"
  if (!replaces(run, autoReplace)) return closePopup(box, run)
  return replace(box, run)
}

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
    const borrowed = box.clipboard
    if (box.copies.length > 0) box.clipboard = box.copies
    const after = box.clipboard
    box.clipboard = borrowed

    if (Capture.copiedNothing(borrowed, after) || !isSelection(after)) return nothingNew(run)
    text = after
  }

  if (Capture.isStale(text, address, box.last)) return nothingNew(run)

  run.capturedText = text
  run.address = address
  run.captured = true
  consume(box, text, address)
  return check(binary, text, run)
}

function check(binary, text, run) {
  const answer = readCheck(
    spawnSync(binary, ["check", "--engine", "harper"], { input: text, encoding: "utf8" }).stdout)
  run.phase = answer.error ? "error" : "result"
  run.issues = answer.error ? null : answer.result.issues
  return run
}

function checkLastAgain(binary, box, previous) {
  const before = previous || {}
  const run = {
    phase: "checking",
    surface: "quick",
    capturedText: box.last.text,
    address: before.address === undefined ? "" : before.address,
    captured: before.captured === true,
    issues: null,
    released: before.released === undefined ? 0 : before.released
  }
  if (box.last.text.length === 0) return nothingNew(run)
  return check(binary, box.last.text, run)
}

function clearCapture(box, run) {
  run.issues = null
  run.focusIndex = 0
  run.applied = false
  release(box, run)
  return nothingNew(run)
}

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

test("a popup that closes releases the primary selection", () => {
  const binary = stub("release")
  const box = machine({ primary: "Their going to the park." })

  const run = closePopup(box, summon(binary, box))
  assert.equal(run.released, 1)
  assert.equal(box.primary, "", "the primary selection was cleared")

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

test("a Replace on a run that captured releases exactly once", () => {
  const binary = stub("replace-once")
  const box = machine({ primary: "Their going to the park." })

  const run = summon(binary, box)
  assert.equal(run.captured, true)

  const replaced = replace(box, run)
  assert.equal(replaced.released, 1, "the keystroke ends the wait and the selection goes")
  assert.equal(box.primary, "")

  closePopup(box, replaced)
  assert.equal(replaced.released, 1, "the close after the paste releases no second time")
})

test("a Replace after a stale summon releases a selection it never took", () => {
  const binary = stub("replace-stale")
  const box = machine({ primary: "Their going to the park." })

  closePopup(box, summon(binary, box))

  box.primary = "Their going to the park."
  const stale = summon(binary, box)
  assert.equal(stale.phase, "empty")
  assert.equal(stale.captured, false, "a stale summon takes nothing")

  const again = checkLastAgain(binary, box, stale)
  assert.equal(again.phase, "result")
  assert.equal(again.captured, false, "and the kept text is no capture either")

  const replaced = replace(box, again)
  assert.equal(replaced.released, 0, "Replace takes no selection this run never held")
  assert.equal(box.primary, "Their going to the park.",
    "the highlight the reader still owns is there")
})

test("Apply after Check last text again copies and types nothing", () => {
  const binary = stub("apply-kept")
  const box = machine({ primary: "Their going to the park." })

  closePopup(box, summon(binary, box))

  box.address = "0xbbb"
  box.primary = ""
  const empty = summon(binary, box)
  assert.equal(empty.phase, "empty")
  assert.equal(empty.captured, false, "that summon took nothing")

  const again = checkLastAgain(binary, box, empty)
  assert.equal(again.phase, "result")

  const applied = applyCorrected(box, again, true)
  assert.equal(applied.applyLabel, "Copy corrected text",
    "the card says copy, because this run holds no Selection")
  assert.deepEqual(box.typed, [], "and nothing was typed into the window that has the keyboard")
  assert.equal(box.clipboard, CORRECTED, "the Corrected text is on the clipboard")
})

test("Apply on a run that captured still replaces when auto-replace is on", () => {
  const binary = stub("apply-captured")
  const box = machine({ primary: "Their going to the park." })

  const run = summon(binary, box)
  assert.equal(run.captured, true)

  const applied = applyCorrected(box, run, true)
  assert.equal(applied.applyLabel, "Replace selection")
  assert.deepEqual(box.typed, [{ text: CORRECTED, window: "0xaaa" }],
    "the Corrected text went into the window the Selection came from")
  assert.equal(applied.released, 1)
})

test("Apply with auto-replace off copies on a run that captured", () => {
  const binary = stub("apply-copy")
  const box = machine({ primary: "Their going to the park." })

  const applied = applyCorrected(box, summon(binary, box), false)
  assert.equal(applied.applyLabel, "Copy corrected text")
  assert.deepEqual(box.typed, [])
  assert.equal(box.clipboard, CORRECTED)
})

// Spec sections 2 and 3: SUPER + ALT + Q opens Compose and captures nothing.
// A terminal drops its own highlight when it loses primary ownership, so a
// surface that took no Selection must take none away either.
test("a Compose that closes releases nothing and records nothing", () => {
  const box = machine({ primary: "Their going to the park." })

  const run = closePopup(box, showCompose())

  assert.equal(run.released, 0)
  assert.equal(box.primary, "Their going to the park.", "the highlight is still there")
  assert.equal(box.last.text, "", "and nothing was recorded as consumed")
})

test("a Compose opened after a Check leaves the kept record whole", () => {
  const binary = stub("compose-after-check")
  const box = machine({ primary: "Their going to the park." })

  const first = summon(binary, box)
  assert.equal(first.phase, "result")
  assert.equal(checkCount("compose-after-check"), 1)

  closePopup(box, showCompose(first))

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

  const second = summon(binary, box)
  assert.equal(second.phase, "empty")
  assert.equal(checkCount("copied-field"), 1)
})

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
  assert.equal(box.last.text, "Their going to the park.")
  assert.equal(checkCount("clear"), 1)

  const again = checkLastAgain(binary, box, cleared)
  assert.equal(again.phase, "result")
  assert.equal(checkCount("clear"), 2)
})

test("Clear on a run that captured releases once and no more", () => {
  const binary = stub("clear-once")
  const box = machine({ primary: "Their going to the park." })

  const run = summon(binary, box)
  assert.equal(run.captured, true)

  const cleared = clearCapture(box, run)
  assert.equal(cleared.released, 1, "the run that took the selection hands it back")
  assert.equal(box.primary, "")

  closePopup(box, cleared)
  assert.equal(cleared.released, 1, "and the close after the Clear releases no second time")
})

test("Clear after a stale summon releases a selection it never took", () => {
  const binary = stub("clear-stale")
  const box = machine({ primary: "Their going to the park." })

  closePopup(box, summon(binary, box))
  assert.equal(box.primary, "")

  box.primary = "Their going to the park."
  const stale = summon(binary, box)
  assert.equal(stale.phase, "empty")
  assert.equal(stale.captured, false, "a stale summon takes nothing")
  assert.equal(checkCount("clear-stale"), 1)

  const again = checkLastAgain(binary, box, stale)
  assert.equal(again.phase, "result")
  assert.equal(again.captured, false, "and the kept text is no capture either")

  const cleared = clearCapture(box, again)
  assert.equal(cleared.released, 0, "Clear takes no selection this run never held")
  assert.equal(box.primary, "Their going to the park.",
    "the highlight the reader still owns is there")

  closePopup(box, cleared)
  assert.equal(cleared.released, 0)
  assert.equal(box.primary, "Their going to the park.")
})

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
    "No new selection. Highlight text and press SUPER + SHIFT + Q, or paste here.")
  assert.equal(Capture.CHECK_LAST_AGAIN, "Check last text again")
})

test("every paste is bounded in bytes and in time before the shell collects it", () => {
  assert.deepEqual(Capture.primaryCommand(), [
    "sh", "-c", "timeout 5 wl-paste --primary --no-newline | head -c 1048576"
  ])
  assert.deepEqual(Capture.fallbackCommand(), [
    "sh", "-c", "timeout 5 wl-paste --no-newline | head -c 1048576"
  ])
  assert.deepEqual(Capture.borrowCommand(), [
    "sh", "-c", "timeout 5 wl-paste --no-newline | head -c 1048576"
  ])
})

test("one bound covers every paste", () => {
  assert.ok(Capture.PASTE_LIMIT_BYTES >= Limits.CHECK_LIMIT_UNITS * 3)
  assert.deepEqual(Capture.borrowCommand(), Capture.fallbackCommand())
})

test("the paste command cuts its output at the byte bound", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "grammachy-paste-"))
  try {
    const file = path.join(directory, "wl-paste")
    fs.writeFileSync(file, "#!/bin/sh\nprintf '%s' 0123456789abcdefghij\n")
    fs.chmodSync(file, 0o755)

    const command = Capture.pasteCommand(false, 8)
    const run = spawnSync(command[0], command.slice(1), {
      env: { PATH: directory + ":" + process.env.PATH },
      encoding: "utf8",
      shell: false
    })

    assert.equal(run.status, 0)
    assert.equal(run.stdout, "01234567")
  } finally {
    fs.rmSync(directory, { recursive: true, force: true })
  }
})

test("the UTF-8 length counts what head counted", () => {
  assert.equal(Capture.utf8Bytes(""), 0)
  assert.equal(Capture.utf8Bytes("abc"), 3)
  assert.equal(Capture.utf8Bytes("\u00e9"), 2)
  assert.equal(Capture.utf8Bytes("\u4e2d"), 3)
  assert.equal(Capture.utf8Bytes("\ud83d\ude00"), 4)
  assert.equal(Capture.utf8Bytes(Buffer.from("a\u00e9\u4e2d\ud83d\ude00").toString()), 10)
})

test("a borrowed clipboard at its bound is not borrowed", () => {
  const bound = Capture.PASTE_LIMIT_BYTES
  assert.equal(Capture.pasteOverflowed("x".repeat(bound - 4)), false)
  assert.equal(Capture.pasteOverflowed("x".repeat(bound - 3)), true)
  assert.equal(Capture.pasteOverflowed("x".repeat(bound)), true)
  // A cut multi-byte tail arrives as U+FFFD, three bytes.
  assert.equal(Capture.pasteOverflowed("x".repeat(bound - 3) + "\ufffd"), true)
  assert.equal(Capture.pasteOverflowed(""), false)
  assert.equal(Capture.pasteOverflowed(undefined), false)
})
