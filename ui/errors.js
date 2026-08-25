// The error cards of spec section 8.
//
// Loaded twice: by Overlay.qml through `import "errors.js" as Errors`, and by
// `errors.test.js` under node. Nothing here may touch a QML or a node API,
// because each side only has one of them.
//
// This file owns the whole route from one run of `grammachy check` to the card
// the popup draws: `errorOf` reads the stdout of spec section 5.1, and `card`
// turns a code into the title, the body, and the buttons of section 8. Keeping
// both halves here is what lets a node test run a stub binary and read the
// card back, which no test of the QML could do.

var CONTRACT_VERSION = 1

// The codes of spec section 5.1, in the order section 8 lists their cards.
var EMPTY_SELECTION = "empty_selection"
var TEXT_TOO_LONG = "text_too_long"
var ENGINE_UNAVAILABLE = "engine_unavailable"
var ENGINE_TIMEOUT = "engine_timeout"
var ENGINE_ERROR = "engine_error"
var BAD_ARGUMENTS = "bad_arguments"

var CODES = [
  EMPTY_SELECTION,
  TEXT_TOO_LONG,
  ENGINE_UNAVAILABLE,
  ENGINE_TIMEOUT,
  ENGINE_ERROR,
  BAD_ARGUMENTS
]

// The buttons a card can carry, spec section 8, with the label each one shows.
var CLOSE = "close"
var RETRY = "retry"
var SETTINGS = "settings"
var SETUP = "setup"
var COMPOSE = "compose"

var BUTTON_LABELS = {
  close: "Close",
  retry: "Retry",
  settings: "Settings",
  setup: "Setup",
  compose: "Open Compose"
}

// The Check timeout of each engine, in seconds. The `engine_timeout` body
// names it, and a run that never answered leaves the shell nothing to read it
// from, so the popup carries its own copy. `cli/tests/overlay_errors.rs` keeps
// the copy in step with the `DEFAULT_TIMEOUT` of each adapter.
var TIMEOUT_SECONDS = {
  languagetool: 10,
  openai: 90,
  harper: 10
}

// What an engine with no entry above would wait. Every slug the Settings layer
// hands over is one of the three keys, so this only guards a hand-edited file.
var FALLBACK_TIMEOUT_SECONDS = 10

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value)
}

function timeoutSeconds(engineSlug) {
  var seconds = TIMEOUT_SECONDS[String(engineSlug)]
  return typeof seconds === "number" ? seconds : FALLBACK_TIMEOUT_SECONDS
}

// The code the shell draws a card for. Spec section 5.1: an unknown code reads
// as `engine_error`, because the Check did not finish and Retry is still the
// right offer.
function known(code) {
  var value = String(code)
  return CODES.indexOf(value) === -1 ? ENGINE_ERROR : value
}

// What one run of `grammachy check` left on stdout, spec section 5.1.
//
// The answer carries either `result`, the envelope whose Issues the popup
// draws, or `error`, the code and message a card is made from. Exactly one of
// the two is set, so the caller parses the stdout once and never twice.
//
// A missing or mismatched `contractVersion` and no JSON at all both read as
// `bad_arguments`, because both say the same thing about the companion tool:
// it is missing or out of date, and the card that offers Setup is the one that
// helps. Neither leaves a message the shell could trust.
function readCheck(stdout) {
  var envelope = null
  try {
    envelope = JSON.parse(stdout)
  } catch (error) {
    envelope = null
  }

  if (!isPlainObject(envelope) || envelope.contractVersion !== CONTRACT_VERSION)
    return { result: null, error: { code: BAD_ARGUMENTS, message: "" } }

  if (!isPlainObject(envelope.error)) return { result: envelope, error: null }

  return {
    result: null,
    error: {
      code: String(envelope.error.code || ""),
      message: typeof envelope.error.message === "string" ? envelope.error.message : ""
    }
  }
}

// The card one code shows, spec section 8, or `null` for `text_too_long`,
// whose card is the one of section 6 and belongs to the quick popup.
//
// `engineLabel` is the display name of the current engine setting rather than
// of the engine that ran: the card offers Settings, so the reader has to
// recognise the engine that dropdown names. `message` is what the CLI said,
// which every card prints in monospace under the body.
//
// `needsDiagnosis` asks the caller for the one-line `grammachy doctor` answer,
// which only the `engine_unavailable` card shows.
function card(code, options) {
  var context = isPlainObject(options) ? options : ({})
  var settled = known(code)
  if (settled === TEXT_TOO_LONG) return null

  var engine = context.engineLabel ? String(context.engineLabel) : "The engine"
  var message = typeof context.message === "string" ? context.message : ""
  var seconds = timeoutSeconds(context.engineSlug)

  var model = {
    code: settled,
    title: "",
    meta: "",
    body: "",
    message: message,
    needsDiagnosis: false,
    buttons: [],
    // The one button the card leads with, drawn in the accent colour. It is
    // Retry wherever Retry is offered, because running the Check again is what
    // the reader came to do.
    primary: ""
  }

  if (settled === EMPTY_SELECTION) {
    model.title = "Nothing selected"
    model.meta = "nothing to check"
    model.body = "Highlight some text, then press SUPER + G."
    model.buttons = [CLOSE, COMPOSE]
    model.primary = COMPOSE
    return model
  }

  if (settled === ENGINE_UNAVAILABLE) {
    model.title = engine + " is not running"
    model.meta = "engine not reachable"
    model.body = "Grammachy could not reach it on this machine."
    model.needsDiagnosis = true
    model.buttons = [CLOSE, RETRY, SETTINGS]
    model.primary = RETRY
    return model
  }

  if (settled === ENGINE_TIMEOUT) {
    model.title = engine + " took too long"
    model.meta = "engine timed out"
    model.body = "No answer within " + seconds + " s. A first start can take a moment."
    model.buttons = [CLOSE, RETRY, SETTINGS]
    model.primary = RETRY
    return model
  }

  if (settled === BAD_ARGUMENTS) {
    model.title = "Grammachy could not run the check"
    model.meta = "the check did not run"
    model.body = "The companion tool is missing or out of date."
    model.buttons = [CLOSE, SETUP]
    model.primary = SETUP
    return model
  }

  model.title = engine + " returned an error"
  model.meta = "engine failed"
  model.body = "The Check did not finish."
  model.buttons = [CLOSE, RETRY, SETTINGS]
  model.primary = RETRY
  return model
}

function buttonLabel(action) {
  var label = BUTTON_LABELS[String(action)]
  return typeof label === "string" ? label : ""
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    CONTRACT_VERSION: CONTRACT_VERSION,
    EMPTY_SELECTION: EMPTY_SELECTION,
    TEXT_TOO_LONG: TEXT_TOO_LONG,
    ENGINE_UNAVAILABLE: ENGINE_UNAVAILABLE,
    ENGINE_TIMEOUT: ENGINE_TIMEOUT,
    ENGINE_ERROR: ENGINE_ERROR,
    BAD_ARGUMENTS: BAD_ARGUMENTS,
    CODES: CODES,
    CLOSE: CLOSE,
    RETRY: RETRY,
    SETTINGS: SETTINGS,
    SETUP: SETUP,
    COMPOSE: COMPOSE,
    BUTTON_LABELS: BUTTON_LABELS,
    TIMEOUT_SECONDS: TIMEOUT_SECONDS,
    timeoutSeconds: timeoutSeconds,
    known: known,
    readCheck: readCheck,
    card: card,
    buttonLabel: buttonLabel
  }
}
