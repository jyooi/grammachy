import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "format.js" as Format
import "capture.js" as Capture

BorderSurface {
  id: root

  property string phase: "checking"
  property string sourceText: ""
  property string fullText: ""
  property bool truncated: false
  property string lastCapturedText: ""
  property int limitUnits: 5000
  property var issues: []
  property var decisions: []
  property int focusIndex: 0
  property string engine: ""
  property int elapsedMs: 0
  property bool applied: false
  property bool autoReplace: false
  property bool runCaptured: false
  property string noticeTitle: ""
  property string noticeBody: ""
  property string noticeMeta: ""
  property string engineMessage: ""

  property var errorCard: null
  property string diagnosis: ""

  property var setupCard: null

  property bool settingsOpen: false
  property string nativeLanguage: "none"
  property string targetEnglish: "en-US"
  property string engineSetting: "languagetool"
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

  property int cardWidth: Style.space(680)
  property int maxCardHeight: Style.space(600)

  signal accepted(int index)
  signal skipped(int index)
  signal acceptAllRequested()
  signal applyRequested()
  signal autoReplaceToggled()
  signal focusRequested(int index)
  signal checkFirstRequested()
  signal checkLastRequested()
  signal clearRequested()
  signal composeRequested()
  signal closeRequested()
  signal settingsToggled()
  signal settingChanged(string name, var value)
  signal engineInstallRequested(string slug)
  signal engineCancelRequested()
  signal engineRemoveRequested(string slug)
  signal engineRemoveConfirmed(string slug)
  signal engineKeepRequested()
  signal errorActionRequested(string action)
  signal setupActionRequested(string action)

  readonly property color acceptedColor: marked.acceptedColor

  readonly property int issueCount: issues ? issues.length : 0
  readonly property int acceptedCount: root.countOf(true)
  readonly property int skippedCount: root.countOf(false)
  readonly property int openCount: root.issueCount - root.acceptedCount - root.skippedCount
  readonly property bool hasIssues: root.phase === "result" && root.issueCount > 0
  readonly property bool hasError: root.phase === "error" && Boolean(root.errorCard)
  readonly property bool showsCheck: !root.settingsOpen
  readonly property bool isEmptyResult: root.phase === "result" && root.issueCount === 0
  readonly property bool isNothingNew: root.phase === "empty"
  readonly property bool hasLastCapture: root.lastCapturedText.length > 0
  readonly property bool replaces: root.autoReplace && root.runCaptured
  readonly property var focusedIssue: root.hasIssues && root.focusIndex >= 0 && root.focusIndex < root.issueCount
    ? root.issues[root.focusIndex] : null

  readonly property int chromeHeight: hero.implicitHeight + tail.implicitHeight
    + layout.spacing * 2 + root.contentTopInset + root.contentBottomInset
  readonly property int textHeight: Math.max(Style.space(96),
    Math.min(marked.contentHeight, root.maxCardHeight - root.chromeHeight))

  function countOf(value) {
    var total = 0
    for (var i = 0; i < root.issueCount; i++) if (root.decisions && root.decisions[i] === value) total++
    return total
  }

  function grouped(count) {
    return Format.grouped(count)
  }

  function units(count) {
    return Format.units(count)
  }

  function metaLine() {
    if (root.phase === "checking") return "checking the selection"
    if (root.hasError) return String(root.errorCard.meta)
    if (root.phase === "setup") return "companion tool missing"
    if (root.phase === "notice") return root.noticeMeta
    if (root.phase === "toolong") return "selection over the limit"
    if (root.isNothingNew) return "nothing new to check"
    var run = root.engine + ", " + root.elapsedMs + " ms"
    if (root.issueCount === 0) return "no issues, " + run
    return root.issueCount + (root.issueCount === 1 ? " issue, " : " issues, ")
      + root.acceptedCount + " accepted, " + run
  }

  function applyLabel() {
    if (root.replaces) return root.applied ? "Replaced" : "Replace selection"
    return root.applied ? "Copied" : "Copy corrected text"
  }

  color: Color.popups.background
  radius: Style.cornerRadius
  padding: Style.spacing.popupPadding
  borderSpec: Border.localOrSurfaceSpec("popups", "border", Color.popups.border, Color.popups.border, Math.max(1, Style.space(2)))

  implicitWidth: root.cardWidth
  implicitHeight: layout.implicitHeight + root.contentTopInset + root.contentBottomInset

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
      id: hero

      Layout.fillWidth: true

      metaText: root.metaLine()
      noteText: root.truncated
        ? Format.truncatedNote(root.sourceText.length, root.fullText.length)
        : ""
      showsAutoReplace: true
      autoReplace: root.autoReplace
      settingsOpen: root.settingsOpen
      actions: root.showsCheck && root.phase !== "toolong"
        ? (root.isNothingNew
          ? [{ id: "compose", text: "Compose", tooltip: "Open the selection in Compose", primary: false }]
          : [{ id: "clear", text: "Clear", tooltip: "Ctrl + L", primary: false },
             { id: "compose", text: "Compose", tooltip: "Open the selection in Compose", primary: false }])
        : []
      onAutoReplaceToggled: root.autoReplaceToggled()
      onSettingsToggled: root.settingsToggled()
      onActionRequested: function(id) {
        if (id === "clear") root.clearRequested()
        else root.composeRequested()
      }
    }

    SettingsView {
      Layout.fillWidth: true
      Layout.topMargin: Style.spacing.md
      Layout.bottomMargin: Style.spacing.md
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

    MarkedText {
      id: marked

      Layout.fillWidth: true
      Layout.preferredHeight: root.textHeight
      visible: root.showsCheck && root.hasIssues

      sourceText: root.sourceText
      issues: root.issues
      decisions: root.decisions
      focusIndex: root.focusIndex
      onMarkActivated: function(index) { root.focusRequested(index) }
    }

    ColumnLayout {
      id: tail

      Layout.fillWidth: true
      spacing: Style.spacing.lg

      Text {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.lg
        Layout.bottomMargin: Style.spacing.lg
        visible: root.showsCheck && root.phase === "checking"
        text: "Checking the selection..."
        color: Color.muted
        font.family: Style.font.family
        font.pixelSize: Style.font.body
      }

      ColumnLayout {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.md
        Layout.bottomMargin: Style.spacing.md
        visible: root.showsCheck && root.phase === "notice"
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
      }

      ErrorCard {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.md
        visible: root.showsCheck && root.hasError

        card: root.errorCard
        diagnosis: root.diagnosis
        onActionRequested: function(action) { root.errorActionRequested(action) }
      }

      SetupCard {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.md
        visible: root.showsCheck && root.phase === "setup"

        card: root.setupCard
        onActionRequested: function(action) { root.setupActionRequested(action) }
      }

      ColumnLayout {
        Layout.fillWidth: true
        Layout.maximumWidth: layout.width
        Layout.topMargin: Style.spacing.lg
        Layout.bottomMargin: Style.spacing.lg
        visible: root.showsCheck && root.isNothingNew
        spacing: Style.spacing.md

        Text {
          Layout.fillWidth: true
          horizontalAlignment: Text.AlignHCenter
          text: "󰅍"
          color: Color.muted
          font.family: Style.font.family
          font.pixelSize: Style.font.displayLarge
        }

        Text {
          Layout.fillWidth: true
          horizontalAlignment: Text.AlignHCenter
          wrapMode: Text.Wrap
          textFormat: Text.PlainText
          text: Capture.NOTHING_NEW
          color: Color.popups.text
          font.family: Style.font.family
          font.pixelSize: Style.font.body
        }
      }

      ColumnLayout {
        Layout.fillWidth: true
        Layout.maximumWidth: layout.width
        Layout.topMargin: Style.spacing.lg
        Layout.bottomMargin: Style.spacing.lg
        visible: root.showsCheck && root.isEmptyResult
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
          text: root.sourceText.length + " characters checked, " + root.engine + ", " + root.elapsedMs + " ms"
          color: Color.muted
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
        }
      }

      ColumnLayout {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.md
        Layout.bottomMargin: Style.spacing.md
        visible: root.showsCheck && root.phase === "toolong"
        spacing: Style.spacing.md

        Text {
          Layout.fillWidth: true
          text: "The selection is too long for one check"
          color: Color.urgent
          wrapMode: Text.Wrap
          font.family: Style.font.family
          font.pixelSize: Style.font.title
          font.bold: true
        }

        Item {
          id: sizeBar

          Layout.fillWidth: true
          implicitHeight: Style.space(10)

          readonly property int selectedUnits: Math.max(1, root.fullText.length)
          readonly property real share: Math.max(0, Math.min(1, root.limitUnits / sizeBar.selectedUnits))

          Rectangle {
            anchors.fill: parent
            radius: Style.cornerRadius > 0 ? height / 2 : 0
            color: Style.normalFill
            border.width: Math.max(1, Style.normalBorderWidth)
            border.color: Style.normalBorderColor
          }

          Rectangle {
            width: Math.round(sizeBar.width * sizeBar.share)
            height: parent.height
            radius: Style.cornerRadius > 0 ? height / 2 : 0
            color: Color.accent
          }
        }

        RowLayout {
          Layout.fillWidth: true
          spacing: Style.spacing.lg

          Text {
            textFormat: Text.PlainText
            text: root.units(root.limitUnits) + " per check"
            color: Color.accent
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }

          Item { Layout.fillWidth: true }

          Text {
            textFormat: Text.PlainText
            text: root.units(root.fullText.length) + " selected"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }

        Text {
          Layout.fillWidth: true
          text: "Check the first part now, or open the whole selection in Compose, which checks it in chunks."
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

      Inspector {
        Layout.fillWidth: true
        visible: root.showsCheck && root.hasIssues

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
          visible: root.showsCheck && root.hasIssues
          acceptedCount: root.acceptedCount
          skippedCount: root.skippedCount
          openCount: root.openCount
        }

        Item { Layout.fillWidth: true }

        Button {
          visible: root.showsCheck && !root.hasIssues && !root.hasError && root.phase !== "setup"
          text: "Close"
          tooltipText: "Esc"
          bordered: true
          foreground: Color.popups.text
          fontFamily: Style.font.family
          onClicked: root.closeRequested()
        }

        Button {
          visible: root.showsCheck && root.isNothingNew && root.hasLastCapture
          text: Capture.CHECK_LAST_AGAIN
          tooltipText: "Runs the check again on the text it captured last"
          bordered: true
          foreground: Color.popups.text
          fontFamily: Style.font.family
          onClicked: root.checkLastRequested()
        }

        Button {
          visible: root.showsCheck && root.phase === "toolong"
          text: "Check the first " + root.grouped(root.limitUnits) + " only"
          bordered: true
          foreground: Color.popups.text
          fontFamily: Style.font.family
          onClicked: root.checkFirstRequested()
        }

        Button {
          visible: root.showsCheck && root.phase === "toolong"
          text: "Open in Compose"
          bordered: true
          foreground: Color.accent
          fontFamily: Style.font.family
          onClicked: root.composeRequested()
        }

        Button {
          visible: root.showsCheck && root.hasIssues
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
          visible: root.showsCheck && root.hasIssues
          enabled: root.acceptedCount > 0 && !root.applied
          opacity: enabled ? 1.0 : 0.4
          text: root.applyLabel()
          tooltipText: root.replaces ? "Ctrl + Enter, or Ctrl + C to copy only" : "Ctrl + Enter"
          bordered: true
          foreground: root.applied ? Color.popups.text : Color.accent
          fontFamily: Style.font.family
          onClicked: root.applyRequested()
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
}
