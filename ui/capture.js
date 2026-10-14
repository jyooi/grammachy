var NOTHING_NEW = "No new selection. Highlight text and press SUPER + SHIFT + Q, or paste here."
var CHECK_LAST_AGAIN = "Check last text again"

function kept(text, address) {
  return {
    text: typeof text === "string" ? text : "",
    address: typeof address === "string" ? address : ""
  }
}

function isStale(text, address, last) {
  if (!last || typeof last.text !== "string" || last.text.length === 0) return false
  if (typeof text !== "string" || text.length === 0) return false
  return text === last.text && String(address || "") === String(last.address || "")
}

function copiedNothing(before, after) {
  return String(before || "") === String(after || "")
}

var PASTE_LIMIT_BYTES = 1048576
var PASTE_TIMEOUT_SECONDS = 5

function pasteCommand(primary, limitBytes) {
  var source = primary ? "wl-paste --primary --no-newline" : "wl-paste --no-newline"
  var line = "timeout " + PASTE_TIMEOUT_SECONDS + " " + source + " | head -c " + limitBytes
  return ["sh", "-c", line]
}

function primaryCommand() {
  return pasteCommand(true, PASTE_LIMIT_BYTES)
}

function fallbackCommand() {
  return pasteCommand(false, PASTE_LIMIT_BYTES)
}

function borrowCommand() {
  return pasteCommand(false, PASTE_LIMIT_BYTES)
}

function utf8Bytes(text) {
  var bytes = 0
  for (var index = 0; index < text.length; index += 1) {
    var code = text.charCodeAt(index)
    if (code < 0x80) bytes += 1
    else if (code < 0x800) bytes += 2
    else if (code >= 0xd800 && code <= 0xdbff) {
      bytes += 4
      index += 1
    } else bytes += 3
  }
  return bytes
}

function pasteOverflowed(text) {
  if (typeof text !== "string") return false
  return utf8Bytes(text) >= PASTE_LIMIT_BYTES - 3
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    NOTHING_NEW: NOTHING_NEW,
    CHECK_LAST_AGAIN: CHECK_LAST_AGAIN,
    PASTE_LIMIT_BYTES: PASTE_LIMIT_BYTES,
    PASTE_TIMEOUT_SECONDS: PASTE_TIMEOUT_SECONDS,
    kept: kept,
    isStale: isStale,
    copiedNothing: copiedNothing,
    pasteCommand: pasteCommand,
    primaryCommand: primaryCommand,
    fallbackCommand: fallbackCommand,
    borrowCommand: borrowCommand,
    utf8Bytes: utf8Bytes,
    pasteOverflowed: pasteOverflowed
  }
}
