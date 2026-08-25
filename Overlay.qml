import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import "ui"
import "ui/settings.js" as Settings
import "ui/keymap.js" as Keymap
import "ui/splice.js" as Splice

// The overlay entry point. `open(payload)` routes a summon to a surface, spec
// section 2. Quick mode captures the Selection (section 3), runs one Check
// through the companion CLI (section 5.1), and shows the marked text with the
// key map, the Apply path, and the too-long card of section 6.
//
// The gear flips the same card to the Settings view of spec section 7. This
// file is the only thing that touches storage: it reads the plugin's entry in
// shell.json reactively and writes one key at a time through
// `shell.updateEntryInline`.
//
// Compose mode and the remaining error cards are their own tickets; this file
// shows a plain notice card where they will land.
Item {
  id: root

  // Handed over by the shell's panel loader.
  property var shell: null
  property var manifest: null

  // The shell reads `opened` to answer isPluginOpen, and calls close().
  property bool opened: false

  // "capturing", "checking", "result", "notice", or "toolong".
  property string phase: "capturing"
  // The whole capture, spec section 3. Every Check runs on this or on its head.
  property string capturedText: ""
  // The exact text the last Check ran on. Every Issue span indexes into it.
  property string selectionText: ""
  // The last Check took the head of the capture, not all of it.
  property bool truncated: false
  property var issues: []
  property var decisions: []
  property int focusIndex: 0
  property string engine: ""
  property int elapsedMs: 0
  // Apply has run on the Corrected text as it stands.
  property bool applied: false
  property string noticeTitle: ""
  property string noticeBody: ""
  property bool settingsOpen: false
  property string engineMessage: ""

  // One Check takes this many UTF-16 code units. This is `MAX_UTF16_UNITS` of
  // `cli/src/check.rs`, which `cli/tests/overlay_limit.rs` keeps in step.
  readonly property int checkLimitUnits: 5000

  // The clipboard the Ctrl + C fallback borrowed, put back once the Selection
  // is in hand. Spec section 3.
  property string borrowedClipboard: ""
  property bool clipboardBorrowed: false
  property bool pendingPrimaryPaste: false
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
  // Storage is this plugin's inline entry in shell.json, spec section 7. There
  // is no own config file. `shell.shellConfig` is reassigned on every write the
  // shell makes, so `entry` is a live binding: a change from the Settings view,
  // from `omarchy-shell shell setBarWidget`, or from a hand edit of the file
  // all reach the view the same way.
  //
  // The rules of section 7 live in ui/settings.js so that node can test them
  // and so that `cli/src/settings.rs` has one shell-side counterpart.
  readonly property var entry: Settings.entryOf(root.shell ? root.shell.shellConfig : null, root.pluginId)

  // The one settings seam. `fallback` defaults to the spec section 7 default
  // of that key, and an unknown stored value reads as it without a rewrite.
  function setting(name, fallback) {
    return Settings.valueOf(root.entry, name, fallback)
  }

  // Persist on change, spec section 7: no Save button, and the Issues on
  // screen stay because nothing here touches the Check.
  function persistSetting(name, value) {
    if (!root.shell || typeof root.shell.updateEntryInline !== "function") {
      console.warn("grammachy: no shell to keep the setting in:", name)
      return
    }
    root.shell.updateEntryInline(root.pluginId, Settings.mergedEntry(root.entry, name, value))
    // A Settings write is the stored value. Drop a hero override so the two
    // controls do not disagree after the file catches up.
    if (name === "autoReplace") root.autoReplaceOverride = null
  }

  // The hero toggle of spec section 6 shows `autoReplace` and flips it for the
  // rest of the session. Writing it back to shell.json belongs to the Settings
  // view, so the override sits beside the stored value rather than over it.
  readonly property bool storedAutoReplace: root.setting("autoReplace") === true
  property var autoReplaceOverride: null
  readonly property bool autoReplace: root.autoReplaceOverride === null
    ? root.storedAutoReplace : root.autoReplaceOverride === true

  function toggleAutoReplace() {
    root.autoReplaceOverride = !root.autoReplace
    // The Apply button just changed what it does, so a done state from the
    // other mode would read as a promise the button never made.
    root.applied = false
  }

  function checkCommand() {
    var command = [root.binaryPath, "check"]
    var nativeLanguage = root.setting("nativeLanguage")
    if (nativeLanguage !== "none") command.push("--native", nativeLanguage)
    command.push("--engine", root.setting("engine"))
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
      root.showCompose()
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

  // `Open in Compose` is on the too-long card already, because that card makes
  // no sense without it. The window it opens is its own ticket.
  function showCompose() {
    root.showNotice("Compose is not ready yet",
      "The Compose window arrives in a later milestone. Check the first part of the selection instead.")
  }

  // ---------------------------------------------------------------- capture

  function startQuick() {
    settleTimer.stop()
    pasteTimer.stop()
    primaryPaste.running = false
    savedClipboard.running = false
    copyKeystroke.running = false
    fallbackPaste.running = false
    checkProcess.launchPending = false
    checkProcess.restartQueued = false
    checkProcess.running = false
    copyProcess.pasteAfter = false
    // New capture.
    root.runGeneration += 1
    // Reset state. The check view is what a summon shows, spec section 7.
    root.settingsOpen = false
    root.phase = "capturing"
    root.capturedText = ""
    root.selectionText = ""
    root.truncated = false
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
    root.engine = ""
    root.elapsedMs = 0
    root.applied = false
    root.engineMessage = ""
    // End the last borrow.
    if (root.clipboardBorrowed && !restoreClipboard.running)
      root.restoreBorrowedClipboard()
    root.borrowedClipboard = ""
    root.clipboardBorrowed = false
    root.beginPrimaryPaste()
  }

  function beginPrimaryPaste() {
    if (restoreClipboard.running) {
      root.pendingPrimaryPaste = true
      return
    }
    root.pendingPrimaryPaste = false
    primaryPaste.generation = root.runGeneration
    primaryPaste.running = true
  }

  function isLive(generation) {
    return generation === root.runGeneration
  }

  function isSelection(text) {
    return typeof text === "string" && text.replace(/^\s+|\s+$/g, "").length > 0
  }

  function captured(text) {
    root.capturedText = text
    root.truncated = false
    root.runCheck(text)
  }

  function onPrimaryCaptured(text, generation) {
    if (!root.isLive(generation)) return
    if (root.isSelection(text)) root.captured(text)
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
    if (root.isSelection(text)) root.captured(text)
    else root.showEmptySelection()
  }

  function showEmptySelection() {
    root.showNotice("Nothing selected", "Highlight some text, then press SUPER + G.")
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
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
    root.applied = false
    root.engineMessage = ""
    root.phase = "checking"
    checkProcess.generation = root.runGeneration
    checkProcess.stdinText = text
    checkProcess.command = root.checkCommand()
    // Writing to stdin closes it, so every run arms the channel again.
    checkProcess.stdinEnabled = true
    checkProcess.restartQueued = checkProcess.running
    checkProcess.launchPending = true
    checkProcess.running = true
  }

  // The `Check the first N only` button of the too-long card, spec section 8.
  function checkFirstUnits() {
    root.truncated = true
    root.runCheck(Splice.firstUnits(root.capturedText, root.checkLimitUnits))
  }

  function finishCheckLaunch() {
    if (checkProcess.running) return
    if (checkProcess.restartQueued) return
    if (!checkProcess.launchPending) return
    if (root.phase !== "checking") return
    checkProcess.launchPending = false
    root.showBinaryMissing()
  }

  function showBinaryMissing() {
    root.showNotice("Grammachy could not run the check",
      "The companion tool is missing or out of date. See docs/dev.md for how to put a binary in bin/grammachy.")
  }

  // Spec section 8 gives each error code its own card. `text_too_long` and
  // `empty_selection` land here; the rest keep the plain notice until the
  // error cards ticket dresses them.
  function showError(code, message) {
    root.engineMessage = message
    if (code === "text_too_long") {
      root.phase = "toolong"
      return
    }
    if (code === "empty_selection") {
      root.showEmptySelection()
      return
    }
    root.showNotice("The check did not finish", message)
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
      root.showBinaryMissing()
      return
    }

    if (Util.isPlainObject(envelope.error)) {
      root.showError(String(envelope.error.code || ""), String(envelope.error.message || ""))
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
    // The Corrected text just changed, so the last Apply is stale.
    root.applied = false
  }

  function nextOpen(from) {
    for (var i = from + 1; i < root.decisions.length; i++) if (root.decisions[i] === null) return i
    for (var j = 0; j < root.decisions.length; j++) if (root.decisions[j] === null) return j
    return Math.min(from, Math.max(0, root.decisions.length - 1))
  }

  // Up and Down walk every Issue, decided or not, and wrap at both ends.
  function moveFocus(step) {
    var count = root.issues.length
    if (count === 0) return
    root.focusIndex = ((root.focusIndex + step) % count + count) % count
  }

  function acceptAllOpen() {
    root.decisions = root.decisions.map(function(value) { return value === null ? true : value })
    root.applied = false
  }

  // ------------------------------------------------------------------ apply
  //
  // Spec section 6. `autoReplace` off copies the Corrected text and stops.
  // `autoReplace` on copies it, closes the popup so the focus goes back to the
  // source window, and only then pastes over the Selection that is still
  // highlighted there. The Corrected text stays in the clipboard either way.

  function correctedText() {
    return Splice.correctedText(root.selectionText, root.issues, root.decisions)
  }

  function canApply() {
    if (root.phase !== "result" || root.issues.length === 0) return false
    if (root.applied) return false
    for (var i = 0; i < root.decisions.length; i++) if (root.decisions[i] === true) return true
    return false
  }

  function copyCorrected() {
    if (!root.canApply()) return
    root.runCopy(false)
  }

  function applyCorrected() {
    if (!root.canApply()) return
    root.runCopy(root.autoReplace)
  }

  function runCopy(pasteAfter) {
    copyProcess.pasteAfter = pasteAfter
    copyProcess.text = root.correctedText()
    copyProcess.stdinEnabled = true
    copyProcess.running = true
    root.applied = true
  }

  // ------------------------------------------------------------------- keys
  //
  // The Qt codes the key map compares against. `ui/keymap.js` holds the map
  // itself, because a node test can run that and cannot run this.
  readonly property var keyCodes: ({
    escape: Qt.Key_Escape,
    returnKey: Qt.Key_Return,
    enter: Qt.Key_Enter,
    space: Qt.Key_Space,
    up: Qt.Key_Up,
    down: Qt.Key_Down,
    a: Qt.Key_A,
    c: Qt.Key_C,
    control: Qt.ControlModifier,
    shift: Qt.ShiftModifier,
    alt: Qt.AltModifier,
    meta: Qt.MetaModifier
  })

  function handleKey(event) {
    // Settings owns its own fields, so review keys stay off while it is open.
    var reviewing = !root.settingsOpen && root.phase === "result" && root.issues.length > 0
    var action = Keymap.action(event, root.keyCodes, reviewing)
    if (action === Keymap.NONE) return

    if (action === Keymap.CLOSE) root.close()
    else if (action === Keymap.ACCEPT) root.decide(root.focusIndex, true)
    else if (action === Keymap.SKIP) root.decide(root.focusIndex, false)
    else if (action === Keymap.FOCUS_PREVIOUS) root.moveFocus(-1)
    else if (action === Keymap.FOCUS_NEXT) root.moveFocus(1)
    else if (action === Keymap.ACCEPT_ALL) root.acceptAllOpen()
    else if (action === Keymap.COPY) root.copyCorrected()
    else if (action === Keymap.APPLY) root.applyCorrected()

    event.accepted = true
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
    onExited: {
      if (!root.pendingPrimaryPaste) return
      root.beginPrimaryPaste()
    }
  }

  Process {
    id: checkProcess
    // Check launch.
    property int generation: 0
    property int startedGeneration: 0
    property string stdinText: ""
    property bool launchPending: false
    property bool restartQueued: false
    // Start hook.
    onStarted: {
      checkProcess.launchPending = false
      checkProcess.restartQueued = false
      checkProcess.startedGeneration = root.runGeneration
      write(checkProcess.stdinText)
      // Close stdin.
      checkProcess.stdinEnabled = false
    }

    onRunningChanged: {
      if (checkProcess.running) return
      if (checkProcess.restartQueued) {
        checkProcess.restartQueued = false
        return
      }
      if (!checkProcess.launchPending) return
      if (root.phase !== "checking") return
      Qt.callLater(root.finishCheckLaunch)
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
    // Replace the Selection once the clipboard holds the Corrected text.
    property bool pasteAfter: false
    command: ["wl-copy"]
    onStarted: {
      write(copyProcess.text)
      copyProcess.stdinEnabled = false
    }
    onExited: {
      if (!copyProcess.pasteAfter) return
      copyProcess.pasteAfter = false
      // wl-copy has claimed the selection by now, so the paste will find it.
      root.close()
      pasteTimer.restart()
    }
  }

  // The compositor needs a moment to give the keyboard back to the source
  // window after the layer-shell surface goes away. The same 150 ms the
  // Ctrl + C capture waits, for the same reason.
  Timer {
    id: pasteTimer
    interval: 150
    repeat: false
    onTriggered: pasteKeystroke.running = true
  }

  Process {
    id: pasteKeystroke
    command: ["wtype", "-M", "ctrl", "v", "-m", "ctrl"]
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
      // handles the key map has to hold the focus inside it.
      Connections {
        target: panel
        function onVisibleChanged() {
          if (panel.visible) Qt.callLater(function() { keyCatcher.forceActiveFocus() })
        }
      }

      Keys.onPressed: function(event) { root.handleKey(event) }

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
        maxCardHeight: parent.height - root.barSize - root.gap * 2

        phase: root.phase
        sourceText: root.selectionText
        fullText: root.capturedText
        truncated: root.truncated
        limitUnits: root.checkLimitUnits
        issues: root.issues
        decisions: root.decisions
        focusIndex: root.focusIndex
        engine: root.engine
        elapsedMs: root.elapsedMs
        applied: root.applied
        autoReplace: root.autoReplace
        noticeTitle: root.noticeTitle
        noticeBody: root.noticeBody
        engineMessage: root.engineMessage

        settingsOpen: root.settingsOpen
        nativeLanguage: root.setting("nativeLanguage")
        engineSetting: root.setting("engine")
        openaiBaseUrl: root.setting("openaiBaseUrl")
        openaiModel: root.setting("openaiModel")

        onSettingsToggled: root.settingsOpen = !root.settingsOpen
        onSettingChanged: function(name, value) { root.persistSetting(name, value) }
        onAccepted: function(index) { root.decide(index, true) }
        onSkipped: function(index) { root.decide(index, false) }
        onAcceptAllRequested: root.acceptAllOpen()
        onApplyRequested: root.applyCorrected()
        onAutoReplaceToggled: root.toggleAutoReplace()
        onFocusRequested: function(index) { root.focusIndex = index }
        onCheckFirstRequested: root.checkFirstUnits()
        onComposeRequested: root.showCompose()
        onCloseRequested: root.close()
      }
    }
  }
}
