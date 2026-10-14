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

function missingRequired(dependencies) {
  var list = Array.isArray(dependencies) ? dependencies : []
  return list.filter(function(dependency) {
    return isPlainObject(dependency) && dependency.required === true && dependency.present !== true
  })
}

function packageList(dependencies) {
  return dependencies.map(function(dependency) { return String(dependency.package) }).join(" and ")
}

function companionMissing(present) {
  return present !== true
}

function retryAfterSetup(surface, selectionText) {
  if (surface === "compose") return "compose"
  if (typeof selectionText !== "string" || selectionText.length === 0) return "startQuick"
  return "retryCheck"
}

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

function card(options) {
  var context = isPlainObject(options) ? options : ({})
  var lock = readLock(typeof context.lockText === "string" ? context.lockText : "")
  var running = context.running === true
  var exitCode = typeof context.exitCode === "number" ? context.exitCode : null
  var log = typeof context.log === "string" ? context.log : ""
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
    showsInstall: state === READY || state === RUNNING || state === FAILED,
    installEnabled: state !== RUNNING && missing.length === 0,
    installReason: "",
    showsLog: state === RUNNING || state === DONE || state === FAILED,
    showsRetry: state === DONE,
    missingDependencies: missing,
    showsDependencies: missing.length > 0 && state !== DONE
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
    RETRY: RETRY,
    CLOSE: CLOSE,
    companionMissing: companionMissing,
    retryAfterSetup: retryAfterSetup,
    readLock: readLock,
    missingRequired: missingRequired,
    card: card
  }
}
