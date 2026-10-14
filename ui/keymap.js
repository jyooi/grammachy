var NONE = ""
var CLOSE = "close"
var ACCEPT = "accept"
var SKIP = "skip"
var FOCUS_PREVIOUS = "focusPrevious"
var FOCUS_NEXT = "focusNext"
var ACCEPT_ALL = "acceptAll"
var COPY = "copy"
var APPLY = "apply"
var CHECK = "check"
var BACK = "back"
var CLEAR = "clear"
var REMOVE_ENGINE = "removeEngine"
var KEEP_ENGINE = "keepEngine"

var MODE_IDLE = "idle"
var MODE_REVIEW = "review"
var MODE_COMPOSE_EDIT = "composeEdit"
var MODE_COMPOSE_REVIEW = "composeReview"
var MODE_QUICK_CLEAR = "quickClear"
var MODE_ENGINE_CONFIRM = "engineConfirm"

function action(event, codes, mode) {
  if (!event || !codes) return NONE

  var key = event.key
  var modifiers = Number(event.modifiers) || 0
  var control = (modifiers & codes.control) !== 0
  var foreign = (modifiers & (codes.alt | codes.meta)) !== 0
  var enter = key === codes.enter || key === codes.returnKey

  if (mode === MODE_ENGINE_CONFIRM) {
    if (key === codes.escape) return KEEP_ENGINE
    if (foreign || control) return NONE
    return enter ? REMOVE_ENGINE : NONE
  }

  if (key === codes.escape) return mode === MODE_COMPOSE_REVIEW ? BACK : CLOSE
  if (foreign) return NONE

  if (mode === MODE_COMPOSE_EDIT) return control && enter ? CHECK : NONE

  if (control && key === codes.l)
    return mode === MODE_REVIEW || mode === MODE_QUICK_CLEAR ? CLEAR : NONE

  if (mode !== MODE_REVIEW && mode !== MODE_COMPOSE_REVIEW) return NONE

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
    APPLY: APPLY,
    CHECK: CHECK,
    BACK: BACK,
    CLEAR: CLEAR,
    REMOVE_ENGINE: REMOVE_ENGINE,
    KEEP_ENGINE: KEEP_ENGINE,
    MODE_IDLE: MODE_IDLE,
    MODE_REVIEW: MODE_REVIEW,
    MODE_COMPOSE_EDIT: MODE_COMPOSE_EDIT,
    MODE_COMPOSE_REVIEW: MODE_COMPOSE_REVIEW,
    MODE_QUICK_CLEAR: MODE_QUICK_CLEAR,
    MODE_ENGINE_CONFIRM: MODE_ENGINE_CONFIRM
  }
}
