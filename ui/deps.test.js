// Node tests for the dependency table of spec section 10. Run with
// `node --test ui/deps.test.js`.

const test = require("node:test")
const assert = require("node:assert/strict")

const Deps = require("./deps.js")

function envelope(present) {
  return JSON.stringify({
    contractVersion: 1,
    engine: "harper",
    ready: true,
    diagnosis: "",
    checks: [],
    dependencies: Deps.DEPENDENCIES.map((spec) => ({
      name: spec.name,
      package: spec.package,
      purpose: spec.purpose,
      required: spec.required,
      present: present[spec.package] === true,
      installCommand: "omarchy pkg add " + spec.package,
      usedBy: spec.usedBy
    }))
  })
}

test("the table names the three packages in doctor's order", () => {
  assert.deepEqual(Deps.DEPENDENCIES.map((spec) => spec.package), ["curl", "wl-clipboard", "jre-openjdk"])
  assert.deepEqual(Deps.DEPENDENCIES.map((spec) => spec.required), [true, true, false])
  for (const spec of Deps.DEPENDENCIES) assert.ok(spec.purpose.endsWith("."), spec.package)
})

test("every install command is omarchy pkg add and never sudo or pacman", () => {
  assert.equal(Deps.installCommand(["curl"]), "omarchy pkg add curl")
  assert.equal(Deps.installCommand(["curl", "wl-clipboard"]), "omarchy pkg add curl wl-clipboard")
  for (const row of Deps.fromDoctor(envelope({}))) {
    assert.equal(row.installCommand, "omarchy pkg add " + row.package)
    assert.ok(!row.installCommand.includes("sudo"))
    assert.ok(!row.installCommand.includes("pacman"))
  }
})

test("fromDoctor reads presence by package and keeps the table's wording", () => {
  const rows = Deps.fromDoctor(envelope({ curl: true, "jre-openjdk": true }))
  assert.deepEqual(rows.map((row) => [row.package, row.present]), [
    ["curl", true], ["wl-clipboard", false], ["jre-openjdk", true]
  ])
  assert.equal(rows[1].purpose, Deps.DEPENDENCIES[1].purpose)
  assert.deepEqual(rows[2].usedBy, ["languagetool"])
})

test("fromDoctor refuses anything that is not a doctor envelope", () => {
  assert.equal(Deps.fromDoctor("not json"), null)
  assert.equal(Deps.fromDoctor(JSON.stringify({ contractVersion: 2, dependencies: [] })), null)
  assert.equal(Deps.fromDoctor(JSON.stringify({ contractVersion: 1 })), null)
})

test("the probe asks command -v for every probe binary and reads the names back", () => {
  const argv = Deps.probeArgv()
  assert.equal(argv[0], "sh")
  assert.deepEqual(argv.slice(-3), ["curl", "wl-copy", "java"])
  const rows = Deps.fromProbe("curl\njava\n")
  assert.deepEqual(rows.map((row) => [row.package, row.present]), [
    ["curl", true], ["wl-clipboard", false], ["jre-openjdk", true]
  ])
  assert.deepEqual(Deps.fromProbe("").map((row) => row.present), [false, false, false])
})

test("missingRequired lists only the required rows that are absent", () => {
  const rows = Deps.fromProbe("curl\n")
  assert.deepEqual(Deps.absent(rows).map((row) => row.package), ["wl-clipboard", "jre-openjdk"])
  assert.deepEqual(Deps.missingRequired(rows).map((row) => row.package), ["wl-clipboard"])
  assert.deepEqual(Deps.missingRequired(null), [])
  assert.deepEqual(Deps.packagesOf(Deps.missingRequired(rows)), ["wl-clipboard"])
})

test("isPresent answers false until the table has been read", () => {
  assert.equal(Deps.isPresent([], Deps.JAVA_PACKAGE), false)
  assert.equal(Deps.isPresent(null, Deps.JAVA_PACKAGE), false)
  assert.equal(Deps.isPresent(Deps.fromProbe("java\n"), Deps.JAVA_PACKAGE), true)
  assert.equal(Deps.isPresent(Deps.fromProbe("curl\n"), Deps.JAVA_PACKAGE), false)
})

test("the terminal runs omarchy pkg add through uwsm-app and xdg-terminal-exec", () => {
  const argv = Deps.terminalArgv(["curl", "wl-clipboard"], "")
  assert.deepEqual(argv.slice(0, 3), ["uwsm-app", "--", "xdg-terminal-exec"])
  assert.ok(argv.includes("--app-id=org.omarchy.terminal"))
  assert.deepEqual(argv.slice(-3, -1), ["bash", "-c"])
  const script = argv[argv.length - 1]
  assert.ok(script.includes("omarchy pkg add curl wl-clipboard"), script)
  assert.ok(script.startsWith("omarchy-show-logo; "), script)
  assert.ok(script.includes("omarchy-show-done"), script)
  assert.ok(!script.includes("sudo"))
  assert.ok(!script.includes("pacman"))
  assert.ok(!argv.includes("setsid"), "uwsm-app must wait for the terminal")
})

test("the seam and an empty or unknown package list open no terminal", () => {
  assert.deepEqual(Deps.terminalArgv(["curl"], Deps.NEVER), [])
  assert.deepEqual(Deps.terminalArgv([], ""), [])
  assert.deepEqual(Deps.terminalArgv(["; rm -rf /"], ""), [])
  assert.deepEqual(Deps.known(["curl", "evil", "jre-openjdk"]), ["curl", "jre-openjdk"])
  assert.equal(Deps.TERMINAL_SEAM, "GRAMMACHY_PKG_TERMINAL")
})
