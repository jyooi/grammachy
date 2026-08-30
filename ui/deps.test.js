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
      installHint: Deps.installHint([spec.package]),
      usedBy: spec.usedBy
    }))
  })
}

test("the table names the three packages in doctor's order", () => {
  assert.deepEqual(Deps.DEPENDENCIES.map((spec) => spec.package), ["curl", "wl-clipboard", "libarchive", "jre-openjdk"])
  assert.deepEqual(Deps.DEPENDENCIES.map((spec) => spec.required), [true, true, false, false])
  for (const spec of Deps.DEPENDENCIES) assert.ok(spec.purpose.endsWith("."), spec.package)
})

test("every install hint names the package and Omarchy Install", () => {
  assert.equal(
    Deps.installHint(["curl"]),
    "Add curl through Omarchy Install. Open SUPER+SPACE, then Install, then Package."
  )
  assert.equal(
    Deps.installHint(["curl", "wl-clipboard"]),
    "Add curl and wl-clipboard through Omarchy Install. Open SUPER+SPACE, then Install, then Package."
  )
  assert.equal(Deps.installHint([]), "")
  for (const row of Deps.fromDoctor(envelope({}))) {
    assert.equal(row.installHint, Deps.installHint([row.package]))
    assert.ok(row.installHint.includes(row.package))
    assert.ok(row.installHint.includes("Omarchy Install"))
  }
})

test("fromDoctor reads presence by package and keeps the table's wording", () => {
  const rows = Deps.fromDoctor(envelope({ curl: true, "jre-openjdk": true }))
  assert.deepEqual(rows.map((row) => [row.package, row.present]), [
    ["curl", true], ["wl-clipboard", false], ["libarchive", false], ["jre-openjdk", true]
  ])
  assert.equal(rows[1].purpose, Deps.DEPENDENCIES[1].purpose)
  assert.deepEqual(rows[3].usedBy, ["languagetool"])
})

test("fromDoctor refuses anything that is not a doctor envelope", () => {
  assert.equal(Deps.fromDoctor("not json"), null)
  assert.equal(Deps.fromDoctor(JSON.stringify({ contractVersion: 2, dependencies: [] })), null)
  assert.equal(Deps.fromDoctor(JSON.stringify({ contractVersion: 1 })), null)
})

test("the probe asks command -v for every probe binary and reads the names back", () => {
  const argv = Deps.probeArgv()
  assert.equal(argv[0], "sh")
  assert.deepEqual(argv.slice(-4), ["curl", "wl-copy", "bsdtar", "java"])
  const rows = Deps.fromProbe("curl\njava\n")
  assert.deepEqual(rows.map((row) => [row.package, row.present]), [
    ["curl", true], ["wl-clipboard", false], ["libarchive", false], ["jre-openjdk", true]
  ])
  assert.deepEqual(Deps.fromProbe("").map((row) => row.present), [false, false, false, false])
})

test("missingRequired lists only the required rows that are absent", () => {
  const rows = Deps.fromProbe("curl\n")
  assert.deepEqual(Deps.absent(rows).map((row) => row.package), ["wl-clipboard", "libarchive", "jre-openjdk"])
  assert.deepEqual(Deps.missingRequired(rows).map((row) => row.package), ["wl-clipboard"])
  assert.deepEqual(Deps.missingRequired(null), [])
  assert.deepEqual(Deps.packagesOf(Deps.missingRequired(rows)), ["wl-clipboard"])
})

test("absentFor names what one part still needs, and needsHint words it", () => {
  const rows = Deps.fromProbe("curl\nwl-copy\n")
  assert.deepEqual(Deps.absentFor(rows, "languagetool").map((row) => row.package), ["libarchive", "jre-openjdk"])
  assert.deepEqual(Deps.absentFor(rows, "capture"), [])
  assert.equal(Deps.needsHint(Deps.absentFor(rows, "languagetool")), "Needs libarchive and a Java runtime")
  assert.equal(Deps.needsHint(Deps.absentFor(Deps.fromProbe("bsdtar\n"), "languagetool")), "Needs a Java runtime")
  assert.equal(Deps.needsHint([]), "")
  assert.deepEqual(Deps.absentFor(null, "languagetool"), [])
})

test("isPresent answers false until the table has been read", () => {
  assert.equal(Deps.isPresent([], Deps.JAVA_PACKAGE), false)
  assert.equal(Deps.isPresent(null, Deps.JAVA_PACKAGE), false)
  assert.equal(Deps.isPresent(Deps.fromProbe("java\n"), Deps.JAVA_PACKAGE), true)
  assert.equal(Deps.isPresent(Deps.fromProbe("curl\n"), Deps.JAVA_PACKAGE), false)
})
