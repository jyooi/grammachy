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
// The card renders state and reports intent. Capture, the Check, the
// clipboard, and the settings storage all live in Overlay.qml.
BorderSurface {
  id: root

  // "checking", "result", or "notice".
  property string phase: "checking"
  property string sourceText: ""
  property var issues: []
  // One entry per Issue: true accepted, false skipped, null still open.
  property var decisions: []
  property int focusIndex: 0
  property string engine: ""
  property int elapsedMs: 0
  property bool copied: false
  property string noticeTitle: ""
  property string noticeBody: ""

  // The Settings view, spec section 7. The values arrive already resolved
  // through the defaults, so an unknown stored value shows the default here.
  // `engineSetting` is the stored choice; `engine` above is what ran the
  // Check that is on screen, which the meta line names.
  property bool settingsOpen: false
  property string nativeLanguage: "none"
  property string engineSetting: "languagetool"
  property bool autoReplace: false
  property string openaiBaseUrl: ""
  property string openaiModel: ""

  property int cardWidth: Style.space(680)
  property int maxTextHeight: Style.space(360)

  signal accepted(int index)
  signal skipped(int index)
  signal acceptAllRequested()
  signal copyRequested()
  signal focusRequested(int index)
  signal closeRequested()
  signal settingsToggled()
  signal settingChanged(string name, var value)

  readonly property int issueCount: issues ? issues.length : 0
  readonly property int acceptedCount: root.countOf(true)
  readonly property int skippedCount: root.countOf(false)
  readonly property int openCount: root.issueCount - root.acceptedCount - root.skippedCount
  readonly property bool hasIssues: root.phase === "result" && root.issueCount > 0
  // The Settings view takes the body over; every check row hangs off this.
  readonly property bool showsCheck: !root.settingsOpen
  readonly property var focusedIssue: root.hasIssues && root.focusIndex >= 0 && root.focusIndex < root.issueCount
    ? root.issues[root.focusIndex] : null

  function countOf(value) {
    var total = 0
    for (var i = 0; i < root.issueCount; i++) if (root.decisions && root.decisions[i] === value) total++
    return total
  }

  function metaLine() {
    if (root.phase === "checking") return "checking the selection"
    if (root.phase === "notice") return "check did not finish"
    if (root.issueCount === 0) return "no issues, " + root.engine + ", " + root.elapsedMs + " ms"
    return root.issueCount + (root.issueCount === 1 ? " issue, " : " issues, ")
      + root.acceptedCount + " accepted, " + root.engine + ", " + root.elapsedMs + " ms"
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

    Rectangle {
      Layout.fillWidth: true
      implicitHeight: Style.spacing.hairline
      color: Style.normalBorderColor
    }

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

    ColumnLayout {
      Layout.fillWidth: true
      Layout.topMargin: Style.spacing.lg
      Layout.bottomMargin: Style.spacing.lg
      visible: root.showsCheck && root.phase === "result" && root.issueCount === 0
      spacing: Style.spacing.sm

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

    MarkedText {
      id: marked

      Layout.fillWidth: true
      Layout.preferredHeight: Math.min(marked.contentHeight, root.maxTextHeight)
      visible: root.showsCheck && root.hasIssues

      sourceText: root.sourceText
      issues: root.issues
      decisions: root.decisions
      focusIndex: root.focusIndex
      onMarkActivated: function(index) { root.focusRequested(index) }
    }

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
              color: marked.acceptedColor
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
            text: "Issue " + (root.focusIndex + 1) + " of " + root.issueCount + ". Click a mark to move."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
          }
        }

        RowLayout {
          Layout.alignment: Qt.AlignTop
          spacing: Style.spacing.lg

          Button {
            text: "Accept"
            bordered: true
            foreground: Color.popups.text
            fontFamily: Style.font.family
            onClicked: root.accepted(root.focusIndex)
          }

          Button {
            text: "Skip"
            bordered: true
            foreground: Color.popups.text
            fontFamily: Style.font.family
            onClicked: root.skipped(root.focusIndex)
          }
        }
      }
    }

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
        visible: root.showsCheck && root.hasIssues
        enabled: root.openCount > 0
        opacity: enabled ? 1.0 : 0.4
        text: "Accept all open"
        bordered: true
        foreground: Color.popups.text
        fontFamily: Style.font.family
        onClicked: root.acceptAllRequested()
      }

      Button {
        visible: root.showsCheck && root.hasIssues
        // Spec 6: Apply stays off until the user accepts one Fix, because
        // copying the Selection back unchanged is never what they asked for.
        enabled: root.acceptedCount > 0 && !root.copied
        opacity: enabled ? 1.0 : 0.4
        text: root.copied ? "Copied" : "Copy corrected text"
        bordered: true
        foreground: root.copied ? Color.popups.text : Color.accent
        fontFamily: Style.font.family
        onClicked: root.copyRequested()
      }

      Button {
        visible: root.showsCheck && !root.hasIssues
        text: "Close"
        bordered: true
        foreground: Color.popups.text
        fontFamily: Style.font.family
        onClicked: root.closeRequested()
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
