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
