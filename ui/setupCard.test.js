// Node tests for the setup card of spec section 10. Run with
// `node --test ui/setupCard.test.js`.

const test = require("node:test")
const assert = require("node:assert/strict")

const Setup = require("./setupCard.js")

test("a missing companion binary opens the setup card before any Check", () => {
  assert.equal(Setup.companionMissing(false), true)
  assert.equal(Setup.companionMissing(undefined), true)
  assert.equal(Setup.companionMissing(null), true)
  assert.equal(Setup.companionMissing(true), false)
})

test("Retry after a bar-click install starts a new capture", () => {
  assert.equal(Setup.retryAfterSetup("quick", ""), "startQuick")
  assert.equal(Setup.retryAfterSetup("quick", "I has two book."), "retryCheck")
  assert.equal(Setup.retryAfterSetup("compose", ""), "compose")
})

test("readLock reads the two pinned fields", () => {
  assert.deepEqual(
    Setup.readLock('{"version": "0.1.0", "sha256": "abc123"}'),
    { version: "0.1.0", sha256: "abc123" }
  )
})

test("readLock treats unparseable or malformed text as an empty lock", () => {
  const empty = { version: "", sha256: "" }
  assert.deepEqual(Setup.readLock(""), empty)
  assert.deepEqual(Setup.readLock("not json"), empty)
  assert.deepEqual(Setup.readLock("[1, 2, 3]"), empty)
  assert.deepEqual(Setup.readLock('{"version": 1, "sha256": null}'), empty)
})

test("an empty pinned sha256 shows the developer path and no Install button", () => {
  const model = Setup.card({ lockText: '{"version": "0.1.0", "sha256": ""}' })
  assert.equal(model.state, Setup.UNPINNED)
  assert.equal(model.showsInstall, false)
  assert.match(model.body, /cargo build --release/)
})

test("a pinned sha256 with no run yet offers Install and names the hash", () => {
  const model = Setup.card({ lockText: '{"version": "0.2.0", "sha256": "deadbeef"}' })
  assert.equal(model.state, Setup.READY)
  assert.equal(model.showsInstall, true)
  assert.equal(model.installEnabled, true)
  assert.match(model.body, /grammachy-x86_64-linux 0\.2\.0/)
  assert.match(model.body, /deadbeef/)
})

test("a run in flight streams its log with the Install button disabled", () => {
  const model = Setup.card({
    lockText: '{"version": "0.2.0", "sha256": "deadbeef"}',
    running: true,
    log: "Downloading grammachy-x86_64-linux v0.2.0\n"
  })
  assert.equal(model.state, Setup.RUNNING)
  assert.equal(model.showsInstall, true)
  assert.equal(model.installEnabled, false)
  assert.equal(model.showsLog, true)
  assert.equal(model.log, "Downloading grammachy-x86_64-linux v0.2.0\n")
})

test("exit 0 reads as installed and offers Retry", () => {
  const model = Setup.card({
    lockText: '{"version": "0.2.0", "sha256": "deadbeef"}',
    running: false,
    exitCode: 0,
    log: "Installed grammachy-x86_64-linux v0.2.0 to bin/grammachy\n"
  })
  assert.equal(model.state, Setup.DONE)
  assert.equal(model.showsInstall, false)
  assert.equal(model.showsRetry, true)
})

test("a non-zero exit reads as failed and the log carries the reason", () => {
  const model = Setup.card({
    lockText: '{"version": "0.2.0", "sha256": "deadbeef"}',
    running: false,
    exitCode: 1,
    log: "sha256 mismatch for grammachy-x86_64-linux v0.2.0\n"
  })
  assert.equal(model.state, Setup.FAILED)
  assert.equal(model.showsInstall, true)
  assert.equal(model.showsLog, true)
  assert.match(model.log, /sha256 mismatch/)
})

const LOCK = '{"version": "0.1.0", "sha256": "abc123"}'

function deps(present) {
  return [
    { name: "curl", package: "curl", purpose: "Fetches the binary.", required: true, present: present.curl === true },
    { name: "wl-clipboard", package: "wl-clipboard", purpose: "Captures text.", required: true, present: present.wl === true },
    { name: "Java runtime", package: "jre-openjdk", purpose: "Runs LanguageTool.", required: false, present: false }
  ]
}

test("a missing required package is listed and blocks the bootstrap Install", () => {
  const model = Setup.card({ lockText: LOCK, dependencies: deps({ wl: true }) })
  assert.equal(model.state, Setup.READY)
  assert.equal(model.showsDependencies, true)
  assert.deepEqual(model.missingDependencies.map((row) => row.package), ["curl"])
  assert.equal(model.missingDependencies[0].purpose, "Fetches the binary.")
  assert.equal(model.showsInstall, true)
  assert.equal(model.installEnabled, false)
  assert.equal(model.installReason, "Install curl first.")
})

test("every missing required package is listed by name", () => {
  const model = Setup.card({ lockText: LOCK, dependencies: deps({}) })
  assert.deepEqual(model.missingDependencies.map((row) => row.package), ["curl", "wl-clipboard"])
  assert.equal(model.installReason, "Install curl and wl-clipboard first.")
})

test("an optional package never blocks the bootstrap and is never listed here", () => {
  const model = Setup.card({ lockText: LOCK, dependencies: deps({ curl: true, wl: true }) })
  assert.equal(model.showsDependencies, false)
  assert.deepEqual(model.missingDependencies, [])
  assert.equal(model.installEnabled, true)
  assert.equal(model.installReason, "")
})

test("an unread table blocks nothing, so a probe that has not answered hides no button", () => {
  const model = Setup.card({ lockText: LOCK })
  assert.equal(model.showsDependencies, false)
  assert.equal(model.installEnabled, true)
  assert.equal(Setup.card({ lockText: LOCK, dependencies: null }).installEnabled, true)
})

test("a finished bootstrap lists no packages", () => {
  const model = Setup.card({ lockText: LOCK, exitCode: 0, dependencies: deps({}) })
  assert.equal(model.state, Setup.DONE)
  assert.equal(model.showsDependencies, false)
  assert.equal(model.installReason, "")
})

test("missingRequired matches the rule of deps.js", () => {
  const Deps = require("./deps.js")
  const rows = Deps.fromProbe("curl\n")
  assert.deepEqual(Setup.missingRequired(rows), Deps.missingRequired(rows))
  assert.deepEqual(Setup.missingRequired(undefined), [])
})
