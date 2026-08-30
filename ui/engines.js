// The Engines list of the Settings view, spec sections 5.4 and 7.
//
// Loaded twice: by QML as `Engines`, and by `engines.test.js` under node.
// Nothing here may touch a QML or a node API, because each side only has one
// of them.
//
// This file owns the whole route from one run of `grammachy engine` to the row
// `ui/EnginesView.qml` draws: `read` reads the envelope of spec section 5.4,
// `rows` turns it into what a row needs, `hint` writes the line under the name,
// `actions` says which button it carries, and `note` turns a failure into the
// one line the view shows. Keeping every half here is what lets a node test run
// a stub binary and read the row back, which no test of the QML could do.
//
// It is the shape of `ui/models.js` with the row swapped, and it says the same
// three state words, because a component on disk and a weights file on disk
// answer the same question. The two files stay apart because QML gives a `.js`
// no way to import another one that a node `require` also understands, and one
// byte formatter is a cheaper thing to keep in step than a loader that only
// half the callers have.

var CONTRACT_VERSION = 1

// The states one component can be in, spec section 5.4.
var ABSENT = "absent"
var PARTIAL = "partial"
var READY = "ready"
var STATES = [ABSENT, PARTIAL, READY]

// The codes `grammachy engine` can answer. The first two are its own; the third
// is the shared code of spec section 5.1.
var CANCELLED = "cancelled"
var DOWNLOAD_FAILED = "download_failed"
var BAD_ARGUMENTS = "bad_arguments"

// What a row offers, spec section 7. The verb is the tooltip and the name is in
// the hint line, so the button itself carries an icon and no text label.
var INSTALL = "install"
var CANCEL = "cancel"
var REMOVE = "remove"

// The icon of each action, from the set `ui/CardHero.qml` already draws.
var ACTION_ICONS = {
  install: "󰇚",
  cancel: "󰅖",
  remove: "󰩹"
}

var ACTION_TOOLTIPS = {
  install: "Install",
  cancel: "Cancel the download",
  remove: "Remove"
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value)
}

// The state one row reports, or `absent` for anything the contract does not
// list. An unknown state is the same statement as nothing on disk: nothing here
// can be installed twice, and offering Install is the one useful answer.
function stateOf(row) {
  if (!isPlainObject(row)) return ABSENT
  var state = String(row.state)
  return STATES.indexOf(state) === -1 ? ABSENT : state
}

// What one run of `grammachy engine` left on stdout, spec section 5.4.
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

  if (!Array.isArray(envelope.engines))
    return { report: null, error: { code: BAD_ARGUMENTS, message: "" } }

  return {
    report: {
      verb: String(envelope.verb || ""),
      directory: String(envelope.directory || ""),
      freeBytes: Number(envelope.freeBytes) || 0,
      engines: rows(envelope.engines)
    },
    error: null
  }
}

// One drawable row per envelope row, with everything the view needs already
// decided. A row without a slug is dropped: nothing can be done with it.
function rows(engines) {
  var out = []
  for (var i = 0; i < engines.length; i++) {
    var row = engines[i]
    if (!isPlainObject(row) || typeof row.slug !== "string" || row.slug.length === 0) continue
    out.push({
      slug: row.slug,
      name: typeof row.name === "string" && row.name.length > 0 ? row.name : row.slug,
      version: typeof row.version === "string" ? row.version : "",
      state: stateOf(row),
      partialBytes: Number(row.partialBytes) || 0,
      sizeBytes: Number(row.sizeBytes) || 0,
      licence: typeof row.licence === "string" ? row.licence : "",
      needsJava: row.needsJava === true,
      path: typeof row.path === "string" ? row.path : "",
      fromPackage: row.fromPackage === true
    })
  }
  return out
}

// The rows of an `install` or a `remove` answer merged into the list on screen.
//
// Those two verbs answer with the one row they acted on, so the list keeps
// every other row rather than shrinking to one. A slug the list does not carry
// is appended, which only a catalogue change between two runs could cause.
function merged(current, answered) {
  var next = Array.isArray(current) ? current.slice() : []
  var incoming = Array.isArray(answered) ? answered : []
  for (var i = 0; i < incoming.length; i++) {
    var found = -1
    for (var j = 0; j < next.length; j++) if (next[j].slug === incoming[i].slug) found = j
    if (found === -1) next.push(incoming[i])
    else next[found] = incoming[i]
  }
  return next
}

// Everything one answer changes on screen, or `null` for an answer that is
// already out of date.
//
// `list` answers every row, so its rows are the whole list. `install` and
// `remove` answer the one row they acted on, so a merge is what keeps the rest.
//
// A `list` run reads the directory the moment it starts, while `install`
// answers only after it has hashed the archive and unpacked it, which takes
// tens of seconds on a 250 MB file. So a poll that fired during the hash is
// still in flight when the verb answers, and it truthfully reports the row as
// `partial` while the tree on disk is already there. `stamp` says which run
// answered and `floor` is the first run no verb has overtaken, so the older of
// the two loses and a finished row never goes back to `partial`.
function absorbed(current, report, stamp, floor) {
  if (!isPlainObject(report)) return null
  var verb = String(report.verb || "")
  if (verb === "list" && (Number(stamp) || 0) < (Number(floor) || 0)) return null
  var answered = Array.isArray(report.engines) ? report.engines : []
  return {
    engines: verb === "list" ? answered : merged(current, answered),
    directory: String(report.directory || ""),
    freeBytes: Number(report.freeBytes) || 0
  }
}

// A byte count as the list says it, the rule `ui/models.js` uses for the same
// question: the unit is chosen per number, one decimal is kept, and the step is
// 1024, because that is what a file manager shows for the same file.
var UNITS = ["B", "KB", "MB", "GB", "TB"]

function bytes(count) {
  var value = Math.max(0, Number(count) || 0)
  var index = 0
  while (value >= 1024 && index < UNITS.length - 1) {
    value = value / 1024
    index += 1
  }
  if (index === 0) return Math.round(value) + " B"
  return (Math.round(value * 10) / 10).toFixed(1) + " " + UNITS[index]
}

// The `.part` length to draw for one row. A `live` below zero, or no `live` at
// all, means the list is the only answer there is.
function partialBytesOf(row, live) {
  var value = Number(live)
  if (isFinite(value) && value >= 0) return value
  return isPlainObject(row) ? Number(row.partialBytes) || 0 : 0
}

// How far an install has got, from 0 to 1. A row with no pinned size cannot be
// measured, so it reads as nothing done rather than as done.
function share(row, live) {
  var size = isPlainObject(row) ? Number(row.sizeBytes) || 0 : 0
  if (size <= 0) return 0
  var done = partialBytesOf(row, live)
  if (stateOf(row) === READY) return 1
  return Math.max(0, Math.min(1, done / size))
}

// The `.part` length one named row reports, or 0 for a slug the list does not
// carry. This is what the overlay keeps the moving byte count in.
function partialOf(engines, slug) {
  var list = Array.isArray(engines) ? engines : []
  var wanted = String(slug || "")
  if (wanted.length === 0) return 0
  for (var i = 0; i < list.length; i++) {
    var row = list[i]
    if (isPlainObject(row) && String(row.slug) === wanted) return Number(row.partialBytes) || 0
  }
  return 0
}

// Every field a row carries, which is the whole of what the list draws.
var ROW_FIELDS = [
  "slug", "name", "version", "state", "partialBytes", "sizeBytes",
  "licence", "needsJava", "path", "fromPackage"
]

// Whether two lists of rows say the same thing.
//
// A QML Repeater rebuilds every delegate the moment its array is replaced, and
// the poll answers once a second, so without this the row is destroyed and
// recreated once a second: the progress bar restarts its animation rather than
// advancing, an open tooltip goes, and a press whose release lands after a
// rebuild never becomes a click.
//
// `movingSlug` is the row an install is running on. Its `.part` length is the
// one number the poll is there to move, and the bar reads it from its own
// property, so a change to it alone is not a reason to rebuild the list.
function sameRows(left, right, movingSlug) {
  var a = Array.isArray(left) ? left : []
  var b = Array.isArray(right) ? right : []
  if (a.length !== b.length) return false
  var moving = String(movingSlug || "")

  for (var i = 0; i < a.length; i++) {
    if (!isPlainObject(a[i]) || !isPlainObject(b[i])) return false
    for (var f = 0; f < ROW_FIELDS.length; f++) {
      var field = ROW_FIELDS[f]
      if (field === "partialBytes" && moving.length > 0 && String(a[i].slug) === moving) continue
      if (a[i][field] !== b[i][field]) return false
    }
  }
  return true
}

// The one line under a row's name, spec section 7.
//
// It names the download size and the Java requirement while the component is
// not here, because those are the whole cost of the button beside it. Once it
// is installed the size is spent and the line says where it came from instead:
// only a tree this plugin unpacked is one Remove can take away again.
function hint(row, busy, live) {
  var state = stateOf(row)
  var size = bytes(isPlainObject(row) ? row.sizeBytes : 0)
  var licence = isPlainObject(row) && row.licence ? String(row.licence) : "unknown licence"
  var java = isPlainObject(row) && row.needsJava === true ? ", needs Java" : ""

  if (busy === true) {
    var done = bytes(partialBytesOf(row, live))
    return "Downloading " + done + " of " + size + ", " + Math.round(share(row, live) * 100) + "%"
  }
  if (state === READY) return "Installed, " + licence + java
  if (state === PARTIAL)
    return "Part downloaded, " + bytes(row.partialBytes) + " of " + size + ", " + licence + java
  if (isPlainObject(row) && row.fromPackage === true)
    return "From the languagetool package, " + licence + java
  return "About " + size + java + ", " + licence
}

// Whether a Check can run on this engine right now, spec section 7.
//
// The pacman package counts: the adapter runs it when no installed tree is
// there, so an engine it supplies is one the dropdown may offer. Only a slug
// the list carries is asked at all; every other engine has nothing to install
// and is always available.
function isAvailable(engines, slug) {
  var wanted = String(slug || "")
  var list = Array.isArray(engines) ? engines : []
  for (var i = 0; i < list.length; i++) {
    var row = list[i]
    if (!isPlainObject(row) || String(row.slug) !== wanted) continue
    return stateOf(row) === READY || row.fromPackage === true
  }
  return true
}

// Every engine slug the list says is not available, so the Settings dropdown
// can leave those rows out (spec section 7).
function unavailable(engines) {
  var list = Array.isArray(engines) ? engines : []
  var out = []
  for (var i = 0; i < list.length; i++) {
    var row = list[i]
    if (!isPlainObject(row) || typeof row.slug !== "string") continue
    if (!isAvailable(list, row.slug)) out.push(row.slug)
  }
  return out
}

// Which buttons one row carries, spec section 7.
//
// The row an install is running on offers Cancel alone: Remove is about a tree
// that is not there yet. An installed tree offers Remove. A component the
// pacman package supplies offers nothing, because Remove here would delete a
// directory this plugin never wrote and would leave the package untouched: that
// is a button that looks like it does something and does not.
function actions(row, options) {
  var context = isPlainObject(options) ? options : ({})
  var state = stateOf(row)
  var busySlug = typeof context.busy === "string" ? context.busy : ""
  var slug = isPlainObject(row) ? String(row.slug) : ""

  if (busySlug === slug && slug.length > 0) return [CANCEL]
  if (state === READY) return [REMOVE]
  if (isPlainObject(row) && row.fromPackage === true) return []

  var out = [INSTALL]
  if (state === PARTIAL) out.push(REMOVE)
  return out
}

// Whether a row's buttons are drawn but cannot be pressed.
//
// One verb of `grammachy engine` runs at a time, so the answer is one fact:
// `working` is true whenever a verb is in flight, which is a running install, a
// running remove, and an open Remove confirm alike. The row an install belongs
// to keeps its Cancel, because stopping the transfer is the point.
//
// A blocked button stays on the row and goes dim rather than vanishing, which
// says why nothing happens and keeps the list from shifting under a click.
function isBlocked(row, options) {
  var context = isPlainObject(options) ? options : ({})
  if (context.working !== true) return false
  var busySlug = typeof context.busy === "string" ? context.busy : ""
  var slug = isPlainObject(row) ? String(row.slug) : ""
  return !(slug.length > 0 && busySlug === slug)
}

// Whether a row needs a system package this machine lacks, spec section 7.
//
// `missing` is the list `Deps.absentFor(dependencies, slug)` answers: the
// packages LanguageTool runs on and is unpacked with. A row with one reads
// "Needs ..." beside its name and names those packages for Omarchy Install.
// Its own Install stays disabled until they are there. An install with no
// bsdtar cannot unpack. A server with no runtime fails the first Check.
function runtimeMissing(row, missing) {
  if (!isPlainObject(row)) return false
  return Array.isArray(missing) && missing.length > 0
}

// Whether one button of a row is drawn but cannot be pressed: everything
// `isBlocked` says, plus Install while a package it needs is missing.
function actionBlocked(action, row, options) {
  if (isBlocked(row, options)) return true
  var context = isPlainObject(options) ? options : ({})
  return String(action) === INSTALL && runtimeMissing(row, context.missing)
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

// The one line the list shows after a verb, spec section 5.4.
function note(code, message, name) {
  var settled = String(code)
  var component = name ? String(name) : "the engine"
  var said = typeof message === "string" ? message : ""

  if (settled === CANCELLED)
    return {
      kind: NOTICE,
      title: "Download of " + component + " stopped",
      body: "What arrived is kept. Install resumes it.",
      message: ""
    }
  if (settled === DOWNLOAD_FAILED)
    return {
      kind: FAILURE,
      title: component + " could not be installed",
      body: "Nothing was installed. Install tries again.",
      message: said
    }
  if (settled === BAD_ARGUMENTS && said.length === 0)
    return {
      kind: FAILURE,
      title: "Grammachy could not read the engine list",
      body: "The companion tool is missing or out of date.",
      message: ""
    }
  return {
    kind: FAILURE,
    title: "Grammachy could not finish that",
    body: "The engines on disk did not change.",
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
    INSTALL: INSTALL,
    CANCEL: CANCEL,
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
    isAvailable: isAvailable,
    unavailable: unavailable,
    actions: actions,
    isBlocked: isBlocked,
    runtimeMissing: runtimeMissing,
    actionBlocked: actionBlocked,
    actionIcon: actionIcon,
    actionTooltip: actionTooltip,
    note: note
  }
}
