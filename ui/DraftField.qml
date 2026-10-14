import QtQuick
import qs.Commons
import qs.Ui

BorderSurface {
  id: root

  property string text: ""
  property string placeholderText: ""
  property var keySink: null

  readonly property bool focused: editor.activeFocus
  readonly property color foreground: Color.popups.text

  signal edited(string text)

  function focusEditor() {
    editor.forceActiveFocus()
  }

  onTextChanged: if (editor.text !== root.text) editor.text = root.text

  color: Style.controlFill(root.focused, false, root.foreground, Color.accent)
  radius: Style.cornerRadius
  padding: Style.spacing.controlPaddingX
  borderSpec: Border.controlSpec(root.focused ? "focus" : "normal", root.foreground, Color.accent)

  Flickable {
    id: view

    anchors.fill: parent
    anchors.topMargin: root.contentTopInset
    anchors.rightMargin: root.contentRightInset
    anchors.bottomMargin: root.contentBottomInset
    anchors.leftMargin: root.contentLeftInset
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    contentWidth: width
    contentHeight: editor.implicitHeight

    function reveal(rect) {
      if (rect.y < view.contentY) view.contentY = rect.y
      else if (rect.y + rect.height > view.contentY + view.height)
        view.contentY = rect.y + rect.height - view.height
    }

    TextEdit {
      id: editor

      width: view.width
      wrapMode: TextEdit.Wrap
      textFormat: TextEdit.PlainText
      color: root.foreground
      selectionColor: Style.selectionFillFor(root.foreground, Color.accent)
      selectedTextColor: root.foreground
      font.family: Style.font.family
      font.pixelSize: Style.font.body
      selectByMouse: true
      persistentSelection: true

      Keys.priority: Keys.BeforeItem
      Keys.forwardTo: root.keySink ? [root.keySink] : []

      onTextChanged: if (editor.text !== root.text) root.edited(editor.text)
      onCursorRectangleChanged: view.reveal(editor.cursorRectangle)
    }

    Text {
      anchors.top: parent.top
      anchors.left: parent.left
      width: view.width
      visible: editor.text.length === 0
      textFormat: Text.PlainText
      text: root.placeholderText
      color: Qt.darker(root.foreground, 1.6)
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.body
    }
  }
}
