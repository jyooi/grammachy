// The Check size limit of one Engine, spec section 4.
//
// The limit belongs to the Engine: the local LLM reads 2,000 UTF-16 code
// units, because a longer Chunk cannot be answered inside the timeout, and
// every other Engine reads 5,000. This file is the shell-side counterpart of
// `EngineSlug::check_limit_utf16` in `cli/src/args.rs`, and
// `cli/tests/overlay_limit.rs` keeps the two in step.
//
// Loaded twice: by `Overlay.qml` through `import "ui/limits.js" as Limits`,
// and by `limits.test.js` under node. Nothing here may touch a QML or a node
// API, because each side only has one of them.

// The slug of the local LLM engine, the one Engine with a smaller limit.
var LOCAL_ENGINE = "openai"

var LOCAL_CHECK_LIMIT_UNITS = 2000
var CHECK_LIMIT_UNITS = 5000

// The limit of one Check on `engineSlug`. An unknown slug reads as the wider
// limit, which is what the settings layer falls back to as well.
function checkLimit(engineSlug) {
  return String(engineSlug) === LOCAL_ENGINE ? LOCAL_CHECK_LIMIT_UNITS : CHECK_LIMIT_UNITS
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    LOCAL_ENGINE: LOCAL_ENGINE,
    LOCAL_CHECK_LIMIT_UNITS: LOCAL_CHECK_LIMIT_UNITS,
    CHECK_LIMIT_UNITS: CHECK_LIMIT_UNITS,
    checkLimit: checkLimit
  }
}
