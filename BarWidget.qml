import QtQuick
import qs.Ui

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
