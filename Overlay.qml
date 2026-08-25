import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import "ui"
import "ui/splice.js" as Splice

// The overlay entry point. `open(payload)` routes a summon to a surface, spec
// section 2. Quick mode captures the Selection (section 3), runs one Check
// through the companion CLI (section 5.1), and shows the marked text.
//
// Compose mode, the Settings view, the error cards, and the popup keys are
// their own tickets; this file shows a plain notice card where they will land.
Item {
  id: root

  // Handed over by the shell's panel loader.
  property var shell: null
  property var manifest: null

  // The shell reads `opened` to answer isPluginOpen, and calls close().
  property bool opened: false

  // "capturing", "checking", "result", or "notice".
  property string phase: "capturing"
  property string selectionText: ""
  property var issues: []
  property var decisions: []
  property int focusIndex: 0
  property string engine: ""
  property int elapsedMs: 0
  property bool copied: false
  property string noticeTitle: ""
  property string noticeBody: ""

  // The clipboard the Ctrl + C fallback borrowed, put back once the Selection
  // is in hand. Spec section 3.
  property string borrowedClipboard: ""
  property bool clipboardBorrowed: false
  property int runGeneration: 0

  readonly property string pluginId: root.manifest && root.manifest.id
    ? String(root.manifest.id) : "io.github.jyooi.grammachy"
  // Spec section 10: the binary sits beside the plugin, never on PATH.
  readonly property string binaryPath: Util.isPlainObject(root.manifest) && root.manifest.__sourceDir
    ? String(root.manifest.__sourceDir).replace(/\/$/, "") + "/bin/grammachy"
    : String(Qt.resolvedUrl("bin/grammachy")).replace(/^file:\/\//, "")

  readonly property var barConfig: root.shell && root.shell.bar ? root.shell.bar : null
  readonly property string barPosition: root.barConfig ? String(root.barConfig.position) : "top"
  readonly property int barSize: root.barConfig ? root.barConfig.barSize : Style.bar.sizeHorizontal
  readonly property int gap: Style.gapsOut + Style.spacing.sm

  // ------------------------------------------------------------- settings
  //
  // Storage is this plugin's inline entry in shell.json, spec section 7. The
  // Settings view writes it; this ticket only reads what a Check needs.
  readonly property var entry: root.settingsEntry()

  function settingsEntry() {
    var config = root.shell ? root.shell.shellConfig : null
    if (!Util.isPlainObject(config)) return ({})
    if (Array.isArray(config.plugins)) {
      for (var i = 0; i < config.plugins.length; i++) {
        if (Util.isPlainObject(config.plugins[i]) && String(config.plugins[i].id) === root.pluginId)
          return config.plugins[i]
      }
    }
    var sections = ["left", "center", "right"]
    var layout = Util.isPlainObject(config.bar) && Util.isPlainObject(config.bar.layout) ? config.bar.layout : null
    for (var s = 0; layout && s < sections.length; s++) {
      var entries = layout[sections[s]]
      if (!Array.isArray(entries)) continue
      for (var e = 0; e < entries.length; e++) {
        if (Util.isPlainObject(entries[e]) && String(entries[e].id) === root.pluginId) return entries[e]
      }
    }
    return ({})
  }

  // An unknown stored value reads as the default, spec section 7.
  function setting(name, allowed, fallback) {
    var value = root.entry ? root.entry[name] : undefined
    if (typeof value !== "string") return fallback
    return allowed.indexOf(value) === -1 ? fallback : value
  }

  function checkCommand() {
    var command = [root.binaryPath, "check"]
    var nativeLanguage = root.setting("nativeLanguage", ["none", "zh", "ms", "es", "fr", "de", "pt", "ja"], "none")
    if (nativeLanguage !== "none") command.push("--native", nativeLanguage)
    command.push("--engine", root.setting("engine", ["languagetool", "openai", "harper"], "languagetool"))
    return command
  }

  // ---------------------------------------------------------------- surface

  function open(payloadJson) {
    var payload = ({})
    try {
      payload = JSON.parse(payloadJson || "{}") || ({})
    } catch (error) {
      console.warn("grammachy: summon payload is not JSON:", payloadJson)
    }

    root.opened = true
    if (String(payload.mode || "quick") === "compose") {
      root.showNotice("Compose is not ready yet", "The Compose window arrives in a later milestone. Use the popup on a shorter selection.")
      return
    }
    root.startQuick()
  }

  function close() {
    root.opened = false
  }

  function showNotice(title, body) {
    root.phase = "notice"
    root.noticeTitle = title
    root.noticeBody = body
  }

  // ---------------------------------------------------------------- capture

  function startQuick() {
    settleTimer.stop()
    primaryPaste.running = false
    savedClipboard.running = false
    copyKeystroke.running = false
    fallbackPaste.running = false
    checkProcess.running = false
    // New capture.
    root.runGeneration += 1
    // Reset state.
    root.phase = "capturing"
    root.selectionText = ""
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
    root.engine = ""
    root.elapsedMs = 0
    root.copied = false
    // End the last borrow.
    if (root.clipboardBorrowed) root.restoreBorrowedClipboard()
    root.borrowedClipboard = ""
    root.clipboardBorrowed = false
    primaryPaste.running = false
    primaryPaste.generation = root.runGeneration
    primaryPaste.running = true
  }

  function isLive(generation) {
    return generation === root.runGeneration
  }

  function isSelection(text) {
    return typeof text === "string" && text.replace(/^\s+|\s+$/g, "").length > 0
  }

  function onPrimaryCaptured(text, generation) {
    if (!root.isLive(generation)) return
    if (root.isSelection(text)) root.runCheck(text)
    else {
      savedClipboard.generation = generation
      savedClipboard.running = true
    }
  }

  // Step 2 of spec section 3: no primary selection, so borrow the clipboard,
  // send Ctrl + C to the window that still holds focus, and read what lands.
  // The popup window stays hidden until this finishes, so the keystroke
  // reaches the source window rather than the overlay.
  function onClipboardBorrowed(text, generation) {
    if (!root.isLive(generation)) return
    root.borrowedClipboard = typeof text === "string" ? text : ""
    root.clipboardBorrowed = true
    copyKeystroke.generation = generation
    copyKeystroke.running = true
  }

  function onCopyKeystrokeSent(generation) {
    if (!root.isLive(generation)) return
    settleTimer.generation = generation
    settleTimer.restart()
  }

  function onFallbackCaptured(text, generation) {
    if (!root.isLive(generation)) return
    root.restoreBorrowedClipboard()
    if (root.isSelection(text)) root.runCheck(text)
    else root.showNotice("Nothing selected", "Highlight some text, then press SUPER + G.")
  }

  function restoreBorrowedClipboard() {
    if (!root.clipboardBorrowed) return
    root.clipboardBorrowed = false
    var hasText = root.borrowedClipboard.length > 0
    restoreClipboard.text = root.borrowedClipboard
    restoreClipboard.command = hasText ? ["wl-copy"] : ["wl-copy", "--clear"]
    restoreClipboard.stdinEnabled = hasText
    restoreClipboard.running = true
  }

  // ------------------------------------------------------------------ check

  function runCheck(text) {
    root.selectionText = text
    root.phase = "checking"
    checkProcess.generation = root.runGeneration
    checkProcess.stdinText = text
    checkProcess.command = root.checkCommand()
    // Writing to stdin closes it, so every run arms the channel again.
    checkProcess.stdinEnabled = true
    checkProcess.running = true
  }

  function onCheckOutput(text, generation) {
    if (!root.isLive(generation)) return
    var envelope = null
    try {
      envelope = JSON.parse(text)
    } catch (error) {
      envelope = null
    }

    if (!Util.isPlainObject(envelope) || envelope.contractVersion !== 1) {
      root.showNotice("Grammachy could not run the check",
        "The companion tool is missing or out of date. See docs/dev.md for how to put a binary in bin/grammachy.")
      return
    }

    if (Util.isPlainObject(envelope.error)) {
      root.showNotice("The check did not finish", String(envelope.error.message || envelope.error.code || ""))
      return
    }

    var verified = Splice.verifiedIssues(root.selectionText, envelope.issues || [])
    for (var i = 0; i < verified.dropped.length; i++) {
      var dropped = verified.dropped[i]
      console.warn("grammachy: dropped an issue whose span does not match its original:",
        JSON.stringify({ start: dropped.start, end: dropped.end, original: dropped.original }))
    }

    root.issues = verified.issues
    root.decisions = verified.issues.map(function() { return null })
    root.focusIndex = 0
    root.engine = String(envelope.engine || "")
    root.elapsedMs = Number(envelope.elapsedMs || 0)
    root.phase = "result"
  }

  // ---------------------------------------------------------------- review

  function decide(index, value) {
    if (index < 0 || index >= root.issues.length) return
    var next = root.decisions.slice()
    next[index] = value
    root.decisions = next
    root.focusIndex = root.nextOpen(index)
    // The Corrected text just changed, so the last copy is stale.
    root.copied = false
  }

  function nextOpen(from) {
    for (var i = from + 1; i < root.decisions.length; i++) if (root.decisions[i] === null) return i
    for (var j = 0; j < root.decisions.length; j++) if (root.decisions[j] === null) return j
    return Math.min(from, Math.max(0, root.decisions.length - 1))
  }

  function acceptAllOpen() {
    root.decisions = root.decisions.map(function(value) { return value === null ? true : value })
    root.copied = false
  }

  function copyCorrected() {
    copyProcess.text = Splice.correctedText(root.selectionText, root.issues, root.decisions)
    copyProcess.stdinEnabled = true
    copyProcess.running = true
    root.copied = true
  }

  // --------------------------------------------------------------- processes

  Process {
    id: primaryPaste
    property int generation: 0
    property int startedGeneration: 0
    // Snapshot at start.
    command: ["wl-paste", "--primary", "--no-newline"]
    onStarted: primaryPaste.startedGeneration = root.runGeneration
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onPrimaryCaptured(text, primaryPaste.startedGeneration)
    }
  }

  Process {
    id: savedClipboard
    property int generation: 0
    property int startedGeneration: 0
    // Snapshot at start.
    command: ["wl-paste", "--no-newline"]
    onStarted: savedClipboard.startedGeneration = root.runGeneration
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onClipboardBorrowed(text, savedClipboard.startedGeneration)
    }
  }

  Process {
    id: copyKeystroke
    property int generation: 0
    property int startedGeneration: 0
    // Snapshot at start.
    command: ["wtype", "-M", "ctrl", "c", "-m", "ctrl"]
    onStarted: copyKeystroke.startedGeneration = root.runGeneration
    onExited: root.onCopyKeystrokeSent(copyKeystroke.startedGeneration)
  }

  Timer {
    id: settleTimer
    property int generation: 0
    interval: 150
    repeat: false
    onTriggered: {
      if (!root.isLive(settleTimer.generation)) return
      fallbackPaste.generation = settleTimer.generation
      fallbackPaste.running = true
    }
  }

  Process {
    id: fallbackPaste
    property int generation: 0
    property int startedGeneration: 0
    // Snapshot at start.
    command: ["wl-paste", "--no-newline"]
    onStarted: fallbackPaste.startedGeneration = root.runGeneration
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onFallbackCaptured(text, fallbackPaste.startedGeneration)
    }
  }

  // The clipboard was borrowed, not taken. Put it back exactly as it was.
  // restoreBorrowedClipboard() sets the command and arms stdin.
  Process {
    id: restoreClipboard
    property string text: ""
    onStarted: {
      if (!restoreClipboard.stdinEnabled) return
      write(restoreClipboard.text)
      restoreClipboard.stdinEnabled = false
    }
  }

  Process {
    id: checkProcess
    property int generation: 0
    property int startedGeneration: 0
    property string stdinText: ""
    onStarted: {
      checkProcess.startedGeneration = root.runGeneration
      write(checkProcess.stdinText)
      // Close stdin.
      checkProcess.stdinEnabled = false
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onCheckOutput(text, checkProcess.startedGeneration)
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.length > 0) console.warn("grammachy check:", text)
    }
  }

  Process {
    id: copyProcess
    property string text: ""
    command: ["wl-copy"]
    onStarted: {
      write(copyProcess.text)
      copyProcess.stdinEnabled = false
    }
  }

  // ----------------------------------------------------------------- window

  PanelWindow {
    id: panel

    // Nothing shows while the Selection is still being captured: a visible
    // overlay would take the keyboard focus that the Ctrl + C fallback needs
    // to land on the source window.
    visible: root.opened && root.phase !== "capturing"
    anchors { top: true; bottom: true; left: true; right: true }
    color: "transparent"
    WlrLayershell.namespace: "grammachy-quick"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
    exclusionMode: ExclusionMode.Ignore

    MouseArea {
      anchors.fill: parent
      onClicked: root.close()
    }

    Item {
      id: keyCatcher

      anchors.fill: parent
      focus: true

      // A layer-shell surface takes exclusive keyboard focus, so the item that
      // handles Esc has to hold the focus inside it.
      Connections {
        target: panel
        function onVisibleChanged() {
          if (panel.visible) Qt.callLater(function() { keyCatcher.forceActiveFocus() })
        }
      }

      // The full key map is its own ticket. Esc is here because a summoned
      // overlay that cannot be dismissed from the keyboard is a trap.
      Keys.onEscapePressed: function(event) {
        root.close()
        event.accepted = true
      }

      QuickCard {
        id: card

        // The bar widget sits on the trailing edge, so the card hangs from the
        // same corner, under the bar. The overlay cannot see the widget's own
        // position; the bar's edge and size are what it does know.
        x: root.barPosition === "left" ? root.gap + root.barSize
          : parent.width - card.width - root.gap - (root.barPosition === "right" ? root.barSize : 0)
        y: root.barPosition === "bottom" ? parent.height - card.height - root.gap - root.barSize
          : root.gap + (root.barPosition === "top" ? root.barSize : 0)

        cardWidth: Math.min(Style.space(680), parent.width - root.gap * 2)
        // The text region is what gives, so the card never grows past the
        // screen. The 220 is the room the hero, the inspector, and the footer
        // take around it.
        maxTextHeight: Math.max(Style.space(120),
          Math.min(Style.space(360), parent.height - root.barSize - root.gap * 2 - Style.space(220)))

        phase: root.phase
        sourceText: root.selectionText
        issues: root.issues
        decisions: root.decisions
        focusIndex: root.focusIndex
        engine: root.engine
        elapsedMs: root.elapsedMs
        copied: root.copied
        noticeTitle: root.noticeTitle
        noticeBody: root.noticeBody

        onAccepted: function(index) { root.decide(index, true) }
        onSkipped: function(index) { root.decide(index, false) }
        onAcceptAllRequested: root.acceptAllOpen()
        onCopyRequested: root.copyCorrected()
        onFocusRequested: function(index) { root.focusIndex = index }
        onCloseRequested: root.close()
      }
    }
  }
}
