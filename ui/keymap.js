// The key map of both surfaces, spec sections 6 and 9.
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
var CHECK = "check"
var BACK = "back"
// The Remove confirm of the Models list, spec section 7.
var REMOVE_MODEL = "removeModel"
var KEEP_MODEL = "keepModel"
// The cloud consent card, `docs/spec/evals.md` section 7.
var CLOUD_CONTINUE = "cloudContinue"
var CLOUD_CANCEL = "cloudCancel"

// Which card the press landed on. The popup review keys are the whole map;
// Compose reuses them in its own review mode and keeps almost nothing in edit
// mode, because there the Draft text area owns every printable key.
var MODE_IDLE = "idle"
var MODE_REVIEW = "review"
var MODE_COMPOSE_EDIT = "composeEdit"
var MODE_COMPOSE_REVIEW = "composeReview"
// The Remove confirm of the Models list, spec section 7. It sits over the
// Settings view, which owns every other key, so this mode carries the two
// answers to that one question and nothing else.
var MODE_MODEL_CONFIRM = "modelConfirm"
// The cloud consent card of `docs/spec/evals.md` section 7. It stands in front
// of the first cloud Check, so it carries the two answers to that one question
// and nothing else.
var MODE_CLOUD_CONSENT = "cloudConsent"

// The action a key press asks for, or NONE.
//
// Esc works from every card, because a summoned overlay that cannot be
// dismissed from the keyboard is a trap. What it means is the mode's: it
// leaves the popup, and it leaves Compose review for the Draft behind it.
// Availability is the caller's call: an action that is asked for while it is
// disabled is a no-op there, not a different action.
function action(event, codes, mode) {
  if (!event || !codes) return NONE

  var key = event.key
  var modifiers = Number(event.modifiers) || 0
  var control = (modifiers & codes.control) !== 0
  // Shift is not in the mask: a reader who sees `A` in the map may well hold
  // it down. Alt and Meta belong to the compositor, so they never land here.
  var foreign = (modifiers & (codes.alt | codes.meta)) !== 0
  var enter = key === codes.enter || key === codes.returnKey

  // The confirm is a question, so Esc answers it rather than leaving the card
  // with the question still open behind it. Only a bare Enter answers the
  // other way: Ctrl + Enter is Apply on every other card, and a reader who
  // pressed it out of habit did not ask for a model to be deleted.
  if (mode === MODE_MODEL_CONFIRM) {
    if (key === codes.escape) return KEEP_MODEL
    if (foreign || control) return NONE
    return enter ? REMOVE_MODEL : NONE
  }

  // The consent card is a question too, and it is answered the same way: Esc
  // sends nothing, and only a bare Enter lets the text go. Ctrl + Enter is
  // Apply on every other card, so a reader who pressed it out of habit never
  // sends their text to a cloud by accident.
  if (mode === MODE_CLOUD_CONSENT) {
    if (key === codes.escape) return CLOUD_CANCEL
    if (foreign || control) return NONE
    return enter ? CLOUD_CONTINUE : NONE
  }

  if (key === codes.escape) return mode === MODE_COMPOSE_REVIEW ? BACK : CLOSE
  if (foreign) return NONE

  // Edit mode hands every other key to the text area, so that typing a draft
  // is typing a draft. Ctrl + Enter is the one key the card keeps.
  if (mode === MODE_COMPOSE_EDIT) return control && enter ? CHECK : NONE

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
    REMOVE_MODEL: REMOVE_MODEL,
    KEEP_MODEL: KEEP_MODEL,
    CLOUD_CONTINUE: CLOUD_CONTINUE,
    CLOUD_CANCEL: CLOUD_CANCEL,
    MODE_IDLE: MODE_IDLE,
    MODE_REVIEW: MODE_REVIEW,
    MODE_COMPOSE_EDIT: MODE_COMPOSE_EDIT,
    MODE_COMPOSE_REVIEW: MODE_COMPOSE_REVIEW,
    MODE_MODEL_CONFIRM: MODE_MODEL_CONFIRM,
    MODE_CLOUD_CONSENT: MODE_CLOUD_CONSENT
  }
}
