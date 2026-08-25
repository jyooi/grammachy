import QtQuick
import qs.Commons
import qs.Ui

// The Draft text area of Compose, spec section 9: a plain multi-line field
// that scrolls inside the card and never grows it.
//
// `qs.Ui.TextField` is single line, so the chrome of a focused input is
// rebuilt here over a `TextEdit` rather than borrowed. Everything else about
// the Draft, including the fact that it is never written to disk, belongs to
// Overlay.qml.
BorderSurface {
  id: root

  // The Draft. The caller owns it, so a write from either side settles on the
  // same string: the guard on each side stops the two from chasing each other.
  property string text: ""
  property string placeholderText: ""
  // The item whose `Keys.onPressed` runs the key map. The field forwards to it
  // first, so Ctrl + Enter runs the Check rather than adding a line.
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

    // Keep the caret in the window as it walks out of either end.
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
      text: root.placeholderText
      color: Qt.darker(root.foreground, 1.6)
      wrapMode: Text.Wrap
      font.family: Style.font.family
      font.pixelSize: Style.font.body
    }
  }
}
