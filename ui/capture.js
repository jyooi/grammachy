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
var NOTHING_NEW = "No new selection. Highlight text and press SUPER + G, or paste here."
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

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    NOTHING_NEW: NOTHING_NEW,
    CHECK_LAST_AGAIN: CHECK_LAST_AGAIN,
    kept: kept,
    isStale: isStale,
    copiedNothing: copiedNothing
  }
}
