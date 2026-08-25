// Node tests for the popup placement and the Replace target. Spec sections
// 3, 6, and 13.
// Run with `node --test ui/anchor.test.js`.
//
// The last block runs a stub `hyprctl` through the two steps `Overlay.qml`
// runs before it types anything. A stub is the only safe seam here: a test
// must never move the focus of the desktop it runs on.

const test = require("node:test")
const assert = require("node:assert/strict")
const fs = require("node:fs")
const os = require("node:os")
const path = require("node:path")
const { spawnSync } = require("node:child_process")

const {
  ADJACENT,
  INSIDE,
  FALLBACK,
  SOURCE_GONE_TITLE,
  SOURCE_GONE_BODY,
  readActiveWindow,
  windowAddress,
  activeWindowCommand,
  focusCommand,
  isFocused,
  placeCard
} = require("./anchor.js")

// One monitor of the machine this ticket was found on: 2400 by 1350 logical
// pixels with a 24 pixel bar, and a second monitor beside it at x 2400.
const BOUNDS = { width: 2400, height: 1350 }
const ORIGIN = { x: 0, y: 0 }
const CARD = { width: 680, height: 340 }
const BAR = { position: "top", size: 24 }
const GAP = 20

// The two windows of the reproduction: the source on the left half, and
// another one filling the trailing half up to the top-right corner.
const LEFT = { address: "0x1a", x: 12, y: 36, width: 1181, height: 1302 }
const RIGHT = { address: "0x2b", x: 1207, y: 36, width: 1181, height: 1302 }

function place(overrides) {
  return placeCard(Object.assign({
    window: LEFT,
    origin: ORIGIN,
    bounds: BOUNDS,
    card: CARD,
    bar: BAR,
    gap: GAP
  }, overrides || {}))
}

// The region a card may occupy, worked out here rather than read from the
// module, so the tests fail when the module changes its mind about it.
function region(bar) {
  const position = bar.position
  return {
    left: GAP + (position === "left" ? bar.size : 0),
    top: GAP + (position === "top" ? bar.size : 0),
    right: BOUNDS.width - GAP - (position === "right" ? bar.size : 0),
    bottom: BOUNDS.height - GAP - (position === "bottom" ? bar.size : 0)
  }
}

function assertOnScreen(placement, bar, card) {
  const bounds = region(bar || BAR)
  const size = card || CARD
  assert.ok(placement.x >= bounds.left, "the card starts inside the region: " + placement.x)
  assert.ok(placement.y >= bounds.top, "the card starts under the bar: " + placement.y)
  assert.ok(placement.x + size.width <= bounds.right,
    "the card ends inside the region: " + (placement.x + size.width))
  assert.ok(placement.y + size.height <= bounds.bottom,
    "the card ends above the region floor: " + (placement.y + size.height))
}

// --------------------------------------------------------------- adjacency

// The card takes the window's top edge, held out of the bar's own strip. A
// tiled window sits closer to the bar than a card is allowed to, so the two
// agree only once the window starts below the region.
function topFor(window) {
  return Math.max(region(BAR).top, window.y)
}

// The bug this ticket is about: the card opened in the bar corner however far
// that was from the window the Selection came from.
test("the card opens on the trailing side of the source window, at its top", () => {
  const placement = place({ window: LEFT })
  assert.equal(placement.mode, ADJACENT)
  assert.equal(placement.x, LEFT.x + LEFT.width + GAP)
  assert.equal(placement.y, topFor(LEFT))
  assertOnScreen(placement)
})

test("a card follows a window down the screen rather than staying at the top", () => {
  const low = { address: "0x9a", x: 12, y: 700, width: 900, height: 500 }
  assert.equal(place({ window: low }).y, 700)
})

test("a window against the trailing edge takes the leading side instead", () => {
  const placement = place({ window: RIGHT })
  assert.equal(placement.mode, ADJACENT)
  assert.equal(placement.x, RIGHT.x - GAP - CARD.width)
  assert.equal(placement.y, topFor(RIGHT))
  assertOnScreen(placement)
})

test("a window with no room on either side holds the card inside itself", () => {
  const full = { address: "0x3c", x: 0, y: 24, width: 2400, height: 1326 }
  const placement = place({ window: full })
  assert.equal(placement.mode, INSIDE)
  assert.equal(placement.x, full.x + full.width - CARD.width - GAP)
  assertOnScreen(placement)
})

// A window narrower than the card leaves the leading side short too, so the
// card overlays it rather than running off the screen.
test("a narrow window against the leading edge still keeps the card on screen", () => {
  const narrow = { address: "0x4d", x: 0, y: 200, width: 1900, height: 900 }
  const placement = place({ window: narrow })
  assert.equal(placement.mode, INSIDE)
  assertOnScreen(placement)
})

// ------------------------------------------------------------------ clamp

test("a window under the bar never pushes the card over it", () => {
  const placement = place({ window: { address: "0x5e", x: 12, y: 0, width: 900, height: 600 } })
  assert.equal(placement.y, region(BAR).top)
  assertOnScreen(placement)
})

test("a window near the floor never pushes the card off the bottom", () => {
  const placement = place({ window: { address: "0x6f", x: 12, y: 1300, width: 900, height: 40 } })
  assert.equal(placement.y, region(BAR).bottom - CARD.height)
  assertOnScreen(placement)
})

// Every bar position takes its own strip out of the region, and no window may
// put the card into it.
test("the card stays inside the region for every bar position", () => {
  for (const position of ["top", "bottom", "left", "right"]) {
    const bar = { position: position, size: 24 }
    for (const window of [
      LEFT,
      RIGHT,
      { address: "0x7a", x: 0, y: 0, width: 2400, height: 1350 },
      { address: "0x7b", x: 2300, y: 1300, width: 100, height: 50 },
      { address: "0x7c", x: -40, y: -40, width: 200, height: 200 }
    ]) {
      const placement = placeCard({
        window: window, origin: ORIGIN, bounds: BOUNDS, card: CARD, bar: bar, gap: GAP
      })
      assertOnScreen(placement, bar)
    }
  }
})

// A card taller or wider than the region has nowhere to fit. It is held to the
// near edge, because a card that starts off screen shows nothing at all.
test("a card larger than the region starts at the near edge", () => {
  const huge = { width: 3000, height: 2000 }
  const placement = placeCard({
    window: LEFT, origin: ORIGIN, bounds: BOUNDS, card: huge, bar: BAR, gap: GAP
  })
  assert.equal(placement.x, region(BAR).left)
  assert.equal(placement.y, region(BAR).top)
})

// --------------------------------------------------------------- fallback

// The desktop: nothing was focused, so there is nothing to open beside and the
// bar corner is what is left. This is the placement the popup always had.
test("no source window keeps the bar corner, for every bar position", () => {
  const wanted = {
    top: { x: BOUNDS.width - CARD.width - GAP, y: GAP + 24 },
    bottom: { x: BOUNDS.width - CARD.width - GAP, y: BOUNDS.height - CARD.height - GAP - 24 },
    left: { x: GAP + 24, y: GAP },
    right: { x: BOUNDS.width - CARD.width - GAP - 24, y: GAP }
  }
  for (const position of ["top", "bottom", "left", "right"]) {
    const bar = { position: position, size: 24 }
    const placement = placeCard({
      window: null, origin: ORIGIN, bounds: BOUNDS, card: CARD, bar: bar, gap: GAP
    })
    assert.equal(placement.mode, FALLBACK, position)
    assert.deepEqual({ x: placement.x, y: placement.y }, wanted[position], position)
    assertOnScreen(placement, bar)
  }
})

// The overlay covers one monitor. A window on the other one is as good as no
// window: there is nothing on this surface to open beside.
test("a window on another monitor falls back to the bar corner", () => {
  const other = { address: "0x8a", x: 2412, y: 36, width: 1181, height: 1302 }
  const placement = place({ window: other })
  assert.equal(placement.mode, FALLBACK)
})

// The same window, seen from the overlay that covers that other monitor.
test("the surface origin is what puts a window on this monitor", () => {
  const other = { address: "0x8a", x: 2412, y: 36, width: 1181, height: 1302 }
  const placement = place({ window: other, origin: { x: 2400, y: 0 } })
  assert.equal(placement.mode, ADJACENT)
  assert.equal(placement.x, 12 + 1181 + GAP)
  assert.equal(placement.y, region(BAR).top)
  assertOnScreen(placement)
})

// ------------------------------------------------- reading the compositor

test("an activewindow answer reads as the window that held the Selection", () => {
  const window = readActiveWindow(JSON.stringify({
    address: "0x5646e7c0ee90", at: [12, 36], size: [1181, 1302], class: "alacritty"
  }))
  assert.deepEqual(window, { address: "0x5646e7c0ee90", x: 12, y: 36, width: 1181, height: 1302 })
  assert.equal(windowAddress(window), "0x5646e7c0ee90")
})

// Hyprland answers `{}` on the desktop, and a missing `hyprctl` answers
// nothing at all. Both mean there is no source window.
test("no active window reads as none", () => {
  for (const stdout of ["{}", "", "\n", "null", "[]", "not json", "hyprctl: not found"]) {
    assert.equal(readActiveWindow(stdout), null, JSON.stringify(stdout))
  }
})

test("an answer missing its geometry or its address reads as none", () => {
  const cases = [
    { address: "0x1a" },
    { address: "0x1a", at: [12, 36] },
    { address: "0x1a", at: [12, 36], size: [0, 100] },
    { address: "0x1a", at: ["x", 36], size: [10, 10] },
    { at: [12, 36], size: [10, 10] },
    { address: "window one", at: [12, 36], size: [10, 10] }
  ]
  for (const value of cases) assert.equal(readActiveWindow(JSON.stringify(value)), null, JSON.stringify(value))
})

test("no window means no address to dispatch to", () => {
  assert.equal(windowAddress(null), "")
  assert.equal(windowAddress({ address: "" }), "")
  assert.equal(windowAddress({ address: "0x1; rm -rf /" }), "")
})

// Omarchy answers `configProvider: lua`, so the dispatch is Lua rather than
// the `focuswindow address:<addr>` line the `.conf` provider takes.
test("the focus dispatch names the window by address, in the Lua the shell takes", () => {
  assert.deepEqual(focusCommand("0x5646e7c0ee90"), [
    "hyprctl", "dispatch", 'hl.dsp.focus({ window = "address:0x5646e7c0ee90" })'
  ])
})

// Nothing but a compositor-shaped address reaches that Lua.
test("anything that is not an address dispatches nothing", () => {
  for (const address of ["", "0x", "window", '0x1a" }) os.execute("boom', "0x1a 0x2b", null, undefined]) {
    assert.deepEqual(focusCommand(address), [], JSON.stringify(address))
  }
})

test("the focus is verified against the address that was asked for", () => {
  const answer = JSON.stringify({ address: "0x1a", at: [0, 0], size: [10, 10] })
  const other = JSON.stringify({ address: "0x2b", at: [0, 0], size: [10, 10] })
  assert.equal(isFocused(answer, "0x1a"), true)
  assert.equal(isFocused(other, "0x1a"), false)
  assert.equal(isFocused("{}", "0x1a"), false)
  assert.equal(isFocused(answer, ""), false)
})

// ------------------------------------------- the Replace, spec section 6
//
// The stub below is a `hyprctl` that keeps its own idea of which window has
// the keyboard, so the two steps below are the route from one Apply to either
// a paste or the notice. It records every call, which is what says the ask
// came before the check and the check before the type.

const stubDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "grammachy-anchor-"))

// A `hyprctl` that answers `activewindow -j` from a state file and moves that
// state on a `dispatch` naming a window it knows about. `alive` is the list of
// addresses the compositor still has.
function hyprctlStub(name, focused, alive) {
  const state = path.join(stubDirectory, name + ".state")
  const log = path.join(stubDirectory, name + ".log")
  const file = path.join(stubDirectory, name)
  fs.writeFileSync(state, focused)
  fs.writeFileSync(log, "")
  fs.writeFileSync(file, [
    "#!/usr/bin/env node",
    'const fs = require("fs")',
    "const STATE = " + JSON.stringify(state),
    "const LOG = " + JSON.stringify(log),
    "const ALIVE = " + JSON.stringify(alive),
    'const args = process.argv.slice(2)',
    'fs.appendFileSync(LOG, args.join(" ") + "\\n")',
    'if (args[0] === "dispatch") {',
    '  const found = args[1].match(/address:(0x[0-9a-f]+)/)',
    // A window that is gone is a warning and an exit 0, which is why the
    // caller may not read anything into the exit status.
    '  if (found && ALIVE.indexOf(found[1]) !== -1) fs.writeFileSync(STATE, found[1])',
    '  else process.stderr.write("hl.focus: window not found\\n")',
    "  process.exit(0)",
    "}",
    'if (args[0] === "activewindow") {',
    '  const address = fs.readFileSync(STATE, "utf8").trim()',
    '  if (address.length === 0) { process.stdout.write("{}"); process.exit(0) }',
    '  process.stdout.write(JSON.stringify({ address: address, at: [12, 36], size: [1181, 1302] }))',
    "  process.exit(0)",
    "}",
    'process.stderr.write("unknown\\n")',
    "process.exit(1)",
    ""
  ].join("\n"))
  fs.chmodSync(file, 0o755)
  return { binary: file, log: log }
}

function calls(stub) {
  return fs.readFileSync(stub.log, "utf8").split("\n").filter(line => line.length > 0)
}

// The Replace of spec section 6, driven in the order `Overlay.qml` drives it:
// the popup is already closed and the Corrected text is already on the
// clipboard, so what is left is the ask, the check, and the type.
// `cli/tests/overlay_anchor.rs` is what keeps `Overlay.qml` on these steps.
function replaceInto(stub, sourceWindow) {
  const run = { pasted: false, notice: null }
  const address = windowAddress(sourceWindow)
  if (address.length === 0) {
    run.pasted = true
    return run
  }

  const command = focusCommand(address)
  spawnSync(stub.binary, command.slice(1), { encoding: "utf8" })

  const query = activeWindowCommand()
  const answer = spawnSync(stub.binary, query.slice(1), { encoding: "utf8" }).stdout
  if (isFocused(answer, address)) {
    run.pasted = true
    return run
  }
  run.notice = { title: SOURCE_GONE_TITLE, body: SOURCE_GONE_BODY }
  return run
}

// The acceptance criterion: another window has the keyboard when the card
// closes, and the paste still lands in the window the Selection came from.
test("Replace asks for the source window and only types once it has it", () => {
  const stub = hyprctlStub("source-alive", RIGHT.address, [LEFT.address, RIGHT.address])
  const run = replaceInto(stub, LEFT)

  assert.equal(run.pasted, true)
  assert.equal(run.notice, null)
  assert.deepEqual(calls(stub), [
    'dispatch hl.dsp.focus({ window = "address:0x1a" })',
    "activewindow -j"
  ])
})

// Spec section 6: nothing is typed into a window that never held the
// Selection, and the Corrected text is on the clipboard either way.
test("a source window that is gone gets the notice and no paste at all", () => {
  const stub = hyprctlStub("source-gone", RIGHT.address, [RIGHT.address])
  const run = replaceInto(stub, LEFT)

  assert.equal(run.pasted, false)
  assert.equal(run.notice.title, "The source window closed")
  assert.ok(run.notice.body.indexOf("clipboard") !== -1, run.notice.body)
  // The ask went out and was answered before anything decided not to type.
  assert.deepEqual(calls(stub), [
    'dispatch hl.dsp.focus({ window = "address:0x1a" })',
    "activewindow -j"
  ])
})

// Nothing was focused at capture time, so there is no window to ask for and
// no promise to keep. The paste goes out as it always did.
test("no source window at capture leaves the paste as it was", () => {
  const stub = hyprctlStub("no-source", "", [])
  const run = replaceInto(stub, null)

  assert.equal(run.pasted, true)
  assert.deepEqual(calls(stub), [])
})
