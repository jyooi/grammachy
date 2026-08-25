import QtQuick
import qs.Ui

// The bar button, spec section 2. It carries no popup of its own: the shell
// routes `summon` to the overlay because the manifest declares a second kind,
// so the popup QML lives in Overlay.qml.
BarWidget {
  id: root

  moduleName: "io.github.jyooi.grammachy"

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  WidgetButton {
    id: button

    anchors.fill: parent
    bar: root.bar
    text: "G"
    tooltipText: "Grammachy: check the selected text"

    onPressed: function(mouseButton) {
      if (mouseButton !== Qt.LeftButton) return
      if (!root.bar || !root.bar.shell) return
      root.bar.shell.summon(root.moduleName, JSON.stringify({ mode: "quick" }))
    }
  }
}
