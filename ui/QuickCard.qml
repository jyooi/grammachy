import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "format.js" as Format

// The quick popup card, spec section 6, laid out after variant B of the
// HUF-174 prototype: hero, marked text, inspector strip, footer.
//
// The gear in the hero flips the body to the Settings view of spec section 7
// and back, so the check view itself stays clean. The check state lives on
// behind it: flipping back shows the same Issues with the same decisions.
//
// A failed Check swaps the body for one of the error cards of spec section 8,
// which `ui/ErrorCard.qml` draws from a model that `ui/errors.js` owns. The
// too-long card of section 6 is this file's own, because its size bar and its
// handover to Compose belong to the popup rather than to that model.
//
// The card renders state and reports intent. Capture, the Check, the key map,
// the clipboard, and the settings storage all live in Overlay.qml.
BorderSurface {
  id: root

  // "checking", "result", "error", "notice", or "toolong".
  property string phase: "checking"
  // The exact text the Check ran on. Every Issue span indexes into it.
  property string sourceText: ""
  // The whole capture, which is longer than sourceText after a first-N Check.
  property string fullText: ""
  property bool truncated: false
  // One Check takes this many UTF-16 code units, spec section 6. The limit
  // belongs to the Engine, so the overlay passes the selected engine's; this
  // default is the default engine's, which `ui/limits.js` also answers.
  property int limitUnits: 5000
  property var issues: []
  // One entry per Issue: true accepted, false skipped, null still open.
  property var decisions: []
  property int focusIndex: 0
  property string engine: ""
  property int elapsedMs: 0
  // Apply has run on the Corrected text as it stands.
  property bool applied: false
  property bool autoReplace: false
  property string noticeTitle: ""
  property string noticeBody: ""
  // The hero line a notice shows, because "check did not finish" is not true
  // of a surface that is simply not built yet.
  property string noticeMeta: ""
  // What the CLI said, shown in monospace under the too-long body, spec 8.
  property string engineMessage: ""

  // The error cards of spec section 8. `errorCard` is a model from
  // `ui/errors.js` and `diagnosis` is the one-line `grammachy doctor` answer
  // that the `engine_unavailable` card shows under its body.
  property var errorCard: null
  property string diagnosis: ""

  // The Settings view, spec section 7. The values arrive already resolved
  // through the defaults, so an unknown stored value shows the default here.
  // `engineSetting` is the stored choice; `engine` above is what ran the
  // Check that is on screen, which the meta line names.
  property bool settingsOpen: false
  property string nativeLanguage: "none"
  property string engineSetting: "languagetool"
  property string openaiBaseUrl: ""
  property string openaiModel: ""

  property int cardWidth: Style.space(680)
  // The whole card fits in this, spec section 6. The marked text is what
  // gives, so a long Selection scrolls rather than pushing the card off screen.
  property int maxCardHeight: Style.space(600)

  signal accepted(int index)
  signal skipped(int index)
  signal acceptAllRequested()
  signal applyRequested()
  signal autoReplaceToggled()
  signal focusRequested(int index)
  signal checkFirstRequested()
  signal composeRequested()
  signal closeRequested()
  signal settingsToggled()
  signal settingChanged(string name, var value)
  // One button of an error card, spec section 8. The action is a button id
  // from `ui/errors.js`; Overlay.qml owns where each one goes.
  signal errorActionRequested(string action)

  // MarkedText owns the accepted green, so the inspector and the empty state
  // read it from there rather than repeating the literal.
  readonly property color acceptedColor: marked.acceptedColor

  readonly property int issueCount: issues ? issues.length : 0
  readonly property int acceptedCount: root.countOf(true)
  readonly property int skippedCount: root.countOf(false)
  readonly property int openCount: root.issueCount - root.acceptedCount - root.skippedCount
  readonly property bool hasIssues: root.phase === "result" && root.issueCount > 0
  readonly property bool hasError: root.phase === "error" && Boolean(root.errorCard)
  // The Settings view takes the body over; every check row hangs off this.
  readonly property bool showsCheck: !root.settingsOpen
  readonly property bool isEmptyResult: root.phase === "result" && root.issueCount === 0
  readonly property var focusedIssue: root.hasIssues && root.focusIndex >= 0 && root.focusIndex < root.issueCount
    ? root.issues[root.focusIndex] : null

  // The height every part but the marked text takes. Measured rather than
  // guessed, so the text region absorbs whatever the rest of the card needs.
  readonly property int chromeHeight: hero.implicitHeight + tail.implicitHeight
    + layout.spacing * 2 + root.contentTopInset + root.contentBottomInset
  readonly property int textHeight: Math.max(Style.space(96),
    Math.min(marked.contentHeight, root.maxCardHeight - root.chromeHeight))

  function countOf(value) {
    var total = 0
    for (var i = 0; i < root.issueCount; i++) if (root.decisions && root.decisions[i] === value) total++
    return total
  }

  // The counted sizes of the too-long card are worded in format.js, which
  // Compose prints from too and which a node test owns.
  function grouped(count) {
    return Format.grouped(count)
  }

  function units(count) {
    return Format.units(count)
  }

  function metaLine() {
    if (root.phase === "checking") return "checking the selection"
    if (root.hasError) return String(root.errorCard.meta)
    if (root.phase === "notice") return root.noticeMeta
    if (root.phase === "toolong") return "selection over the limit"
    var run = root.engine + ", " + root.elapsedMs + " ms"
    if (root.issueCount === 0) return "no issues, " + run
    return root.issueCount + (root.issueCount === 1 ? " issue, " : " issues, ")
      + root.acceptedCount + " accepted, " + run
  }

  function applyLabel() {
    if (root.autoReplace) return root.applied ? "Replaced" : "Replace selection"
    return root.applied ? "Copied" : "Copy corrected text"
  }

  color: Color.popups.background
  radius: Style.cornerRadius
  padding: Style.spacing.popupPadding
  borderSpec: Border.localOrSurfaceSpec("popups", "border", Color.popups.border, Color.popups.border, Math.max(1, Style.space(2)))

  implicitWidth: root.cardWidth
  implicitHeight: layout.implicitHeight + root.contentTopInset + root.contentBottomInset

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
      id: hero

      Layout.fillWidth: true

      metaText: root.metaLine()
      // A first-N Check saw part of the Selection, which the reader has to
      // know before they trust the counts above.
      noteText: root.truncated
        ? "First " + root.grouped(root.limitUnits) + " of " + root.units(root.fullText.length) + " checked"
        : ""
      showsAutoReplace: true
      autoReplace: root.autoReplace
      settingsOpen: root.settingsOpen
      // Spec sections 2 and 6: the header carries the Selection into Compose,
      // even one under the limit. The too-long card offers the same handover
      // as its primary button, so the hero stands aside there.
      actions: root.showsCheck && root.phase !== "toolong"
        ? [{ id: "compose", text: "Compose", tooltip: "Open the selection in Compose", primary: false }]
        : []
      onAutoReplaceToggled: root.autoReplaceToggled()
      onSettingsToggled: root.settingsToggled()
      onActionRequested: root.composeRequested()
    }

    SettingsView {
      Layout.fillWidth: true
      Layout.topMargin: Style.spacing.md
      Layout.bottomMargin: Style.spacing.md
      visible: root.settingsOpen

      nativeLanguage: root.nativeLanguage
      engine: root.engineSetting
      autoReplace: root.autoReplace
      openaiBaseUrl: root.openaiBaseUrl
      openaiModel: root.openaiModel
      onSettingChanged: function(name, value) { root.settingChanged(name, value) }
    }

    // ---------------------------------------------------------- marked text

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

    // ---------------------------------------- every other card, and the feet

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
      }

      // The error cards of spec section 8. They carry their own buttons, so
      // the footer below stays out of their way. No bottom margin: the empty
      // footer already leaves the column's own spacing under them.
      ErrorCard {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.md
        visible: root.showsCheck && root.hasError

        card: root.errorCard
        diagnosis: root.diagnosis
        onActionRequested: function(action) { root.errorActionRequested(action) }
      }

      // ------------------------------------------------------- empty state

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
          text: root.sourceText.length + " characters checked, " + root.engine + ", " + root.elapsedMs + " ms"
          color: Color.muted
          font.family: Style.font.family
          font.pixelSize: Style.font.bodySmall
        }
      }

      // ---------------------------------------------------- too-long card

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

        // The size bar: the whole track is what the user selected, the filled
        // part is what one Check takes. The gap is the point of the card.
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
            text: root.units(root.limitUnits) + " per check"
            color: Color.accent
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }

          Item { Layout.fillWidth: true }

          Text {
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
          text: root.engineMessage
          color: Color.muted
          wrapMode: Text.Wrap
          font.family: "monospace"
          font.pixelSize: Style.font.caption
        }
      }

      // ----------------------------------------------------- inspector strip

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

      // ---------------------------------------------------------- the footer

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
          // An error card draws its own Close, in its own button row.
          visible: root.showsCheck && !root.hasIssues && !root.hasError
          text: "Close"
          tooltipText: "Esc"
          bordered: true
          foreground: Color.popups.text
          fontFamily: Style.font.family
          onClicked: root.closeRequested()
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
          // Spec 6: Apply stays off until the user accepts one Fix, because
          // handing the Selection back unchanged is never what they asked for.
          enabled: root.acceptedCount > 0 && !root.applied
          opacity: enabled ? 1.0 : 0.4
          text: root.applyLabel()
          tooltipText: root.autoReplace ? "Ctrl + Enter, or Ctrl + C to copy only" : "Ctrl + Enter"
          bordered: true
          foreground: root.applied ? Color.popups.text : Color.accent
          fontFamily: Style.font.family
          onClicked: root.applyRequested()
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
}
