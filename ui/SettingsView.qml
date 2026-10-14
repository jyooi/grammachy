import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "engines.js" as EnginesJs
import "settings.js" as Settings

ColumnLayout {
  id: root

  property string nativeLanguage: "none"
  property string targetEnglish: "en-US"
  property string engine: "harper"
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

  signal settingChanged(string name, var value)
  signal engineInstallRequested(string slug)
  signal engineCancelRequested()
  signal engineRemoveRequested(string slug)
  signal engineRemoveConfirmed(string slug)
  signal engineKeepRequested()

  onNativeLanguageChanged: nativeLanguageDropdown.value = root.nativeLanguage
  onTargetEnglishChanged: targetEnglishDropdown.value = root.targetEnglish
  onEngineChanged: engineDropdown.value = root.engine
  onEnginesChanged: engineDropdown.value = root.engine
  onQuickHotkeyChanged: quickHotkeyField.text = root.quickHotkey
  onComposeHotkeyChanged: composeHotkeyField.text = root.composeHotkey

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
      id: targetEnglishDropdown

      Layout.fillWidth: true
      label: "Target English"
      options: Settings.TARGET_ENGLISH_OPTIONS
      value: root.targetEnglish
      foreground: Color.popups.text
      background: Color.popups.background
      onChanged: function(value) { root.settingChanged("targetEnglish", Settings.normalised("targetEnglish", value)) }
    }

    Dropdown {
      id: engineDropdown

      Layout.fillWidth: true
      label: "Engine"
      options: Settings.engineOptions(EnginesJs.unavailable(root.engines), root.engine)
      value: root.engine
      foreground: Color.popups.text
      background: Color.popups.background
      onChanged: function(value) { root.settingChanged("engine", Settings.normalised("engine", value)) }
    }
  }

  RowLayout {
    Layout.fillWidth: true
    spacing: Style.spacing.xxl

    ColumnLayout {
      Layout.fillWidth: true
      spacing: Style.spacing.labelGap

      Text {
        text: "Quick popup hotkey"
        color: Qt.darker(Color.popups.text, 1.4)
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
        font.bold: true
      }

      TextField {
        id: quickHotkeyField

        Layout.fillWidth: true
        text: root.quickHotkey
        foreground: Color.popups.text
        onEditingFinished: {
          var value = Settings.normalised("quickHotkey", quickHotkeyField.text)
          if (value !== root.quickHotkey) root.settingChanged("quickHotkey", value)
          quickHotkeyField.text = value
        }
      }
    }

    ColumnLayout {
      Layout.fillWidth: true
      spacing: Style.spacing.labelGap

      Text {
        text: "Compose hotkey"
        color: Qt.darker(Color.popups.text, 1.4)
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
        font.bold: true
      }

      TextField {
        id: composeHotkeyField

        Layout.fillWidth: true
        text: root.composeHotkey
        foreground: Color.popups.text
        onEditingFinished: {
          var value = Settings.normalised("composeHotkey", composeHotkeyField.text)
          if (value !== root.composeHotkey) root.settingChanged("composeHotkey", value)
          composeHotkeyField.text = value
        }
      }
    }
  }

  Toggle {
    Layout.fillWidth: true
    label: "Auto-replace"
    description: "Replaces the highlighted text by pasting over it"
    checked: root.autoReplace
    foreground: Color.popups.text
    onClicked: root.settingChanged("autoReplace", !root.autoReplace)
  }

  EnginesView {
    Layout.fillWidth: true

    engines: root.engines
    busy: root.engineBusy
    busyBytes: root.engineBusyBytes
    working: root.enginesBusy
    confirmSlug: root.engineConfirm
    selected: root.engine
    directory: root.enginesDirectory
    freeBytes: root.enginesFreeBytes
    note: root.engineNote
    dependencies: root.dependencies

    onInstall: function(slug) { root.engineInstallRequested(slug) }
    onCancel: root.engineCancelRequested()
    onRemove: function(slug) { root.engineRemoveRequested(slug) }
    onConfirmRemove: function(slug) { root.engineRemoveConfirmed(slug) }
    onKeepEngine: root.engineKeepRequested()
  }
}
