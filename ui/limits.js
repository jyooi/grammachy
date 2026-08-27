// The Check size limit, spec section 4.
//
// Every remaining Engine reads 5,000 UTF-16 code units. This file is the
// shell-side counterpart of `EngineSlug::check_limit_utf16` in
// `cli/src/args.rs`, and `cli/tests/overlay_limit.rs` keeps the two in step.
//
// Loaded twice: by `Overlay.qml` through `import "ui/limits.js" as Limits`,
// and by `limits.test.js` under node. Nothing here may touch a QML or a node
// API, because each side only has one of them.

var CHECK_LIMIT_UNITS = 5000

// The limit of one Check on `engineSlug`. Every Engine shares the one limit,
// so the argument exists only to keep this the one call site a future
// per-engine limit would have to change.
function checkLimit(engineSlug) {
  return CHECK_LIMIT_UNITS
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    CHECK_LIMIT_UNITS: CHECK_LIMIT_UNITS,
    checkLimit: checkLimit
  }
}
