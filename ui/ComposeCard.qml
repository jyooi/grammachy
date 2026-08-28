import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "format.js" as Format

// The Compose card, spec section 9: one centred card with two modes over the
// same hero, inspector, and footer as the quick popup of spec section 6.
//
// Edit mode holds the Draft in a plain text area. Check switches to review
// mode with the marked text; `Back to edit` hands the Corrected text back as
// the new Draft. Apply is `Copy corrected text` only, because auto-replace has
// no Selection to paste over here.
//
// Check splits a Draft under the cap into Chunks, with Cancel and a failure card.
// The confirm of spec section 2 and the setup card of spec section 10 are extra cards.
//
// The card renders state and reports intent. The Draft itself, the Check, the
// key map, the clipboard, and the settings storage all live in Overlay.qml.
BorderSurface {
  id: root

  // "editing", "confirm", "checking", "result", "error", "setup", or "notice".
  property string phase: "editing"
  // The Draft. Edit mode reads and writes it; review mode leaves it alone.
  property string draftText: ""
  // The Draft a trigger wants in place of the one above, spec section 2. The
  // confirm card is what stands between the two.
  property string pendingDraft: ""
  // The exact text the Check ran on. Every Issue span indexes into it.
  property string sourceText: ""
  property var issues: []
  // One entry per Issue: true accepted, false skipped, null still open.
  property var decisions: []
  property int focusIndex: 0
  property string engine: ""
  property int elapsedMs: 0
  // Apply has run on the Corrected text as it stands.
  property bool applied: false
  property string noticeTitle: ""
  property string noticeBody: ""
  // What the CLI said, shown in monospace under the notice body, spec 8.
  property string engineMessage: ""

  // The inline failure of one Chunk, spec section 9: a card model from
  // `ui/errors.js` and the one-line `grammachy doctor` answer beside it.
  property var errorCard: null
  property string diagnosis: ""

  // The setup card of spec section 10, shown from the `setup` phase.
  property var setupCard: null

  // The chunked run of spec section 9. `chunkNumber` is the Chunk being
  // checked, counted from one, and `chunkElapsedMs` is the wall clock of this
  // attempt rather than engine time, because that is the wait on screen.
  property int chunkNumber: 1
  property int chunkCount: 0
  property int chunkElapsedMs: 0
  // The engine the progress line names before any Chunk has answered.
  property string runningEngine: ""

  // The Draft cap of spec section 5.2, which decides whether the Check may run
  // at all. `cli/tests/overlay_limit.rs` keeps the copy in Overlay.qml equal to
  // the CLI's own. Anything under it is checkable, however many Chunks it takes.
  property int draftCapUnits: 50000

  // The Settings view, spec section 7, reached through the same gear as the
  // popup. The values arrive already resolved through the defaults.
  property bool settingsOpen: false
  property string nativeLanguage: "none"
  property string engineSetting: "languagetool"
  property bool autoReplace: false
  property string quickHotkey: "SUPER + G"
  property string composeHotkey: "SUPER + SHIFT + G"

  // The Engines list of spec section 5.4, passed straight to the Settings view.
  // The card knows nothing about it either: Overlay.qml owns every process.
  property var engines: []
  property string engineBusy: ""
  property double engineBusyBytes: 0
  property bool enginesBusy: false
  property string engineConfirm: ""
  property string enginesDirectory: ""
  property double enginesFreeBytes: 0
  property var engineNote: null

  // Spec section 9: about 900 px wide and 80 percent of the screen height.
  property int cardWidth: Style.space(900)
  property int cardHeight: Style.space(600)

  // The item whose `Keys.onPressed` runs the key map, handed to the Draft text
  // area so that Ctrl + Enter reaches the card rather than the text.
  property var keySink: null

  signal draftEdited(string text)
  signal clearRequested()
  signal checkRequested()
  signal cancelRequested()
  signal backToEditRequested()
  signal replaceDraftRequested()
  signal keepDraftRequested()
  // One button of the inline failure of spec section 9. The action is a button
  // id from `ui/errors.js`; Overlay.qml owns where each one goes.
  signal errorActionRequested(string action)
  signal setupActionRequested(string action)
  signal accepted(int index)
  signal skipped(int index)
  signal acceptAllRequested()
  signal applyRequested()
  signal focusRequested(int index)
  signal closeRequested()
  signal settingsToggled()
  signal settingChanged(string name, var value)
  signal engineInstallRequested(string slug)
  signal engineCancelRequested()
  signal engineRemoveRequested(string slug)
  signal engineRemoveConfirmed(string slug)
  signal engineKeepRequested()

  readonly property color acceptedColor: marked.acceptedColor

  readonly property int issueCount: issues ? issues.length : 0
  readonly property int acceptedCount: root.countOf(true)
  readonly property int skippedCount: root.countOf(false)
  readonly property int openCount: root.issueCount - root.acceptedCount - root.skippedCount
  // The Settings view takes the body over; every other row hangs off this.
  readonly property bool showsCard: !root.settingsOpen
  readonly property bool editing: root.showsCard && root.phase === "editing"
  readonly property bool confirming: root.showsCard && root.phase === "confirm"
  readonly property bool checking: root.showsCard && root.phase === "checking"
  readonly property bool reviewing: root.showsCard && root.phase === "result"
  readonly property bool hasError: root.showsCard && root.phase === "error" && Boolean(root.errorCard)
  readonly property bool hasSetup: root.showsCard && root.phase === "setup"
  readonly property bool hasIssues: root.reviewing && root.issueCount > 0
  readonly property bool isEmptyResult: root.reviewing && root.issueCount === 0
  readonly property var focusedIssue: root.hasIssues && root.focusIndex >= 0 && root.focusIndex < root.issueCount
    ? root.issues[root.focusIndex] : null

  readonly property int draftUnits: root.draftText.length
  // Why the Check will not run, or "" when it will. One rule, in format.js,
  // so that a node test owns the wording of the cap of spec section 9.
  readonly property string refusal: Format.draftRefusal(root.draftUnits, root.draftCapUnits)

  readonly property int chunksDone: Math.max(0, root.chunkNumber - 1)

  // What the reader has to know before they trust the counts: a run that was
  // cancelled, or that stopped on a failure, checked only the head of the Draft.
  // The failure names what is in hand too, so `Review what we have` says what
  // the reader would be reviewing.
  function partialNote() {
    if (!root.reviewing && !root.hasError) return ""
    if (root.chunkCount === 0 || root.chunksDone >= root.chunkCount) return ""
    var note = "Checked " + root.chunksDone + " of " + root.chunkCount + " chunks"
    if (!root.hasError) return note
    return note + ", " + root.issueCount + (root.issueCount === 1 ? " issue so far" : " issues so far")
  }

  function countOf(value) {
    var total = 0
    for (var i = 0; i < root.issueCount; i++) if (root.decisions && root.decisions[i] === value) total++
    return total
  }

  function metaLine() {
    if (root.phase === "editing") return "draft, " + Format.units(root.draftUnits)
    if (root.phase === "confirm") return "replace the draft?"
    // The progress line of spec section 9, once the Chunk list says how many
    // Chunks there are. Before that the run is still deciding.
    if (root.phase === "checking") {
      if (root.chunkCount === 0) return "splitting the draft into chunks"
      return Format.chunkProgress(root.chunkNumber, root.chunkCount,
        root.runningEngine, root.chunkElapsedMs)
    }
    if (root.phase === "error") return root.errorCard ? String(root.errorCard.meta) : "check did not finish"
    if (root.phase === "setup") return "companion tool missing"
    if (root.phase === "notice") return "check did not finish"
    // A chunked run is counted the way the progress line above it was, because
    // a Draft of many Chunks takes seconds rather than a moment.
    var run = root.engine + ", " + Format.elapsed(root.elapsedMs)
    if (root.issueCount === 0) return "no issues, " + run
    return root.issueCount + (root.issueCount === 1 ? " issue, " : " issues, ")
      + root.acceptedCount + " accepted, " + run
  }

  // Focus follows the mode: the Draft owns the keyboard while it is being
  // written, and the key map owns it while the Issues are being decided.
  function takeFocus() {
    if (root.editing) draft.focusEditor()
    else if (root.keySink) root.keySink.forceActiveFocus()
  }

  onEditingChanged: Qt.callLater(root.takeFocus)

  color: Color.popups.background
  radius: Style.cornerRadius
  padding: Style.spacing.popupPadding
  borderSpec: Border.localOrSurfaceSpec("popups", "border", Color.popups.border, Color.popups.border, Math.max(1, Style.space(2)))

  implicitWidth: root.cardWidth
  implicitHeight: root.cardHeight

  // The card owns its own click so a press inside it never reaches the
  // backdrop that closes the overlay.
  MouseArea {
    anchors.fill: parent
    onClicked: {}
  }

  ColumnLayout {
    id: layout

    anchors.fill: parent
    anchors.topMargin: root.contentTopInset
    anchors.rightMargin: root.contentRightInset
    anchors.bottomMargin: root.contentBottomInset
    anchors.leftMargin: root.contentLeftInset
    spacing: Style.spacing.lg

    CardHero {
      Layout.fillWidth: true

      metaText: root.metaLine()
      // What the run kept after a Cancel or a failure, so the counts above the
      // Issues never read as the whole Draft when they are not.
      noteText: root.partialNote()
      // Spec section 9: auto-replace never applies in Compose, so the toggle
      // that promises it stays on the popup.
      showsAutoReplace: false
      settingsOpen: root.settingsOpen
      // Spec section 9: the progress line carries a Cancel that stops the run
      // after the Chunk in flight.
      actions: root.checking
        ? [{ id: "cancel", text: "Cancel", tooltip: "Stop after this chunk", primary: false }]
        : []
      onSettingsToggled: root.settingsToggled()
      onActionRequested: root.cancelRequested()
    }

    SettingsView {
      Layout.fillWidth: true
      Layout.topMargin: Style.spacing.md
      visible: root.settingsOpen

      nativeLanguage: root.nativeLanguage
      engine: root.engineSetting
      autoReplace: root.autoReplace
      quickHotkey: root.quickHotkey
      composeHotkey: root.composeHotkey
      engines: root.engines
      engineBusy: root.engineBusy
      engineBusyBytes: root.engineBusyBytes
      enginesBusy: root.enginesBusy
      engineConfirm: root.engineConfirm
      enginesDirectory: root.enginesDirectory
      enginesFreeBytes: root.enginesFreeBytes
      engineNote: root.engineNote
      onEngineInstallRequested: function(slug) { root.engineInstallRequested(slug) }
      onEngineCancelRequested: root.engineCancelRequested()
      onEngineRemoveRequested: function(slug) { root.engineRemoveRequested(slug) }
      onEngineRemoveConfirmed: function(slug) { root.engineRemoveConfirmed(slug) }
      onEngineKeepRequested: root.engineKeepRequested()
      onSettingChanged: function(name, value) { root.settingChanged(name, value) }
    }

    // The card keeps its height whichever body it draws, so the Settings view
    // needs the same push that keeps the footer on the bottom edge.
    Item {
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.settingsOpen
    }

    // ------------------------------------------------------------------ body
    //
    // One region that every mode draws into, so the card keeps its height and
    // the text inside it scrolls, spec section 9.

    Item {
      id: body

      Layout.fillWidth: true
      Layout.fillHeight: true
      Layout.minimumHeight: Style.space(96)
      visible: root.showsCard

      DraftField {
        id: draft

        anchors.fill: parent
        visible: root.editing

        text: root.draftText
        placeholderText: "Write or paste a draft here, then press Check."
        keySink: root.keySink
        onEdited: function(text) { root.draftEdited(text) }
      }

      MarkedText {
        id: marked

        anchors.fill: parent
        visible: root.hasIssues

        sourceText: root.sourceText
        issues: root.issues
        decisions: root.decisions
        focusIndex: root.focusIndex
        onMarkActivated: function(index) { root.focusRequested(index) }
      }

      // --------------------------------------------------- chunked progress
      //
      // Spec section 9. The bar is the Chunks behind the run against the whole
      // Draft, so a 20,000 unit Draft shows something moving rather than one
      // sentence that never changes.

      ColumnLayout {
        anchors.centerIn: parent
        width: parent.width
        visible: root.checking
        spacing: Style.spacing.md

        Text {
          Layout.alignment: Qt.AlignHCenter
          text: root.chunkCount === 0
            ? "Splitting the draft into chunks..."
            : "Checking chunk " + root.chunkNumber + " of " + root.chunkCount + "..."
          color: Color.popups.text
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }

        Rectangle {
          Layout.fillWidth: true
          Layout.maximumWidth: Style.space(360)
          Layout.alignment: Qt.AlignHCenter
          implicitHeight: Style.space(6)
          radius: height / 2
          color: Style.normalBorderColor

          Rectangle {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            width: root.chunkCount === 0 ? 0 : Math.round(parent.width * root.chunksDone / root.chunkCount)
            height: parent.height
            radius: parent.radius
            color: Color.accent

            Behavior on width {
              NumberAnimation { duration: 160; easing.type: Easing.OutCubic }
            }
          }
        }

        Text {
          Layout.alignment: Qt.AlignHCenter
          visible: root.issueCount > 0
          text: root.issueCount + (root.issueCount === 1 ? " issue so far" : " issues so far")
          color: Color.muted
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
        }
      }

      // ------------------------------------------------ the replace confirm
      //
      // Spec section 2: a trigger that carries a text replaces a non-empty
      // Draft only after this. The Draft is memory only, so nothing else holds
      // a copy of what the reader would lose.

      ColumnLayout {
        anchors.top: parent.top
        width: parent.width
        visible: root.confirming
        spacing: Style.spacing.md

        Text {
          Layout.fillWidth: true
          text: "Replace the draft?"
          color: Color.popups.text
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.title
          font.bold: true
        }

        Text {
          Layout.fillWidth: true
          text: "Compose already holds a draft of " + Format.units(root.draftUnits)
            + ". The new text is " + Format.units(root.pendingDraft.length)
            + ". The draft is kept in memory only, so replacing it loses it."
          color: Color.popups.text
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }
      }

      // -------------------------------------------- the inline chunk failure
      //
      // Spec section 9: the error of the Chunk that failed, over the Issues the
      // finished Chunks already found.

      ErrorCard {
        anchors.top: parent.top
        width: parent.width
        visible: root.hasError

        card: root.errorCard
        diagnosis: root.diagnosis
        onActionRequested: function(action) { root.errorActionRequested(action) }
      }

      SetupCard {
        anchors.top: parent.top
        width: parent.width
        visible: root.hasSetup

        card: root.setupCard
        onActionRequested: function(action) { root.setupActionRequested(action) }
      }

      // ------------------------------------------------------- empty state

      ColumnLayout {
        anchors.centerIn: parent
        width: parent.width
        visible: root.isEmptyResult
        spacing: Style.spacing.sm

        Text {
          Layout.fillWidth: true
          horizontalAlignment: Text.AlignHCenter
          text: "✓"
          color: root.acceptedColor
          font.family: Style.font.family
          font.pixelSize: Style.font.displayLarge
        }

        Text {
          Layout.fillWidth: true
          horizontalAlignment: Text.AlignHCenter
          text: "No issues found"
          color: Color.popups.text
          font.family: Style.font.family
          font.pixelSize: Style.font.title
          font.bold: true
        }

        Text {
          Layout.fillWidth: true
          horizontalAlignment: Text.AlignHCenter
          wrapMode: Text.Wrap
          text: root.sourceText.length + " characters checked, " + root.engine
            + ", " + Format.elapsed(root.elapsedMs)
          color: Color.muted
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
        }
      }

      // ------------------------------------------------------ notice card

      ColumnLayout {
        anchors.top: parent.top
        width: parent.width
        visible: root.showsCard && root.phase === "notice"
        spacing: Style.spacing.md

        Text {
          Layout.fillWidth: true
          text: root.noticeTitle
          color: Color.urgent
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.title
          font.bold: true
        }

        Text {
          Layout.fillWidth: true
          text: root.noticeBody
          color: Color.popups.text
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }

        Text {
          Layout.fillWidth: true
          visible: root.engineMessage.length > 0
          text: root.engineMessage
          color: Color.muted
          wrapMode: Text.Wrap
          font.family: "monospace"
          font.pixelSize: Style.font.caption
        }
      }
    }

    // The size of the Draft, and why the Check will not take it. Spec section
    // 9 asks for the count and the cap, so the refusal carries both. An empty
    // Draft is not yet a mistake, so only the cap reads as one.
    Text {
      Layout.fillWidth: true
      visible: root.editing && root.refusal.length > 0
      text: root.refusal
      color: root.draftUnits > root.draftCapUnits ? Color.urgent : Color.muted
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.bodySmall
    }

    // ------------------------------------------------------- inspector strip

    Inspector {
      Layout.fillWidth: true
      visible: root.hasIssues

      issue: root.focusedIssue
      focusIndex: root.focusIndex
      issueCount: root.issueCount
      acceptedColor: root.acceptedColor
      onAccepted: function(index) { root.accepted(index) }
      onSkipped: function(index) { root.skipped(index) }
    }

    // ------------------------------------------------------------ the footer

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.spacing.lg

      ReviewCounts {
        visible: root.hasIssues
        acceptedCount: root.acceptedCount
        skippedCount: root.skippedCount
        openCount: root.openCount
      }

      Item { Layout.fillWidth: true }

      Button {
        visible: root.showsCard && (root.checking || root.isEmptyResult || root.phase === "notice")
        text: "Close"
        bordered: true
        foreground: Color.popups.text
        fontFamily: Style.font.family
        onClicked: root.closeRequested()
      }

      // Spec section 2: the confirm keeps the Draft unless the reader says
      // otherwise, so keeping it is the safe side and leads.
      Button {
        visible: root.confirming
        text: "Keep the draft"
        bordered: true
        foreground: Color.accent
        fontFamily: Style.font.family
        onClicked: root.keepDraftRequested()
      }

      Button {
        visible: root.confirming
        text: "Replace it"
        bordered: true
        foreground: Color.urgent
        fontFamily: Style.font.family
        onClicked: root.replaceDraftRequested()
      }

      // Spec section 9: the Draft is memory only, so this is the one way to
      // be rid of it without ending the shell.
      Button {
        visible: root.editing
        enabled: root.draftUnits > 0
        opacity: enabled ? 1.0 : 0.4
        text: "Clear"
        bordered: true
        foreground: Color.popups.text
        fontFamily: Style.font.family
        onClicked: root.clearRequested()
      }

      // The inline failure and the confirm carry their own buttons, and a run
      // still walking has no Draft to go back to yet.
      Button {
        visible: root.reviewing || (root.showsCard && root.phase === "notice")
        text: "Back to edit"
        tooltipText: "Esc"
        bordered: true
        foreground: Color.popups.text
        fontFamily: Style.font.family
        onClicked: root.backToEditRequested()
      }

      Button {
        visible: root.hasIssues
        enabled: root.openCount > 0
        opacity: enabled ? 1.0 : 0.4
        text: "Accept all open"
        tooltipText: "A"
        bordered: true
        foreground: Color.popups.text
        fontFamily: Style.font.family
        onClicked: root.acceptAllRequested()
      }

      // Spec section 9: Compose copies and never pastes, because the Draft
      // came from this card rather than from a window holding a Selection.
      Button {
        visible: root.hasIssues
        enabled: root.acceptedCount > 0 && !root.applied
        opacity: enabled ? 1.0 : 0.4
        text: root.applied ? "Copied" : "Copy corrected text"
        tooltipText: "Ctrl + Enter"
        bordered: true
        foreground: root.applied ? Color.popups.text : Color.accent
        fontFamily: Style.font.family
        onClicked: root.applyRequested()
      }

      Button {
        visible: root.editing
        enabled: root.refusal.length === 0
        opacity: enabled ? 1.0 : 0.4
        text: "Check"
        tooltipText: "Ctrl + Enter"
        bordered: true
        foreground: Color.accent
        fontFamily: Style.font.family
        onClicked: root.checkRequested()
      }

      // Settings persists on change, so the only thing left to do is leave.
      // Spec section 7: there is no Save button.
      Button {
        visible: root.settingsOpen
        text: "Back"
        bordered: true
        foreground: Color.accent
        fontFamily: Style.font.family
        onClicked: root.settingsToggled()
      }
    }
  }
}
