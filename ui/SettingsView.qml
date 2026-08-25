import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "settings.js" as Settings

// The Settings view of the quick popup, spec section 7. The gear in the hero
// flips the card to it and back.
//
// The view draws the stored Settings and reports one changed key at a time.
// Storage lives in Overlay.qml, which is the only thing that touches
// `shell.json`. There is no Save button: every control persists on change.
ColumnLayout {
  id: root

  // The stored values, already resolved through the spec section 7 defaults.
  property string nativeLanguage: "none"
  property string engine: "languagetool"
  property bool autoReplace: false
  property string openaiBaseUrl: "http://127.0.0.1:8080"
  property string openaiModel: "gemma-4-e4b-it"

  signal settingChanged(string name, var value)

  readonly property bool showsOpenai: root.engine === "openai"

  // How long a text field waits after the last keystroke before it persists.
  readonly property int commitDelayMs: 500

  // Keep on screen exactly what the user is typing, but never store a value
  // the CLI would ignore: an emptied field stores the default.
  function commit(name, field, timer) {
    timer.stop()
    root.settingChanged(name, Settings.normalised(name, field.text))
  }

  // The edit is over, so the field can settle on what was actually stored.
  function commitAndSettle(name, field, timer) {
    root.commit(name, field, timer)
    field.text = Settings.normalised(name, field.text)
  }

  Timer {
    id: baseUrlTimer
    interval: root.commitDelayMs
    onTriggered: root.commit("openaiBaseUrl", baseUrlField, baseUrlTimer)
  }

  Timer {
    id: modelTimer
    interval: root.commitDelayMs
    onTriggered: root.commit("openaiModel", modelField, modelTimer)
  }

  // Dropdown writes its own `value` when the user picks a row, which drops the
  // declarative binding. Re-asserting it on every change of the stored value
  // is what keeps the view live for a write from outside, such as
  // `omarchy-shell shell setBarWidget <id> engine '"harper"'`.
  onNativeLanguageChanged: nativeLanguageDropdown.value = root.nativeLanguage
  onEngineChanged: engineDropdown.value = root.engine
  // A text field the user is typing in must not be yanked from under them, so
  // an outside write lands only while the field is idle.
  onOpenaiBaseUrlChanged: if (!baseUrlField.activeFocus) baseUrlField.text = root.openaiBaseUrl
  onOpenaiModelChanged: if (!modelField.activeFocus) modelField.text = root.openaiModel

  spacing: Style.spacing.lg

  Text {
    Layout.fillWidth: true
    text: "Every change is kept at once and applies to the next check."
    color: Color.muted
    wrapMode: Text.Wrap
    font.family: Style.font.family
    font.pixelSize: Style.font.bodySmall
  }

  RowLayout {
    Layout.fillWidth: true
    spacing: Style.spacing.xxl

    Dropdown {
      id: nativeLanguageDropdown

      Layout.fillWidth: true
      label: "Native language"
      options: Settings.NATIVE_LANGUAGE_OPTIONS
      value: root.nativeLanguage
      foreground: Color.popups.text
      background: Color.popups.background
      onChanged: function(value) { root.settingChanged("nativeLanguage", Settings.normalised("nativeLanguage", value)) }
    }

    Dropdown {
      id: engineDropdown

      Layout.fillWidth: true
      label: "Engine"
      options: Settings.ENGINE_OPTIONS
      value: root.engine
      foreground: Color.popups.text
      background: Color.popups.background
      onChanged: function(value) { root.settingChanged("engine", Settings.normalised("engine", value)) }
    }
  }

  // Spec section 6 words the hint, because Replace only reaches a Selection
  // that is still highlighted in the window it came from.
  Toggle {
    Layout.fillWidth: true
    label: "Auto-replace"
    description: "Replaces the highlighted text by pasting over it"
    checked: root.autoReplace
    foreground: Color.popups.text
    onClicked: root.settingChanged("autoReplace", !root.autoReplace)
  }

  // Spec section 7 shows the two OpenAI fields for the Local LLM engine only.
  //
  // A text field persists on a short pause rather than on every keystroke,
  // because a keystroke would rewrite shell.json that many times. `textEdited`
  // is the user's own typing only, so the re-assert above never arms the
  // timer. Waiting for the edit to finish instead would lose what the user
  // typed when Esc closes the popup with the caret still in the field.
  ColumnLayout {
    Layout.fillWidth: true
    visible: root.showsOpenai
    spacing: Style.spacing.labelGap

    Text {
      text: "Local LLM server"
      color: Qt.darker(Color.popups.text, 1.4)
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
      font.bold: true
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.spacing.xxl

      TextField {
        id: baseUrlField

        Layout.fillWidth: true
        text: root.openaiBaseUrl
        placeholderText: Settings.defaultOf("openaiBaseUrl")
        foreground: Color.popups.text
        onTextEdited: baseUrlTimer.restart()
        onEditingFinished: root.commitAndSettle("openaiBaseUrl", baseUrlField, baseUrlTimer)
      }

      TextField {
        id: modelField

        Layout.fillWidth: true
        text: root.openaiModel
        placeholderText: Settings.defaultOf("openaiModel")
        foreground: Color.popups.text
        onTextEdited: modelTimer.restart()
        onEditingFinished: root.commitAndSettle("openaiModel", modelField, modelTimer)
      }
    }

    Text {
      Layout.fillWidth: true
      text: "The base URL must stay on this machine. The API key and the English variant are file only."
      color: Color.muted
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
  }
}
