var PLUGIN_ID = "io.github.jyooi.grammachy"

var DESCRIPTORS = {
  nativeLanguage: { type: "enum", values: ["none", "zh", "ms", "es", "fr", "de", "pt", "ja"], fallback: "none" },
  targetEnglish: { type: "enum", values: ["en-US", "en-GB"], fallback: "en-US" },
  engine: { type: "enum", values: ["languagetool", "harper"], fallback: "harper" },
  autoReplace: { type: "boolean", fallback: false },
  quickHotkey: { type: "text", fallback: "SUPER + SHIFT + Q" },
  composeHotkey: { type: "text", fallback: "SUPER + ALT + Q" }
}

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

var TARGET_ENGLISH_OPTIONS = [
  { value: "en-US", label: "American (en-US)" },
  { value: "en-GB", label: "British (en-GB)" }
]

var ENGINE_OPTIONS = [
  { value: "languagetool", label: "LanguageTool" },
  { value: "harper", label: "Harper" }
]

var BUILT_IN_ENGINE = "harper"

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

// `updateEntryInline` writes the bar layout entry first and the top level
// `plugins` entry only when the layout has none.
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

function isKnown(name, value) {
  var descriptor = DESCRIPTORS[name]
  if (!descriptor) return false
  if (descriptor.type === "boolean") return typeof value === "boolean"
  if (typeof value !== "string") return false
  if (descriptor.type === "enum") return descriptor.values.indexOf(value) !== -1
  return value.trim().length > 0
}

function valueOf(entry, name, fallback) {
  var missing = fallback === undefined ? defaultOf(name) : fallback
  if (!isPlainObject(entry)) return missing
  return isKnown(name, entry[name]) ? entry[name] : missing
}

function labelOf(options, value) {
  if (!Array.isArray(options)) return String(value)
  for (var i = 0; i < options.length; i++) {
    if (isPlainObject(options[i]) && options[i].value === value) return String(options[i].label)
  }
  return String(value)
}

function normalised(name, value) {
  return isKnown(name, value) ? value : defaultOf(name)
}

// `shell.updateEntryInline` replaces the entry rather than merging into it.
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
    TARGET_ENGLISH_OPTIONS: TARGET_ENGLISH_OPTIONS,
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
