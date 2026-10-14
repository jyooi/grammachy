var ADJACENT = "adjacent"
var INSIDE = "inside"
var FALLBACK = "fallback"

var SOURCE_GONE_TITLE = "The source window closed"
var SOURCE_GONE_BODY = "Nothing was replaced. The corrected text is on the clipboard, so you can still paste it where you want it."
var SOURCE_GONE_META = "not replaced"

var ADDRESS = /^0x[0-9a-fA-F]+$/

function isFiniteNumber(value) {
  return typeof value === "number" && isFinite(value)
}

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

function activeWindowCommand() {
  return ["hyprctl", "activewindow", "-j"]
}

// Omarchy answers `configProvider: lua`, so `hyprctl dispatch` reads its
// argument as Lua rather than as the `focuswindow address:<addr>` line the
// `.conf` provider takes.
function focusCommand(address) {
  if (!ADDRESS.test(String(address))) return []
  return ["hyprctl", "dispatch", 'hl.dsp.focus({ window = "address:' + address + '" })']
}

// The dispatch exits 0 even for a window that is gone.
function isFocused(stdout, address) {
  if (!ADDRESS.test(String(address))) return false
  var window = readActiveWindow(stdout)
  return window !== null && window.address === address
}

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

function clamp(value, low, high) {
  if (high < low) return low
  return Math.min(Math.max(value, low), high)
}

function barCorner(region, card, position) {
  return {
    x: position === "left" ? region.left : region.right - card.width,
    y: position === "bottom" ? region.bottom - card.height : region.top
  }
}

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

function placeCard(options) {
  var bounds = options.bounds
  var card = options.card
  var gap = Math.max(0, Number(options.gap) || 0)
  var region = regionOf(bounds, options.bar, gap)
  var barPosition = options.bar && options.bar.position ? String(options.bar.position) : "top"
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
