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
import "ui/deps.js" as Deps

Item {
  id: root

  // Handed over by the shell's panel loader.
  property var shell: null
  property var manifest: null

  // The shell reads `opened` to answer isPluginOpen, and calls close().
  property bool opened: false

  property string surface: "quick"

  property string phase: "capturing"
  property string capturedText: ""
  property string selectionText: ""
  property bool truncated: false
  property var issues: []
  property var decisions: []
  property int focusIndex: 0
  property string engine: ""
  property int elapsedMs: 0
  property bool applied: false
  property string noticeTitle: ""
  property string noticeMeta: ""
  property string noticeBodyText: ""
  property bool noticeNamesEngine: false
  readonly property string noticeBody: root.noticeNamesEngine
    ? root.noticeBodyText + root.engineLabel(root.setting("engine")) + "."
    : root.noticeBodyText
  property bool settingsOpen: false
  property string engineMessage: ""

  property var errorCard: null
  property string errorDiagnosis: ""
  property int cardSerial: 0

  property bool bootstrapRunning: false
  property var bootstrapExitCode: null
  property string bootstrapLog: ""
  readonly property var setupCardModel: Setup.card({
    lockText: cliLockFile.text(),
    running: root.bootstrapRunning,
    exitCode: root.bootstrapExitCode,
    log: root.bootstrapLog,
    dependencies: root.dependencies
  })

  property var dependencies: null

  readonly property int checkLimitUnits: Limits.checkLimit(root.setting("engine"))

  readonly property int draftCapUnits: 50000

  property string draftText: ""

  property string pendingDraft: ""

  property var chunks: []
  property int chunkIndex: 0
  property bool chunkRun: false
  property bool chunkCancelled: false
  property string chunkEngine: ""
  property int chunkElapsedMs: 0
  property double chunkStartedAt: 0
  property int chunkTickMs: 0

  readonly property string runningEngine: root.engine.length > 0
    ? root.engine : String(root.setting("engine"))

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
  property bool runCaptured: false

  property var engines: []
  property string engineBusy: ""
  property double engineBusyBytes: 0
  property string enginesDirectory: ""
  property double enginesFreeBytes: 0
  property int engineListSerial: 0
  property int engineListFloor: 0
  property var engineNote: null
  property string engineConfirm: ""
  property string phaseBeforeEngineConfirm: ""

  readonly property bool enginesBusy: engineActionProcess.running
    || root.engineConfirm.length > 0

  property string borrowedClipboard: ""
  property bool clipboardBorrowed: false
  property bool pendingPrimaryPaste: false
  property int runGeneration: 0

  readonly property string pluginId: root.manifest && root.manifest.id
    ? String(root.manifest.id) : "io.github.jyooi.grammachy"
  readonly property string binaryPath: Util.isPlainObject(root.manifest) && root.manifest.__sourceDir
    ? String(root.manifest.__sourceDir).replace(/\/$/, "") + "/bin/grammachy"
    : String(Qt.resolvedUrl("bin/grammachy")).replace(/^file:\/\//, "")
  property bool companionFound: false

  readonly property string pluginRoot: Util.isPlainObject(root.manifest) && root.manifest.__sourceDir
    ? String(root.manifest.__sourceDir).replace(/\/$/, "")
    : String(Qt.resolvedUrl(".")).replace(/^file:\/\//, "").replace(/\/$/, "")

  readonly property string bootstrapPath: root.pluginRoot + "/bin/bootstrap.sh"
  readonly property string cliLockPath: root.pluginRoot + "/cli.lock"

  readonly property var barConfig: root.shell && root.shell.bar ? root.shell.bar : null
  readonly property string barPosition: root.barConfig ? String(root.barConfig.position) : "top"
  readonly property int barSize: root.barConfig ? root.barConfig.barSize : Style.bar.sizeHorizontal
  readonly property int gap: Style.gapsOut + Style.spacing.sm

  // Storage is this plugin's inline entry in shell.json, spec section 7. There
  // is no own config file. `shell.shellConfig` is reassigned on every write the
  // shell makes, so `entry` is a live binding: a change from the Settings view,
  // from `omarchy-shell shell setBarWidget`, or from a hand edit of the file
  // all reach the view the same way.
  //
  readonly property var entry: Settings.entryOf(root.shell ? root.shell.shellConfig : null, root.pluginId)

  function setting(name, fallback) {
    return Settings.valueOf(root.entry, name, fallback)
  }

  onSettingsOpenChanged: {
    // The Engines list is drawn whatever engine is selected, because the whole
    // point is to add one the dropdown cannot offer yet (HUF-237). So Settings
    // being open is the whole condition for reading it.
    if (root.settingsOpen) {
      root.refreshEngines()
      root.refreshDependencies()
    }
    else if (root.phase === "confirmEngine") root.closeEngineConfirm()
  }

  function persistSetting(name, value) {
    if (!root.shell || typeof root.shell.updateEntryInline !== "function") {
      console.warn("grammachy: no shell to keep the setting in:", name)
      return
    }
    root.shell.updateEntryInline(root.pluginId, Settings.mergedEntry(root.entry, name, value))
    if (name === "autoReplace") root.autoReplaceOverride = null
    // Spec section 2: Hyprland only reads `bindings.lua` when told to, so a
    // hotkey change re-runs `grammachy setup`, which writes the block from
    // the Settings just stored and reloads the compositor.
    if (name === "quickHotkey" || name === "composeHotkey") root.applyHotkeys()
  }

  function applyHotkeys() {
    if (hotkeyApplyProcess.running) {
      hotkeyApplyProcess.pending = true
      return
    }
    hotkeyApplyProcess.pending = false
    hotkeyApplyProcess.command = [root.binaryPath, "setup"]
    hotkeyApplyProcess.running = true
  }

  readonly property bool storedAutoReplace: root.setting("autoReplace") === true
  property var autoReplaceOverride: null
  readonly property bool autoReplace: root.autoReplaceOverride === null
    ? root.storedAutoReplace : root.autoReplaceOverride === true

  function toggleAutoReplace() {
    root.autoReplaceOverride = !root.autoReplace
    root.applied = false
  }

  function engineLabel(engineSlug) {
    return Settings.labelOf(Settings.ENGINE_OPTIONS, engineSlug)
  }

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

  function open(payloadJson) {
    var payload = ({})
    try {
      payload = JSON.parse(payloadJson || "{}") || ({})
    } catch (error) {
      console.warn("grammachy: summon payload is not JSON:", payloadJson)
    }

    root.opened = true
    if (String(payload.mode || "quick") === "compose") {
      if (typeof payload.text === "string") root.composeWith(payload.text)
      else root.showCompose()
      return
    }
    root.startQuick()
  }

  function close() {
    root.cancelChunkRun()
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

  function showEngineNotice(title, body, meta) {
    root.showNotice(title, body, meta)
    root.noticeNamesEngine = true
  }

  function resetRun() {
    sourceProbe.launchPending = false
    focusSource.launchPending = false
    verifySource.launchPending = false
    checkProcess.launchPending = false
    chunkProcess.launchPending = false
    copyProcess.pasteAfter = false
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
    root.replacePending = false
    root.runCaptured = false
    root.engineConfirm = ""
    root.phaseBeforeEngineConfirm = ""
    root.clearChunkRun()
    if (root.clipboardBorrowed && !restoreClipboard.running)
      root.restoreBorrowedClipboard()
    root.borrowedClipboard = ""
    root.clipboardBorrowed = false
  }

  function showCompose() {
    root.resetRun()
    root.surface = "compose"
    root.phase = "editing"
    Qt.callLater(root.restoreFocus)
  }

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

  function showSetup() {
    root.phase = "setup"
    root.refreshDependencies()
  }

  function refreshDependencies() {
    if (root.companionFound) {
      depsDoctorProcess.command = [root.binaryPath, "doctor", "--json"]
      if (depsDoctorProcess.running) {
        depsDoctorProcess.restartQueued = true
        return
      }
      depsDoctorProcess.running = true
      return
    }
    depsProbeProcess.command = Deps.probeArgv()
    if (depsProbeProcess.running) {
      depsProbeProcess.restartQueued = true
      return
    }
    depsProbeProcess.running = true
  }

  function onDependencyOutput(table) {
    if (Array.isArray(table)) root.dependencies = table
  }

  function installBootstrap() {
    if (root.bootstrapRunning) return
    root.bootstrapRunning = true
    root.bootstrapExitCode = null
    root.bootstrapLog = ""
    bootstrapProcess.launchPending = true
    bootstrapProcess.running = true
  }

  function finishBootstrapLaunch() {
    if (bootstrapProcess.running) return
    if (!bootstrapProcess.launchPending) return
    bootstrapProcess.launchPending = false
    root.bootstrapRunning = false
    root.bootstrapExitCode = 1
    root.bootstrapLog += "bin/bootstrap.sh could not be started.\n"
  }

  function runSetupAction(action) {
    if (action === Setup.INSTALL) root.installBootstrap()
    else if (action === Setup.RETRY) root.retrySetupCheck()
    else if (action === Setup.CLOSE) root.close()
  }

  function retrySetupCheck() {
    if (root.surface === "compose") {
      root.phase = "editing"
      root.startComposeCheck()
      return
    }
    if (Setup.retryAfterSetup(root.surface, root.selectionText) === "startQuick") {
      root.startQuick()
      return
    }
    root.retryCheck()
  }

  // The popup window is still hidden here, so the compositor still calls the
  // window the user selected in the active one. That is the only moment it can
  // be read, which is why the capture waits on the answer rather than racing
  // it: a card that opened first would open at the wrong place and then jump.
  function startQuick() {
    root.resetRun()
    root.surface = "quick"
    if (Setup.companionMissing(root.companionFound)) {
      root.showSetup()
      return
    }
    root.phase = "capturing"
    root.capturedText = ""
    root.probeSourceWindow()
  }

  function probeSourceWindow() {
    sourceProbe.command = Anchor.activeWindowCommand()
    sourceProbe.launchPending = true
    sourceProbe.running = true
  }

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

  function lastCapture() {
    return Capture.kept(root.lastCapturedText, root.lastCapturedWindow)
  }

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

  function showNothingNew() {
    root.capturedText = ""
    root.selectionText = ""
    root.truncated = false
    root.errorCard = null
    root.errorDiagnosis = ""
    root.engineMessage = ""
    root.phase = "empty"
  }

  function checkLastAgain() {
    if (root.lastCapturedText.length === 0) return
    root.capturedText = root.lastCapturedText
    root.truncated = false
    root.runCheck(root.lastCapturedText)
  }

  function clearCapture() {
    if (root.surface !== "quick") return
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
    if (Capture.pasteOverflowed(text)) {
      root.showNothingNew()
      return
    }
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

  function engineCommand(verbArgs) {
    return [root.binaryPath, "engine"].concat(verbArgs)
  }

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

  function installEngine(slug) {
    if (root.enginesBusy) return
    root.engineNote = null
    root.engineBusy = slug
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

  function removeEngine(slug) {
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
    root.engineListFloor = root.engineListSerial + 1
    var answer = EnginesJs.read(text)
    if (answer.error) {
      root.engineNote = EnginesJs.note(answer.error.code, answer.error.message, slug)
      root.refreshEngines()
      return
    }
    root.engineNote = null
    root.absorbEngineReport(answer.report)
    if (verb === "remove") root.fallBackFromRemovedEngine(slug)
  }

  function fallBackFromRemovedEngine(slug) {
    if (EnginesJs.isAvailable(root.engines, slug)) return
    var next = Settings.engineAfterRemoval(root.setting("engine"), slug)
    if (next === null) return
    root.persistSetting("engine", next)
  }

  function finishEngineAction() {
    enginePoll.stop()
    root.engineBusy = ""
    engineActionProcess.verbName = ""
    engineActionProcess.verb = ""
  }

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

  function checkFirstUnits() {
    root.truncated = true
    root.runCheck(Splice.firstUnits(root.capturedText, root.checkLimitUnits))
  }

  function editDraft(text) {
    root.draftText = text
  }

  function clearDraft() {
    root.draftText = ""
  }

  function draftRefusal() {
    return Format.draftRefusal(root.draftText.length, root.draftCapUnits)
  }

  function startComposeCheck() {
    if (root.surface !== "compose" || root.phase !== "editing") return
    if (root.draftRefusal().length > 0) return
    if (Setup.companionMissing(root.companionFound)) {
      root.showSetup()
      return
    }
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

  function beginChunkAttempt() {
    root.chunkRun = true
    root.chunkCancelled = false
    root.chunkStartedAt = Date.now()
    root.chunkTickMs = 0
    root.engine = ""
    root.phase = "checking"
    chunkTicker.start()
  }

  function clearChunkRun() {
    root.stopChunkRun()
    root.chunks = []
    root.chunkIndex = 0
    root.chunkEngine = ""
    root.chunkElapsedMs = 0
    root.chunkTickMs = 0
  }

  function stopChunkRun() {
    chunkTicker.stop()
    root.chunkRun = false
    root.chunkCancelled = false
  }

  function runChunkList() {
    chunkProcess.generation = root.runGeneration
    chunkProcess.stdinText = root.draftText
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
    root.launchCheck(Splice.chunkText(root.draftText, root.chunks[root.chunkIndex]), root.runEngine())
  }

  function absorbChunk(envelope) {
    var chunk = root.chunks[root.chunkIndex]
    var shifted = Splice.shiftIssues(envelope.issues || [], chunk.start)
    var verified = Splice.verifiedIssues(root.selectionText, shifted)
    root.warnDropped(verified.dropped)

    root.issues = Splice.mergeIssues(root.issues, verified.issues)
    root.decisions = root.issues.map(function() { return null })
    root.engine = String(envelope.engine || root.engine)
    root.chunkElapsedMs += Number(envelope.elapsedMs || 0)
    root.chunkIndex += 1

    if (root.chunkCancelled || root.chunkIndex >= root.chunks.length) {
      root.finishChunkRun()
      return
    }
    root.runChunk()
  }

  function finishChunkRun() {
    chunkTicker.stop()
    root.chunkRun = false
    if (root.chunkCancelled && root.chunkIndex === 0) {
      root.backToEdit()
      return
    }
    root.focusIndex = 0
    root.elapsedMs = root.chunkElapsedMs
    root.phase = "result"
  }

  function cancelChunkRun() {
    if (!root.chunkRun) return
    root.chunkCancelled = true
  }

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

  function dropChunkListForNewEngine() {
    root.chunks = []
    root.chunkIndex = 0
    root.chunkEngine = ""
    root.chunkElapsedMs = 0
    root.issues = []
    root.decisions = []
    root.focusIndex = 0
  }

  function retryRemaining() {
    if (root.surface !== "compose" || root.phase !== "error") return
    root.runGeneration += 1
    root.errorCard = null
    root.errorDiagnosis = ""
    root.engineMessage = ""
    if (Limits.checkLimit(root.chunkEngine) !== Limits.checkLimit(root.setting("engine")))
      root.dropChunkListForNewEngine()
    else root.chunkEngine = root.setting("engine")
    root.beginChunkAttempt()
    if (root.chunks.length === 0) root.runChunkList()
    else root.runChunk()
  }

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
    if (root.phase === "result") root.draftText = root.correctedText()
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

  function showBinaryMissing() {
    root.showError(Errors.BAD_ARGUMENTS, "")
  }

  function showError(code, message) {
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
    if (!Util.isPlainObject(report) || report.contractVersion !== 1) return
    root.errorDiagnosis = typeof report.diagnosis === "string" ? report.diagnosis : ""
  }

  function retryCheck() {
    if (root.selectionText.length === 0) return
    root.runCheck(root.selectionText)
  }

  function runErrorAction(action) {
    if (action === Errors.CLOSE) root.close()
    else if (action === Errors.RETRY) root.retryCheck()
    else if (action === Errors.SETTINGS) root.settingsOpen = true
    else if (action === Errors.SETUP) root.showSetup()
    else if (action === Errors.COMPOSE) root.showCompose()
    else if (action === Errors.RETRY_REMAINING) root.retryRemaining()
    else if (action === Errors.REVIEW_PARTIAL) root.reviewPartial()
  }

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

  function decide(index, value) {
    if (index < 0 || index >= root.issues.length) return
    var next = root.decisions.slice()
    next[index] = value
    root.decisions = next
    root.focusIndex = root.nextOpen(index)
    root.applied = false
  }

  function nextOpen(from) {
    for (var i = from + 1; i < root.decisions.length; i++) if (root.decisions[i] === null) return i
    for (var j = 0; j < root.decisions.length; j++) if (root.decisions[j] === null) return j
    return Math.min(from, Math.max(0, root.decisions.length - 1))
  }

  function moveFocus(step) {
    var count = root.issues.length
    if (count === 0) return
    root.focusIndex = ((root.focusIndex + step) % count + count) % count
  }

  function acceptAllOpen() {
    root.decisions = root.decisions.map(function(value) { return value === null ? true : value })
    root.applied = false
  }

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

  function showSourceGone(generation) {
    if (!root.isLive(generation)) return
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

  function keyMode() {
    if (root.phase === "confirmEngine") return Keymap.MODE_ENGINE_CONFIRM
    if (root.settingsOpen) return Keymap.MODE_IDLE
    if (root.surface === "compose") {
      if (root.phase === "editing") return Keymap.MODE_COMPOSE_EDIT
      if (root.phase === "result" || root.phase === "notice") return Keymap.MODE_COMPOSE_REVIEW
      return Keymap.MODE_IDLE
    }
    if (root.phase === "result" && root.issues.length > 0) return Keymap.MODE_REVIEW
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
    command: Capture.primaryCommand()
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
    command: Capture.borrowCommand()
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
    command: Capture.fallbackCommand()
    onStarted: fallbackPaste.startedGeneration = root.runGeneration
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onFallbackCaptured(text, fallbackPaste.startedGeneration)
    }
  }

  Process {
    id: clearPrimary
    command: ["wl-copy", "--primary", "--clear"]
  }

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
    property int generation: 0
    property int startedGeneration: 0
    property string stdinText: ""
    property bool launchPending: false
    property bool restartQueued: false
    onStarted: {
      checkProcess.launchPending = false
      checkProcess.restartQueued = false
      checkProcess.startedGeneration = root.runGeneration
      write(checkProcess.stdinText)
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
    id: chunkProcess
    property int generation: 0
    property int startedGeneration: 0
    property string stdinText: ""
    property bool launchPending: false
    property bool restartQueued: false
    onStarted: {
      chunkProcess.launchPending = false
      chunkProcess.restartQueued = false
      chunkProcess.startedGeneration = root.runGeneration
      write(chunkProcess.stdinText)
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

  Timer {
    id: enginePoll
    interval: 1000
    repeat: true
    onTriggered: root.refreshEngines()
  }

  Timer {
    id: chunkTicker
    interval: 200
    repeat: true
    onTriggered: root.chunkTickMs = Date.now() - root.chunkStartedAt
  }

  Process {
    id: doctorProcess
    property int startedSerial: 0
    property bool restartQueued: false
    onStarted: doctorProcess.startedSerial = root.cardSerial
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
    id: depsDoctorProcess
    property bool restartQueued: false
    onRunningChanged: {
      if (depsDoctorProcess.running) return
      if (!depsDoctorProcess.restartQueued) return
      depsDoctorProcess.restartQueued = false
      depsDoctorProcess.running = true
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onDependencyOutput(Deps.fromDoctor(text))
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.length > 0) console.warn("grammachy doctor:", text)
    }
  }

  Process {
    id: depsProbeProcess
    property bool restartQueued: false
    onRunningChanged: {
      if (depsProbeProcess.running) return
      if (!depsProbeProcess.restartQueued) return
      depsProbeProcess.restartQueued = false
      depsProbeProcess.running = true
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.onDependencyOutput(Deps.fromProbe(text))
    }
  }

  FileView {
    id: cliLockFile
    path: root.cliLockPath
    preload: true
    onLoadFailed: console.warn("grammachy: cli.lock could not be read:", root.cliLockPath)
  }

  // FileView does not watch a missing file. Watch the bin directory instead.
  // The probe runs again when Install writes bin/grammachy.
  FileView {
    id: companionDir
    path: root.pluginRoot + "/bin"
    watchChanges: true
    printErrors: false
    onFileChanged: companionProbe.running = true
  }

  Process {
    id: companionProbe
    running: true
    command: ["test", "-x", root.binaryPath]
    onExited: function(exitCode) {
      var found = exitCode === 0
      var changed = found !== root.companionFound
      root.companionFound = found
      if (changed && root.phase === "setup") root.refreshDependencies()
    }
  }

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
      if (exitCode === 0) root.companionFound = true
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
    id: hotkeyApplyProcess
    property bool pending: false
    onRunningChanged: {
      if (hotkeyApplyProcess.running) return
      if (!hotkeyApplyProcess.pending) return
      root.applyHotkeys()
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.length > 0) console.warn("grammachy setup:", text)
    }
  }

  Process {
    id: copyProcess
    property string text: ""
    property int generation: 0
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

  // `hyprctl dispatch` exits 0 whether or not the window is still there.
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
      root.showSourceGone(focusSource.generation)
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.length > 0) console.warn("grammachy focus:", text)
    }
  }

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

    Rectangle {
      anchors.fill: parent
      color: "black"
      opacity: root.surface === "compose" ? 0.45 : 0

      Behavior on opacity {
        NumberAnimation { duration: 120; easing.type: Easing.OutCubic }
      }
    }

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
        targetEnglish: root.setting("targetEnglish")
        engineSetting: root.setting("engine")
        quickHotkey: root.setting("quickHotkey")
        composeHotkey: root.setting("composeHotkey")

        engines: root.engines
        engineBusy: root.engineBusy
        engineBusyBytes: root.engineBusyBytes
        enginesBusy: root.enginesBusy
        engineConfirm: root.engineConfirm
        enginesDirectory: root.enginesDirectory
        enginesFreeBytes: root.enginesFreeBytes
        engineNote: root.engineNote
        dependencies: root.dependencies || []

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
        onComposeRequested: root.composeWith(root.capturedText)
        onErrorActionRequested: function(action) { root.runErrorAction(action) }
        onSetupActionRequested: function(action) { root.runSetupAction(action) }
        onCloseRequested: root.close()
      }

      ComposeCard {
        id: composeCard

        anchors.centerIn: parent
        visible: root.surface === "compose"

        cardWidth: Math.min(Style.space(900), parent.width - root.gap * 2)
        cardHeight: Math.min(Math.round(parent.height * 0.8), parent.height - root.gap * 2)

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
        setupCard: root.setupCardModel
        draftCapUnits: root.draftCapUnits

        chunkNumber: root.chunkIndex + 1
        chunkCount: root.chunks.length
        chunkElapsedMs: root.chunkTickMs
        runningEngine: root.runningEngine

        settingsOpen: root.settingsOpen
        nativeLanguage: root.setting("nativeLanguage")
        targetEnglish: root.setting("targetEnglish")
        engineSetting: root.setting("engine")
        quickHotkey: root.setting("quickHotkey")
        composeHotkey: root.setting("composeHotkey")
        autoReplace: root.autoReplace

        engines: root.engines
        engineBusy: root.engineBusy
        engineBusyBytes: root.engineBusyBytes
        enginesBusy: root.enginesBusy
        engineConfirm: root.engineConfirm
        enginesDirectory: root.enginesDirectory
        enginesFreeBytes: root.enginesFreeBytes
        engineNote: root.engineNote
        dependencies: root.dependencies || []

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
        onSetupActionRequested: function(action) { root.runSetupAction(action) }
        onAccepted: function(index) { root.decide(index, true) }
        onSkipped: function(index) { root.decide(index, false) }
        onAcceptAllRequested: root.acceptAllOpen()
        onApplyRequested: root.applyCorrected()
        onFocusRequested: function(index) { root.focusIndex = index }
        onCloseRequested: root.close()
      }
    }
  }

  function restoreFocus() {
    if (!panel.visible) return
    if (root.surface === "compose") composeCard.takeFocus()
    else keyCatcher.forceActiveFocus()
  }
}
