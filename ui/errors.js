// Loaded twice: by QML as `Errors`, and by `errors.test.js` under node.
// Nothing here may touch a QML or a node API, because each side only has one
// of them.

var CONTRACT_VERSION = 1

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

var CLOSE = "close"
var RETRY = "retry"
var SETTINGS = "settings"
var SETUP = "setup"
var COMPOSE = "compose"
var RETRY_REMAINING = "retryRemaining"
var REVIEW_PARTIAL = "reviewPartial"

var BUTTON_LABELS = {
  close: "Close",
  retry: "Retry",
  settings: "Settings",
  setup: "Setup",
  compose: "Open Compose",
  retryRemaining: "Retry remaining",
  reviewPartial: "Review what we have"
}

var TIMEOUT_SECONDS = {
  languagetool: 10,
  harper: 10
}

var FALLBACK_TIMEOUT_SECONDS = 10

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value)
}

function timeoutSeconds(engineSlug) {
  var seconds = TIMEOUT_SECONDS[String(engineSlug)]
  return typeof seconds === "number" ? seconds : FALLBACK_TIMEOUT_SECONDS
}

function known(code) {
  var value = String(code)
  return CODES.indexOf(value) === -1 ? ENGINE_ERROR : value
}

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
    primary: ""
  }

  if (settled === EMPTY_SELECTION) {
    model.title = "Nothing selected"
    model.meta = "nothing to check"
    model.body = "Highlight some text, then press SUPER + SHIFT + Q."
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

function readChunks(stdout) {
  var envelope = null
  try {
    envelope = JSON.parse(stdout)
  } catch (error) {
    envelope = null
  }

  if (!isPlainObject(envelope) || envelope.contractVersion !== CONTRACT_VERSION)
    return { chunks: null, error: { code: BAD_ARGUMENTS, message: "" } }

  if (isPlainObject(envelope.error)) {
    return {
      chunks: null,
      error: {
        code: String(envelope.error.code || ""),
        message: typeof envelope.error.message === "string" ? envelope.error.message : ""
      }
    }
  }

  if (!Array.isArray(envelope.chunks))
    return { chunks: null, error: { code: BAD_ARGUMENTS, message: "" } }

  return { chunks: envelope.chunks, error: null }
}

function chunkCard(code, options) {
  var context = isPlainObject(options) ? options : ({})
  var settled = known(code)
  var model = card(settled === TEXT_TOO_LONG ? ENGINE_ERROR : settled, context)

  model.buttons = context.hasPartial === true
    ? [RETRY_REMAINING, REVIEW_PARTIAL]
    : [CLOSE, RETRY_REMAINING, SETTINGS]
  model.primary = RETRY_REMAINING
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
    RETRY_REMAINING: RETRY_REMAINING,
    REVIEW_PARTIAL: REVIEW_PARTIAL,
    BUTTON_LABELS: BUTTON_LABELS,
    TIMEOUT_SECONDS: TIMEOUT_SECONDS,
    timeoutSeconds: timeoutSeconds,
    known: known,
    readCheck: readCheck,
    readChunks: readChunks,
    card: card,
    chunkCard: chunkCard,
    buttonLabel: buttonLabel
  }
}
