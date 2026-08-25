// Where the quick popup sits, and how the Apply of spec section 6 finds the
// window the Selection came from.
//
// Both answers come from the same fact: the window that held the Selection
// when the Check was captured, spec section 3. The popup opens beside it so
// the card is near the text it is about, and Replace types into it rather
// than into whatever the compositor gave the keyboard to when the card went
// away.
//
// Loaded twice: by the QML overlay through `import "anchor.js" as Anchor`,
// and by `anchor.test.js` under node. Nothing here may touch a QML or a node
// API, because each side only has one of them.
//
// Every length is a logical pixel, which is what Hyprland reports for a window
// and what a Quickshell surface is measured in.

// How the card was placed, which is what a test reads back.
var ADJACENT = "adjacent"
var INSIDE = "inside"
var FALLBACK = "fallback"

// The notice of spec section 6 when the Selection's window is gone by the time
// Replace runs. The Corrected text is on the clipboard either way, so the card
// says where it went rather than only what failed.
var SOURCE_GONE_TITLE = "The source window closed"
var SOURCE_GONE_BODY = "Nothing was replaced. The corrected text is on the clipboard, so you can still paste it where you want it."
var SOURCE_GONE_META = "not replaced"

// A Hyprland window address, the one shape that may reach a dispatch.
var ADDRESS = /^0x[0-9a-fA-F]+$/

function isFiniteNumber(value) {
  return typeof value === "number" && isFinite(value)
}

// One `hyprctl activewindow -j` answer as the window that held the Selection,
// or null when nothing did. Hyprland answers `{}` on the desktop, and a
// missing `hyprctl` answers nothing at all; both mean the same thing here.
function readActiveWindow(stdout) {
  var value = null
  try {
    value = JSON.parse(stdout)
  } catch (error) {
    return null
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) return null

  var address = typeof value.address === "string" ? value.address : ""
  if (!ADDRESS.test(address)) return null
  if (!Array.isArray(value.at) || !Array.isArray(value.size)) return null

  var window = {
    address: address,
    x: Number(value.at[0]),
    y: Number(value.at[1]),
    width: Number(value.size[0]),
    height: Number(value.size[1])
  }
  if (!isFiniteNumber(window.x) || !isFiniteNumber(window.y)) return null
  if (!isFiniteNumber(window.width) || !isFiniteNumber(window.height)) return null
  if (window.width <= 0 || window.height <= 0) return null
  return window
}

function windowAddress(window) {
  if (!window || typeof window.address !== "string") return ""
  return ADDRESS.test(window.address) ? window.address : ""
}

// The query both steps of the Apply use. `hyprctl` is what Omarchy ships to
// talk to the compositor, and this answer is the only thing that says which
// window has the keyboard.
function activeWindowCommand() {
  return ["hyprctl", "activewindow", "-j"]
}

// Ask the compositor to give the keyboard back to one window.
//
// Omarchy answers `configProvider: lua`, so `hyprctl dispatch` reads its
// argument as Lua rather than as the `focuswindow address:<addr>` line the
// `.conf` provider takes. The address is checked against `ADDRESS` first, so
// nothing but a compositor-shaped address is ever pasted into that Lua.
function focusCommand(address) {
  if (!ADDRESS.test(String(address))) return []
  return ["hyprctl", "dispatch", 'hl.dsp.focus({ window = "address:' + address + '" })']
}

// Whether that ask worked. The dispatch exits 0 even for a window that is
// gone, so this answer, and not the exit status, is what decides the paste.
function isFocused(stdout, address) {
  if (!ADDRESS.test(String(address))) return false
  var window = readActiveWindow(stdout)
  return window !== null && window.address === address
}

// The part of the overlay surface a card may occupy: the whole surface less
// the bar it must not cover and one gap on every side.
function regionOf(bounds, bar, gap) {
  var position = bar && bar.position ? String(bar.position) : "top"
  var size = Math.max(0, Number(bar && bar.size) || 0)
  return {
    left: gap + (position === "left" ? size : 0),
    top: gap + (position === "top" ? size : 0),
    right: bounds.width - gap - (position === "right" ? size : 0),
    bottom: bounds.height - gap - (position === "bottom" ? size : 0)
  }
}

// A card wider or taller than the region has nowhere to fit, so the low edge
// wins and the card runs off the far one rather than off the near one.
function clamp(value, low, high) {
  if (high < low) return low
  return Math.min(Math.max(value, low), high)
}

// The bar corner the popup hung from before it knew about the source window,
// and where it still hangs when there is no source window to hang beside: the
// corner the bar widget itself sits in.
function barCorner(region, card, position) {
  return {
    x: position === "left" ? region.left : region.right - card.width,
    y: position === "bottom" ? region.bottom - card.height : region.top
  }
}

// The window rect in the overlay surface's own coordinates, or null when that
// window is not on this surface at all. Hyprland reports a window in the
// global layout, and the overlay covers one monitor of it.
function localWindow(window, origin, bounds) {
  if (!window) return null
  var local = {
    x: window.x - (Number(origin && origin.x) || 0),
    y: window.y - (Number(origin && origin.y) || 0),
    width: window.width,
    height: window.height
  }
  var overlaps = local.x < bounds.width && local.x + local.width > 0
    && local.y < bounds.height && local.y + local.height > 0
  return overlaps ? local : null
}

// Where the quick popup goes.
//
// `options.window` is the window that held the Selection, in the global
// layout, or null. `options.origin` is where this overlay surface starts in
// that layout, `options.bounds` is how big the surface is, `options.card` is
// how big the card is, and `options.bar` and `options.gap` are what the card
// may not cover.
//
// The card takes the source window's top edge on its trailing side, falls back
// to the leading side when the trailing one has no room, and sits inside the
// window when neither side does. Whatever comes out is clamped into the
// region, so no bar position and no window can push the card off the screen.
// With no source window on this surface the bar corner is what is left.
function placeCard(options) {
  var bounds = options.bounds
  var card = options.card
  var gap = Math.max(0, Number(options.gap) || 0)
  var region = regionOf(bounds, options.bar, gap)
  var barPosition = options.bar && options.bar.position ? String(options.bar.position) : "top"
  // The far edge a card may start at and still end inside the region.
  var lastX = region.right - card.width
  var lastY = region.bottom - card.height

  var local = localWindow(options.window, options.origin, bounds)
  if (local === null) {
    var corner = barCorner(region, card, barPosition)
    return {
      x: clamp(corner.x, region.left, lastX),
      y: clamp(corner.y, region.top, lastY),
      mode: FALLBACK
    }
  }

  var trailing = local.x + local.width + gap
  var leading = local.x - gap - card.width
  var x = trailing
  var mode = ADJACENT
  if (trailing + card.width > region.right) {
    if (leading >= region.left) x = leading
    else {
      // Neither side has room, so the card overlays the window it belongs to,
      // held to that window's own trailing edge.
      x = local.x + local.width - card.width - gap
      mode = INSIDE
    }
  }

  return {
    x: clamp(x, region.left, lastX),
    y: clamp(local.y, region.top, lastY),
    mode: mode
  }
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    ADJACENT: ADJACENT,
    INSIDE: INSIDE,
    FALLBACK: FALLBACK,
    SOURCE_GONE_TITLE: SOURCE_GONE_TITLE,
    SOURCE_GONE_BODY: SOURCE_GONE_BODY,
    SOURCE_GONE_META: SOURCE_GONE_META,
    readActiveWindow: readActiveWindow,
    windowAddress: windowAddress,
    activeWindowCommand: activeWindowCommand,
    focusCommand: focusCommand,
    isFocused: isFocused,
    placeCard: placeCard
  }
}
