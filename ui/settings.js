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

// Every key the overlay reads, with the default of spec section 7.
// `targetEnglish` is file only, so it gets no descriptor and no control;
// `mergedEntry` still carries it across a write untouched.
var DESCRIPTORS = {
  nativeLanguage: { type: "enum", values: ["none", "zh", "ms", "es", "fr", "de", "pt", "ja"], fallback: "none" },
  engine: { type: "enum", values: ["languagetool", "harper"], fallback: "harper" },
  autoReplace: { type: "boolean", fallback: false }
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
  { value: "harper", label: "Harper" }
]

// The engine a fresh install checks with, spec section 4 and HUF-237.
//
// It is `harper` because that is the one engine compiled into the binary: it
// needs no download, no pacman command, and no server, so the first Check on a
// machine that has just been set up answers. It is also where the dropdown
// falls back when the engine it was on stops being available.
var BUILT_IN_ENGINE = "harper"

// ------------------------------------------------------- optional engines

// The dropdown rows to draw, spec section 7 and HUF-237.
//
// An engine that is not on this machine is not offered: picking it would only
// buy the reader an `engine_unavailable` card. `unavailable` is the list of
// slugs `ui/engines.js` read out of `grammachy engine list`, so this file
// never decides what is installed and only decides what that means.
//
// The engine the reader is already on stays in the list whatever that says.
// A dropdown that drops its own value shows a blank box, and the stored value
// is untouched until they choose something else, so hiding it would say the
// setting is one thing while the file says another.
function engineOptions(unavailable, current) {
  var missing = Array.isArray(unavailable) ? unavailable : []
  var selected = String(current === undefined ? "" : current)
  var out = []
  for (var i = 0; i < ENGINE_OPTIONS.length; i++) {
    var option = ENGINE_OPTIONS[i]
    if (option.value === selected || missing.indexOf(option.value) === -1) out.push(option)
  }
  return out
}

// The engine to fall back to when the selected one has just been removed.
//
// It is always the built-in one: it is the only engine that cannot go away, so
// it is the only answer that is true whatever else the machine has. `null`
// means nothing has to change, which is every case but the one where the
// engine that went is the engine a Check would use.
function engineAfterRemoval(current, removed) {
  return String(current) === String(removed) ? BUILT_IN_ENGINE : null
}

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

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    PLUGIN_ID: PLUGIN_ID,
    DESCRIPTORS: DESCRIPTORS,
    NATIVE_LANGUAGE_OPTIONS: NATIVE_LANGUAGE_OPTIONS,
    ENGINE_OPTIONS: ENGINE_OPTIONS,
    BUILT_IN_ENGINE: BUILT_IN_ENGINE,
    entryOf: entryOf,
    defaultOf: defaultOf,
    isKnown: isKnown,
    valueOf: valueOf,
    labelOf: labelOf,
    engineOptions: engineOptions,
    engineAfterRemoval: engineAfterRemoval,
    normalised: normalised,
    mergedEntry: mergedEntry
  }
}
