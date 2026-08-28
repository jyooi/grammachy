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
// body points at the developer path instead (docs/dev.md section 18).

var ASSET = "grammachy-x86_64-linux"

var UNPINNED = "unpinned"
var READY = "ready"
var RUNNING = "running"
var DONE = "done"
var FAILED = "failed"

var INSTALL = "install"
var INSTALL_DEPS = "installDeps"
var RETRY = "retry"
var CLOSE = "close"

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value)
}

// The required rows of a `ui/deps.js` table that are absent. The rule lives
// in deps.js too; it is repeated here because QML gives a `.js` no way to
// import another one that a node `require` also understands.
function missingRequired(dependencies) {
  var list = Array.isArray(dependencies) ? dependencies : []
  return list.filter(function(dependency) {
    return isPlainObject(dependency) && dependency.required === true && dependency.present !== true
  })
}

function packageList(dependencies) {
  return dependencies.map(function(dependency) { return String(dependency.package) }).join(" and ")
}

// The exact command the dependency Install button runs, spec section 10.
function installCommand(dependencies) {
  if (dependencies.length === 0) return ""
  return "omarchy pkg add " + dependencies.map(function(dependency) {
    return String(dependency.package)
  }).join(" ")
}

// Whether a launch must open the setup card instead of a Check.
// Pass true when bin/grammachy exists and the file is executable.
// A missing binary opens the setup card on both surfaces, before capture
// or chunking. Spec section 12 step 2 is a bar click with no Check first.
function companionMissing(present) {
  return present !== true
}

// Where Retry on the setup card goes.
// Compose returns to the Draft Check.
// An empty Selection on the quick surface starts a new capture.
// A populated Selection retries the failed Check with no new capture.
function retryAfterSetup(surface, selectionText) {
  if (surface === "compose") return "compose"
  if (typeof selectionText !== "string" || selectionText.length === 0) return "startQuick"
  return "retryCheck"
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
// `dependencies` is the table of `ui/deps.js` for this machine, or null
// while it has not been read yet, and `depsInstalling` is whether the
// terminal that runs `omarchy pkg add` is still open. A required package that
// is absent is listed with its purpose and one Install button, and the
// bootstrap Install stays disabled with the reason shown, because
// bin/bootstrap.sh needs curl before it can fetch anything (spec section 10).
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
  var depsInstalling = context.depsInstalling === true
  var missing = missingRequired(context.dependencies)

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
    installEnabled: state !== RUNNING && missing.length === 0,
    installReason: "",
    showsLog: state === RUNNING || state === DONE || state === FAILED,
    showsRetry: state === DONE,
    // The required packages this machine lacks, each with its purpose, and
    // the one command the Install button beside them runs in a terminal.
    missingDependencies: missing,
    showsDependencies: missing.length > 0 && state !== DONE,
    depsInstalling: depsInstalling,
    depsInstallEnabled: !depsInstalling && state !== RUNNING,
    depsInstallCommand: installCommand(missing)
  }

  if (missing.length > 0 && state !== DONE) {
    model.installReason = "Install " + packageList(missing) + " first."
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
    INSTALL_DEPS: INSTALL_DEPS,
    RETRY: RETRY,
    CLOSE: CLOSE,
    companionMissing: companionMissing,
    retryAfterSetup: retryAfterSetup,
    readLock: readLock,
    missingRequired: missingRequired,
    card: card
  }
}
