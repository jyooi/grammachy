import { execFile } from "node:child_process"
import { createHash } from "node:crypto"
import { existsSync } from "node:fs"
import { mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { basename, dirname, join, posix } from "node:path"
import { fileURLToPath } from "node:url"
import { promisify } from "node:util"

const runFile = promisify(execFile)
const root = fileURLToPath(new URL("..", import.meta.url))
const folder = "verification"
const recordFile = posix.join(folder, "record.json")
const chartFolder = posix.join(folder, "charts")
const skillName = "verify-quick-run"
const defaultBranch = "origin/main"
const passed = "Model checking completed. No error has been found."

const sha256 = (data) => createHash("sha256").update(data).digest("hex")

const git = async (...args) =>
  (await runFile("git", args, { cwd: root, maxBuffer: 64 * 1024 * 1024 })).stdout

const committed = async (path) => {
  try {
    return await git("show", `HEAD:${path}`)
  } catch {
    return undefined
  }
}

const violated = (run, name) => run.output.includes(`Invariant ${name} is violated`)

const tlcError = (run) => {
  const lines = run.output.split("\n")
  const start = lines.findIndex((line) => line.startsWith("Error:"))
  return lines
    .slice(start === -1 ? 0 : start)
    .slice(0, 120)
    .join("\n")
}

const chartFile = (branch) =>
  posix.join(chartFolder, `${branch.replace(/[^A-Za-z0-9._-]/gu, "-")}.md`)

const chartDigest = (text) => {
  const body = /^```mermaid[ \t]*\n([\s\S]*?)^```[ \t]*$/mu.exec(text)?.[1]
  return body === undefined ? undefined : sha256(body)
}

const coveredDigest = (record) =>
  sha256(
    [...record.covers, ...record.model].map((entry) => `${entry.sha256}  ${entry.file}\n`).join(""),
  )

async function toolJar(tool) {
  const path = join(root, folder, ".cache", "tla2tools", tool.version, "tla2tools.jar")
  if (!existsSync(path)) {
    const response = await fetch(tool.url)
    if (!response.ok) throw new Error(`Cannot download ${tool.url}: HTTP ${response.status}.`)
    const jar = new Uint8Array(await response.arrayBuffer())
    if (sha256(jar) !== tool.sha256) {
      throw new Error(`${tool.url} has SHA-256 ${sha256(jar)}, not the pinned ${tool.sha256}.`)
    }
    await mkdir(dirname(path), { recursive: true })
    await writeFile(`${path}.download`, jar)
    await rename(`${path}.download`, path)
  }
  const actual = sha256(await readFile(path))
  if (actual !== tool.sha256) {
    throw new Error(`${path} has SHA-256 ${actual}, not the pinned ${tool.sha256}. Delete it and run the check again.`)
  }
  return path
}

async function tlc(jar, specName, spec, config, checkDeadlock) {
  const directory = await mkdtemp(join(tmpdir(), `${skillName}-`))
  try {
    await writeFile(join(directory, specName), spec)
    await writeFile(join(directory, "Check.cfg"), config)
    const args = [
      "-XX:+UseParallelGC",
      `-Djava.io.tmpdir=${directory}`,
      "-cp",
      jar,
      "tlc2.TLC",
      "-workers",
      "1",
      ...(checkDeadlock ? [] : ["-deadlock"]),
      "-metadir",
      join(directory, "states"),
      "-config",
      "Check.cfg",
      specName,
    ]
    try {
      const { stdout, stderr } = await runFile("java", args, {
        cwd: directory,
        maxBuffer: 64 * 1024 * 1024,
      })
      return { code: 0, output: `${stdout}${stderr}` }
    } catch (error) {
      if (error.code === "ENOENT") {
        throw new Error("The check needs Java 11 or later on the PATH to run TLC.")
      }
      if (typeof error.code !== "number") throw error
      return { code: error.code, output: `${error.stdout ?? ""}${error.stderr ?? ""}` }
    }
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
}

async function checkRecord(record) {
  const problems = []
  for (const entry of [...record.covers, ...record.model]) {
    const path = join(root, entry.file)
    if (!existsSync(path)) {
      problems.push(`Verified file ${entry.file} is missing. Follow the ${skillName} skill to update the model and ${recordFile}.`)
      continue
    }
    const actual = sha256(await readFile(path))
    if (actual !== entry.sha256) {
      problems.push(
        `Verified code changed and the verification record did not: ${entry.file} has SHA-256 ${actual}, and ${recordFile} has ${entry.sha256}. Follow the ${skillName} skill to confirm the model, then record the new digest.`,
      )
    }
  }
  return problems
}

async function checkModel(record) {
  const [specEntry, configEntry] = record.model
  const spec = await readFile(join(root, specEntry.file), "utf8")
  const config = await readFile(join(root, configEntry.file), "utf8")
  const specName = basename(specEntry.file)
  const [base = "", invariantBlock = ""] = config.split(/^INVARIANTS\n/mu)
  const configured = invariantBlock.split(/\s+/u).filter((name) => name !== "")
  const recorded = ["TypeOK", ...record.properties.map((property) => property.name)]
  const problems =
    configured.join(" ") === recorded.join(" ")
      ? []
      : [
          `${configEntry.file} lists the invariants ${configured.join(", ")}, and ${recordFile} lists ${recorded.join(", ")}. Make the two lists the same.`,
        ]
  const withInvariants = (names) => `${base}INVARIANTS\n${names.map((name) => `  ${name}\n`).join("")}`
  const holding = record.properties.filter((property) => property.status === "holds")
  const findings = record.properties.filter((property) => property.status === "finding")
  const jar = await toolJar(record.tool)

  const holdsRun = tlc(jar, specName, spec, withInvariants(["TypeOK", ...holding.map((property) => property.name)]), true).then(
    (run) => {
      if (run.code !== 0 || !run.output.includes(passed)) {
        return { problems: [`The verification fails:\n${tlcError(run)}`], notes: [] }
      }
      const states = /([\d,]+) distinct states found, 0 states left on queue/u.exec(run.output)?.[1] ?? "?"
      return {
        problems: [],
        notes: [
          `Properties that hold: ${holding.map((property) => property.name).join(", ")}.`,
          `TLC ${record.tool.version} checked ${states} distinct states.`,
        ],
      }
    },
  )

  const findingRuns = findings.map((property) =>
    tlc(jar, specName, spec, withInvariants([property.name]), false).then((run) => {
      if (violated(run, property.name)) {
        return { problems: [], notes: [`Recorded finding ${property.name} still breaks.`] }
      }
      if (run.code === 0) {
        return {
          problems: [
            `The recorded finding ${property.name} no longer breaks. Confirm the fix, set its status to holds, and record a break for it.`,
          ],
          notes: [],
        }
      }
      return { problems: [`The verification fails on ${property.name}:\n${tlcError(run)}`], notes: [] }
    }),
  )

  const breakRuns = holding.map(async (property) => {
    const recordedBreak = property.break
    if (recordedBreak === undefined) {
      return { problems: [`Property ${property.name} has no recorded break in ${recordFile}.`], notes: [] }
    }
    if (spec.split(recordedBreak.find).length !== 2) {
      return {
        problems: [
          `The recorded break of ${property.name} does not match ${specEntry.file} exactly one time. Update the break in ${recordFile}.`,
        ],
        notes: [],
      }
    }
    const broken = spec.replace(recordedBreak.find, () => recordedBreak.replace)
    const run = await tlc(jar, specName, broken, withInvariants([property.name]), false)
    if (violated(run, property.name)) {
      return { problems: [], notes: [`Recorded break of ${property.name} is caught.`] }
    }
    return {
      problems: [
        `The verification does not catch the recorded break of ${property.name}. The property cannot fail, so correct the model or the property.`,
      ],
      notes: [],
    }
  })

  const outcomes = await Promise.all([holdsRun, ...findingRuns, ...breakRuns])
  return {
    problems: [...problems, ...outcomes.flatMap((outcome) => outcome.problems)],
    notes: outcomes.flatMap((outcome) => outcome.notes),
  }
}

async function checkChart(record, branch) {
  let base
  try {
    base = (await git("merge-base", defaultBranch, "HEAD")).trim()
  } catch {
    return {
      problems: [`Cannot find the merge base of HEAD and ${defaultBranch}. Fetch ${defaultBranch} with the full history.`],
      notes: [],
    }
  }
  if (base === (await git("rev-parse", "HEAD")).trim()) {
    return { problems: [], notes: [`State chart file: not checked, because HEAD has no change from ${defaultBranch}.`] }
  }
  const changes = (await git("diff", "--name-status", "--no-renames", base, "HEAD"))
    .split("\n")
    .filter((line) => line !== "")
    .map((line) => line.split("\t"))
  const charts = changes
    .filter(([status, path = ""]) => status !== "D" && posix.dirname(path) === chartFolder && path.endsWith(".md"))
    .map(([, path = ""]) => path)
  const expected = branch === undefined ? undefined : chartFile(branch)
  if (expected !== undefined && !charts.includes(expected)) {
    return {
      problems: [
        `The change adds or changes no state chart file ${expected} for the branch ${branch}. Follow the ${skillName} skill to add one.`,
      ],
      notes: [],
    }
  }
  if (charts.length === 0) {
    return {
      problems: [`The change adds or changes no state chart file in ${chartFolder}. Follow the ${skillName} skill to add one.`],
      notes: [],
    }
  }
  const verified = new Set([...record.covers, ...record.model].map((entry) => entry.file))
  const touchesVerification = changes.some(([, path = ""]) => verified.has(path))
  const digest = coveredDigest(record)
  const problems = []
  for (const chart of charts) {
    const text = (await committed(chart)) ?? ""
    const drawn = chartDigest(text)
    if (drawn === undefined && !/^No state change\b/mu.test(text)) {
      problems.push(`${chart} has no mermaid chart and no "No state change" note.`)
    }
    if (drawn !== undefined) {
      const image = chart.replace(/\.md$/u, ".svg")
      const svg = await committed(image)
      const named = svg === undefined ? undefined : /Chart digest: ([0-9a-f]{64})/u.exec(svg)?.[1]
      if (svg === undefined) {
        problems.push(
          `${chart} has a chart, and no SVG image ${image} is beside it. Render the chart to that file as the ${skillName} skill says.`,
        )
      } else if (named !== drawn) {
        problems.push(
          `${image} is older than the chart text of ${chart}: the image names chart digest ${named ?? "none"}, and the chart text has ${drawn}. Render the image again from the chart text, and write the new digest in it.`,
        )
      }
    }
    if (touchesVerification && !text.includes(digest)) {
      problems.push(
        `The change touches verified code or the model, and ${chart} does not name the covered digest ${digest}. Confirm the chart against the change, then write the digest in it.`,
      )
    }
  }
  return { problems, notes: problems.length === 0 ? [`State chart file: ${charts.join(", ")}.`] : [] }
}

async function currentBranch() {
  if (process.env.GITHUB_HEAD_REF) return process.env.GITHUB_HEAD_REF
  try {
    return (await git("symbolic-ref", "--quiet", "--short", "HEAD")).trim()
  } catch {
    return undefined
  }
}

async function check() {
  if (!existsSync(join(root, recordFile))) {
    return {
      problems: [`The repository has no verification record. Expected ${recordFile}. Follow the ${skillName} skill to set up the verification.`],
      notes: [],
    }
  }
  const record = JSON.parse(await readFile(join(root, recordFile), "utf8"))
  const outcomes = await Promise.all([
    checkRecord(record).then((problems) => ({ problems, notes: [] })),
    checkModel(record),
    currentBranch().then((branch) => checkChart(record, branch)),
  ])
  return {
    problems: outcomes.flatMap((outcome) => outcome.problems),
    notes: outcomes.flatMap((outcome) => outcome.notes),
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const started = performance.now()
  try {
    const { problems, notes } = await check()
    const seconds = ((performance.now() - started) / 1000).toFixed(1)
    for (const note of notes) console.log(note)
    if (problems.length > 0) {
      console.error(`Verification failed in ${seconds} s:\n\n${problems.join("\n\n")}`)
      process.exit(1)
    }
    console.log(`Verification passed in ${seconds} s.`)
  } catch (error) {
    console.error(`Verification failed: ${error.message}`)
    process.exit(1)
  }
}
