// The Models list of the Settings view, spec sections 5.3 and 7.
//
// Loaded twice: by QML as `Models`, and by `models.test.js` under node.
// Nothing here may touch a QML or a node API, because each side only has one
// of them.
//
// This file owns the whole route from one run of `grammachy model` to the rows
// `ui/ModelsView.qml` draws: `read` reads the envelope of spec section 5.3,
// `rows` turns it into what a row needs, and `note` turns a failure into the
// one line the view shows under the list. Keeping every half here is what lets
// a node test run a stub binary and read the rows back, which no test of the
// QML could do.

var CONTRACT_VERSION = 1

// The states one catalogue row can be in, spec section 5.3.
var ABSENT = "absent"
var PARTIAL = "partial"
var READY = "ready"
var STATES = [ABSENT, PARTIAL, READY]

// The codes `grammachy model` can answer. The first two are its own; the third
// is the shared code of spec section 5.1.
var CANCELLED = "cancelled"
var DOWNLOAD_FAILED = "download_failed"
var BAD_ARGUMENTS = "bad_arguments"

// What a row offers, spec section 7. The verb is the tooltip and the name is in
// the hint line, so the button itself carries an icon and no text label.
var DOWNLOAD = "download"
var CANCEL = "cancel"
var USE = "use"
var REMOVE = "remove"

// The icon of each action, from the set `ui/CardHero.qml` already draws.
var ACTION_ICONS = {
  download: "󰇚",
  cancel: "󰅖",
  use: "󰄬",
  remove: "󰩹"
}

var ACTION_TOOLTIPS = {
  download: "Download",
  cancel: "Cancel the download",
  use: "Use this model",
  remove: "Remove"
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value)
}

// The state one row reports, or `absent` for anything the contract does not
// list. An unknown state is the same statement as no file: nothing here can be
// downloaded twice, and offering Download is the one useful answer.
function stateOf(row) {
  if (!isPlainObject(row)) return ABSENT
  var state = String(row.state)
  return STATES.indexOf(state) === -1 ? ABSENT : state
}

// What one run of `grammachy model` left on stdout, spec section 5.3.
//
// The answer carries either `report`, whose rows the view draws, or `error`,
// the code and message the note is made from. Exactly one of the two is set.
//
// A missing or mismatched `contractVersion` and no JSON at all both read as
// `bad_arguments`, because both say the same thing about the companion tool:
// it is missing or out of date. Neither leaves a message the shell could trust.
function read(stdout) {
  var envelope = null
  try {
    envelope = JSON.parse(stdout)
  } catch (error) {
    envelope = null
  }

  if (!isPlainObject(envelope) || envelope.contractVersion !== CONTRACT_VERSION)
    return { report: null, error: { code: BAD_ARGUMENTS, message: "" } }

  if (isPlainObject(envelope.error)) {
    return {
      report: null,
      error: {
        code: String(envelope.error.code || ""),
        message: typeof envelope.error.message === "string" ? envelope.error.message : ""
      }
    }
  }

  if (!Array.isArray(envelope.models))
    return { report: null, error: { code: BAD_ARGUMENTS, message: "" } }

  return {
    report: {
      verb: String(envelope.verb || ""),
      directory: String(envelope.directory || ""),
      freeBytes: Number(envelope.freeBytes) || 0,
      models: rows(envelope.models)
    },
    error: null
  }
}

// One drawable row per envelope row, with everything the view needs already
// decided. A row without a name is dropped: nothing can be done with it.
function rows(models) {
  var out = []
  for (var i = 0; i < models.length; i++) {
    var row = models[i]
    if (!isPlainObject(row) || typeof row.name !== "string" || row.name.length === 0) continue
    out.push({
      name: row.name,
      fileName: typeof row.fileName === "string" ? row.fileName : "",
      state: stateOf(row),
      partialBytes: Number(row.partialBytes) || 0,
      sizeBytes: Number(row.sizeBytes) || 0,
      licence: typeof row.licence === "string" ? row.licence : ""
    })
  }
  return out
}

// The rows of a `download` or a `remove` answer merged into the list on screen.
//
// Those two verbs answer with the one row they acted on, so the list keeps
// every other row rather than shrinking to one. A name the list does not carry
// is appended, which only a catalogue change between two runs could cause.
function merged(current, answered) {
  var next = Array.isArray(current) ? current.slice() : []
  var incoming = Array.isArray(answered) ? answered : []
  for (var i = 0; i < incoming.length; i++) {
    var found = -1
    for (var j = 0; j < next.length; j++) if (next[j].name === incoming[i].name) found = j
    if (found === -1) next.push(incoming[i])
    else next[found] = incoming[i]
  }
  return next
}

// A byte count as the list says it, spec section 7.
//
// Weights are gigabytes, so the unit is chosen per number and one decimal is
// kept: `2.5 GB` is a size the reader compares at a glance, and `2497281120`
// is not. The step is 1024, because that is what a file manager shows for the
// same file.
var UNITS = ["B", "KB", "MB", "GB", "TB"]

function bytes(count) {
  var value = Math.max(0, Number(count) || 0)
  var index = 0
  while (value >= 1024 && index < UNITS.length - 1) {
    value = value / 1024
    index += 1
  }
  // Whole bytes have no fraction to show, so the unit decides the shape.
  if (index === 0) return Math.round(value) + " B"
  return (Math.round(value * 10) / 10).toFixed(1) + " " + UNITS[index]
}

// How far a download has got, from 0 to 1. A row with no pinned size cannot be
// measured, so it reads as nothing done rather than as done.
function share(row) {
  var size = isPlainObject(row) ? Number(row.sizeBytes) || 0 : 0
  if (size <= 0) return 0
  var done = Number(row.partialBytes) || 0
  if (stateOf(row) === READY) return 1
  return Math.max(0, Math.min(1, done / size))
}

// The one line under a row's name, spec section 7. It names the licence and the
// size always, because those are what the reader chooses between, and it names
// the progress only while there is progress to name.
function hint(row, busy) {
  var state = stateOf(row)
  var size = bytes(isPlainObject(row) ? row.sizeBytes : 0)
  var licence = isPlainObject(row) && row.licence ? String(row.licence) : "unknown licence"

  if (busy === true) {
    var done = bytes(isPlainObject(row) ? row.partialBytes : 0)
    return "Downloading " + done + " of " + size + ", " + Math.round(share(row) * 100) + "%"
  }
  if (state === READY) return "Ready, " + size + ", " + licence
  if (state === PARTIAL)
    return "Part downloaded, " + bytes(row.partialBytes) + " of " + size + ", " + licence
  return "Not downloaded, " + size + ", " + licence
}

// Which catalogue row the `openaiModel` setting names, spec section 7.
//
// `unit::model_file` in the CLI is the authority, because it is what a Check
// resolves the setting with: the exact `<setting>.gguf` wins, and failing that
// the first `.gguf` whose name begins with the setting, ignoring case. Only a
// `ready` row has a `.gguf` on disk, so only a `ready` row can be the answer.
// Mirroring the rule here is what keeps the "in use" mark and the Remove
// confirm on the file the next Check would load, rather than on a name that
// happens to be spelled the same way.
function resolvedName(models, setting) {
  var wanted = String(setting || "")
  if (wanted.length === 0) return ""
  var lowered = wanted.toLowerCase()
  var list = Array.isArray(models) ? models : []
  var candidates = []

  for (var i = 0; i < list.length; i++) {
    var row = list[i]
    if (!isPlainObject(row) || stateOf(row) !== READY) continue
    var fileName = typeof row.fileName === "string" ? row.fileName : ""
    if (fileName.length === 0) continue
    if (fileName === wanted + ".gguf") return String(row.name)
    if (fileName.toLowerCase().indexOf(lowered) === 0) candidates.push(row)
  }

  // Several files can begin with the same name, and the CLI sorts them and
  // takes the first, so the same one wins here.
  candidates.sort(function (left, right) {
    if (left.fileName === right.fileName) return 0
    return left.fileName < right.fileName ? -1 : 1
  })
  return candidates.length > 0 ? String(candidates[0].name) : ""
}

// Whether this row is the one the setting resolves to.
function resolves(row, setting, models) {
  var name = isPlainObject(row) ? String(row.name) : ""
  if (name.length === 0) return false
  return resolvedName(models, setting) === name
}

// Which buttons one row carries, spec section 7.
//
// The row a download is running on offers Cancel alone: Use and Remove are
// about a file that is not there yet. Every other row keeps the buttons it
// would carry anyway, and `isBlocked` is what draws them disabled while one
// download holds the single verb the CLI runs at a time. The list never shifts
// under a click that way.
//
// Use is offered on a Ready row the setting does not resolve to, because
// picking the model that is already picked does nothing.
function actions(row, options) {
  var context = isPlainObject(options) ? options : ({})
  var state = stateOf(row)
  var busyName = typeof context.busy === "string" ? context.busy : ""
  var name = isPlainObject(row) ? String(row.name) : ""

  if (busyName === name && name.length > 0) return [CANCEL]

  var out = []
  if (state === READY) {
    if (!resolves(row, context.setting, context.models)) out.push(USE)
    out.push(REMOVE)
    return out
  }

  out.push(DOWNLOAD)
  if (state === PARTIAL) out.push(REMOVE)
  return out
}

// Whether a row's buttons are drawn but cannot be pressed.
//
// One verb of `grammachy model` runs at a time, so while a download is in
// flight every other row's Download, Use, and Remove would be a dead click.
// They stay on the row and go dim instead, which says why nothing happens.
function isBlocked(row, options) {
  var context = isPlainObject(options) ? options : ({})
  var busyName = typeof context.busy === "string" ? context.busy : ""
  var name = isPlainObject(row) ? String(row.name) : ""
  return busyName.length > 0 && busyName !== name
}

function actionIcon(action) {
  var icon = ACTION_ICONS[String(action)]
  return typeof icon === "string" ? icon : ""
}

function actionTooltip(action, name) {
  var verb = ACTION_TOOLTIPS[String(action)]
  if (typeof verb !== "string") return ""
  return name ? verb + " " + name : verb
}

// What a note is: something that went wrong, or something that simply
// happened. A cancel is the reader's own decision, so drawing it in the colour
// of a failure would tell them they broke something.
var NOTICE = "notice"
var FAILURE = "failure"

// The one line the list shows after a verb, spec section 5.3.
//
// A cancel is not a failure and says so: the part file is kept and the same
// button starts it again. Everything else names what went wrong and leaves the
// CLI message under it, the way the error cards of section 8 do.
function note(code, message, name) {
  var settled = String(code)
  var model = name ? String(name) : "the model"
  var said = typeof message === "string" ? message : ""

  if (settled === CANCELLED)
    return {
      kind: NOTICE,
      title: "Download of " + model + " stopped",
      body: "What arrived is kept. Download resumes it.",
      message: ""
    }
  if (settled === DOWNLOAD_FAILED)
    return {
      kind: FAILURE,
      title: model + " could not be downloaded",
      body: "Nothing was installed. Download tries again.",
      message: said
    }
  if (settled === BAD_ARGUMENTS && said.length === 0)
    return {
      kind: FAILURE,
      title: "Grammachy could not read the model list",
      body: "The companion tool is missing or out of date.",
      message: ""
    }
  return {
    kind: FAILURE,
    title: "Grammachy could not finish that",
    body: "The models on disk did not change.",
    message: said
  }
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    CONTRACT_VERSION: CONTRACT_VERSION,
    ABSENT: ABSENT,
    PARTIAL: PARTIAL,
    READY: READY,
    STATES: STATES,
    NOTICE: NOTICE,
    FAILURE: FAILURE,
    CANCELLED: CANCELLED,
    DOWNLOAD_FAILED: DOWNLOAD_FAILED,
    BAD_ARGUMENTS: BAD_ARGUMENTS,
    DOWNLOAD: DOWNLOAD,
    CANCEL: CANCEL,
    USE: USE,
    REMOVE: REMOVE,
    ACTION_ICONS: ACTION_ICONS,
    ACTION_TOOLTIPS: ACTION_TOOLTIPS,
    stateOf: stateOf,
    read: read,
    rows: rows,
    merged: merged,
    bytes: bytes,
    share: share,
    hint: hint,
    resolvedName: resolvedName,
    resolves: resolves,
    actions: actions,
    isBlocked: isBlocked,
    actionIcon: actionIcon,
    actionTooltip: actionTooltip,
    note: note
  }
}
