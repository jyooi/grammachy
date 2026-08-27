import QtQuick
import QtQuick.Layouts
import qs.Commons
import qs.Ui
import "engines.js" as EnginesJs
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
  property string engine: "harper"
  property bool autoReplace: false

  // The Engines list of spec section 5.4. Everything it needs arrives from
  // Overlay.qml, which is the only thing that runs `grammachy engine`.
  property var engines: []
  property string engineBusy: ""
  property double engineBusyBytes: 0
  property bool enginesBusy: false
  property string engineConfirm: ""
  property string enginesDirectory: ""
  property double enginesFreeBytes: 0
  property var engineNote: null

  signal settingChanged(string name, var value)
  signal engineInstallRequested(string slug)
  signal engineCancelRequested()
  signal engineRemoveRequested(string slug)
  signal engineRemoveConfirmed(string slug)
  signal engineKeepRequested()

  // Dropdown writes its own `value` when the user picks a row, which drops the
  // declarative binding. Re-asserting it on every change of the stored value
  // is what keeps the view live for a write from outside, such as
  // `omarchy-shell shell setBarWidget <id> engine '"harper"'`.
  onNativeLanguageChanged: nativeLanguageDropdown.value = root.nativeLanguage
  onEngineChanged: engineDropdown.value = root.engine
  // The option list narrows as the Engines list lands, and a Dropdown that
  // has written its own `value` once no longer follows the binding, so the
  // value is re-asserted when the rows move as well as when the setting does.
  onEnginesChanged: engineDropdown.value = root.engine

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

    // Spec section 7 and HUF-237: only an engine this machine has is offered.
    // The Engines list below is where a missing one is added.
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

  // Spec section 5.4: the optional engine components this machine keeps. It is
  // drawn whatever engine is selected, because the whole point is to add one
  // the dropdown above cannot offer yet.
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

    onInstall: function(slug) { root.engineInstallRequested(slug) }
    onCancel: root.engineCancelRequested()
    onRemove: function(slug) { root.engineRemoveRequested(slug) }
    onConfirmRemove: function(slug) { root.engineRemoveConfirmed(slug) }
    onKeepEngine: root.engineKeepRequested()
  }
}
