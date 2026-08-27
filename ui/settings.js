// The Settings layer of the overlay, spec section 7.
//
// Loaded twice: by the QML overlay through `import "settings.js" as Settings`,
// and by `settings.test.js` under node. Nothing here may touch a QML or a node
// API, because each side only has one of them.
//
// Storage is this plugin's entry in `~/.config/omarchy/shell.json`. The rules
// this file owns are the ones spec section 7 states: an unknown stored value
// reads as the default and nothing is rewritten until the user changes it.
// `cli/src/settings.rs` is the same contract on the Rust side, so the two must
// agree on the entry lookup and on what counts as unknown.

var PLUGIN_ID = "io.github.jyooi.grammachy"

// What the empty `openrouterModel` field shows, the recommended cloud model of
// `docs/spec/evals.md` section 5.1. It is a placeholder and never a value:
// spec section 7 gives that key no built-in default, so an empty field stays
// empty and the CLI answers `bad_arguments`. `settings::OPENROUTER_MODEL_
// PLACEHOLDER` is the Rust copy, kept equal by `cli/tests/overlay_cloud.rs`.
var OPENROUTER_MODEL_PLACEHOLDER = "google/gemini-3.7-flash"

// Every key the overlay reads, with the default of spec section 7.
// `targetEnglish` and `openaiApiKey` are file only, so they get no descriptor
// and no control; `mergedEntry` still carries them across a write untouched.
//
// `cloudConsent` is file only in the sense of section 7 too: the Settings view
// draws no control for it, and only the consent card writes it. It still needs
// a descriptor, because the overlay reads it through `valueOf` to decide
// whether that card is due.
//
// `openrouterModel` is the one text field whose fallback is the empty string,
// which is what makes an unset cloud model `bad_arguments` rather than a model
// nobody chose.
var DESCRIPTORS = {
  nativeLanguage: { type: "enum", values: ["none", "zh", "ms", "es", "fr", "de", "pt", "ja"], fallback: "none" },
  engine: { type: "enum", values: ["languagetool", "openai", "harper", "openrouter"], fallback: "languagetool" },
  autoReplace: { type: "boolean", fallback: false },
  openaiBaseUrl: { type: "string", fallback: "http://127.0.0.1:8080" },
  openaiModel: { type: "string", fallback: "qwen3.8-4b" },
  localThinking: { type: "boolean", fallback: true },
  openrouterModel: { type: "string", fallback: "" },
  cloudConsent: { type: "boolean", fallback: false }
}

// The dropdown rows, in the order spec section 7 fixes. The labels are the
// display names; the values are what the file and the CLI speak.
var NATIVE_LANGUAGE_OPTIONS = [
  { value: "none", label: "None" },
  { value: "zh", label: "Chinese" },
  { value: "ms", label: "Malay" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
  { value: "pt", label: "Portuguese" },
  { value: "ja", label: "Japanese" }
]

var ENGINE_OPTIONS = [
  { value: "languagetool", label: "LanguageTool" },
  { value: "openai", label: "Local LLM" },
  { value: "harper", label: "Harper" },
  { value: "openrouter", label: "Cloud LLM (OpenRouter)" }
]

// The one engine that sends text off this machine, `docs/spec/evals.md`
// section 7. Every rule below that says "cloud" means this slug.
var CLOUD_ENGINE = "openrouter"

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value)
}

function findById(entries, id) {
  if (!Array.isArray(entries)) return null
  for (var i = 0; i < entries.length; i++) {
    if (isPlainObject(entries[i]) && String(entries[i].id) === id) return entries[i]
  }
  return null
}

// This plugin's entry in one shell config, from the bar layout first and the
// top level `plugins` array next. That order is what the shell itself writes
// in: `updateEntryInline` and `setBarWidget` both reach for the bar layout
// before anything else, so a stale `plugins` copy must never win.
function entryOf(config, pluginId) {
  var id = pluginId === undefined ? PLUGIN_ID : String(pluginId)
  if (!isPlainObject(config)) return ({})
  var sections = ["left", "center", "right"]
  var layout = isPlainObject(config.bar) && isPlainObject(config.bar.layout) ? config.bar.layout : null
  for (var s = 0; layout && s < sections.length; s++) {
    var found = findById(layout[sections[s]], id)
    if (found) return found
  }
  return findById(config.plugins, id) || ({})
}

function defaultOf(name) {
  var descriptor = DESCRIPTORS[name]
  return descriptor ? descriptor.fallback : undefined
}

// Whether a candidate value is one this key accepts. A text field that carries
// nothing is not: `settings::non_empty` in `cli/src/settings.rs` trims before
// it decides, so a field of blanks reads as the default on both sides and no
// Check runs on a model id or an address that is only whitespace.
function isKnown(name, value) {
  var descriptor = DESCRIPTORS[name]
  if (!descriptor) return false
  if (descriptor.type === "boolean") return typeof value === "boolean"
  if (typeof value !== "string") return false
  if (descriptor.type === "enum") return descriptor.values.indexOf(value) !== -1
  return value.trim().length > 0
}

// What the Settings view shows and what a Check runs with. An unknown stored
// value reads as `fallback`, which defaults to the spec section 7 default of
// that key. Reading never writes, so the unknown value stays in the file.
function valueOf(entry, name, fallback) {
  var missing = fallback === undefined ? defaultOf(name) : fallback
  if (!isPlainObject(entry)) return missing
  return isKnown(name, entry[name]) ? entry[name] : missing
}

// The display name of one option value, from the same list the dropdown draws.
// The error cards of spec section 8 name the engine, and they have to name it
// the way the Settings view does, so the labels live in one place only.
// An unlisted value answers with itself, which beats an empty title.
function labelOf(options, value) {
  if (!Array.isArray(options)) return String(value)
  for (var i = 0; i < options.length; i++) {
    if (isPlainObject(options[i]) && options[i].value === value) return String(options[i].label)
  }
  return String(value)
}

// What to store for a value the user just chose. A text field the user emptied
// falls back to the default rather than writing a value the CLI would ignore.
function normalised(name, value) {
  return isKnown(name, value) ? value : defaultOf(name)
}

// The whole entry to hand to `shell.updateEntryInline`, which replaces the
// entry rather than merging into it. Carrying every stored key across is what
// keeps the file-only keys and any unknown value the user has not touched.
function mergedEntry(entry, name, value) {
  var next = ({})
  if (isPlainObject(entry)) {
    for (var key in entry) if (key !== "id") next[key] = entry[key]
  }
  next[name] = value
  return next
}

// ------------------------------------------------------------- the cloud

// Whether the consent card of `docs/spec/evals.md` section 7 is due.
//
// Only a Check on the cloud engine asks, and it asks once: the stored
// `cloudConsent` is the whole answer. Picking the engine in the dropdown never
// asks, because no text has left the machine yet, so a hand-edited
// `shell.json` still meets this gate on its first Check.
function needsCloudConsent(engineSlug, entry) {
  if (String(engineSlug) !== CLOUD_ENGINE) return false
  return valueOf(entry, "cloudConsent") !== true
}

// The consent card itself. The overlay draws the title, the body, and the meta
// line; `Continue` writes `cloudConsent` and `Cancel` sends nothing.
function cloudConsentCard(modelId) {
  var model = typeof modelId === "string" ? modelId.trim() : ""
  return {
    title: "Send text to OpenRouter?",
    body: "The cloud engine sends the text of this check to openrouter.ai, which passes it to the model provider."
      + " No other engine sends your text off this machine."
      + " Continue keeps this answer, and every later cloud check runs without this card.",
    meta: model.length > 0 ? "cloud engine, " + model : "cloud engine, no model set"
  }
}

// The state words the `key` check of `grammachy doctor` carries, one per key
// file state. `cli/src/doctor/report.rs` is the authority on this list.
var KEY_READY = "ready"
var KEY_MISSING = "missing"
var KEY_EMPTY = "empty"
var KEY_LOOSE = "loose"
var KEY_NO_HOME = "noHome"

// The OpenRouter key state, read out of one `grammachy doctor --json` report.
//
// The key is a 0600 file the CLI owns, so `doctor` is the only reader the
// overlay has and no QML ever touches the key itself. A report that never
// arrived, or one that carries no `key` check, answers null rather than a
// guess at which state it is in.
function keyState(report) {
  if (!isPlainObject(report) || !Array.isArray(report.checks)) return null
  for (var i = 0; i < report.checks.length; i++) {
    var check = report.checks[i]
    if (!isPlainObject(check) || String(check.id) !== "key") continue
    return {
      present: check.ok === true,
      state: typeof check.state === "string" ? check.state : "",
      remedy: typeof check.remedy === "string" ? check.remedy : ""
    }
  }
  return null
}

// The label of the hint line, from the state word the `key` check carries.
//
// A key file that exists but holds nothing, or that another user can read, is
// neither present nor missing: the remedy beside it acts on a file that is
// there. So those two states get a label of their own, and a hint never offers
// a chmod for a key it calls missing.
//
// An older binary sends no state word. Then the pair `ok` names is still the
// truthful answer, so the label degrades rather than breaks.
function keyLabel(state) {
  var word = typeof state.state === "string" ? state.state : ""
  if (word === KEY_LOOSE || word === KEY_EMPTY) return "key: found, not usable"
  if (word === KEY_READY) return "key: present"
  if (word === KEY_MISSING || word === KEY_NO_HOME) return "key: missing"
  return state.present === true ? "key: present" : "key: missing"
}

// The hint line under the cloud model field. The setup command is whatever
// `doctor` named as the remedy, so the two never drift apart.
function keyHint(state) {
  if (!isPlainObject(state)) return ""
  var head = keyLabel(state)
  var remedy = typeof state.remedy === "string" ? state.remedy : ""
  return remedy.length > 0 ? head + ". Run: " + remedy : head
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    PLUGIN_ID: PLUGIN_ID,
    DESCRIPTORS: DESCRIPTORS,
    NATIVE_LANGUAGE_OPTIONS: NATIVE_LANGUAGE_OPTIONS,
    ENGINE_OPTIONS: ENGINE_OPTIONS,
    CLOUD_ENGINE: CLOUD_ENGINE,
    OPENROUTER_MODEL_PLACEHOLDER: OPENROUTER_MODEL_PLACEHOLDER,
    entryOf: entryOf,
    defaultOf: defaultOf,
    isKnown: isKnown,
    valueOf: valueOf,
    labelOf: labelOf,
    normalised: normalised,
    mergedEntry: mergedEntry,
    needsCloudConsent: needsCloudConsent,
    cloudConsentCard: cloudConsentCard,
    keyState: keyState,
    keyLabel: keyLabel,
    keyHint: keyHint
  }
}
