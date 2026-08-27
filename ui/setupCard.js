// The setup card of spec section 10, drawn when bin/grammachy is missing.
//
// Loaded twice: by QML as `SetupCard`, and by `setupCard.test.js` under
// node. Nothing here may touch a QML or a node API, because each side only
// has one of them.
//
// This file owns the whole route from cli.lock's text and one run of
// bin/bootstrap.sh to the card ui/SetupCard.qml draws: `readLock` reads the
// two fields cli.lock pins, and `card` turns those fields plus the run's
// state into the title, the body, and what the card offers. A cli.lock with
// no sha256 yet pins no release, so there is no Install button and the
// body points at the developer path instead (spec section 10: "ship
// cli.lock with version 0.1.0 and an explicitly empty hash").

var ASSET = "grammachy-x86_64-linux"

var UNPINNED = "unpinned"
var READY = "ready"
var RUNNING = "running"
var DONE = "done"
var FAILED = "failed"

var INSTALL = "install"
var RETRY = "retry"
var CLOSE = "close"

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value)
}

// Whether a launch must open the setup card instead of a Check.
// Pass true when bin/grammachy exists and the file is executable.
// A missing binary opens the setup card on both surfaces, before capture
// or chunking. Spec section 12 step 2 is a bar click with no Check first.
function companionMissing(present) {
  return present !== true
}

// The two fields cli.lock pins, spec section 10. Text that will not parse,
// or that carries no string for either field, reads the same as an empty
// lock: nothing is pinned, so there is nothing to install from.
function readLock(text) {
  var parsed = null
  try {
    parsed = JSON.parse(text)
  } catch (error) {
    parsed = null
  }
  if (!isPlainObject(parsed)) return { version: "", sha256: "" }
  return {
    version: typeof parsed.version === "string" ? parsed.version : "",
    sha256: typeof parsed.sha256 === "string" ? parsed.sha256 : ""
  }
}

// The card model for the current state.
//
// `lockText` is the raw text of cli.lock, `running` is whether
// bin/bootstrap.sh is still going, `exitCode` is its exit status once it has
// finished (null while none has run this session), and `log` is its stdout
// and stderr, streamed in the order they arrived.
//
// States:
// - `unpinned`: no sha256 yet, so the card names the developer path and
//   offers no Install button.
// - `ready`: a hash is pinned and idle, so Install runs bin/bootstrap.sh.
// - `running`: the run is going, and the log grows as its output streams in.
// - `done`: the run finished with the binary in place.
// - `failed`: the run finished without it, and the log carries why.
function card(options) {
  var context = isPlainObject(options) ? options : ({})
  var lock = readLock(typeof context.lockText === "string" ? context.lockText : "")
  var running = context.running === true
  var exitCode = typeof context.exitCode === "number" ? context.exitCode : null
  var log = typeof context.log === "string" ? context.log : ""

  var state = READY
  if (running) state = RUNNING
  else if (exitCode === 0) state = DONE
  else if (exitCode !== null) state = FAILED
  else if (lock.sha256.length === 0) state = UNPINNED

  var model = {
    state: state,
    asset: ASSET,
    version: lock.version,
    sha256: lock.sha256,
    log: log,
    title: "",
    body: "",
    // The button stays on screen through the run, disabled while it goes, so
    // the reader watching the log also sees why nothing else is happening.
    showsInstall: state === READY || state === RUNNING || state === FAILED,
    installEnabled: state !== RUNNING,
    showsLog: state === RUNNING || state === DONE || state === FAILED,
    showsRetry: state === DONE
  }

  if (state === UNPINNED) {
    model.title = "cli.lock pins no release yet"
    model.body = "cli.lock carries no hash to install. Build from source instead. " +
      "Run cargo build --release in cli/, then copy the binary into bin/grammachy."
    return model
  }

  if (state === DONE) {
    model.title = "Installed"
    model.body = ASSET + " " + lock.version + " is in bin/grammachy."
    return model
  }

  model.title = "The companion tool is absent"
  model.body = "Grammachy needs " + ASSET + " " + lock.version +
    " in bin/grammachy, pinned to sha256 " + lock.sha256 + "."
  return model
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    ASSET: ASSET,
    UNPINNED: UNPINNED,
    READY: READY,
    RUNNING: RUNNING,
    DONE: DONE,
    FAILED: FAILED,
    INSTALL: INSTALL,
    RETRY: RETRY,
    CLOSE: CLOSE,
    companionMissing: companionMissing,
    readLock: readLock,
    card: card
  }
}
