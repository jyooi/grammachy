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

// Every key the Settings view owns, with the default of spec section 7.
// `targetEnglish` and `openaiApiKey` are file only, so they get no descriptor
// and no control; `mergedEntry` still carries them across a write untouched.
var DESCRIPTORS = {
  nativeLanguage: { type: "enum", values: ["none", "zh", "ms", "es", "fr", "de", "pt", "ja"], fallback: "none" },
  engine: { type: "enum", values: ["languagetool", "openai", "harper"], fallback: "languagetool" },
  autoReplace: { type: "boolean", fallback: false },
  openaiBaseUrl: { type: "string", fallback: "http://127.0.0.1:8080" },
  openaiModel: { type: "string", fallback: "gemma-4-e4b-it" }
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
  { value: "harper", label: "Harper" }
]

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

// Whether a candidate value is one this key accepts. An empty string is not:
// `cli/src/settings.rs` reads an empty `openaiBaseUrl` as absent, so the two
// text fields have to agree that empty means the default.
function isKnown(name, value) {
  var descriptor = DESCRIPTORS[name]
  if (!descriptor) return false
  if (descriptor.type === "boolean") return typeof value === "boolean"
  if (typeof value !== "string") return false
  if (descriptor.type === "enum") return descriptor.values.indexOf(value) !== -1
  return value.length > 0
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

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    PLUGIN_ID: PLUGIN_ID,
    DESCRIPTORS: DESCRIPTORS,
    NATIVE_LANGUAGE_OPTIONS: NATIVE_LANGUAGE_OPTIONS,
    ENGINE_OPTIONS: ENGINE_OPTIONS,
    entryOf: entryOf,
    defaultOf: defaultOf,
    isKnown: isKnown,
    valueOf: valueOf,
    labelOf: labelOf,
    normalised: normalised,
    mergedEntry: mergedEntry
  }
}
