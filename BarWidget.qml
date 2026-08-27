import QtQuick
import qs.Ui
import "ui/settings.js" as Settings

// The bar button, spec section 2. It carries no popup of its own: the shell
// routes `summon` to the overlay because the manifest declares a second kind,
// so the popup QML lives in Overlay.qml.
//
// The one thing it draws beyond the `G` is the cloud glyph of
// `docs/spec/evals.md` section 7: the cloud engine is the one engine that
// sends text off this machine, so the bar says so wherever the reader is.
BarWidget {
  id: root

  moduleName: "io.github.jyooi.grammachy"

  // The stored engine. `settings` is this widget's own inline entry, which the
  // bar host re-assigns on every write, so the Settings dropdown and a hand
  // edit of `shell.json` both move the glyph with no reload. The rules of spec
  // section 7 are read through `ui/settings.js`, the same file Overlay.qml
  // reads them through, so an unknown stored engine reads as the default here
  // too.
  readonly property bool cloudEngine: Settings.valueOf(root.settings, "engine") === Settings.CLOUD_ENGINE

  // U+F0167, the Material Design cloud-upload glyph of the Nerd Font the bar
  // draws with. It is the direction that matters: text goes out.
  readonly property string cloudGlyph: "󰅧"

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  WidgetButton {
    id: button

    anchors.fill: parent
    bar: root.bar
    text: root.cloudEngine ? "G " + root.cloudGlyph : "G"
    tooltipText: root.cloudEngine
      ? "Grammachy: cloud engine, text is sent to OpenRouter"
      : "Grammachy: check the selected text"

    onPressed: function(mouseButton) {
      if (mouseButton !== Qt.LeftButton) return
      if (!root.bar || !root.bar.shell) return
      root.bar.shell.summon(root.moduleName, JSON.stringify({ mode: "quick" }))
    }
  }
}
