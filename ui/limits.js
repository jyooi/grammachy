// Loaded twice: by `Overlay.qml` through `import "ui/limits.js" as Limits`,
// and by `limits.test.js` under node. Nothing here may touch a QML or a node
// API, because each side only has one of them.

var CHECK_LIMIT_UNITS = 5000

function checkLimit(engineSlug) {
  return CHECK_LIMIT_UNITS
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    CHECK_LIMIT_UNITS: CHECK_LIMIT_UNITS,
    checkLimit: checkLimit
  }
}
