import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui

// The quick popup card, spec section 6, laid out after variant B of the
// HUF-174 prototype: hero, marked text, inspector strip, footer.
//
// The gear in the hero flips the body to the Settings view of spec section 7
// and back, so the check view itself stays clean. The check state lives on
// behind it: flipping back shows the same Issues with the same decisions.
//
// The card renders state and reports intent. Capture, the Check, the key map,
// the clipboard, and the settings storage all live in Overlay.qml.
BorderSurface {
  id: root

  // "checking", "result", "notice", or "toolong".
  property string phase: "checking"
  // The exact text the Check ran on. Every Issue span indexes into it.
  property string sourceText: ""
  // The whole capture, which is longer than sourceText after a first-N Check.
  property string fullText: ""
  property bool truncated: false
  // One Check takes this many UTF-16 code units, spec section 6.
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
  // What the CLI said, shown in monospace under the too-long body, spec 8.
  property string engineMessage: ""

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

  // MarkedText owns the accepted green, so the inspector and the empty state
  // read it from there rather than repeating the literal.
  readonly property color acceptedColor: marked.acceptedColor

  readonly property int issueCount: issues ? issues.length : 0
  readonly property int acceptedCount: root.countOf(true)
  readonly property int skippedCount: root.countOf(false)
  readonly property int openCount: root.issueCount - root.acceptedCount - root.skippedCount
  readonly property bool hasIssues: root.phase === "result" && root.issueCount > 0
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

  // Thousands separators, because the too-long card is about a size the
  // reader has to compare at a glance.
  function grouped(count) {
    return String(count).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
  }

  function units(count) {
    return root.grouped(count) + (count === 1 ? " unit" : " units")
  }

  function metaLine() {
    if (root.phase === "checking") return "checking the selection"
    if (root.phase === "notice") return "check did not finish"
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

    // ------------------------------------------------------------------ hero

    ColumnLayout {
      id: hero

      Layout.fillWidth: true
      spacing: Style.spacing.lg

      RowLayout {
        Layout.fillWidth: true
        spacing: Style.spacing.xxl

        Text {
          text: "G"
          color: Color.accent
          font.family: Style.font.family
          font.pixelSize: Style.font.heading
          font.bold: true
        }

        ColumnLayout {
          Layout.fillWidth: true
          spacing: Style.spacing.xxs

          Text {
            text: "Grammachy"
            color: Color.popups.text
            font.family: Style.font.family
            font.pixelSize: Style.font.title
            font.bold: true
          }

          Text {
            Layout.fillWidth: true
            text: root.metaLine()
            color: Color.muted
            elide: Text.ElideRight
            font.family: Style.font.family
            font.pixelSize: Style.font.bodySmall
          }

          // A first-N Check saw part of the Selection, which the reader has to
          // know before they trust the counts above.
          Text {
            Layout.fillWidth: true
            visible: root.truncated
            text: "First " + root.grouped(root.limitUnits) + " of " + root.units(root.fullText.length) + " checked"
            color: Color.accent
            elide: Text.ElideRight
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }

        // The auto-replace toggle, spec sections 6 and 7. The Settings view
        // persists it; the hero holds a session-local override beside that.
        RowLayout {
          Layout.alignment: Qt.AlignVCenter
          spacing: Style.spacing.lg

          Text {
            text: "Auto-replace"
            color: Color.popups.text
            font.family: Style.font.family
            font.pixelSize: Style.font.bodySmall
          }

          ToggleSwitch {
            checked: root.autoReplace
            foreground: Color.popups.text
            onToggled: root.autoReplaceToggled()
          }
        }

        // The one control that flips the body, spec section 7. It sits in the
        // hero so it is reachable from every phase, the notice card included,
        // and it stays a gear both ways so the hero never shifts under a click.
        Button {
          Layout.alignment: Qt.AlignVCenter
          iconText: "󰒓"
          tooltipText: root.settingsOpen ? "Back to the check" : "Settings"
          bordered: true
          selected: root.settingsOpen
          foreground: Color.popups.text
          fontFamily: Style.font.family
          onClicked: root.settingsToggled()
        }
      }

      // The hint spans the hero rather than riding beside the switch, so it
      // stays one line at any theme font and never squeezes the meta line.
      Text {
        Layout.fillWidth: true
        Layout.topMargin: -Style.spacing.sm
        text: "Replaces the highlighted text by pasting over it"
        color: Color.muted
        elide: Text.ElideRight
        horizontalAlignment: Text.AlignRight
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
      }

      Rectangle {
        Layout.fillWidth: true
        implicitHeight: Style.spacing.hairline
        color: Style.normalBorderColor
      }
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

      // ------------------------------------------------------- empty state

      ColumnLayout {
        Layout.fillWidth: true
        Layout.topMargin: Style.spacing.lg
        Layout.bottomMargin: Style.spacing.lg
        visible: root.showsCheck && root.isEmptyResult
        spacing: Style.spacing.sm

        Text {
          Layout.alignment: Qt.AlignHCenter
          text: "✓"
          color: root.acceptedColor
          font.family: Style.font.family
          font.pixelSize: Style.font.displayLarge
        }

        Text {
          Layout.alignment: Qt.AlignHCenter
          text: "No issues found"
          color: Color.popups.text
          font.family: Style.font.family
          font.pixelSize: Style.font.title
          font.bold: true
        }

        Text {
          Layout.alignment: Qt.AlignHCenter
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

      BorderSurface {
        Layout.fillWidth: true
        visible: root.showsCheck && root.hasIssues
        color: "transparent"
        radius: Style.cornerRadius
        padding: Style.spacing.lg
        borderSpec: Border.controlSpec("normal", Color.popups.text, Color.accent)
        implicitHeight: inspector.implicitHeight + topPadding + bottomPadding + borderTop + borderBottom

        RowLayout {
          id: inspector

          anchors.fill: parent
          anchors.margins: Style.spacing.lg
          spacing: Style.spacing.xxl

          ColumnLayout {
            Layout.fillWidth: true
            spacing: Style.spacing.sm

            RowLayout {
              Layout.fillWidth: true
              spacing: Style.spacing.lg

              Text {
                text: root.focusedIssue ? root.focusedIssue.original : ""
                color: Color.urgent
                font.family: Style.font.family
                font.pixelSize: Style.font.subtitle
                font.strikeout: true
              }

              Text {
                text: "→"
                color: Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.subtitle
              }

              Text {
                text: root.focusedIssue ? root.focusedIssue.fix : ""
                color: root.acceptedColor
                font.family: Style.font.family
                font.pixelSize: Style.font.subtitle
                font.bold: true
              }

              BorderSurface {
                Layout.alignment: Qt.AlignVCenter
                color: "transparent"
                radius: Style.cornerRadius
                padding: Style.spacing.xs
                borderSpec: Border.controlSpec("normal", Color.popups.text, Color.accent)
                implicitWidth: category.implicitWidth + padding * 2 + borderLeft + borderRight
                implicitHeight: category.implicitHeight + padding * 2 + borderTop + borderBottom

                Text {
                  id: category
                  anchors.centerIn: parent
                  text: root.focusedIssue ? root.focusedIssue.category : ""
                  color: Color.muted
                  font.family: Style.font.family
                  font.pixelSize: Style.font.caption
                }
              }

              Item { Layout.fillWidth: true }
            }

            Text {
              Layout.fillWidth: true
              text: root.focusedIssue ? root.focusedIssue.reason : ""
              color: Color.muted
              wrapMode: Text.Wrap
              font.family: Style.font.family
              font.pixelSize: Style.font.bodySmall
            }

            Text {
              Layout.fillWidth: true
              text: "Issue " + (root.focusIndex + 1) + " of " + root.issueCount
                + ". Enter accepts, Space skips, Up and Down move, A accepts all."
              color: Color.muted
              wrapMode: Text.Wrap
              font.family: Style.font.family
              font.pixelSize: Style.font.caption
            }
          }

          RowLayout {
            Layout.alignment: Qt.AlignTop
            spacing: Style.spacing.lg

            Button {
              text: "Accept"
              tooltipText: "Enter"
              bordered: true
              foreground: Color.popups.text
              fontFamily: Style.font.family
              onClicked: root.accepted(root.focusIndex)
            }

            Button {
              text: "Skip"
              tooltipText: "Space"
              bordered: true
              foreground: Color.popups.text
              fontFamily: Style.font.family
              onClicked: root.skipped(root.focusIndex)
            }
          }
        }
      }

      // ---------------------------------------------------------- the footer

      RowLayout {
        Layout.fillWidth: true
        spacing: Style.spacing.lg

        Row {
          visible: root.showsCheck && root.hasIssues
          spacing: Style.spacing.xxl

          Text {
            text: root.acceptedCount + " accepted"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.bodySmall
          }

          Text {
            text: root.skippedCount + " skipped"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.bodySmall
          }

          Text {
            text: root.openCount + " open"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.bodySmall
          }
        }

        Item { Layout.fillWidth: true }

        Button {
          visible: root.showsCheck && !root.hasIssues
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
