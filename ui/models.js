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

// Everything one answer changes on screen, or `null` for an answer that is
// already out of date.
//
// `list` answers every row, so its rows are the whole list. `download` and
// `remove` answer the one row they acted on, so a merge is what keeps the rest.
//
// A `list` run reads the directory the moment it starts, while `download`
// answers only after it has hashed the `.part` file and renamed it, which takes
// tens of seconds on a multi-gigabyte file. So a poll that fired during the
// hash is still in flight when the verb answers, and it truthfully reports the
// row as `partial` while the file on disk is already `ready`. `stamp` says
// which run answered and `floor` is the first run no verb has overtaken, so the
// older of the two loses and a finished row never goes back to `partial`.
function absorbed(current, report, stamp, floor) {
  if (!isPlainObject(report)) return null
  var verb = String(report.verb || "")
  if (verb === "list" && (Number(stamp) || 0) < (Number(floor) || 0)) return null
  var answered = Array.isArray(report.models) ? report.models : []
  return {
    models: verb === "list" ? answered : merged(current, answered),
    directory: String(report.directory || ""),
    freeBytes: Number(report.freeBytes) || 0
  }
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

// The `.part` length to draw for one row.
//
// The poll moves that one number once a second while a download runs, and the
// row it belongs to reads it from a property of its own rather than from the
// list. That is what lets the list keep its identity across a poll, so the bar
// animates rather than being rebuilt. A `live` below zero, or no `live` at all,
// means the list is the only answer there is.
function partialBytesOf(row, live) {
  var value = Number(live)
  if (isFinite(value) && value >= 0) return value
  return isPlainObject(row) ? Number(row.partialBytes) || 0 : 0
}

// How far a download has got, from 0 to 1. A row with no pinned size cannot be
// measured, so it reads as nothing done rather than as done.
function share(row, live) {
  var size = isPlainObject(row) ? Number(row.sizeBytes) || 0 : 0
  if (size <= 0) return 0
  var done = partialBytesOf(row, live)
  if (stateOf(row) === READY) return 1
  return Math.max(0, Math.min(1, done / size))
}

// The `.part` length one named row reports, or 0 for a name the list does not
// carry. This is what the overlay keeps the moving byte count in.
function partialOf(models, name) {
  var list = Array.isArray(models) ? models : []
  var wanted = String(name || "")
  if (wanted.length === 0) return 0
  for (var i = 0; i < list.length; i++) {
    var row = list[i]
    if (isPlainObject(row) && String(row.name) === wanted) return Number(row.partialBytes) || 0
  }
  return 0
}

// Every field a row carries, which is the whole of what the list draws.
var ROW_FIELDS = ["name", "fileName", "state", "partialBytes", "sizeBytes", "licence"]

// Whether two lists of rows say the same thing.
//
// A QML Repeater does not diff a JavaScript array, it rebuilds every delegate
// the moment the array is replaced. The poll answers once a second, so without
// this the rows are destroyed and recreated once a second: the progress bar
// restarts its animation rather than advancing, an open tooltip goes, and a
// press whose release lands after a rebuild never becomes a click.
//
// `movingName` is the row a download is running on. Its `.part` length is the
// one number the poll is there to move, and the bar reads it from its own
// property, so a change to it alone is not a reason to rebuild the list.
function sameRows(left, right, movingName) {
  var a = Array.isArray(left) ? left : []
  var b = Array.isArray(right) ? right : []
  if (a.length !== b.length) return false
  var moving = String(movingName || "")

  for (var i = 0; i < a.length; i++) {
    if (!isPlainObject(a[i]) || !isPlainObject(b[i])) return false
    for (var f = 0; f < ROW_FIELDS.length; f++) {
      var field = ROW_FIELDS[f]
      if (field === "partialBytes" && moving.length > 0 && String(a[i].name) === moving) continue
      if (a[i][field] !== b[i][field]) return false
    }
  }
  return true
}

// The one line under a row's name, spec section 7. It names the licence and the
// size always, because those are what the reader chooses between, and it names
// the progress only while there is progress to name.
function hint(row, busy, live) {
  var state = stateOf(row)
  var size = bytes(isPlainObject(row) ? row.sizeBytes : 0)
  var licence = isPlainObject(row) && row.licence ? String(row.licence) : "unknown licence"

  if (busy === true) {
    var done = bytes(partialBytesOf(row, live))
    return "Downloading " + done + " of " + size + ", " + Math.round(share(row, live) * 100) + "%"
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
// One verb of `grammachy model` runs at a time, so the answer to spec section 7
// is one fact rather than a list of states: `working` is true whenever a verb
// is in flight, which is a running download, a running remove, and an open
// Remove confirm alike. Naming the states one at a time is what left a `remove`
// drawing every row live while every press on it was dropped, so the rule asks
// the one question instead.
//
// Two things stay live under it. The row a download belongs to keeps its
// Cancel, because stopping the transfer is the point. The confirm's own Keep
// and Remove are not row buttons at all, and answering is the only way to close
// the question.
//
// A blocked button stays on the row and goes dim rather than vanishing, which
// says why nothing happens and keeps the list from shifting under a click.
function isBlocked(row, options) {
  var context = isPlainObject(options) ? options : ({})
  if (context.working !== true) return false
  var busyName = typeof context.busy === "string" ? context.busy : ""
  var name = isPlainObject(row) ? String(row.name) : ""
  return !(name.length > 0 && busyName === name)
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
    absorbed: absorbed,
    bytes: bytes,
    share: share,
    partialBytesOf: partialBytesOf,
    partialOf: partialOf,
    sameRows: sameRows,
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
