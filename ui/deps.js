// Loaded twice: by QML as `Deps`, and by `deps.test.js` under node. Nothing
// here may touch a QML or a node API, because each side only has one of them.

var INSTALL_HINT_TAIL = "through Omarchy Install. Open SUPER+SPACE, then Install, then Package."

var DEPENDENCIES = [
  {
    name: "curl",
    package: "curl",
    purpose: "bin/bootstrap.sh downloads the pinned companion binary with it.",
    required: true,
    probe: "curl",
    usedBy: ["bootstrap"]
  },
  {
    name: "wl-clipboard",
    package: "wl-clipboard",
    purpose: "Capture, paste, and the restored Selection all go through wl-copy and wl-paste.",
    required: true,
    probe: "wl-copy",
    usedBy: ["capture"]
  },
  {
    name: "libarchive",
    package: "libarchive",
    purpose: "grammachy engine install unpacks the LanguageTool release with bsdtar.",
    required: false,
    probe: "bsdtar",
    usedBy: ["languagetool"]
  },
  {
    name: "Java runtime",
    package: "jre-openjdk",
    purpose: "LanguageTool runs on it, and Harper needs none.",
    required: false,
    probe: "java",
    usedBy: ["languagetool"]
  }
]

var JAVA_PACKAGE = "jre-openjdk"

function absentFor(dependencies, part) {
  return absent(dependencies).filter(function(dependency) {
    return Array.isArray(dependency.usedBy) && dependency.usedBy.indexOf(String(part)) !== -1
  })
}

function needsHint(dependencies) {
  var list = Array.isArray(dependencies) ? dependencies : []
  var words = list.map(function(dependency) {
    return dependency.package === JAVA_PACKAGE ? "a Java runtime" : String(dependency.package)
  })
  if (words.length === 0) return ""
  return "Needs " + words.join(" and ")
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value)
}

function row(spec, present) {
  return {
    name: spec.name,
    package: spec.package,
    purpose: spec.purpose,
    required: spec.required === true,
    present: present === true,
    installHint: installHint([spec.package]),
    usedBy: spec.usedBy.slice()
  }
}

function installHint(packages) {
  var list = Array.isArray(packages) ? packages : []
  if (list.length === 0) return ""
  return "Add " + list.join(" and ") + " " + INSTALL_HINT_TAIL
}

function fromDoctor(stdout) {
  var envelope = null
  try {
    envelope = JSON.parse(stdout)
  } catch (error) {
    envelope = null
  }
  if (!isPlainObject(envelope) || envelope.contractVersion !== 1) return null
  if (!Array.isArray(envelope.dependencies)) return null

  var present = {}
  for (var i = 0; i < envelope.dependencies.length; i++) {
    var reported = envelope.dependencies[i]
    if (isPlainObject(reported) && typeof reported.package === "string")
      present[reported.package] = reported.present === true
  }
  return DEPENDENCIES.map(function(spec) { return row(spec, present[spec.package]) })
}

function probeArgv() {
  var script = 'for name in "$@"; do command -v "$name" >/dev/null 2>&1 && echo "$name"; done; exit 0'
  return ["sh", "-c", script, "grammachy-deps"].concat(DEPENDENCIES.map(function(spec) { return spec.probe }))
}

function fromProbe(stdout) {
  var found = String(stdout || "").split("\n")
  return DEPENDENCIES.map(function(spec) { return row(spec, found.indexOf(spec.probe) !== -1) })
}

function absent(dependencies) {
  var list = Array.isArray(dependencies) ? dependencies : []
  return list.filter(function(dependency) {
    return isPlainObject(dependency) && dependency.present !== true
  })
}

function missingRequired(dependencies) {
  return absent(dependencies).filter(function(dependency) { return dependency.required === true })
}

function isPresent(dependencies, pkg) {
  var list = Array.isArray(dependencies) ? dependencies : []
  for (var i = 0; i < list.length; i++) {
    if (isPlainObject(list[i]) && list[i].package === pkg) return list[i].present === true
  }
  return false
}

function packagesOf(dependencies) {
  var list = Array.isArray(dependencies) ? dependencies : []
  return list.map(function(dependency) { return String(dependency.package) })
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    INSTALL_HINT_TAIL: INSTALL_HINT_TAIL,
    DEPENDENCIES: DEPENDENCIES,
    JAVA_PACKAGE: JAVA_PACKAGE,
    installHint: installHint,
    fromDoctor: fromDoctor,
    probeArgv: probeArgv,
    fromProbe: fromProbe,
    absent: absent,
    absentFor: absentFor,
    needsHint: needsHint,
    missingRequired: missingRequired,
    isPresent: isPresent,
    packagesOf: packagesOf
  }
}
