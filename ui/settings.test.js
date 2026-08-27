// Node tests for the Settings layer. Spec sections 7 and 13.
// Run with `node --test ui/settings.test.js`.

const test = require("node:test")
const assert = require("node:assert/strict")
const fs = require("node:fs")
const os = require("node:os")
const path = require("node:path")
const { spawnSync } = require("node:child_process")

const {
  PLUGIN_ID,
  NATIVE_LANGUAGE_OPTIONS,
  ENGINE_OPTIONS,
  CLOUD_ENGINE,
  OPENROUTER_MODEL_PLACEHOLDER,
  entryOf,
  defaultOf,
  isKnown,
  valueOf,
  labelOf,
  normalised,
  mergedEntry,
  needsCloudConsent,
  cloudConsentCard,
  keyState,
  keyHint
} = require("./settings.js")

function config(entry, where) {
  const document = { version: 1, bar: { layout: { left: [], center: [], right: [] } }, plugins: [] }
  if (where === "plugins") document.plugins.push(entry)
  else document.bar.layout[where || "right"].push(entry)
  return document
}

test("the entry comes from the bar layout, whichever section holds it", () => {
  for (const section of ["left", "center", "right"]) {
    const entry = { id: PLUGIN_ID, engine: "harper" }
    assert.deepEqual(entryOf(config(entry, section)), entry)
  }
})

test("the entry comes from the plugins array when the bar layout has none", () => {
  const entry = { id: PLUGIN_ID, engine: "harper" }
  assert.deepEqual(entryOf(config(entry, "plugins")), entry)
})

// `updateEntryInline` and `setBarWidget` both write into the bar layout, so a
// leftover `plugins` copy must never be the one the view reads.
test("the bar layout wins over a stale plugins copy", () => {
  const document = config({ id: PLUGIN_ID, engine: "harper" }, "right")
  document.plugins.push({ id: PLUGIN_ID, engine: "openai" })
  assert.equal(entryOf(document).engine, "harper")
})

test("a config with no entry, and a config that is not an object, read as empty", () => {
  assert.deepEqual(entryOf(config({ id: "other.plugin", engine: "harper" })), {})
  assert.deepEqual(entryOf(null), {})
  assert.deepEqual(entryOf("nonsense"), {})
})

test("a stored value the spec lists reads back as it stands", () => {
  const entry = {
    id: PLUGIN_ID,
    nativeLanguage: "ms",
    engine: "harper",
    autoReplace: true,
    openaiBaseUrl: "http://127.0.0.1:9090",
    openaiModel: "some-model",
    localThinking: false
  }
  assert.equal(valueOf(entry, "nativeLanguage"), "ms")
  assert.equal(valueOf(entry, "engine"), "harper")
  assert.equal(valueOf(entry, "autoReplace"), true)
  assert.equal(valueOf(entry, "openaiBaseUrl"), "http://127.0.0.1:9090")
  assert.equal(valueOf(entry, "openaiModel"), "some-model")
  assert.equal(valueOf(entry, "localThinking"), false)
})

test("a missing key reads as the spec section 7 default", () => {
  assert.equal(valueOf({ id: PLUGIN_ID }, "nativeLanguage"), "none")
  assert.equal(valueOf({ id: PLUGIN_ID }, "engine"), "languagetool")
  assert.equal(valueOf({ id: PLUGIN_ID }, "autoReplace"), false)
  assert.equal(valueOf({ id: PLUGIN_ID }, "openaiBaseUrl"), "http://127.0.0.1:8080")
  assert.equal(valueOf({ id: PLUGIN_ID }, "openaiModel"), "qwen3.8-4b")
  assert.equal(valueOf({ id: PLUGIN_ID }, "localThinking"), true)
})

// Spec section 4: thinking is on by default for the local engine, and the
// stored `false` is a value rather than a missing key, so it has to survive.
test("thinking is on by default and off only when the file says so", () => {
  assert.equal(valueOf({}, "localThinking"), true)
  assert.equal(valueOf({ localThinking: false }, "localThinking"), false)
  assert.equal(valueOf({ localThinking: true }, "localThinking"), true)
  assert.equal(defaultOf("localThinking"), true)
})

test("an unknown stored value reads as the default", () => {
  assert.equal(valueOf({ nativeLanguage: "kl" }, "nativeLanguage"), "none")
  assert.equal(valueOf({ engine: "claude" }, "engine"), "languagetool")
  assert.equal(valueOf({ engine: 7 }, "engine"), "languagetool")
  assert.equal(valueOf({ autoReplace: "yes" }, "autoReplace"), false)
  assert.equal(valueOf({ openaiBaseUrl: "" }, "openaiBaseUrl"), "http://127.0.0.1:8080")
  assert.equal(valueOf({ openaiModel: null }, "openaiModel"), "qwen3.8-4b")
  assert.equal(valueOf({ localThinking: "off" }, "localThinking"), true)
  assert.equal(valueOf({ localThinking: 0 }, "localThinking"), true)
})

test("a caller may pass its own fallback, which is the seam the overlay uses", () => {
  assert.equal(valueOf({ engine: "claude" }, "engine", "harper"), "harper")
  assert.equal(valueOf({}, "autoReplace", true), true)
})

test("reading never rewrites the entry it was handed", () => {
  const entry = { id: PLUGIN_ID, engine: "claude" }
  valueOf(entry, "engine")
  valueOf(entry, "nativeLanguage")
  assert.deepEqual(entry, { id: PLUGIN_ID, engine: "claude" })
})

test("a write keeps a known value and replaces an unknown one with the default", () => {
  assert.equal(normalised("engine", "harper"), "harper")
  assert.equal(normalised("engine", "claude"), "languagetool")
  assert.equal(normalised("openaiModel", ""), "qwen3.8-4b")
  assert.equal(normalised("openaiBaseUrl", "http://127.0.0.1:9090"), "http://127.0.0.1:9090")
  assert.equal(normalised("autoReplace", true), true)
  assert.equal(normalised("localThinking", false), false)
  assert.equal(normalised("localThinking", "off"), true)
})

// `updateEntryInline` replaces the entry, so anything the merge drops is gone
// from the file. The file-only keys of spec section 7 are the ones that hurt.
test("a write carries the file-only keys across", () => {
  const entry = {
    id: PLUGIN_ID,
    targetEnglish: "en-GB",
    openaiApiKey: "secret",
    engine: "languagetool"
  }
  assert.deepEqual(mergedEntry(entry, "engine", "harper"), {
    targetEnglish: "en-GB",
    openaiApiKey: "secret",
    engine: "harper"
  })
})

test("a write leaves an unknown value of another key untouched", () => {
  const entry = { id: PLUGIN_ID, engine: "claude", nativeLanguage: "none" }
  assert.equal(mergedEntry(entry, "nativeLanguage", "ms").engine, "claude")
})

test("a write drops the id, because updateEntryInline sets it itself", () => {
  assert.equal(mergedEntry({ id: PLUGIN_ID }, "engine", "harper").id, undefined)
})

test("a write against a missing entry still produces the one key", () => {
  assert.deepEqual(mergedEntry({}, "engine", "harper"), { engine: "harper" })
  assert.deepEqual(mergedEntry(null, "engine", "harper"), { engine: "harper" })
})

test("the dropdown rows are the spec section 7 values, in that order", () => {
  assert.deepEqual(NATIVE_LANGUAGE_OPTIONS.map((o) => o.value), ["none", "zh", "ms", "es", "fr", "de", "pt", "ja"])
  assert.deepEqual(ENGINE_OPTIONS.map((o) => o.value), ["languagetool", "openai", "harper", "openrouter"])
  assert.deepEqual(ENGINE_OPTIONS.map((o) => o.label),
    ["LanguageTool", "Local LLM", "Harper", "Cloud LLM (OpenRouter)"])
})

test("every default is itself a value the dropdowns can select", () => {
  assert.ok(NATIVE_LANGUAGE_OPTIONS.some((o) => o.value === defaultOf("nativeLanguage")))
  assert.ok(ENGINE_OPTIONS.some((o) => o.value === defaultOf("engine")))
})

// Spec section 8 names the engine on its error cards, and it has to be the
// name the Settings dropdown shows, so both read the same list.
test("a label comes from the option list the dropdown draws", () => {
  assert.equal(labelOf(ENGINE_OPTIONS, "languagetool"), "LanguageTool")
  assert.equal(labelOf(ENGINE_OPTIONS, "openai"), "Local LLM")
  assert.equal(labelOf(ENGINE_OPTIONS, "harper"), "Harper")
  assert.equal(labelOf(NATIVE_LANGUAGE_OPTIONS, "ms"), "Malay")
})

test("an unlisted value labels itself rather than nothing", () => {
  assert.equal(labelOf(ENGINE_OPTIONS, "claude"), "claude")
  assert.equal(labelOf(null, "claude"), "claude")
})

// ------------------------------------------------------- the cloud engine
//
// `docs/spec/evals.md` section 7. The cloud engine is the one engine that
// sends text off this machine, so its Settings entries and its consent gate
// are the rules this block owns.

test("the cloud engine is a dropdown value with the label the spec fixes", () => {
  assert.equal(CLOUD_ENGINE, "openrouter")
  assert.equal(labelOf(ENGINE_OPTIONS, CLOUD_ENGINE), "Cloud LLM (OpenRouter)")
  assert.equal(valueOf({ engine: "openrouter" }, "engine"), "openrouter")
})

// v1 spec section 7: `openrouterModel` has no built-in default, so an empty
// field stays empty and `cli/src/settings.rs` answers `bad_arguments` for it.
// The placeholder is what the empty field shows and is never a value.
test("the cloud model has no built-in default and the placeholder is not one", () => {
  assert.equal(defaultOf("openrouterModel"), "")
  assert.equal(valueOf({}, "openrouterModel"), "")
  assert.equal(valueOf({ openrouterModel: "" }, "openrouterModel"), "")
  assert.equal(valueOf({ openrouterModel: 7 }, "openrouterModel"), "")
  assert.equal(valueOf({ openrouterModel: "vendor/model" }, "openrouterModel"), "vendor/model")
  assert.equal(normalised("openrouterModel", ""), "")
  assert.equal(normalised("openrouterModel", "vendor/model"), "vendor/model")
  assert.equal(OPENROUTER_MODEL_PLACEHOLDER, "google/gemini-3.7-flash")
  assert.notEqual(OPENROUTER_MODEL_PLACEHOLDER, defaultOf("openrouterModel"))
})

test("cloudConsent is a boolean that defaults to false", () => {
  assert.equal(defaultOf("cloudConsent"), false)
  assert.equal(valueOf({}, "cloudConsent"), false)
  assert.equal(valueOf({ cloudConsent: "yes" }, "cloudConsent"), false)
  assert.equal(valueOf({ cloudConsent: true }, "cloudConsent"), true)
})

// The gate: only a cloud Check asks, and only until the answer is stored.
test("the consent gate stands in front of a cloud check and of nothing else", () => {
  assert.equal(needsCloudConsent("openrouter", {}), true)
  assert.equal(needsCloudConsent("openrouter", { cloudConsent: false }), true)
  assert.equal(needsCloudConsent("openrouter", { cloudConsent: true }), false)
  for (const engine of ["languagetool", "openai", "harper", "claude", ""]) {
    assert.equal(needsCloudConsent(engine, {}), false, engine)
  }
})

// The acceptance rule: a hand-edited shell.json still meets the gate, because
// the gate reads the engine of the Check rather than how it was chosen.
test("a hand-edited entry still meets the gate on its first check", () => {
  const entry = { id: PLUGIN_ID, engine: "openrouter", openrouterModel: "vendor/model" }
  assert.equal(needsCloudConsent(valueOf(entry, "engine"), entry), true)
})

test("the consent card names the model the pending check would ask for", () => {
  const card = cloudConsentCard("google/gemini-3.7-flash")
  assert.equal(card.title, "Send text to OpenRouter?")
  assert.ok(card.body.includes("openrouter.ai"))
  assert.equal(card.meta, "cloud engine, google/gemini-3.7-flash")
  assert.equal(cloudConsentCard("").meta, "cloud engine, no model set")
  assert.equal(cloudConsentCard(undefined).meta, "cloud engine, no model set")
})

// Storing the answer is one key, and the merge keeps every other key, the
// file-only ones included.
test("continuing writes cloudConsent and keeps every other stored key", () => {
  const entry = {
    id: PLUGIN_ID,
    engine: "openrouter",
    openrouterModel: "vendor/model",
    openaiApiKey: "secret",
    targetEnglish: "en-GB"
  }
  const next = mergedEntry(entry, "cloudConsent", true)
  assert.deepEqual(next, {
    engine: "openrouter",
    openrouterModel: "vendor/model",
    openaiApiKey: "secret",
    targetEnglish: "en-GB",
    cloudConsent: true
  })
  assert.equal(needsCloudConsent("openrouter", next), false)
})

// A text field of blanks carries nothing. `settings::non_empty` in
// `cli/src/settings.rs` trims before it decides, so the shell has to agree or
// it labels a model as chosen for a Check the CLI refuses.
test("a text field of blanks reads as the default on every string key", () => {
  const blanks = [" ", "   ", "\t", "\n  "]

  for (const blank of blanks) {
    assert.equal(isKnown("openrouterModel", blank), false, blank)
    assert.equal(isKnown("openaiModel", blank), false, blank)
    assert.equal(isKnown("openaiBaseUrl", blank), false, blank)

    assert.equal(valueOf({ openrouterModel: blank }, "openrouterModel"), "")
    assert.equal(valueOf({ openaiModel: blank }, "openaiModel"), "gemma-4-e4b-it")
    assert.equal(valueOf({ openaiBaseUrl: blank }, "openaiBaseUrl"), "http://127.0.0.1:8080")

    // A field the user blanked out stores the default, not the blanks.
    assert.equal(normalised("openrouterModel", blank), "")
    assert.equal(normalised("openaiModel", blank), "gemma-4-e4b-it")
  }

  // A value with something in it is still kept exactly as it was typed.
  assert.equal(isKnown("openrouterModel", "google/gemini-3.7-flash"), true)
  assert.equal(normalised("openrouterModel", "google/gemini-3.7-flash"), "google/gemini-3.7-flash")
})

// The consent card names the model the pending Check would ask for, so a field
// of blanks has to read as no model rather than print the spaces.
test("a blank cloud model reads as no model set on the consent card", () => {
  for (const blank of ["", " ", "   "]) {
    assert.equal(cloudConsentCard(blank).meta, "cloud engine, no model set", blank)
  }
  assert.equal(cloudConsentCard(" google/gemini-3.7-flash ").meta,
    "cloud engine, google/gemini-3.7-flash")
})

// The key hint. The key is a 0600 file the CLI owns, so `doctor` reports its
// state and names the command that writes it, and no rule here reads the file.
test("the key hint reads the doctor report and carries the setup command", () => {
  const report = {
    contractVersion: 1,
    engine: "openrouter",
    checks: [
      { id: "binary", name: "Grammachy CLI", ok: true },
      {
        id: "key",
        name: "OpenRouter key",
        ok: false,
        state: "missing",
        detail: "No OpenRouter key.",
        remedy: "printf '%s' \"$KEY\" | grammachy setup --openrouter-key"
      }
    ]
  }
  assert.deepEqual(keyState(report), {
    present: false,
    state: "missing",
    remedy: "printf '%s' \"$KEY\" | grammachy setup --openrouter-key"
  })
  assert.equal(keyHint(keyState(report)),
    "key: missing. Run: printf '%s' \"$KEY\" | grammachy setup --openrouter-key")
})

test("a key that is in place says so and needs no command", () => {
  const report = { contractVersion: 1, checks: [{ id: "key", ok: true, state: "ready" }] }
  assert.deepEqual(keyState(report), { present: true, state: "ready", remedy: "" })
  assert.equal(keyHint(keyState(report)), "key: present")
})

// A key file that exists but cannot be used is neither present nor missing.
// The remedy beside it acts on a file that is there, so a hint that called it
// missing would offer a command the words make no sense of.
test("a loose key file is found and not usable, with the chmod beside it", () => {
  const report = {
    contractVersion: 1,
    checks: [{
      id: "key",
      ok: false,
      state: "loose",
      detail: "The OpenRouter key /home/u/.config/grammachy/openrouter-key is mode 0644.",
      remedy: "chmod 600 /home/u/.config/grammachy/openrouter-key"
    }]
  }
  assert.equal(keyState(report).state, "loose")
  assert.equal(keyHint(keyState(report)),
    "key: found, not usable. Run: chmod 600 /home/u/.config/grammachy/openrouter-key")
})

test("an empty key file is found and not usable", () => {
  const report = {
    contractVersion: 1,
    checks: [{
      id: "key",
      ok: false,
      state: "empty",
      remedy: "printf '%s' \"$KEY\" | grammachy setup --openrouter-key"
    }]
  }
  assert.equal(keyHint(keyState(report)),
    "key: found, not usable. Run: printf '%s' \"$KEY\" | grammachy setup --openrouter-key")
})

// No HOME means no key file at all, so it reads as missing. It carries no
// command, because nothing the user can run from here sets HOME.
test("no HOME reads as a missing key and offers no command", () => {
  const report = { contractVersion: 1, checks: [{ id: "key", ok: false, state: "noHome" }] }
  assert.deepEqual(keyState(report), { present: false, state: "noHome", remedy: "" })
  assert.equal(keyHint(keyState(report)), "key: missing")
})

// An older binary sends no state word. The pair `ok` names is still truthful,
// so the hint degrades to it rather than losing the label.
test("a report with no state word falls back to present and missing", () => {
  assert.equal(keyHint(keyState({ contractVersion: 1, checks: [{ id: "key", ok: true }] })),
    "key: present")
  assert.equal(
    keyHint(keyState({ contractVersion: 1, checks: [{ id: "key", ok: false, remedy: "run me" }] })),
    "key: missing. Run: run me")
})

// A word nothing here knows is not a licence to guess: `ok` answers instead.
test("an unknown state word falls back to ok", () => {
  const report = { contractVersion: 1, checks: [{ id: "key", ok: false, state: "sideways" }] }
  assert.equal(keyHint(keyState(report)), "key: missing")
})

// A report that never arrived is not a state: the view draws no hint rather
// than claiming a key it did not read about.
test("no report and no key check both read as unknown", () => {
  assert.equal(keyState(null), null)
  assert.equal(keyState({ contractVersion: 1 }), null)
  assert.equal(keyState({ contractVersion: 1, checks: [{ id: "binary", ok: true }] }), null)
  assert.equal(keyHint(null), "")
  assert.equal(keyHint(undefined), "")
})

// ----------------------------------------- the gate in front of one check
//
// The overlay puts this gate on `launchCheck`, which is the one place a Check
// leaves for the CLI. The harness below is that route: it asks the same
// question with the same rules and runs a stub binary only when the answer
// lets it. The acceptance criterion is what the stub records, because "Cancel
// sends nothing" is a claim about a process that never ran.
//
// `cli/tests/overlay_cloud.rs` is what keeps `Overlay.qml` on these calls,
// because no QML test can run the real file.

let stubDirectory = null

test.before(() => {
  stubDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "grammachy-consent-"))
})

test.after(() => {
  if (stubDirectory) fs.rmSync(stubDirectory, { recursive: true, force: true })
})

// A stub that answers an empty result envelope and appends one line per run,
// so a test can count what actually left for the CLI.
function countingStub(name) {
  const log = path.join(stubDirectory, name + ".runs")
  const file = path.join(stubDirectory, name)
  fs.writeFileSync(file, [
    "#!/bin/sh",
    "cat > /dev/null",
    'printf "run\\n" >> ' + JSON.stringify(log),
    `printf '%s' '{"contractVersion":1,"engine":"openrouter","elapsedMs":7,"issues":[]}'`,
    ""
  ].join("\n"))
  fs.chmodSync(file, 0o755)
  return { file: file, runs: () => (fs.existsSync(log) ? fs.readFileSync(log, "utf8").trim().split("\n").length : 0) }
}

// `Overlay.launchCheck`, `continueCloudCheck`, and `cancelCloudCheck` as one
// small object. `entry` is the stored plugin entry, the way the overlay reads
// and writes it.
function overlay(binary, entry) {
  return {
    entry: entry || {},
    consentGiven: false,
    phase: "checking",
    pendingText: "",
    pendingEngine: "",
    result: null,

    needsConsent(engineSlug) {
      if (this.consentGiven) return false
      return needsCloudConsent(engineSlug, this.entry)
    },

    launchCheck(text, engineSlug) {
      if (this.needsConsent(engineSlug)) {
        this.pendingText = text
        this.pendingEngine = engineSlug
        this.phase = "cloudConsent"
        return
      }
      this.phase = "checking"
      this.result = spawnSync(binary, ["check", "--engine", engineSlug],
        { input: text, encoding: "utf8" }).stdout
    },

    cloudContinue() {
      if (this.phase !== "cloudConsent") return
      const text = this.pendingText
      const engineSlug = this.pendingEngine
      this.consentGiven = true
      this.entry = mergedEntry(this.entry, "cloudConsent", true)
      this.pendingText = ""
      this.pendingEngine = ""
      this.launchCheck(text, engineSlug)
    },

    cloudCancel() {
      if (this.phase !== "cloudConsent") return
      this.pendingText = ""
      this.pendingEngine = ""
      this.phase = "editing"
    }
  }
}

test("Cancel closes the card, keeps the engine, and sends nothing", () => {
  const stub = countingStub("consent-cancel")
  const overlayed = overlay(stub.file, { engine: "openrouter", openrouterModel: "vendor/model" })

  overlayed.launchCheck("He go home.", "openrouter")
  assert.equal(overlayed.phase, "cloudConsent")
  assert.equal(stub.runs(), 0)

  overlayed.cloudCancel()
  assert.equal(overlayed.phase, "editing")
  assert.equal(overlayed.result, null)
  // The acceptance criterion: no process ran, so no text left this machine.
  assert.equal(stub.runs(), 0)
  // The engine setting stays as it was, so the next Check asks again.
  assert.equal(valueOf(overlayed.entry, "engine"), "openrouter")
  assert.equal(needsCloudConsent("openrouter", overlayed.entry), true)
})

test("Continue stores the answer and runs the check that waited on it", () => {
  const stub = countingStub("consent-continue")
  const overlayed = overlay(stub.file, { engine: "openrouter", openrouterModel: "vendor/model" })

  overlayed.launchCheck("He go home.", "openrouter")
  overlayed.cloudContinue()

  assert.equal(overlayed.phase, "checking")
  assert.equal(stub.runs(), 1)
  assert.equal(JSON.parse(overlayed.result).engine, "openrouter")
  assert.equal(valueOf(overlayed.entry, "cloudConsent"), true)
})

// Once only. A chunked Draft is many Checks through one `launchCheck`, so a
// card between two Chunks of one run would be unusable.
test("every later cloud check runs with no card", () => {
  const stub = countingStub("consent-once")
  const overlayed = overlay(stub.file, { engine: "openrouter", openrouterModel: "vendor/model" })

  overlayed.launchCheck("First chunk.", "openrouter")
  overlayed.cloudContinue()
  overlayed.launchCheck("Second chunk.", "openrouter")
  overlayed.launchCheck("Third chunk.", "openrouter")

  assert.equal(overlayed.phase, "checking")
  assert.equal(stub.runs(), 3)
})

// A hand-edited shell.json never sees the dropdown, and the gate still holds.
test("a stored consent runs straight away and a hand-edited engine still asks", () => {
  const stub = countingStub("consent-stored")
  const consented = overlay(stub.file, { engine: "openrouter", cloudConsent: true })
  consented.launchCheck("He go home.", "openrouter")
  assert.equal(consented.phase, "checking")
  assert.equal(stub.runs(), 1)

  const handEdited = overlay(stub.file, { engine: "openrouter" })
  handEdited.launchCheck("He go home.", "openrouter")
  assert.equal(handEdited.phase, "cloudConsent")
  assert.equal(stub.runs(), 1, "the second overlay ran nothing")
})

// No other engine waits on a card it never has to answer.
test("a local check never reaches the gate", () => {
  const stub = countingStub("consent-local")
  const overlayed = overlay(stub.file, { engine: "openrouter" })

  overlayed.launchCheck("He go home.", "harper")

  assert.equal(overlayed.phase, "checking")
  assert.equal(stub.runs(), 1)
})
