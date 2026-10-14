import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "format.js" as Format

BorderSurface {
  id: root

  property string phase: "editing"
  property string draftText: ""
  property string pendingDraft: ""
  property string sourceText: ""
  property var issues: []
  property var decisions: []
  property int focusIndex: 0
  property string engine: ""
  property int elapsedMs: 0
  property bool applied: false
  property string noticeTitle: ""
  property string noticeBody: ""
  property string engineMessage: ""

  property var errorCard: null
  property string diagnosis: ""

  property var setupCard: null

  property int chunkNumber: 1
  property int chunkCount: 0
  property int chunkElapsedMs: 0
  property string runningEngine: ""

  property int draftCapUnits: 50000

  property bool settingsOpen: false
  property string nativeLanguage: "none"
  property string targetEnglish: "en-US"
  property string engineSetting: "languagetool"
  property bool autoReplace: false
  property string quickHotkey: "SUPER + SHIFT + Q"
  property string composeHotkey: "SUPER + ALT + Q"

  property var engines: []
  property string engineBusy: ""
  property double engineBusyBytes: 0
  property bool enginesBusy: false
  property string engineConfirm: ""
  property string enginesDirectory: ""
  property double enginesFreeBytes: 0
  property var engineNote: null
  property var dependencies: []

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
  readonly property string refusal: Format.draftRefusal(root.draftUnits, root.draftCapUnits)

  readonly property int chunksDone: Math.max(0, root.chunkNumber - 1)

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
    if (root.phase === "checking") {
      if (root.chunkCount === 0) return "splitting the draft into chunks"
      return Format.chunkProgress(root.chunkNumber, root.chunkCount,
        root.runningEngine, root.chunkElapsedMs)
    }
    if (root.phase === "error") return root.errorCard ? String(root.errorCard.meta) : "check did not finish"
    if (root.phase === "setup") return "companion tool missing"
    if (root.phase === "notice") return "check did not finish"
    var run = root.engine + ", " + Format.elapsed(root.elapsedMs)
    if (root.issueCount === 0) return "no issues, " + run
    return root.issueCount + (root.issueCount === 1 ? " issue, " : " issues, ")
      + root.acceptedCount + " accepted, " + run
  }

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
      noteText: root.partialNote()
      showsAutoReplace: false
      settingsOpen: root.settingsOpen
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
      targetEnglish: root.targetEnglish
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
      dependencies: root.dependencies
      onEngineInstallRequested: function(slug) { root.engineInstallRequested(slug) }
      onEngineCancelRequested: root.engineCancelRequested()
      onEngineRemoveRequested: function(slug) { root.engineRemoveRequested(slug) }
      onEngineRemoveConfirmed: function(slug) { root.engineRemoveConfirmed(slug) }
      onEngineKeepRequested: root.engineKeepRequested()
      onSettingChanged: function(name, value) { root.settingChanged(name, value) }
    }

    Item {
      Layout.fillWidth: true
      Layout.fillHeight: true
      visible: root.settingsOpen
    }

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

      ColumnLayout {
        anchors.centerIn: parent
        width: parent.width
        visible: root.checking
        spacing: Style.spacing.md

        Text {
          Layout.alignment: Qt.AlignHCenter
          textFormat: Text.PlainText
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
          textFormat: Text.PlainText
          text: root.issueCount + (root.issueCount === 1 ? " issue so far" : " issues so far")
          color: Color.muted
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
        }
      }

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
          textFormat: Text.PlainText
          text: "Compose already holds a draft of " + Format.units(root.draftUnits)
            + ". The new text is " + Format.units(root.pendingDraft.length)
            + ". The draft is kept in memory only, so replacing it loses it."
          color: Color.popups.text
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }
      }

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
          textFormat: Text.PlainText
          text: root.sourceText.length + " characters checked, " + root.engine
            + ", " + Format.elapsed(root.elapsedMs)
          color: Color.muted
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
        }
      }

      ColumnLayout {
        anchors.top: parent.top
        width: parent.width
        visible: root.showsCard && root.phase === "notice"
        spacing: Style.spacing.md

        Text {
          Layout.fillWidth: true
          textFormat: Text.PlainText
          text: root.noticeTitle
          color: Color.urgent
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.title
          font.bold: true
        }

        Text {
          Layout.fillWidth: true
          textFormat: Text.PlainText
          text: root.noticeBody
          color: Color.popups.text
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }

        Text {
          Layout.fillWidth: true
          visible: root.engineMessage.length > 0
          textFormat: Text.PlainText
          text: root.engineMessage
          color: Color.muted
          wrapMode: Text.Wrap
          font.family: "monospace"
          font.pixelSize: Style.font.caption
        }
      }
    }

    Text {
      Layout.fillWidth: true
      visible: root.editing && root.refusal.length > 0
      textFormat: Text.PlainText
      text: root.refusal
      color: root.draftUnits > root.draftCapUnits ? Color.urgent : Color.muted
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.bodySmall
    }

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
