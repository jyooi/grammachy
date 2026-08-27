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
import "ui/anchor.js" as Anchor
import "ui/capture.js" as Capture
import "ui/limits.js" as Limits
import "ui/engines.js" as EnginesJs
import "ui/setupCard.js" as Setup

// The overlay entry point. `open(payload)` routes a summon to a surface, spec
// section 2. Quick mode captures the Selection (section 3), runs one Check
// through the companion CLI (section 5.1), and shows the marked text with the
// key map, the Apply path, and the too-long card of section 6. Compose mode
// (section 9) captures nothing: it holds a Draft, checks it in Chunks on
// demand, and reviews the answer over the same hero, inspector, footer, and
// keys.
//
// A Draft of any size under the cap is one `grammachy chunk` followed by one
// `grammachy check` per Chunk in sequence, each Chunk's spans moved by its own
// start before they merge into one list. Cancel stops the run after the Chunk
// in flight and a failed Chunk keeps what the finished ones found, so what the
// engine already answered is never thrown away.
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
//
// Capture also records which window the Selection came from, and that one
// fact answers two questions the popup used to guess at: where the card opens,
// and where Replace types. `ui/anchor.js` owns both answers.
Item {
  id: root

  // Handed over by the shell's panel loader.
  property var shell: null
  property var manifest: null

  // The shell reads `opened` to answer isPluginOpen, and calls close().
  property bool opened: false

  // Which surface of spec section 2 is on screen: "quick" or "compose".
  property string surface: "quick"

  // Quick: "capturing", "empty", "checking", "result", "error", "notice",
  // "setup", or "toolong".
  // Compose: "editing", "confirm", "checking", "result", "error", or "notice".
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
  property string noticeMeta: ""
  // The body a notice was raised with. A notice that ends by naming the Engine
  // keeps only the words before the name here, because the name is read at the
  // moment the card is drawn.
  property string noticeBodyText: ""
  property bool noticeNamesEngine: false
  readonly property string noticeBody: root.noticeNamesEngine
    ? root.noticeBodyText + root.engineLabel(root.setting("engine")) + "."
    : root.noticeBodyText
  property bool settingsOpen: false
  property string engineMessage: ""

  // The error card on screen, spec section 8: a model from `ui/errors.js`, the
  // `grammachy doctor` line the `engine_unavailable` card shows, and a counter
  // that tells a late doctor answer whether its card is still the one showing.
  property var errorCard: null
  property string errorDiagnosis: ""
  property int cardSerial: 0

  // The setup card of spec section 10: `bootstrapRunning` and
  // `bootstrapExitCode` (null until a run has finished) are bin/bootstrap.sh's
  // own state, and `bootstrapLog` is its stdout and stderr in the order they
  // streamed in. `setupCardModel` turns those plus cli.lock's text into the
  // model `ui/SetupCard.qml` draws.
  property bool bootstrapRunning: false
  property var bootstrapExitCode: null
  property string bootstrapLog: ""
  readonly property var setupCardModel: Setup.card({
    lockText: cliLockFile.text(),
    running: root.bootstrapRunning,
    exitCode: root.bootstrapExitCode,
    log: root.bootstrapLog
  })

  // One Check takes this many UTF-16 code units. The limit belongs to the
  // Engine (spec section 4), so it moves with the engine setting.
  // `ui/limits.js` is the one place that answers it, and
  // `cli/tests/overlay_limit.rs` keeps that file equal to the CLI.
  readonly property int checkLimitUnits: Limits.checkLimit(root.setting("engine"))

  // A whole Draft takes this many. This is `MAX_DRAFT_UTF16_UNITS` of
  // `cli/src/chunk.rs`, kept in step by the same test. Spec section 9: over it
  // Compose refuses the Check rather than sending a request that would fail.
  readonly property int draftCapUnits: 50000

  // The Draft of spec section 9. It lives here for as long as the shell runs
  // and goes nowhere else: no file, no clipboard, no setting.
  property string draftText: ""

  // The Draft a trigger of spec section 2 wants in place of a non-empty one.
  // It waits here while the confirm card is on screen, because the Draft is the
  // one thing the plugin keeps and nothing else holds a copy of it.
  property string pendingDraft: ""

  // The chunked Check of spec section 9. `chunks` is the tiling `grammachy
  // chunk` answered, `chunkIndex` is the Chunk a Check is running on or the one
  // a failure stopped at, and `chunkRun` says the Check in flight belongs to
  // this loop rather than to the quick popup.
  property var chunks: []
  property int chunkIndex: 0
  property bool chunkRun: false
  property bool chunkCancelled: false
  // The engine slug the Chunk list was packed for. The limit belongs to the
  // Engine (spec section 4), so a Chunk list only fits the size that Engine
  // reads: a Chunk packed for a wider Engine is refused by a narrower one.
  // Every Check of this run names it, so a setting changed mid-run reaches the
  // next run rather than the Chunks already cut.
  property string chunkEngine: ""
  // Engine time from every Chunk that finished, which is what the result line
  // names, the same number the popup's does.
  property int chunkElapsedMs: 0
  // The wall clock of this attempt, which is what the progress line names,
  // because that is the wait the reader is watching.
  property double chunkStartedAt: 0
  property int chunkTickMs: 0

  // The engine the progress line names before any Chunk has answered: the
  // setting, until an envelope says which engine actually ran.
  readonly property string runningEngine: root.engine.length > 0
    ? root.engine : String(root.setting("engine"))

  // The window that held the Selection, read from the compositor at capture
  // time and kept until the next summon: `{ address, x, y, width, height }`
  // in the global layout, or null when nothing was focused. The quick popup
  // opens beside it and Replace types into it, so both stop guessing.
  property var sourceWindow: null

  // The capture the last Check consumed, spec section 3: the exact text and
  // the address of the window it came from. The compositor keeps the primary
  // selection for as long as the source window owns it, so a summon with
  // nothing highlighted reads this text again; `ui/capture.js` is what calls
  // that stale. It outlives a summon, because the summon after the next one
  // reads the same selection.
  property string lastCapturedText: ""
  property string lastCapturedWindow: ""
  // A Replace has closed the popup and has still to type, spec section 6. The
  // source window keeps its highlight until then, so the primary selection is
  // held back until the keystroke is out.
  property bool replacePending: false
  // This run took a Selection from a source window, spec section 3. Compose
  // captures nothing, and a summon that found nothing new took nothing either.
  // Neither of those owns the primary selection, so neither may record a
  // capture or release one when it closes.
  property bool runCaptured: false

  // ------------------------------------------------------------- engines
  //
  // The optional engine components this machine keeps, spec section 5.4. The
  // list lives here rather than in the card because an install runs for
  // minutes and closing the overlay does not cancel it, so `engines` and
  // `engineBusy` are what a second summon comes back to.
  property var engines: []
  property string engineBusy: ""
  property double engineBusyBytes: 0
  property string enginesDirectory: ""
  property double enginesFreeBytes: 0
  property int engineListSerial: 0
  property int engineListFloor: 0
  property var engineNote: null
  // The slug a Remove confirm is waiting on, spec section 7.
  property string engineConfirm: ""
  // The phase the card comes back to once that confirm is answered.
  property string phaseBeforeEngineConfirm: ""

  // The one fact every engine verb and every drawn button asks.
  readonly property bool enginesBusy: engineActionProcess.running
    || root.engineConfirm.length > 0

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

  // The plugin folder's own root, the same one `binaryPath` sits under.
  readonly property string pluginRoot: Util.isPlainObject(root.manifest) && root.manifest.__sourceDir
    ? String(root.manifest.__sourceDir).replace(/\/$/, "")
    : String(Qt.resolvedUrl(".")).replace(/^file:\/\//, "").replace(/\/$/, "")

  // The setup card of spec section 10: what it installs, and what it pins.
  readonly property string bootstrapPath: root.pluginRoot + "/bin/bootstrap.sh"
  readonly property string cliLockPath: root.pluginRoot + "/cli.lock"

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

  // A question that is off the screen must never still be answerable: the
  // confirm is a phase, and a phase answers the keyboard whether or not its
  // card is drawn.
  onSettingsOpenChanged: {
    // The Engines list is drawn whatever engine is selected, because the whole
    // point is to add one the dropdown cannot offer yet (HUF-237). So Settings
    // being open is the whole condition for reading it.
    if (root.settingsOpen) root.refreshEngines()
    // A question that is off the screen must never still be answerable, the
    // rule the Engines confirm keeps.
    else if (root.phase === "confirmEngine") root.closeEngineConfirm()
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

  // The engine an error card names, spec section 8: the display name of one
  // engine slug, which is the name the Settings dropdown shows for it.
  function engineLabel(engineSlug) {
    return Settings.labelOf(Settings.ENGINE_OPTIONS, engineSlug)
  }

  // The Engine a Chunk run belongs to. The list is packed before the first
  // Check, so a run in flight always has one; outside a run the setting is
  // what the next Check will name.
  function runEngine() {
    return root.chunkEngine !== "" ? root.chunkEngine : root.setting("engine")
  }

  function checkCommand(engineSlug) {
    var command = [root.binaryPath, "check"]
    var nativeLanguage = root.setting("nativeLanguage")
    if (nativeLanguage !== "none") command.push("--native", nativeLanguage)
    command.push("--engine", engineSlug)
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
      // Spec section 2: a payload with no `text` opens Compose on the kept
      // Draft, which is what the menu entry and SUPER + SHIFT + G send.
      if (typeof payload.text === "string") root.composeWith(payload.text)
      else root.showCompose()
      return
    }
    root.startQuick()
  }

  function close() {
    // A chunked run launches one Check after another, so a card that is gone
    // must not leave one walking the rest of the Draft.
    root.cancelChunkRun()
    // Spec section 3: a run that captured is over, whether it ended in Apply,
    // Replace, Clear, or Close, so what it captured is recorded and the primary
    // selection it came from is released. A run that captured nothing owns no
    // selection, so it takes none away and records none. `resetRun` leaves
    // `capturedText` in place while it drops the source window, so the text
    // alone cannot answer this. The release holds the same rule of its own.
    if (root.runCaptured)
      root.consumeCapture(root.capturedText, Anchor.windowAddress(root.sourceWindow))
    root.releasePrimary()
    root.opened = false
  }

  function showNotice(title, body, meta) {
    root.phase = "notice"
    root.errorCard = null
    root.noticeTitle = title
    root.noticeBodyText = body
    root.noticeNamesEngine = false
    root.noticeMeta = meta === undefined ? "" : meta
  }

  // A notice whose last words are the Engine name. `body` ends where the name
  // starts, and the binding above finishes the sentence from the setting.
  function showEngineNotice(title, body, meta) {
    root.showNotice(title, body, meta)
    root.noticeNamesEngine = true
  }

  // Every summon starts from the same clean state: nothing of the last one is
  // still in flight, and no Check is on screen. The check view is what a
  // summon shows, spec section 7, and the Draft is what it never touches.
  function resetRun() {
    sourceProbe.launchPending = false
    focusSource.launchPending = false
    verifySource.launchPending = false
    checkProcess.launchPending = false
    chunkProcess.launchPending = false
    copyProcess.pasteAfter = false
    // Whatever answers next belongs to an older run than this one.
    root.runGeneration += 1
    settleTimer.stop()
    pasteTimer.stop()
    chunkTicker.stop()
    sourceProbe.running = false
    focusSource.running = false
    verifySource.running = false
    primaryPaste.running = false
    savedClipboard.running = false
    copyKeystroke.running = false
    fallbackPaste.running = false
    checkProcess.restartQueued = false
    checkProcess.running = false
    chunkProcess.restartQueued = false
    chunkProcess.running = false
    doctorProcess.restartQueued = false
    doctorProcess.running = false
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
    root.pendingDraft = ""
    root.sourceWindow = null
    // Whatever the last Replace was waiting on, this summon is not it.
    root.replacePending = false
    // This summon has captured nothing yet, so it owns no primary selection.
    root.runCaptured = false
    // Spec section 5.4: closing the overlay does not cancel an install, so a
    // summon leaves the process in flight alone. The confirm is a question
    // about a card that is gone, so it goes.
    root.engineConfirm = ""
    root.phaseBeforeEngineConfirm = ""
    root.clearChunkRun()
    // End the last borrow.
    if (root.clipboardBorrowed && !restoreClipboard.running)
      root.restoreBorrowedClipboard()
    root.borrowedClipboard = ""
    root.clipboardBorrowed = false
  }

  // Spec sections 2 and 9: Compose opens on the kept Draft and captures
  // nothing. This is where SUPER + SHIFT + G and the `grammachy.compose` menu
  // entry land, and where `composeWith` lands once the Draft is settled.
  function showCompose() {
    root.resetRun()
    root.surface = "compose"
    root.phase = "editing"
    Qt.callLater(root.restoreFocus)
  }

  // Compose on a text that came from somewhere else, spec section 2: the
  // `Open in Compose` button of the too-long card, the Compose button in the
  // popup header, or a `{"mode": "compose", "text": "..."}` payload.
  //
  // A non-empty Draft is replaced only after a confirm, because it is the one
  // thing the plugin keeps and no file holds a second copy of it. An empty one
  // has nothing to lose, so it takes the new text straight away.
  function composeWith(text) {
    var wanted = typeof text === "string" ? text : ""
    if (wanted.length === 0 || root.draftText.length === 0) {
      root.showCompose()
      if (wanted.length > 0) root.draftText = wanted
      return
    }

    root.resetRun()
    root.surface = "compose"
    root.pendingDraft = wanted
    root.phase = "confirm"
  }

  function replaceDraft() {
    if (root.phase !== "confirm") return
    root.draftText = root.pendingDraft
    root.pendingDraft = ""
    root.phase = "editing"
    Qt.callLater(root.restoreFocus)
  }

  function keepDraft() {
    if (root.phase !== "confirm") return
    root.pendingDraft = ""
    root.phase = "editing"
    Qt.callLater(root.restoreFocus)
  }

  // `Setup` on the `bad_arguments` card, spec section 10: the card names the
  // pinned binary and its hash, and Install runs `bin/bootstrap.sh`. Neither
  // it nor `resetRun` touches the run's own state, so closing and reopening
  // this card mid-install shows the run still going, the same rule an engine
  // install keeps.
  function showSetup() {
    root.phase = "setup"
  }

  function installBootstrap() {
    if (root.bootstrapRunning) return
    root.bootstrapRunning = true
    root.bootstrapExitCode = null
    root.bootstrapLog = ""
    bootstrapProcess.launchPending = true
    bootstrapProcess.running = true
  }

  // `bin/bootstrap.sh` never started at all, the same fact `finishCheckLaunch`
  // reads for the companion binary itself. The setup card is what the reader
  // used to get here, so the failure lands as its own log line rather than as
  // another `bad_arguments` card.
  function finishBootstrapLaunch() {
    if (bootstrapProcess.running) return
    if (!bootstrapProcess.launchPending) return
    bootstrapProcess.launchPending = false
    root.bootstrapRunning = false
    root.bootstrapExitCode = 1
    root.bootstrapLog += "bin/bootstrap.sh could not be started.\n"
  }

  // Where each button of the setup card goes, spec section 10.
  function runSetupAction(action) {
    if (action === Setup.INSTALL) root.installBootstrap()
    else if (action === Setup.RETRY) root.retryCheck()
    else if (action === Setup.CLOSE) root.close()
  }

  // ---------------------------------------------------------------- capture

  // The popup window is still hidden here, so the compositor still calls the
  // window the user selected in the active one. That is the only moment it can
  // be read, which is why the capture waits on the answer rather than racing
  // it: a card that opened first would open at the wrong place and then jump.
  function startQuick() {
    root.resetRun()
    root.surface = "quick"
    root.phase = "capturing"
    root.capturedText = ""
    root.probeSourceWindow()
  }

  function probeSourceWindow() {
    sourceProbe.command = Anchor.activeWindowCommand()
    sourceProbe.launchPending = true
    sourceProbe.running = true
  }

  // With no answer there is no source window, and every step after this one
  // already knows what to do without one: the card takes the bar corner and
  // Replace types wherever the keyboard went.
  function onSourceProbed(text, generation) {
    if (!root.isLive(generation)) return
    root.sourceWindow = Anchor.readActiveWindow(text)
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

  // The record `ui/capture.js` reads: what the last consumed capture held and
  // where it came from.
  function lastCapture() {
    return Capture.kept(root.lastCapturedText, root.lastCapturedWindow)
  }

  // The record of what this run captured, spec section 3. It is what the next
  // summon is measured against, and it is all this does: the primary selection
  // is released later, when the popup closes.
  function consumeCapture(text, address) {
    if (typeof text !== "string" || text.length === 0) return
    var window = typeof address === "string" ? address : ""
    if (Capture.isStale(text, window, root.lastCapture())) return
    root.lastCapturedText = text
    root.lastCapturedWindow = window
  }

  // Spec section 3: the primary selection goes when the popup closes, and not
  // before. A terminal drops its own highlight when it loses primary
  // ownership, and Replace pastes over that highlight, so a release at capture
  // time would take the Selection away from under the Apply the reader is
  // still deciding on.
  //
  // Only a run that took a Selection releases one, and it releases at most
  // once, whichever exit it takes. Both rules live here rather than at each
  // exit, so no call site can hold a different one: the close, the Clear, and
  // the keystroke that ends a Replace all call this plainly.
  //
  // Replace is the one path that outlives the close: it closes the popup, asks
  // for the source window, and only then types. `replacePending` is what makes
  // the release wait for that keystroke, so the claim has to outlive the close
  // that armed the wait. That is why the wait answers before the claim goes.
  function releasePrimary() {
    if (!root.runCaptured) return
    if (root.replacePending) return
    root.runCaptured = false
    clearPrimary.running = true
  }

  function captured(text) {
    var address = Anchor.windowAddress(root.sourceWindow)
    // Spec section 3: the same text from the same window is the selection the
    // last Check already ran on, so nothing is checked again for free.
    if (Capture.isStale(text, address, root.lastCapture())) {
      root.showNothingNew()
      return
    }
    root.capturedText = text
    root.truncated = false
    root.runCaptured = true
    root.consumeCapture(text, address)
    root.runCheck(text)
  }

  // Nothing new to check, spec sections 3 and 6. The popup stays open on the
  // empty state, which offers the kept text rather than a second capture.
  function showNothingNew() {
    root.capturedText = ""
    root.selectionText = ""
    root.truncated = false
    root.errorCard = null
    root.errorDiagnosis = ""
    root.engineMessage = ""
    root.phase = "empty"
  }

  // `Check last text again`, spec section 6: the kept text with no capture at
  // all, so a selection that has since changed cannot reach the engine.
  function checkLastAgain() {
    if (root.lastCapturedText.length === 0) return
    root.capturedText = root.lastCapturedText
    root.truncated = false
    root.runCheck(root.lastCapturedText)
  }

  // `Clear`, spec section 6. The popup stays open on the empty state with
  // nothing of the last Check left on it. The Draft is not touched: it is the
  // one thing the plugin keeps, and no file holds a second copy of it.
  function clearCapture() {
    if (root.surface !== "quick") return
    // A Check still in flight answers into a card that has moved on.
    root.runGeneration += 1
    checkProcess.launchPending = false
    checkProcess.restartQueued = false
    checkProcess.running = false
    doctorProcess.restartQueued = false
    doctorProcess.running = false
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
    root.engine = ""
    root.elapsedMs = 0
    root.applied = false
    root.noticeTitle = ""
    root.noticeBody = ""
    root.noticeMeta = ""
    // A borrow of the clipboard is the reader's clipboard, so it goes back.
    if (root.clipboardBorrowed && !restoreClipboard.running)
      root.restoreBorrowedClipboard()
    root.borrowedClipboard = ""
    root.clipboardBorrowed = false
    root.releasePrimary()
    root.showNothingNew()
    Qt.callLater(root.restoreFocus)
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
    var borrowed = root.borrowedClipboard
    root.restoreBorrowedClipboard()
    // Spec section 3: a field with nothing selected answers Ctrl + C by
    // leaving the clipboard as it was, so what came back is an earlier copy
    // rather than a Selection.
    if (Capture.copiedNothing(borrowed, text)) {
      root.showNothingNew()
      return
    }
    if (root.isSelection(text)) root.captured(text)
    // The capture found nothing at all, which is the same answer for the
    // reader: there is nothing new to check.
    else root.showNothingNew()
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


  // ----------------------------------------------------------------- engines
  //
  // Spec section 5.4. Every verb of `grammachy engine` runs through one of two
  // processes: `engineListProcess` reads the list and is what the poll repeats,
  // and `engineActionProcess` carries the one verb the user asked for. Both
  // answer the same envelope, so one reader serves them.

  function engineCommand(verbArgs) {
    return [root.binaryPath, "engine"].concat(verbArgs)
  }

  // The list as it is right now. It runs when Settings opens and once a second
  // while an install is in flight.
  function refreshEngines() {
    engineListProcess.command = root.engineCommand(["list"])
    if (engineListProcess.running) {
      engineListProcess.restartQueued = true
      return
    }
    engineListProcess.running = true
  }

  function onEngineListOutput(text, stamp) {
    var answer = EnginesJs.read(text)
    if (answer.error) {
      // A poll that failed says nothing new: the rows on screen are still the
      // last true answer, and the note the verb itself left is the one to keep.
      if (root.engines.length === 0)
        root.engineNote = EnginesJs.note(answer.error.code, answer.error.message, "")
      return
    }
    root.absorbEngineReport(answer.report, stamp)
  }

  // The rows of one answer merged into the list. The list is replaced only
  // when it says something new, so the poll does not rebuild the row once a
  // second and restart the bar's animation.
  function absorbEngineReport(report, stamp) {
    var settled = EnginesJs.absorbed(root.engines, report, stamp, root.engineListFloor)
    if (!settled) return
    var next = settled.engines
    root.engineBusyBytes = EnginesJs.partialOf(next, root.engineBusy)
    if (!EnginesJs.sameRows(root.engines, next, root.engineBusy)) root.engines = next
    root.enginesDirectory = settled.directory
    root.enginesFreeBytes = settled.freeBytes
  }

  // Spec section 5.4: one verb at a time, so an Install while a verb is in
  // flight is a no-op rather than a second transfer. The buttons that would
  // reach here are drawn disabled from the same `enginesBusy`, so this guard is
  // what makes the drawing true rather than a second opinion.
  function installEngine(slug) {
    if (root.enginesBusy) return
    root.engineNote = null
    root.engineBusy = slug
    // The running row's bar reads the live count rather than the list, so it
    // starts where the list already is.
    root.engineBusyBytes = EnginesJs.partialOf(root.engines, slug)
    root.runEngineAction(["install", slug])
    enginePoll.start()
  }

  // Cancel is a SIGTERM, which the CLI turns into a kept `.part` file and the
  // `cancelled` code. Killing the process outright would leave curl running.
  function cancelEngineInstall() {
    if (root.engineBusy.length === 0) return
    engineActionProcess.signal(15)
  }

  // Remove asks once when a Check would run on this engine, because the next
  // Check would then have nothing to reach. Every other row goes straight out.
  function removeEngine(slug) {
    // One question at a time too: a second bin press would take the phase to
    // restore back with it and leave the confirm with no way out.
    if (root.enginesBusy) return
    root.engineNote = null
    if (String(root.setting("engine")) !== String(slug)) {
      root.runEngineAction(["remove", slug])
      return
    }
    root.askRemoveEngine(slug)
  }

  function askRemoveEngine(slug) {
    root.engineConfirm = slug
    root.phaseBeforeEngineConfirm = root.phase
    root.phase = "confirmEngine"
  }

  function confirmRemoveEngine(slug) {
    if (root.phase !== "confirmEngine") return
    root.closeEngineConfirm()
    root.runEngineAction(["remove", slug])
  }

  function keepEngine() {
    if (root.phase !== "confirmEngine") return
    root.closeEngineConfirm()
  }

  function closeEngineConfirm() {
    root.engineConfirm = ""
    root.phase = root.phaseBeforeEngineConfirm.length > 0
      ? root.phaseBeforeEngineConfirm : root.phase
    root.phaseBeforeEngineConfirm = ""
  }

  // One verb at a time. Setting `command` under a process that is already
  // running would change what the answer on its way back belongs to, and
  // setting `running` again is a no-op, so the second verb would vanish.
  function runEngineAction(verbArgs) {
    if (engineActionProcess.running) return
    engineActionProcess.verbName = verbArgs.length > 1 ? String(verbArgs[1]) : ""
    engineActionProcess.verb = String(verbArgs[0])
    engineActionProcess.command = root.engineCommand(verbArgs)
    engineActionProcess.launchPending = true
    engineActionProcess.running = true
  }

  function onEngineActionOutput(text, slug, verb) {
    // The verb has spoken about the directory, so every `list` run that started
    // before now read it too early to know that.
    root.engineListFloor = root.engineListSerial + 1
    var answer = EnginesJs.read(text)
    if (answer.error) {
      root.engineNote = EnginesJs.note(answer.error.code, answer.error.message, slug)
      // A cancel or a failure both leave a `.part` file worth redrawing.
      root.refreshEngines()
      return
    }
    root.engineNote = null
    root.absorbEngineReport(answer.report)
    if (verb === "remove") root.fallBackFromRemovedEngine(slug)
  }

  // Spec section 7: removing the engine a Check would run on leaves the
  // Settings consistent, so the setting moves to the one engine that cannot go
  // away. Anything else would leave a stored engine no dropdown row offers and
  // every Check answering `engine_unavailable`.
  //
  // The pacman package is the one case a Remove does not change: the CLI runs
  // that LanguageTool when no installed tree is there, so the row is still
  // available and the setting stays where it is.
  function fallBackFromRemovedEngine(slug) {
    if (EnginesJs.isAvailable(root.engines, slug)) return
    var next = Settings.engineAfterRemoval(root.setting("engine"), slug)
    if (next === null) return
    root.persistSetting("engine", next)
  }

  // The verb is over, whatever it answered, so the poll stops and every other
  // Install comes back.
  function finishEngineAction() {
    enginePoll.stop()
    root.engineBusy = ""
    engineActionProcess.verbName = ""
    engineActionProcess.verb = ""
  }

  // ------------------------------------------------------------------ check

  // One `grammachy check` on this text. What the answer means belongs to the
  // caller: the quick popup checks the whole Selection, and the chunked run of
  // spec section 9 checks one Chunk of the Draft.
  function launchCheck(text, engineSlug) {
    checkProcess.generation = root.runGeneration
    checkProcess.stdinText = text
    checkProcess.command = root.checkCommand(engineSlug)
    // Writing to stdin closes it, so every run arms the channel again.
    checkProcess.stdinEnabled = true
    checkProcess.restartQueued = checkProcess.running
    checkProcess.launchPending = true
    checkProcess.running = true
  }

  // One Check on one text, whose Issues are the whole answer. This is the quick
  // popup's path; Compose walks `runChunk` instead.
  function runCheck(text) {
    root.clearChunkRun()
    root.selectionText = text
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
    root.applied = false
    root.engineMessage = ""
    root.errorCard = null
    root.errorDiagnosis = ""
    root.phase = "checking"
    root.launchCheck(text, root.setting("engine"))
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
  // button reads the same rule, so the two can never disagree. Anything under
  // the cap is checkable, however many Chunks it takes.
  function draftRefusal() {
    return Format.draftRefusal(root.draftText.length, root.draftCapUnits)
  }

  // The chunked Check of spec section 9: one `grammachy chunk`, then one
  // `grammachy check` per Chunk in sequence.
  function startComposeCheck() {
    if (root.surface !== "compose" || root.phase !== "editing") return
    if (root.draftRefusal().length > 0) return
    // A second Check must not be answered by the first one's output.
    root.runGeneration += 1
    root.selectionText = root.draftText
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
    root.applied = false
    root.engineMessage = ""
    root.errorCard = null
    root.errorDiagnosis = ""
    root.chunks = []
    root.chunkIndex = 0
    root.chunkElapsedMs = 0
    root.beginChunkAttempt()
    root.runChunkList()
  }

  // What every attempt at the remaining Chunks starts from, whether it is the
  // first one or a `Retry remaining` after a failure.
  function beginChunkAttempt() {
    root.chunkRun = true
    root.chunkCancelled = false
    root.chunkStartedAt = Date.now()
    root.chunkTickMs = 0
    root.engine = ""
    root.phase = "checking"
    chunkTicker.start()
  }

  // No run is in flight and nothing of one is left behind.
  function clearChunkRun() {
    root.stopChunkRun()
    root.chunks = []
    root.chunkIndex = 0
    root.chunkEngine = ""
    root.chunkElapsedMs = 0
    root.chunkTickMs = 0
  }

  // The run stops where it stands. The Chunk list and the Chunk index stay, so
  // `Retry remaining` still resumes at the Chunk that stopped it.
  function stopChunkRun() {
    chunkTicker.stop()
    root.chunkRun = false
    root.chunkCancelled = false
  }

  function runChunkList() {
    chunkProcess.generation = root.runGeneration
    chunkProcess.stdinText = root.draftText
    // The Chunks are packed to the selected engine's limit, so the engine is
    // named here the way `checkCommand` names it, and the run remembers which
    // one it packed for.
    var engineSlug = root.setting("engine")
    root.chunkEngine = engineSlug
    chunkProcess.command = [root.binaryPath, "chunk", "--engine", engineSlug]
    // Writing to stdin closes it, so every run arms the channel again.
    chunkProcess.stdinEnabled = true
    chunkProcess.restartQueued = chunkProcess.running
    chunkProcess.launchPending = true
    chunkProcess.running = true
  }

  function onChunkListOutput(text, generation) {
    if (!root.isLive(generation)) return
    if (root.chunkCancelled) {
      root.finishChunkRun()
      return
    }

    var answer = Errors.readChunks(text)
    if (answer.error) {
      root.showChunkError(answer.error.code, answer.error.message)
      return
    }
    // A Draft the refusal let through always tiles into at least one Chunk, so
    // an empty list says the companion tool is out of step with section 5.2.
    if (answer.chunks.length === 0) {
      root.showChunkError(Errors.BAD_ARGUMENTS, "")
      return
    }

    root.chunks = answer.chunks
    root.chunkIndex = 0
    root.runChunk()
  }

  function runChunk() {
    if (root.chunkIndex >= root.chunks.length) {
      root.finishChunkRun()
      return
    }
    // The Chunk was cut to fit the Engine the list was packed for, so that is
    // the Engine that reads it, whatever the setting says by now.
    root.launchCheck(Splice.chunkText(root.draftText, root.chunks[root.chunkIndex]), root.runEngine())
  }

  // One Chunk's answer merged into the run, spec section 9. Every span moves by
  // the Chunk's own start before it is verified against the whole Draft, so a
  // mark near a Chunk boundary sits on the text the engine found it in.
  function absorbChunk(envelope) {
    var chunk = root.chunks[root.chunkIndex]
    var shifted = Splice.shiftIssues(envelope.issues || [], chunk.start)
    var verified = Splice.verifiedIssues(root.selectionText, shifted)
    root.warnDropped(verified.dropped)

    root.issues = Splice.mergeIssues(root.issues, verified.issues)
    // Nothing is decided while the run is still walking, so one null per Issue
    // is the whole of what the review starts from.
    root.decisions = root.issues.map(function() { return null })
    root.engine = String(envelope.engine || root.engine)
    root.chunkElapsedMs += Number(envelope.elapsedMs || 0)
    root.chunkIndex += 1

    // Spec section 9: Cancel stops the run after the Chunk in flight, so what
    // the engine already answered is kept.
    if (root.chunkCancelled || root.chunkIndex >= root.chunks.length) {
      root.finishChunkRun()
      return
    }
    root.runChunk()
  }

  function finishChunkRun() {
    chunkTicker.stop()
    root.chunkRun = false
    // A Cancel that landed before any Chunk answered leaves nothing to review,
    // so the Draft comes back rather than an empty result that would read as
    // "no issues found".
    if (root.chunkCancelled && root.chunkIndex === 0) {
      root.backToEdit()
      return
    }
    root.focusIndex = 0
    root.elapsedMs = root.chunkElapsedMs
    root.phase = "result"
  }

  // Spec section 9: Cancel stops after the Chunk in flight rather than killing
  // it, because a Chunk the engine has already worked on is Issues in hand.
  function cancelChunkRun() {
    if (!root.chunkRun) return
    root.chunkCancelled = true
  }

  // A failed Chunk keeps the Issues from the finished ones and shows the engine
  // message inline, spec section 9. `chunkIndex` still names the Chunk that
  // failed, so `Retry remaining` resumes there rather than at the top.
  function showChunkError(code, message) {
    chunkTicker.stop()
    root.chunkRun = false
    root.engineMessage = message
    root.cardSerial += 1
    root.errorDiagnosis = ""
    root.errorCard = Errors.chunkCard(code, {
      engineLabel: root.engineLabel(root.runEngine()),
      engineSlug: root.runEngine(),
      message: message,
      hasPartial: root.issues.length > 0
    })
    root.phase = "error"
    if (root.errorCard.needsDiagnosis) root.runDoctor(root.runEngine())
  }

  // A Chunk list fits only the size it was packed to, because the limit belongs
  // to the Engine (spec section 4). A reader who opens Settings at the failure
  // and picks an Engine of another size leaves every remaining Chunk the wrong
  // length, so that list ends here and the retry packs a new one.
  // The Issues of the finished Chunks go with it, or the Chunks that answer
  // again would report each of them twice.
  function dropChunkListForNewEngine() {
    root.chunks = []
    root.chunkIndex = 0
    root.chunkEngine = ""
    root.chunkElapsedMs = 0
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
  }

  // `Retry remaining`, spec section 9. A Chunk list that never arrived starts
  // the run over; a Chunk that failed resumes at itself, so every Chunk before
  // it keeps the Issues it already found. The wall clock starts again, because
  // the reader may have taken minutes to fix what the message named.
  function retryRemaining() {
    if (root.surface !== "compose" || root.phase !== "error") return
    root.runGeneration += 1
    root.errorCard = null
    root.errorDiagnosis = ""
    root.engineMessage = ""
    // The reader may have picked another Engine at the failure, and the retry
    // is where that choice takes effect. A list packed to another size cannot
    // be resumed and is packed again; one of the same size resumes on the new
    // Engine and keeps the Issues the finished Chunks found.
    if (Limits.checkLimit(root.chunkEngine) !== Limits.checkLimit(root.setting("engine")))
      root.dropChunkListForNewEngine()
    else root.chunkEngine = root.setting("engine")
    root.beginChunkAttempt()
    if (root.chunks.length === 0) root.runChunkList()
    else root.runChunk()
  }

  // `Review what we have`, spec section 9: the Issues of the finished Chunks,
  // reviewed as if the run had ended there.
  function reviewPartial() {
    if (root.surface !== "compose" || root.phase !== "error") return
    root.errorCard = null
    root.errorDiagnosis = ""
    root.engineMessage = ""
    root.focusIndex = 0
    root.elapsedMs = root.chunkElapsedMs
    root.phase = "result"
  }

  function backToEdit() {
    if (root.surface !== "compose" || root.phase === "editing") return
    // Spec section 9: what the reader accepted becomes the Draft they go back
    // to. A Check that never reached a result leaves the Draft as it was.
    if (root.phase === "result") root.draftText = root.correctedText()
    // A Check still in flight answers into a card that has moved on.
    root.runGeneration += 1
    root.clearChunkRun()
    root.phase = "editing"
    root.selectionText = ""
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
    root.applied = false
    root.engineMessage = ""
    root.errorCard = null
    root.errorDiagnosis = ""
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

  function finishChunkListLaunch() {
    if (chunkProcess.running) return
    if (chunkProcess.restartQueued) return
    if (!chunkProcess.launchPending) return
    if (root.phase !== "checking") return
    chunkProcess.launchPending = false
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
    // Every Check Compose runs is one Chunk of a run that may already have
    // Issues behind it, and neither card of section 8 that asks about a
    // Selection has one to ask about there. Spec section 9 gives that failure
    // its own inline card, so Compose goes there instead.
    if (root.surface === "compose") {
      root.showChunkError(code, message)
      return
    }

    root.engineMessage = message
    var settled = Errors.known(code)
    if (settled === Errors.TEXT_TOO_LONG) {
      root.errorCard = null
      root.phase = "toolong"
      return
    }

    root.cardSerial += 1
    root.errorDiagnosis = ""
    root.errorCard = Errors.card(settled, {
      engineLabel: root.engineLabel(root.setting("engine")),
      engineSlug: root.setting("engine"),
      message: message
    })
    root.phase = "error"
    if (root.errorCard.needsDiagnosis) root.runDoctor(root.setting("engine"))
  }

  // Spec section 8: the `engine_unavailable` card shows the one-line diagnosis
  // that `grammachy doctor` gives for the engine the card names.
  function runDoctor(engineSlug) {
    doctorProcess.command = [root.binaryPath, "doctor", "--engine", engineSlug, "--json"]
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
    // The two recovery buttons of a failed Chunk, spec section 9.
    else if (action === Errors.RETRY_REMAINING) root.retryRemaining()
    else if (action === Errors.REVIEW_PARTIAL) root.reviewPartial()
  }

  // Spec section 5.1: an Issue whose slice does not match its original would
  // splice the wrong characters, so it is dropped with a warning on stderr.
  function warnDropped(dropped) {
    for (var i = 0; i < dropped.length; i++) {
      console.warn("grammachy: dropped an issue whose span does not match its original:",
        JSON.stringify({ start: dropped[i].start, end: dropped[i].end, original: dropped[i].original }))
    }
  }

  function onCheckOutput(text, generation) {
    if (!root.isLive(generation)) return

    var answer = Errors.readCheck(text)
    if (answer.error) {
      root.showError(answer.error.code, answer.error.message)
      return
    }

    if (root.chunkRun) {
      root.absorbChunk(answer.result)
      return
    }

    var envelope = answer.result
    var verified = Splice.verifiedIssues(root.selectionText, envelope.issues || [])
    root.warnDropped(verified.dropped)

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
  // `autoReplace` on copies it, closes the popup, asks the compositor for the
  // window the Selection came from, and only then pastes over the Selection
  // that is still highlighted there. The Corrected text stays in the clipboard
  // either way.
  //
  // The ask is the whole point: closing the popup hands the keyboard to
  // whatever the compositor picks, which is not the source window whenever
  // another window sits under the card. A paste without the ask lands there.

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
    //
    // Spec section 6: Replace only works while the Selection is still
    // highlighted in the source window. A run that took no Selection holds
    // none, so `Check last text again` is copy-only whatever the setting says.
    // `runCaptured` is the same fact the release rests on.
    root.runCopy(root.surface === "quick" && root.autoReplace && root.runCaptured)
  }

  function runCopy(pasteAfter) {
    copyProcess.pasteAfter = pasteAfter
    copyProcess.generation = root.runGeneration
    copyProcess.text = root.correctedText()
    copyProcess.stdinEnabled = true
    copyProcess.running = true
    root.applied = true
  }

  // Step one of the Replace: ask for the source window by address. With no
  // address there is nothing to ask for, so the paste goes out as it always
  // did rather than being refused over a window nobody recorded.
  function focusSourceWindow(generation) {
    if (!root.isLive(generation)) return
    var address = Anchor.windowAddress(root.sourceWindow)
    if (address.length === 0) {
      root.launchPasteKeystroke(generation)
      return
    }
    focusSource.command = Anchor.focusCommand(address)
    focusSource.launchPending = true
    focusSource.generation = root.runGeneration
    focusSource.running = true
  }

  // Step two: the dispatch exits 0 for a window that is gone, so the only
  // honest answer comes from asking the compositor who holds the keyboard now.
  function verifySourceFocus(generation) {
    if (!root.isLive(generation)) return
    verifySource.command = Anchor.activeWindowCommand()
    verifySource.launchPending = true
    verifySource.generation = generation
    verifySource.running = true
  }

  function onSourceFocusVerified(text, generation) {
    if (!root.isLive(generation)) return
    if (Anchor.isFocused(text, Anchor.windowAddress(root.sourceWindow))) {
      root.launchPasteKeystroke(generation)
      return
    }
    root.showSourceGone(generation)
  }

  // The source window is gone, so there is nothing to replace. Nothing is
  // typed anywhere, and the card comes back to say where the text went.
  function showSourceGone(generation) {
    if (!root.isLive(generation)) return
    // No keystroke is coming, so nothing is holding the release back.
    root.replacePending = false
    root.opened = true
    root.surface = "quick"
    root.settingsOpen = false
    root.showNotice(Anchor.SOURCE_GONE_TITLE, Anchor.SOURCE_GONE_BODY, Anchor.SOURCE_GONE_META)
  }

  function launchPasteKeystroke(generation) {
    if (!root.isLive(generation)) return
    pasteKeystroke.generation = generation
    pasteKeystroke.running = true
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
    l: Qt.Key_L,
    control: Qt.ControlModifier,
    shift: Qt.ShiftModifier,
    alt: Qt.AltModifier,
    meta: Qt.MetaModifier
  })

  // Which card the press landed on, spec sections 6 and 9. Settings owns its
  // own fields, so every card key stays off while it is open.
  function keyMode() {
    // The Remove confirm sits over the Settings view and is one question, so it
    // takes the keyboard from every other card while it is up.
    if (root.phase === "confirmEngine") return Keymap.MODE_ENGINE_CONFIRM
    if (root.settingsOpen) return Keymap.MODE_IDLE
    if (root.surface === "compose") {
      if (root.phase === "editing") return Keymap.MODE_COMPOSE_EDIT
      if (root.phase === "result" || root.phase === "notice") return Keymap.MODE_COMPOSE_REVIEW
      // A Check in flight, a Draft replacement waiting on a confirm, and the
      // inline failure of a Chunk each carry their own buttons and no Issues to
      // decide, so Esc leaves the way it does everywhere else. The Draft stays.
      return Keymap.MODE_IDLE
    }
    if (root.phase === "result" && root.issues.length > 0) return Keymap.MODE_REVIEW
    // Spec section 6: the quick cards that carry the Clear button and no Issues
    // to decide answer Esc and Ctrl + L, and nothing else. `ui/QuickCard.qml`
    // draws that button on these five phases and on no other, so the button and
    // the key reach exactly the same cards.
    if (root.phase === "checking" || root.phase === "error" || root.phase === "setup"
      || root.phase === "notice" || root.phase === "result")
      return Keymap.MODE_QUICK_CLEAR
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
    else if (action === Keymap.CLEAR) root.clearCapture()
    else if (action === Keymap.APPLY) root.applyCorrected()
    else if (action === Keymap.REMOVE_ENGINE) root.confirmRemoveEngine(root.engineConfirm)
    else if (action === Keymap.KEEP_ENGINE) root.keepEngine()

    event.accepted = true
  }

  // --------------------------------------------------------------- processes

  // Which window the Selection is being taken from, spec section 3. It runs
  // before the capture, because the popup window is what would take that
  // answer away. `launchPending` is what a missing `hyprctl` falls through:
  // the capture has to go on either way.
  Process {
    id: sourceProbe
    property int startedGeneration: 0
    property bool launchPending: false
    onStarted: {
      sourceProbe.launchPending = false
      sourceProbe.startedGeneration = root.runGeneration
    }
    onRunningChanged: {
      if (sourceProbe.running) return
      if (!sourceProbe.launchPending) return
      sourceProbe.launchPending = false
      root.onSourceProbed("", root.runGeneration)
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onSourceProbed(text, sourceProbe.startedGeneration)
    }
  }

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

  // Spec section 3: the primary selection is released when the popup closes,
  // so the summon after this one reads what the reader highlights next rather
  // than what they highlighted before. `releasePrimary` is the only caller,
  // and it holds this back until a Replace has typed.
  Process {
    id: clearPrimary
    command: ["wl-copy", "--primary", "--clear"]
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

  // The Chunk list of spec section 5.2, which opens every chunked Check. It
  // reads the whole Draft on stdin and answers the tiling the run then walks.
  Process {
    id: chunkProcess
    // Chunk list launch.
    property int generation: 0
    property int startedGeneration: 0
    property string stdinText: ""
    property bool launchPending: false
    property bool restartQueued: false
    // Start hook.
    onStarted: {
      chunkProcess.launchPending = false
      chunkProcess.restartQueued = false
      chunkProcess.startedGeneration = root.runGeneration
      write(chunkProcess.stdinText)
      // Close stdin.
      chunkProcess.stdinEnabled = false
    }

    onRunningChanged: {
      if (chunkProcess.running) return
      if (chunkProcess.restartQueued) {
        chunkProcess.restartQueued = false
        return
      }
      if (!chunkProcess.launchPending) return
      if (root.phase !== "checking") return
      Qt.callLater(root.finishChunkListLaunch)
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onChunkListOutput(text, chunkProcess.startedGeneration)
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.length > 0) console.warn("grammachy chunk:", text)
    }
  }


  // The Engines list of spec section 5.4. It runs when Settings opens and once
  // a second while an install is in flight, which is how the progress bar
  // moves: the CLI prints nothing while curl runs, so the `.part` file on disk
  // is the only progress there is to read.
  Process {
    id: engineListProcess
    property bool restartQueued: false
    // When this run read the directory, which is the moment it started.
    property int startedSerial: 0

    onStarted: {
      root.engineListSerial += 1
      engineListProcess.startedSerial = root.engineListSerial
    }
    onRunningChanged: {
      if (engineListProcess.running) return
      if (!engineListProcess.restartQueued) return
      engineListProcess.restartQueued = false
      engineListProcess.running = true
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onEngineListOutput(text, engineListProcess.startedSerial)
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.length > 0) console.warn("grammachy engine list:", text)
    }
  }

  // The one verb the user asked for. An install lives here for minutes, which
  // is why Cancel signals this process rather than ending it: the CLI turns a
  // SIGTERM into a kept `.part` file and the `cancelled` code.
  Process {
    id: engineActionProcess
    property string verbName: ""
    property string verb: ""
    property string startedName: ""
    property string startedVerb: ""
    property bool launchPending: false

    onStarted: {
      engineActionProcess.launchPending = false
      engineActionProcess.startedName = engineActionProcess.verbName
      engineActionProcess.startedVerb = engineActionProcess.verb
    }
    onRunningChanged: {
      if (engineActionProcess.running) return
      // No binary to run, so there is no stdout for the reader to answer from.
      if (engineActionProcess.launchPending) {
        engineActionProcess.launchPending = false
        root.engineNote = EnginesJs.note(
          EnginesJs.BAD_ARGUMENTS, "", engineActionProcess.verbName)
      }
      root.finishEngineAction()
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onEngineActionOutput(
        text, engineActionProcess.startedName, engineActionProcess.startedVerb)
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.length > 0) console.warn("grammachy engine:", text)
    }
  }

  // One second, the smallest change a reader notices on a bar that takes
  // minutes to fill.
  Timer {
    id: enginePoll
    interval: 1000
    repeat: true
    onTriggered: root.refreshEngines()
  }

  // The progress line of spec section 9 names the time the reader is waiting,
  // which only a clock can tell while a Chunk is still out.
  Timer {
    id: chunkTicker
    interval: 200
    repeat: true
    onTriggered: root.chunkTickMs = Date.now() - root.chunkStartedAt
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

  // The two fields cli.lock pins, spec section 10, read as a plain local file
  // the same way every other path on this machine is: never as a URL.
  FileView {
    id: cliLockFile
    path: root.cliLockPath
    preload: true
    onLoadFailed: console.warn("grammachy: cli.lock could not be read:", root.cliLockPath)
  }

  // The setup card's Install button, spec section 10: `bin/bootstrap.sh`,
  // streamed into one log in the order its lines arrived, stdout and stderr
  // folded together because the reader wants the story, not which stream
  // each line came out of.
  Process {
    id: bootstrapProcess
    property bool launchPending: false
    command: [root.bootstrapPath]
    onStarted: bootstrapProcess.launchPending = false
    onRunningChanged: {
      if (bootstrapProcess.running) return
      if (!bootstrapProcess.launchPending) return
      Qt.callLater(root.finishBootstrapLaunch)
    }
    onExited: function(exitCode, exitStatus) {
      root.bootstrapRunning = false
      root.bootstrapExitCode = exitCode
    }
    stdout: SplitParser {
      splitMarker: "\n"
      onRead: function(line) { root.bootstrapLog += line + "\n" }
    }
    stderr: SplitParser {
      splitMarker: "\n"
      onRead: function(line) { root.bootstrapLog += line + "\n" }
    }
  }

  Process {
    id: copyProcess
    property string text: ""
    property int generation: 0
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
      if (!root.isLive(copyProcess.generation)) return
      // The source window still holds the highlight this is about to paste
      // over, so the primary selection is held back until the keystroke is out.
      root.replacePending = true
      // wl-copy has claimed the selection by now, so the paste will find it.
      root.close()
      pasteTimer.generation = copyProcess.generation
      pasteTimer.restart()
    }
  }

  // The compositor needs a moment to take the layer-shell surface down before
  // it will move the keyboard anywhere. The same 150 ms the Ctrl + C capture
  // waits, for the same reason.
  Timer {
    id: pasteTimer
    property int generation: 0
    interval: 150
    repeat: false
    onTriggered: {
      if (!root.isLive(pasteTimer.generation)) return
      root.focusSourceWindow(pasteTimer.generation)
    }
  }

  // `hyprctl dispatch` for the source window. It answers 0 whether or not the
  // window is still there, so nothing is typed on its word alone.
  Process {
    id: focusSource
    property int generation: 0
    property bool launchPending: false
    onStarted: focusSource.launchPending = false
    onExited: {
      if (focusSource.launchPending) return
      root.verifySourceFocus(focusSource.generation)
    }
    onRunningChanged: {
      if (focusSource.running) return
      if (!focusSource.launchPending) return
      focusSource.launchPending = false
      // No `hyprctl` to ask, so the source window cannot be reached and the
      // Corrected text stays on the clipboard rather than landing anywhere.
      root.showSourceGone(focusSource.generation)
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.length > 0) console.warn("grammachy focus:", text)
    }
  }

  // Who holds the keyboard now. This answer, and only this one, lets the paste
  // go out.
  Process {
    id: verifySource
    property int generation: 0
    property bool launchPending: false
    onStarted: verifySource.launchPending = false
    onRunningChanged: {
      if (verifySource.running) return
      if (!verifySource.launchPending) return
      verifySource.launchPending = false
      root.showSourceGone(verifySource.generation)
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onSourceFocusVerified(text, verifySource.generation)
    }
  }

  // The Replace is over once this is out, so the primary selection the
  // Selection came from may go, spec section 3.
  Process {
    id: pasteKeystroke
    property int generation: 0
    command: ["wtype", "-M", "ctrl", "v", "-m", "ctrl"]
    onExited: {
      root.replacePending = false
      if (!root.isLive(pasteKeystroke.generation)) return
      root.releasePrimary()
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

        // The card opens beside the window the Selection came from, so it is
        // near the text it is about rather than in a corner of the screen.
        // `ui/anchor.js` owns that arithmetic, including the bar corner it
        // falls back to when no window on this monitor held the Selection.
        //
        // Hyprland reports a window in the global layout and this surface
        // covers one monitor of it, so the surface's own origin is what turns
        // the one into the other.
        readonly property var placement: Anchor.placeCard({
          window: root.sourceWindow,
          origin: {
            x: panel.screen ? panel.screen.x : 0,
            y: panel.screen ? panel.screen.y : 0
          },
          bounds: { width: parent.width, height: parent.height },
          card: { width: card.width, height: card.height },
          bar: { position: root.barPosition, size: root.barSize },
          gap: root.gap
        })

        x: card.placement.x
        y: card.placement.y

        cardWidth: Math.min(Style.space(680), parent.width - root.gap * 2)
        maxCardHeight: parent.height - root.barSize - root.gap * 2

        phase: root.phase
        sourceText: root.selectionText
        fullText: root.capturedText
        truncated: root.truncated
        lastCapturedText: root.lastCapturedText
        limitUnits: root.checkLimitUnits
        issues: root.issues
        decisions: root.decisions
        focusIndex: root.focusIndex
        engine: root.engine
        elapsedMs: root.elapsedMs
        applied: root.applied
        autoReplace: root.autoReplace
        runCaptured: root.runCaptured
        noticeTitle: root.noticeTitle
        noticeBody: root.noticeBody
        noticeMeta: root.noticeMeta
        engineMessage: root.engineMessage
        errorCard: root.errorCard
        diagnosis: root.errorDiagnosis
        setupCard: root.setupCardModel

        settingsOpen: root.settingsOpen
        nativeLanguage: root.setting("nativeLanguage")
        engineSetting: root.setting("engine")

        engines: root.engines
        engineBusy: root.engineBusy
        engineBusyBytes: root.engineBusyBytes
        enginesBusy: root.enginesBusy
        engineConfirm: root.engineConfirm
        enginesDirectory: root.enginesDirectory
        enginesFreeBytes: root.enginesFreeBytes
        engineNote: root.engineNote

        onEngineInstallRequested: function(slug) { root.installEngine(slug) }
        onEngineCancelRequested: root.cancelEngineInstall()
        onEngineRemoveRequested: function(slug) { root.removeEngine(slug) }
        onEngineRemoveConfirmed: function(slug) { root.confirmRemoveEngine(slug) }
        onEngineKeepRequested: root.keepEngine()

        onSettingsToggled: root.settingsOpen = !root.settingsOpen
        onSettingChanged: function(name, value) { root.persistSetting(name, value) }
        onAccepted: function(index) { root.decide(index, true) }
        onSkipped: function(index) { root.decide(index, false) }
        onAcceptAllRequested: root.acceptAllOpen()
        onApplyRequested: root.applyCorrected()
        onAutoReplaceToggled: root.toggleAutoReplace()
        onFocusRequested: function(index) { root.focusIndex = index }
        onCheckFirstRequested: root.checkFirstUnits()
        onCheckLastRequested: root.checkLastAgain()
        onClearRequested: root.clearCapture()
        // Spec section 2: both the too-long card's `Open in Compose` and the
        // Compose button in the hero carry the Selection over as the Draft.
        onComposeRequested: root.composeWith(root.capturedText)
        onErrorActionRequested: function(action) { root.runErrorAction(action) }
        onSetupActionRequested: function(action) { root.runSetupAction(action) }
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
        pendingDraft: root.pendingDraft
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
        errorCard: root.errorCard
        diagnosis: root.errorDiagnosis
        draftCapUnits: root.draftCapUnits

        // The chunked run of spec section 9, counted from one for the reader.
        chunkNumber: root.chunkIndex + 1
        chunkCount: root.chunks.length
        chunkElapsedMs: root.chunkTickMs
        runningEngine: root.runningEngine

        settingsOpen: root.settingsOpen
        nativeLanguage: root.setting("nativeLanguage")
        engineSetting: root.setting("engine")
        autoReplace: root.autoReplace

        engines: root.engines
        engineBusy: root.engineBusy
        engineBusyBytes: root.engineBusyBytes
        enginesBusy: root.enginesBusy
        engineConfirm: root.engineConfirm
        enginesDirectory: root.enginesDirectory
        enginesFreeBytes: root.enginesFreeBytes
        engineNote: root.engineNote

        onEngineInstallRequested: function(slug) { root.installEngine(slug) }
        onEngineCancelRequested: root.cancelEngineInstall()
        onEngineRemoveRequested: function(slug) { root.removeEngine(slug) }
        onEngineRemoveConfirmed: function(slug) { root.confirmRemoveEngine(slug) }
        onEngineKeepRequested: root.keepEngine()

        onSettingsToggled: root.settingsOpen = !root.settingsOpen
        onSettingChanged: function(name, value) { root.persistSetting(name, value) }
        onDraftEdited: function(text) { root.editDraft(text) }
        onClearRequested: root.clearDraft()
        onCheckRequested: root.startComposeCheck()
        onCancelRequested: root.cancelChunkRun()
        onBackToEditRequested: root.backToEdit()
        onReplaceDraftRequested: root.replaceDraft()
        onKeepDraftRequested: root.keepDraft()
        onErrorActionRequested: function(action) { root.runErrorAction(action) }
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
