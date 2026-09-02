// The freshness rule of the capture, spec section 3.
//
// Loaded twice: by Overlay.qml through `import "capture.js" as Capture`, and
// by `capture.test.js` under node. Nothing here may touch a QML or a node API,
// because each side only has one of them.
//
// The compositor keeps the last primary selection for as long as the source
// window owns it, so a summon with nothing highlighted reads the text of the
// summon before it. The Ctrl + C fallback of step 2 has the same shape: a
// keystroke that copies nothing leaves the clipboard as it was, and what the
// shell reads back is then the clipboard rather than a Selection.
//
// This file owns both answers. The overlay owns the processes, the kept
// record, and the card.

// The one line the empty state prints, spec sections 3 and 6.
var NOTHING_NEW = "No new selection. Highlight text and press SUPER + SHIFT + Q, or paste here."
// The secondary button beside it, which runs the Check on the kept text with
// no second capture.
var CHECK_LAST_AGAIN = "Check last text again"

// The record the overlay keeps of the last consumed capture: the exact text
// and the address of the window it came from. An address of "" is no source
// window, which the compositor answers for the desktop background.
function kept(text, address) {
  return {
    text: typeof text === "string" ? text : "",
    address: typeof address === "string" ? address : ""
  }
}

// A capture is stale when it is the text the last consumed capture held and it
// came from the same window. The same words highlighted in another window are
// a fresh Selection, and so is any other text in the same one.
function isStale(text, address, last) {
  if (!last || typeof last.text !== "string" || last.text.length === 0) return false
  if (typeof text !== "string" || text.length === 0) return false
  return text === last.text && String(address || "") === String(last.address || "")
}

// Step 2 of spec section 3 copied nothing when what the clipboard holds after
// the keystroke is what it held before it. A field with nothing selected
// answers Ctrl + C that way, so what the shell reads back is the clipboard of
// some earlier copy rather than a Selection.
//
// Two windows that hold the same words are the cost of this rule: the reader
// gets the empty state and the button that checks the kept text again.
function copiedNothing(before, after) {
  return String(before || "") === String(after || "")
}

// The bounds on what `wl-paste` may hand the shell. The shell collects a
// process's whole output before it looks at it, so the bound has to sit in
// front of the collector, and the command line is what puts it there.
//
// A Selection also goes to Compose, whose Draft cap is 50,000 UTF-16 units
// (`chunk::MAX_DRAFT_UTF16_UNITS`). One UTF-8 character of four bytes gives
// two UTF-16 units, and no character gives fewer units per byte than that,
// so 200,000 bytes always hold more than the cap. Text past the cap
// therefore reaches the oversize-Draft refusal of `Overlay.qml` rather than
// a silent cut.
var CAPTURE_LIMIT_BYTES = 200000
// The borrowed clipboard goes back exactly as it was, so it may be larger.
// A clipboard past this bound cannot go back whole, so it is not borrowed.
var CLIPBOARD_BORROW_LIMIT_BYTES = 1048576
// How long `wl-paste` may wait on a selection owner that does not answer.
var PASTE_TIMEOUT_SECONDS = 5

// The command that reads one selection within the bounds above. `timeout`
// ends a paste whose owner never answers, and `head` stops the collector at
// the byte bound. Both are coreutils. Every word is a literal of this file.
function pasteCommand(primary, limitBytes) {
  var source = primary ? "wl-paste --primary --no-newline" : "wl-paste --no-newline"
  var line = "timeout " + PASTE_TIMEOUT_SECONDS + " " + source + " | head -c " + limitBytes
  return ["sh", "-c", line]
}

function primaryCommand() {
  return pasteCommand(true, CAPTURE_LIMIT_BYTES)
}

function fallbackCommand() {
  return pasteCommand(false, CAPTURE_LIMIT_BYTES)
}

function borrowCommand() {
  return pasteCommand(false, CLIPBOARD_BORROW_LIMIT_BYTES)
}

// The UTF-8 length of one string, which is what `head -c` counted.
function utf8Bytes(text) {
  var bytes = 0
  for (var index = 0; index < text.length; index += 1) {
    var code = text.charCodeAt(index)
    if (code < 0x80) bytes += 1
    else if (code < 0x800) bytes += 2
    else if (code >= 0xd800 && code <= 0xdbff) {
      // A surrogate pair is one four-byte character.
      bytes += 4
      index += 1
    } else bytes += 3
  }
  return bytes
}

// Whether the borrowed clipboard reached its bound. `head` cuts on a byte,
// so the last character may have come back as a replacement character of
// three bytes. Anything within that of the bound counts as cut, and a
// clipboard that was cut is not borrowed, because it could not go back whole.
function borrowOverflowed(text) {
  if (typeof text !== "string") return false
  return utf8Bytes(text) >= CLIPBOARD_BORROW_LIMIT_BYTES - 3
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    NOTHING_NEW: NOTHING_NEW,
    CHECK_LAST_AGAIN: CHECK_LAST_AGAIN,
    CAPTURE_LIMIT_BYTES: CAPTURE_LIMIT_BYTES,
    CLIPBOARD_BORROW_LIMIT_BYTES: CLIPBOARD_BORROW_LIMIT_BYTES,
    PASTE_TIMEOUT_SECONDS: PASTE_TIMEOUT_SECONDS,
    kept: kept,
    isStale: isStale,
    copiedNothing: copiedNothing,
    pasteCommand: pasteCommand,
    primaryCommand: primaryCommand,
    fallbackCommand: fallbackCommand,
    borrowCommand: borrowCommand,
    utf8Bytes: utf8Bytes,
    borrowOverflowed: borrowOverflowed
  }
}
