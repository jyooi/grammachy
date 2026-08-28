// Node tests for the Settings layer. Spec sections 7 and 13.
// Run with `node --test ui/settings.test.js`.

const test = require("node:test")
const assert = require("node:assert/strict")

const {
  PLUGIN_ID,
  NATIVE_LANGUAGE_OPTIONS,
  ENGINE_OPTIONS,
  entryOf,
  defaultOf,
  isKnown,
  valueOf,
  labelOf,
  normalised,
  mergedEntry
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
  document.plugins.push({ id: PLUGIN_ID, engine: "languagetool" })
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
    autoReplace: true
  }
  assert.equal(valueOf(entry, "nativeLanguage"), "ms")
  assert.equal(valueOf(entry, "engine"), "harper")
  assert.equal(valueOf(entry, "autoReplace"), true)
})

test("a missing key reads as the spec section 7 default", () => {
  assert.equal(valueOf({ id: PLUGIN_ID }, "nativeLanguage"), "none")
  assert.equal(valueOf({ id: PLUGIN_ID }, "engine"), "harper")
  assert.equal(valueOf({ id: PLUGIN_ID }, "autoReplace"), false)
})

test("an unknown stored value reads as the default", () => {
  assert.equal(valueOf({ nativeLanguage: "kl" }, "nativeLanguage"), "none")
  assert.equal(valueOf({ engine: "claude" }, "engine"), "harper")
  assert.equal(valueOf({ engine: 7 }, "engine"), "harper")
  assert.equal(valueOf({ autoReplace: "yes" }, "autoReplace"), false)
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
  assert.equal(normalised("engine", "claude"), "harper")
  assert.equal(normalised("autoReplace", true), true)
})

// `updateEntryInline` replaces the entry, so anything the merge drops is gone
// from the file. The file-only keys of spec section 7 are the ones that hurt.
test("a write carries the file-only keys across", () => {
  const entry = {
    id: PLUGIN_ID,
    targetEnglish: "en-GB",
    engine: "languagetool"
  }
  assert.deepEqual(mergedEntry(entry, "engine", "harper"), {
    targetEnglish: "en-GB",
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
  assert.deepEqual(ENGINE_OPTIONS.map((o) => o.value), ["languagetool", "harper"])
  assert.deepEqual(ENGINE_OPTIONS.map((o) => o.label), ["LanguageTool", "Harper"])
})

test("every default is itself a value the dropdowns can select", () => {
  assert.ok(NATIVE_LANGUAGE_OPTIONS.some((o) => o.value === defaultOf("nativeLanguage")))
  assert.ok(ENGINE_OPTIONS.some((o) => o.value === defaultOf("engine")))
})

// Spec section 8 names the engine on its error cards, and it has to be the
// name the Settings dropdown shows, so both read the same list.
test("a label comes from the option list the dropdown draws", () => {
  assert.equal(labelOf(ENGINE_OPTIONS, "languagetool"), "LanguageTool")
  assert.equal(labelOf(ENGINE_OPTIONS, "harper"), "Harper")
  assert.equal(labelOf(NATIVE_LANGUAGE_OPTIONS, "ms"), "Malay")
})

test("an unlisted value labels itself rather than nothing", () => {
  assert.equal(labelOf(ENGINE_OPTIONS, "claude"), "claude")
  assert.equal(labelOf(null, "claude"), "claude")
})

test("an enum key rejects a value outside its list, blanks included", () => {
  for (const blank of [" ", "   ", "\t", "\n  "]) {
    assert.equal(isKnown("nativeLanguage", blank), false, blank)
  }
})

// Spec section 2 and 7: the two trigger hotkeys are remappable text fields,
// so an unknown or blank stored value reads as the spec default, the same
// rule every other key follows.
test("the two hotkeys default to the spec section 2 bindings", () => {
  assert.equal(valueOf({ id: PLUGIN_ID }, "quickHotkey"), "SUPER + SHIFT + Q")
  assert.equal(valueOf({ id: PLUGIN_ID }, "composeHotkey"), "SUPER + ALT + Q")
  assert.equal(defaultOf("quickHotkey"), "SUPER + SHIFT + Q")
  assert.equal(defaultOf("composeHotkey"), "SUPER + ALT + Q")
})

test("a stored hotkey reads back as it stands", () => {
  const entry = { id: PLUGIN_ID, quickHotkey: "SUPER + H", composeHotkey: "SUPER + SHIFT + H" }
  assert.equal(valueOf(entry, "quickHotkey"), "SUPER + H")
  assert.equal(valueOf(entry, "composeHotkey"), "SUPER + SHIFT + H")
})

test("a blank or missing hotkey reads as the default rather than the empty string", () => {
  for (const blank of ["", " ", "   ", "\t"]) {
    assert.equal(isKnown("quickHotkey", blank), false, blank)
    assert.equal(valueOf({ quickHotkey: blank }, "quickHotkey"), "SUPER + SHIFT + Q")
    assert.equal(valueOf({ composeHotkey: blank }, "composeHotkey"), "SUPER + ALT + Q")
  }
})

test("a write to a hotkey keeps a non-blank value and defaults a blank one", () => {
  assert.equal(normalised("quickHotkey", "SUPER + H"), "SUPER + H")
  assert.equal(normalised("quickHotkey", "  "), "SUPER + SHIFT + Q")
  assert.equal(normalised("composeHotkey", ""), "SUPER + ALT + Q")
})
