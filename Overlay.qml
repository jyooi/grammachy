import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import "ui"
import "ui/errors.js" as Errors
import "ui/settings.js" as Settings
import "ui/keymap.js" as Keymap
import "ui/splice.js" as Splice
import "ui/format.js" as Format

// The overlay entry point. `open(payload)` routes a summon to a surface, spec
// section 2. Quick mode captures the Selection (section 3), runs one Check
// through the companion CLI (section 5.1), and shows the marked text with the
// key map, the Apply path, and the too-long card of section 6. Compose mode
// (section 9) captures nothing: it holds a Draft, checks it on demand, and
// reviews the answer over the same hero, inspector, footer, and keys.
//
// Both surfaces share one Check, one review state, and one key map, so what
// differs between them is `surface` and the card that draws it.
//
// The gear flips either card to the Settings view of spec section 7. This
// file is the only thing that touches storage: it reads the plugin's entry in
// shell.json reactively and writes one key at a time through
// `shell.updateEntryInline`. The Draft is the one thing it keeps in memory and
// never writes anywhere.
//
// A Check that fails on the quick surface shows one of the error cards of
// spec section 8, and this file routes their buttons: Retry re-runs the Check
// on the same Selection with no second capture, Settings flips the same card
// to the Settings view, Open Compose opens the Compose surface, and Setup
// still lands on a notice until that card arrives.
Item {
  id: root

  // Handed over by the shell's panel loader.
  property var shell: null
  property var manifest: null

  // The shell reads `opened` to answer isPluginOpen, and calls close().
  property bool opened: false

  // Which surface of spec section 2 is on screen: "quick" or "compose".
  property string surface: "quick"

  // Quick: "capturing", "checking", "result", "error", "notice", or "toolong".
  // Compose: "editing", "checking", "result", or "notice".
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
  property string noticeMeta: ""
  property bool settingsOpen: false
  property string engineMessage: ""

  // The error card on screen, spec section 8: a model from `ui/errors.js`, the
  // `grammachy doctor` line the `engine_unavailable` card shows, and a counter
  // that tells a late doctor answer whether its card is still the one showing.
  property var errorCard: null
  property string errorDiagnosis: ""
  property int cardSerial: 0

  // One Check takes this many UTF-16 code units. This is `MAX_UTF16_UNITS` of
  // `cli/src/check.rs`, which `cli/tests/overlay_limit.rs` keeps in step.
  readonly property int checkLimitUnits: 5000

  // A whole Draft takes this many. This is `MAX_DRAFT_UTF16_UNITS` of
  // `cli/src/chunk.rs`, kept in step by the same test. Spec section 9: over it
  // Compose refuses the Check rather than sending a request that would fail.
  readonly property int draftCapUnits: 50000

  // The Draft of spec section 9. It lives here for as long as the shell runs
  // and goes nowhere else: no file, no clipboard, no setting.
  property string draftText: ""

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

  // The engine an error card names, spec section 8: the display name of the
  // current engine setting, which is the name the Settings dropdown shows.
  function engineLabel() {
    return Settings.labelOf(Settings.ENGINE_OPTIONS, root.setting("engine"))
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

  function showNotice(title, body, meta) {
    root.phase = "notice"
    root.errorCard = null
    root.noticeTitle = title
    root.noticeBody = body
    root.noticeMeta = meta === undefined ? "" : meta
  }

  // Every summon starts from the same clean state: nothing of the last one is
  // still in flight, and no Check is on screen. The check view is what a
  // summon shows, spec section 7, and the Draft is what it never touches.
  function resetRun() {
    settleTimer.stop()
    pasteTimer.stop()
    primaryPaste.running = false
    savedClipboard.running = false
    copyKeystroke.running = false
    fallbackPaste.running = false
    checkProcess.launchPending = false
    checkProcess.restartQueued = false
    checkProcess.running = false
    doctorProcess.restartQueued = false
    doctorProcess.running = false
    copyProcess.pasteAfter = false
    // Whatever answers next belongs to an older run than this one.
    root.runGeneration += 1
    root.settingsOpen = false
    root.selectionText = ""
    root.truncated = false
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
    root.engine = ""
    root.elapsedMs = 0
    root.applied = false
    root.engineMessage = ""
    root.errorCard = null
    root.errorDiagnosis = ""
    // End the last borrow.
    if (root.clipboardBorrowed && !restoreClipboard.running)
      root.restoreBorrowedClipboard()
    root.borrowedClipboard = ""
    root.clipboardBorrowed = false
  }

  // Spec sections 2 and 9: Compose opens on the kept Draft and captures
  // nothing. The `{"mode": "compose", "text": "..."}` payload, which replaces
  // a non-empty Draft after a confirm, arrives with the remaining triggers in
  // their own ticket; this function is the seam they land on.
  function showCompose() {
    root.resetRun()
    root.surface = "compose"
    root.phase = "editing"
    Qt.callLater(root.restoreFocus)
  }

  // `Setup` on the `bad_arguments` card. The setup card of spec section 10
  // names the pinned binary and runs `bin/bootstrap.sh`; the release ticket
  // builds it, so until then the button says where the binary comes from.
  function showSetup() {
    root.showNotice("Setup is not ready yet",
      "The setup card arrives with the release milestone. See docs/dev.md for how to build the companion binary into bin/grammachy.",
      "not ready yet")
  }

  // ---------------------------------------------------------------- capture

  function startQuick() {
    root.resetRun()
    root.surface = "quick"
    root.phase = "capturing"
    root.capturedText = ""
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
    // Capture found nothing, so the CLI would answer `empty_selection` on an
    // empty stdin. Showing that card here saves the round trip.
    else root.showError(Errors.EMPTY_SELECTION, "")
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
    root.errorCard = null
    root.errorDiagnosis = ""
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

  // ---------------------------------------------------------------- compose
  //
  // Spec section 9. The Draft is edited here, checked only when the reader
  // asks, and reviewed with the same Issues and the same decisions as the
  // popup. `Back to edit` is the one path that writes the Corrected text back.

  function editDraft(text) {
    root.draftText = text
  }

  function clearDraft() {
    root.draftText = ""
  }

  // Why Compose will not check this Draft, or "" when it will. The Check
  // button reads the same rule, so the two can never disagree.
  function draftRefusal() {
    return Format.draftRefusal(root.draftText.length, root.checkLimitUnits, root.draftCapUnits)
  }

  // One Check on the whole Draft, which is what fits while a Draft is one
  // Chunk. Chunked checking replaces this body and nothing around it.
  function startComposeCheck() {
    if (root.surface !== "compose" || root.phase !== "editing") return
    if (root.draftRefusal().length > 0) return
    // A second Check must not be answered by the first one's output.
    root.runGeneration += 1
    root.runCheck(root.draftText)
  }

  function backToEdit() {
    if (root.surface !== "compose" || root.phase === "editing") return
    // Spec section 9: what the reader accepted becomes the Draft they go back
    // to. A Check that never reached a result leaves the Draft as it was.
    if (root.phase === "result") root.draftText = root.correctedText()
    // A Check still in flight answers into a card that has moved on.
    root.runGeneration += 1
    root.phase = "editing"
    root.selectionText = ""
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
    root.applied = false
    root.engineMessage = ""
    Qt.callLater(root.restoreFocus)
  }

  function finishCheckLaunch() {
    if (checkProcess.running) return
    if (checkProcess.restartQueued) return
    if (!checkProcess.launchPending) return
    if (root.phase !== "checking") return
    checkProcess.launchPending = false
    root.showBinaryMissing()
  }

  // The binary never started, so there is no stdout at all. Spec section 8
  // puts that on the same card as no JSON on stdout.
  function showBinaryMissing() {
    root.showError(Errors.BAD_ARGUMENTS, "")
  }

  // Spec section 8 gives each error code its own card. `text_too_long` keeps
  // the card of section 6, which this popup owns; every other code gets its
  // card from `ui/errors.js`.
  function showError(code, message) {
    root.engineMessage = message
    // Both cards of section 8 are about a Selection, so Compose keeps the
    // plain notice: it has no Selection to size and none to ask for.
    if (root.surface === "compose") {
      root.showNotice("The check did not finish", "The engine reported an error.")
      return
    }
    var settled = Errors.known(code)
    if (settled === Errors.TEXT_TOO_LONG) {
      root.errorCard = null
      root.phase = "toolong"
      return
    }

    root.cardSerial += 1
    root.errorDiagnosis = ""
    root.errorCard = Errors.card(settled, {
      engineLabel: root.engineLabel(),
      engineSlug: root.setting("engine"),
      message: message
    })
    root.phase = "error"
    if (root.errorCard.needsDiagnosis) root.runDoctor()
  }

  // Spec section 8: the `engine_unavailable` card shows the one-line diagnosis
  // that `grammachy doctor` gives for the engine the setting names.
  function runDoctor() {
    doctorProcess.command = [root.binaryPath, "doctor", "--engine", root.setting("engine"), "--json"]
    if (doctorProcess.running) {
      doctorProcess.restartQueued = true
      doctorProcess.running = false
      return
    }
    doctorProcess.running = true
  }

  function onDoctorOutput(text, serial) {
    if (serial !== root.cardSerial) return
    var report = null
    try {
      report = JSON.parse(text)
    } catch (error) {
      report = null
    }
    // A doctor that cannot answer leaves the card as it is. The body already
    // says the engine is not running, which is the part that matters.
    if (!Util.isPlainObject(report) || report.contractVersion !== 1) return
    root.errorDiagnosis = typeof report.diagnosis === "string" ? report.diagnosis : ""
  }

  // Spec section 8: Retry re-runs the Check with the same Selection and no
  // re-capture, so a selection that changed in the source window since the
  // failure cannot reach the engine.
  function retryCheck() {
    if (root.selectionText.length === 0) return
    root.runCheck(root.selectionText)
  }

  // Where each button of an error card goes, spec section 8.
  function runErrorAction(action) {
    if (action === Errors.CLOSE) root.close()
    else if (action === Errors.RETRY) root.retryCheck()
    // Settings opens the Settings view of the same card, so the error card is
    // still behind it when the user comes back.
    else if (action === Errors.SETTINGS) root.settingsOpen = true
    else if (action === Errors.SETUP) root.showSetup()
    else if (action === Errors.COMPOSE) root.showCompose()
  }

  function onCheckOutput(text, generation) {
    if (!root.isLive(generation)) return

    var answer = Errors.readCheck(text)
    if (answer.error) {
      root.showError(answer.error.code, answer.error.message)
      return
    }

    var envelope = answer.result
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
    // Spec section 9: auto-replace never applies in Compose, because the Draft
    // came from this card rather than from a window still holding a Selection.
    root.runCopy(root.surface === "quick" && root.autoReplace)
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

  // Which card the press landed on, spec sections 6 and 9. Settings owns its
  // own fields, so every card key stays off while it is open.
  function keyMode() {
    if (root.settingsOpen) return Keymap.MODE_IDLE
    if (root.surface === "compose") {
      if (root.phase === "editing") return Keymap.MODE_COMPOSE_EDIT
      // A Check in flight has no Issues to decide and no Draft to go back to
      // yet, so Esc leaves the way it does everywhere else. The Draft stays.
      if (root.phase === "checking") return Keymap.MODE_IDLE
      return Keymap.MODE_COMPOSE_REVIEW
    }
    if (root.phase === "result" && root.issues.length > 0) return Keymap.MODE_REVIEW
    return Keymap.MODE_IDLE
  }

  function handleKey(event) {
    var action = Keymap.action(event, root.keyCodes, root.keyMode())
    if (action === Keymap.NONE) return

    if (action === Keymap.CLOSE) root.close()
    else if (action === Keymap.CHECK) root.startComposeCheck()
    else if (action === Keymap.BACK) root.backToEdit()
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

  // The one-line engine diagnosis of the `engine_unavailable` card. It runs
  // after the failed Check, so a slow doctor never delays the card itself.
  Process {
    id: doctorProcess
    property int startedSerial: 0
    property bool restartQueued: false
    // Snapshot at start.
    onStarted: doctorProcess.startedSerial = root.cardSerial
    // A second card while the first doctor is still out may be about another
    // engine, so the answer has to come from a run that started after the card.
    onRunningChanged: {
      if (doctorProcess.running) return
      if (!doctorProcess.restartQueued) return
      doctorProcess.restartQueued = false
      doctorProcess.running = true
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onDoctorOutput(text, doctorProcess.startedSerial)
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.length > 0) console.warn("grammachy doctor:", text)
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
    WlrLayershell.namespace: "grammachy"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
    exclusionMode: ExclusionMode.Ignore

    // Spec section 9: Compose is a centred card over a dimmed backdrop. The
    // popup hangs off the bar instead and leaves the desktop as it is.
    Rectangle {
      anchors.fill: parent
      color: "black"
      opacity: root.surface === "compose" ? 0.45 : 0

      Behavior on opacity {
        NumberAnimation { duration: 120; easing.type: Easing.OutCubic }
      }
    }

    // A click that reaches this far is a click outside the card, which closes
    // either surface.
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
          if (panel.visible) Qt.callLater(root.restoreFocus)
        }
      }

      Keys.onPressed: function(event) { root.handleKey(event) }

      QuickCard {
        id: card

        visible: root.surface === "quick"

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
        noticeMeta: root.noticeMeta
        engineMessage: root.engineMessage
        errorCard: root.errorCard
        diagnosis: root.errorDiagnosis

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
        onErrorActionRequested: function(action) { root.runErrorAction(action) }
        onCloseRequested: root.close()
      }

      ComposeCard {
        id: composeCard

        anchors.centerIn: parent
        visible: root.surface === "compose"

        // Spec section 9: about 900 px wide and 80 percent of the screen high.
        // The screen is what bounds both, so a small display still fits.
        cardWidth: Math.min(Style.space(900), parent.width - root.gap * 2)
        cardHeight: Math.min(Math.round(parent.height * 0.8), parent.height - root.gap * 2)

        // The Draft text area holds the keyboard while it is being written, so
        // it forwards to the same item the key map runs on.
        keySink: keyCatcher

        phase: root.phase
        draftText: root.draftText
        sourceText: root.selectionText
        issues: root.issues
        decisions: root.decisions
        focusIndex: root.focusIndex
        engine: root.engine
        elapsedMs: root.elapsedMs
        applied: root.applied
        noticeTitle: root.noticeTitle
        noticeBody: root.noticeBody
        engineMessage: root.engineMessage
        checkLimitUnits: root.checkLimitUnits
        draftCapUnits: root.draftCapUnits

        settingsOpen: root.settingsOpen
        nativeLanguage: root.setting("nativeLanguage")
        engineSetting: root.setting("engine")
        autoReplace: root.autoReplace
        openaiBaseUrl: root.setting("openaiBaseUrl")
        openaiModel: root.setting("openaiModel")

        onSettingsToggled: root.settingsOpen = !root.settingsOpen
        onSettingChanged: function(name, value) { root.persistSetting(name, value) }
        onDraftEdited: function(text) { root.editDraft(text) }
        onClearRequested: root.clearDraft()
        onCheckRequested: root.startComposeCheck()
        onBackToEditRequested: root.backToEdit()
        onAccepted: function(index) { root.decide(index, true) }
        onSkipped: function(index) { root.decide(index, false) }
        onAcceptAllRequested: root.acceptAllOpen()
        onApplyRequested: root.applyCorrected()
        onFocusRequested: function(index) { root.focusIndex = index }
        onCloseRequested: root.close()
      }
    }
  }

  // The Draft text area takes the keyboard in Compose edit mode, and the key
  // map takes it everywhere else. Both live inside the panel, so this is the
  // one place that decides which of them holds it.
  function restoreFocus() {
    if (!panel.visible) return
    if (root.surface === "compose") composeCard.takeFocus()
    else keyCatcher.forceActiveFocus()
  }
}
