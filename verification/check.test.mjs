import assert from "node:assert/strict"
import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import test from "node:test"
import { fileURLToPath, pathToFileURL } from "node:url"

const sha256 = (text) => createHash("sha256").update(text).digest("hex")

const body = "stateDiagram-v2\n  [*] --> capture\n  capture --> closed\n"
const mermaid = `## State chart\n\n\`\`\`mermaid\n${body}\`\`\`\n`
const svg = (digest) => `<svg><desc>Chart digest: ${digest}</desc></svg>\n`

const record = {
  model: [{ file: "verification/Model.tla", sha256: "a".repeat(64) }],
  covers: [{ file: "Overlay.qml", sha256: "b".repeat(64) }],
}
const covered = sha256(`${"b".repeat(64)}  Overlay.qml\n${"a".repeat(64)}  verification/Model.tla\n`)

const scratchFolder = mkdtempSync(join(tmpdir(), "check-test-"))
test.after(() => rmSync(scratchFolder, { recursive: true, force: true }))

async function scratch(files) {
  const root = mkdtempSync(join(scratchFolder, "repo-"))
  const git = (...args) =>
    execFileSync("git", ["-c", "user.name=test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false", ...args], {
      cwd: root,
    })
  mkdirSync(join(root, "verification"))
  copyFileSync(join(dirname(fileURLToPath(import.meta.url)), "check.mjs"), join(root, "verification", "check.mjs"))
  git("init", "--quiet", "--initial-branch=main")
  git("add", ".")
  git("commit", "--quiet", "-m", "base")
  git("update-ref", "refs/remotes/origin/main", "HEAD")
  git("checkout", "--quiet", "-b", "feat/run")
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true })
    writeFileSync(join(root, path), text)
  }
  if (Object.keys(files).length > 0) {
    git("add", ".")
    git("commit", "--quiet", "-m", "change")
  }
  const { checkChart } = await import(pathToFileURL(join(root, "verification", "check.mjs")).href)
  return (branch = "feat/run") => checkChart(record, branch)
}

test("a chart with an image of the same chart digest passes", async () => {
  const check = await scratch({
    "verification/charts/feat-run.md": mermaid,
    "verification/charts/feat-run.svg": svg(sha256(body)),
  })
  assert.deepEqual((await check()).problems, [])
})

test("an image that names an older chart digest fails", async () => {
  const check = await scratch({
    "verification/charts/feat-run.md": mermaid,
    "verification/charts/feat-run.svg": svg("c".repeat(64)),
  })
  const { problems } = await check()
  assert.equal(problems.length, 1)
  assert.match(problems[0], /feat-run\.svg is older than the chart text/u)
})

test("a chart with no image beside it fails", async () => {
  const check = await scratch({ "verification/charts/feat-run.md": mermaid })
  const { problems } = await check()
  assert.equal(problems.length, 1)
  assert.match(problems[0], /no SVG image verification\/charts\/feat-run\.svg/u)
})

test("a chart that is not named for the branch fails", async () => {
  const check = await scratch({ "verification/charts/other.md": "No state change in this PR.\n" })
  const { problems } = await check()
  assert.equal(problems.length, 1)
  assert.match(problems[0], /no state chart file verification\/charts\/feat-run\.md for the branch feat\/run/u)
})

test("a note of no state change passes without an image", async () => {
  const check = await scratch({ "verification/charts/feat-run.md": "No state change in this PR.\n" })
  assert.deepEqual((await check()).problems, [])
})

test("a chart with neither a mermaid block nor a note fails", async () => {
  const check = await scratch({ "verification/charts/feat-run.md": "## State chart\n" })
  const { problems } = await check()
  assert.equal(problems.length, 1)
  assert.match(problems[0], /has no mermaid chart and no "No state change" note/u)
})

test("a change to verified code needs the covered digest in the chart", async () => {
  const without = await scratch({
    "Overlay.qml": "Item {}\n",
    "verification/charts/feat-run.md": "No state change in this PR.\n",
  })
  const { problems } = await without()
  assert.equal(problems.length, 1)
  assert.match(problems[0], new RegExp(`does not name the covered digest ${covered}`, "u"))

  const named = await scratch({
    "Overlay.qml": "Item {}\n",
    "verification/charts/feat-run.md": `No state change in this PR.\n\nCovered digest: ${covered}\n`,
  })
  assert.deepEqual((await named()).problems, [])
})

test("a branch with no change from main is not checked", async () => {
  const check = await scratch({})
  const { problems, notes } = await check()
  assert.deepEqual(problems, [])
  assert.match(notes[0], /not checked, because HEAD has no change/u)
})
