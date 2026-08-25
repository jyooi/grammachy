// The quick popup key map, spec section 6.
//
// Loaded twice: by Overlay.qml through `import "keymap.js" as Keymap`, and by
// `keymap.test.js` under node. Nothing here may touch a QML or a node API,
// because each side only has one of them.
//
// The Qt key codes arrive in `codes` rather than as literals, because `Qt` is
// a QML global that node does not have. The overlay owns that one table; this
// file owns which action a key asks for.

var NONE = ""
var CLOSE = "close"
var ACCEPT = "accept"
var SKIP = "skip"
var FOCUS_PREVIOUS = "focusPrevious"
var FOCUS_NEXT = "focusNext"
var ACCEPT_ALL = "acceptAll"
var COPY = "copy"
var APPLY = "apply"

// The action a key press asks for, or NONE.
//
// `reviewing` is true only while the card shows Issues to decide on. Esc works
// from every card, because a summoned overlay that cannot be dismissed from
// the keyboard is a trap. Availability is the caller's call: an action that is
// asked for while it is disabled is a no-op there, not a different action.
function action(event, codes, reviewing) {
  if (!event || !codes) return NONE

  var key = event.key
  var modifiers = Number(event.modifiers) || 0
  var control = (modifiers & codes.control) !== 0
  // Shift is not in the mask: a reader who sees `A` in the map may well hold
  // it down. Alt and Meta belong to the compositor, so they never land here.
  var foreign = (modifiers & (codes.alt | codes.meta)) !== 0
  var enter = key === codes.enter || key === codes.returnKey

  if (key === codes.escape) return CLOSE
  if (!reviewing) return NONE
  if (foreign) return NONE

  if (control) {
    if (enter) return APPLY
    if (key === codes.c) return COPY
    return NONE
  }

  if (enter) return ACCEPT
  if (key === codes.space) return SKIP
  if (key === codes.up) return FOCUS_PREVIOUS
  if (key === codes.down) return FOCUS_NEXT
  if (key === codes.a) return ACCEPT_ALL
  return NONE
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    action: action,
    NONE: NONE,
    CLOSE: CLOSE,
    ACCEPT: ACCEPT,
    SKIP: SKIP,
    FOCUS_PREVIOUS: FOCUS_PREVIOUS,
    FOCUS_NEXT: FOCUS_NEXT,
    ACCEPT_ALL: ACCEPT_ALL,
    COPY: COPY,
    APPLY: APPLY
  }
}
