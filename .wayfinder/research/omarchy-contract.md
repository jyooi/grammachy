# Research: Omarchy bar widget plugin runtime contract (HUF-172)

Collected 2026-08-25 on this machine.
All paths are under `/usr/share/omarchy/shell/` unless noted.
Sources are read-only.
Nothing under `/usr/share` or `~/.config` was changed.

Installed versions:

- Quickshell 0.3.0 (`quickshell-git 0.3.0.r20.g28771c7-2`, revision `28771c7c`, AUR).
- Qt 6.11.2 (`qt6-declarative 6.11.2-1`, `qt6-base 6.11.2-2`).
- `qmllint` at `/usr/bin/qmllint`, reports `qmllint 1.0`.
- Hyprland 0.56.2.

## 1. Manifest `barWidget.schema`

### Supported setting types

The shell does not validate `schema` at all.
`services/PluginRegistry.qml:43-91` (`validateManifest`) checks only these fields: `schemaVersion`, the required fields, the id, `kinds`, `entryPoints`, and `barWidget.defaultSection`.
`shell.qml:686-699` copies the metadata into the bar widget registry verbatim:

```qml
      var meta = manifest.barWidget || {}
      meta = {
        displayName: meta.displayName || manifest.name,
        description: meta.description || manifest.description,
        category: meta.category || "Plugin",
        allowMultiple: meta.allowMultiple === true,
        defaults: meta.defaults || {},
        settingsForm: meta.settingsForm || "",
        schema: meta.schema || [],
        pluginId: manifest.id,
        sourceDir: manifest.__sourceDir || "",
        source: "plugin"
      }
```

No QML or JS file in the installed shell reads `schema` entries or `settingsForm` after that point.
A grep for `"enum"`, `multiselect`, and `settingsForm` in `*.qml` and `*.js` finds only the manifests and a comment in `MultiSelect.qml:74`.
The comment at `shell.qml:710-711` says "the settings panel reads metadata from the registry".
So the schema is metadata for a settings UI, not something the widget runtime enforces.
The type vocabulary in use by first-party manifests is: `string`, `integer`, `boolean`, `enum`, `multiselect`, `path`.

### Enum (dropdown) declaration

`plugins/agents/manifest.json:32-38`:

```json
    "schema": [
      { "key": "refreshIntervalSec", "type": "integer", "label": "Refresh interval (seconds)", "min": 30, "max": 3600, "step": 30, "defaultValue": 900 },
      { "key": "syncMode", "type": "enum", "label": "Synced aggregation", "options": ["Off", "On"], "defaultValue": "Off", "description": "When On, write this machine's local usage snapshot and merge snapshots from other machines." },
      { "key": "syncDir", "type": "path", "label": "Sync folder", "defaultValue": "", "description": "A folder synced by Syncthing, Dropbox, rsync, etc." },
      { "key": "syncFileName", "type": "string", "label": "Snapshot file name", "defaultValue": "", "description": "Optional. Defaults to <hostname>.json. Use a different file name on each machine, such as laptop.json or desktop.json." },
      { "key": "syncDeviceId", "type": "string", "label": "Device id", "defaultValue": "", "description": "Optional stable device name used inside synced aggregate snapshots." }
    ]
```

`multiselect` uses object options `{ "value", "label", "description" }` plus `noSelectionText`, `placeholderText`, `emptyText` (`plugins/bar/widgets/Indicators.manifest.json:59-100`).
`boolean` example, same file, lines 101-107:

```json
      {
        "key": "alwaysShow",
        "type": "boolean",
        "label": "Always Show",
        "description": "Show inactive indicators without waiting for hover.",
        "defaultValue": false
      }
```

### `defaults`

`barWidget.defaults` is an object of key to value (`plugins/agents/manifest.json:20-31`, `README.md:66`).
It is stored in registry metadata only.
The bar does not merge `defaults` into the widget's `settings` property.
Widgets supply their own fallback in code via `setting(name, fallback)` (see section 2).
The clock has no `defaults` block and falls back in QML: `setting("format", "dddd HH:mm")` (`plugins/panels/clock/BarWidget.qml:21`).
Treat `defaults` as documentation for the settings UI and duplicate the fallbacks in QML.

### `settingsForm`

A string that names a custom settings form, used instead of `schema` by two first-party widgets.
Weather has `"settingsForm": "weatherSettings"` (`plugins/panels/weather/manifest.json:39`).
Spacer has `"settingsForm": "spacerSettings"` (`plugins/bar/widgets/Spacer.manifest.json:137`).
No installed QML resolves those names.
Third-party plugins have no way to register a form, so use `schema`.

### Minimal manifest (`README.md:51-71`)

```json
{
  "schemaVersion": 1,
  "id": "my.org.cool-clock",
  "name": "Cool clock",
  "version": "1.0.0",
  "author": "You",
  "description": "A clock that does cool things",
  "kinds": ["bar-widget"],
  "entryPoints": { "barWidget": "Widget.qml" },
  "barWidget": {
    "displayName": "Cool clock",
    "category": "Time",
    "allowMultiple": false,
    "defaultSection": "left",
    "defaults": { "format": "HH:mm" },
    "schema": [
      { "key": "format", "type": "string", "label": "Format" }
    ]
  }
}
```

Rules enforced by `omarchy plugin validate` (`/usr/share/omarchy/bin/omarchy-plugin-validate`):

- `schemaVersion` must be the JSON number `1`.
- `id` must match `^[A-Za-z0-9][A-Za-z0-9._-]*$`, contain no `..`, and not start with `omarchy.`.
- `kinds` must be a non-empty array.
- Every `entryPoints` value must be relative, contain no `..`, and exist as a file.
- Each kind needs its entry point key. `bar-widget` needs `entryPoints.barWidget`.
- No symlinks inside the folder.

## 2. Reading settings at runtime

Base type: `Ui/BarWidget.qml` (`import qs.Ui`).
The bar injects three properties into every widget slot (`Ui/BarWidget.qml:4-17`):

```qml
Item {
  id: root

  property QtObject bar: null
  property string moduleName: ""
  property var settings: ({})
```

`settings` is the inline `shell.json` layout entry, for example `{ "id": "omarchy.clock", "format": "HH:mm" }`.
Helper, `Ui/BarWidget.qml:41-44`:

```qml
  function setting(name, fallback) {
    var value = settings ? settings[name] : undefined
    return value === undefined || value === null ? fallback : value
  }
```

Injection happens in `plugins/bar/Bar.qml:1745-1753`:

```qml
    onActiveItemChanged: Qt.callLater(injectProps)
    onModuleSettingsChanged: injectProps()

    function injectProps() {
      var target = activeItem
      if (!target) return
      if ("bar" in target) target.bar = root
      if ("moduleName" in target) target.moduleName = moduleName
      if ("settings" in target) target.settings = moduleSettings
    }
```

Reaction to change: when `shell.json` is saved and only inline settings changed, the bar patches running widgets in place.
It does not rebuild them (`plugins/bar/Bar.qml:361-388`, `applySettingsDelta` does `item.settings = settings`).
The widget sees this as the standard QML `settingsChanged` signal.
Use `readonly property` bindings that call `setting()` so they re-evaluate, or an `onSettingsChanged` handler.
Clock example, `plugins/panels/clock/BarWidget.qml:19-24` and `109-110`:

```qml
  readonly property string configuredFormat: vertical
    ? setting("verticalFormat", "HH\n—\nmm")
    : setting("format", "dddd HH:mm")
  ...
  onBarChanged: injectPanel()
  onSettingsChanged: injectPanel()
```

Writing a setting back from the widget, `plugins/panels/clock/BarWidget.qml:44-52`:

```qml
    var entry = { id: root.moduleName }
    for (var key in root.settings) if (key !== "id") entry[key] = root.settings[key]
    entry[vertical ? "verticalFormat" : "format"] = next
    root.settings = entry
    if (root.bar && root.bar.shell && typeof root.bar.shell.updateEntryInline === "function")
      root.bar.shell.updateEntryInline(root.moduleName, entry)
```

CLI equivalent: `omarchy bar set` calls IPC `setBarWidget <id> <key> <valueJson> <selectorJson>` (`shell.qml:955-964`).
That writes `entry[key] = value` (`PluginRegistry.qml:354`).

A nested panel loaded by the widget gets the same props by hand (`plugins/panels/clock/BarWidget.qml:97-104`):

```qml
  function injectPanel() {
    var target = panelLoader.item
    if (!target) return
    if ("bar" in target) target.bar = root.bar
    if ("settings" in target) target.settings = root.settings
    if ("anchorItem" in target) target.anchorItem = button
    if ("hostWidget" in target) target.hostWidget = root
  }
```

## 3. `summon` and the payload

Wrapper: `/usr/share/omarchy/bin/omarchy-shell` runs `qs ipc -n -p $OMARCHY_PATH/shell call -- shell summon <id> <json>`.
For `summon` and `toggle` with no payload the wrapper appends `{}`.
IPC entry, `shell.qml:1002-1004`:

```qml
    function summon(id: string, payloadJson: string): string {
      return shell.summon(id, payloadJson) ? "ok" : "unknown"
    }
```

`shell.summon` is at `shell.qml:440-478`.
The decisive lines are 455-461:

```qml
    // Bar widgets take no payload; payloadJson is dropped on this path.
    if (shell.isBarWidgetPanelPlugin(id)) {
      var summoned = shell.bar && typeof shell.bar.summonBarWidget === "function"
        && shell.bar.summonBarWidget(id)
      if (!summoned) console.warn("summon: no live bar widget for:", id)
      return summoned === true
    }
```

`isBarWidgetPanelPlugin` (`shell.qml:426-438`) is true for a plugin whose `kinds` include `bar-widget` and none of `panel`, `overlay`, `menu`.
`Bar.summonBarWidget` (`plugins/bar/Bar.qml:518-523`) resolves the live instance on the focused monitor via `findPanelWidget`.
`findPanelWidget` needs `open` and `close` functions and an `opened` property on the widget root (`Bar.qml:501-516`).
It then calls `item.open()` with no argument.

So for a plain bar widget the payload never reaches `open()`.
The `open(payloadJson)` path with the payload exists only for `panel`, `overlay`, and `menu` kinds.
`deliverIfLoaded` (`shell.qml:541-556`) calls `loader.item.open(queue[i])` and the plugin parses it, for example `plugins/menu/Menu.qml:21-23`:

```qml
  function open(payloadJson) {
    var payload = ({})
    try { payload = JSON.parse(payloadJson || "{}") } catch (e) { payload = ({}) }
```

A plugin that declares `kinds: ["bar-widget", "panel"]` is routed through the panel loader (`omarchy.menu` does this).
But then `summon` opens the separate `entryPoints.panel` component, not the bar widget, and the panel is not anchored to the bar button.
omarchyplugins.com/develop.html states the same rule: one manifest per bar widget, nested panels load through a `Loader`, and no separate `panel` kind.

Two working ways to pass text into a bar widget.

Way 1: own `IpcHandler` on the widget root, as the clock does (`plugins/panels/clock/BarWidget.qml:129-140`).

- Arguments are typed QML strings.
  So `omarchy-shell my.grammachy check "$(wl-paste --primary)"` reaches `function check(text: string): void`.
- An IPC target routes to one per-monitor instance.
  Use `root.broadcast("method")` or the clock pattern for multi-monitor.
- Size limit is the kernel argv limit (`ARG_MAX` is 2097152 bytes here, one argument up to 131072 bytes).
  The wrapper also applies a 2 s `OMARCHY_SHELL_IPC_TIMEOUT`.
- Quoting is ordinary shell quoting.
  The Hyprland `bindings.lua` command string runs through a shell, so `$(...)` works, but newlines and quotes in the selection must survive the bind string.
  Base64 avoids that (`Qt.atob` on the QML side, as `applyTheme` does at `shell.qml:879-882`).

Way 2: capture inside the plugin.

- Call `summon` with no payload.
- In `open()` start a `Process` that runs `wl-paste --primary --no-newline` and collects stdout.
- The built-in clipboard plugin takes this approach (`plugins/clipboard/Clipboard.qml:283-291`, `capture.sh:62` `wl-paste --type text --no-newline`).
- Recommended: it keeps the Hyprland bind to a fixed string and has no size or quoting concern.

`shell call <id> <method> <arg>` (`shell.qml:1027-1029`, `callIfLoaded` at 567-579) also reaches only panel-loader plugins, not bar widgets.

## 4. Spawning a `Process` with stdin and JSON stdout

`Quickshell.Io.Process` API from `/usr/lib/qt6/qml/Quickshell/Io/quickshell-io.qmltypes` (Quickshell 0.3.0):

- Properties: `running`, `command` (list), `workingDirectory`, `environment`, `clearEnvironment`, `stdout`, `stderr`, `stdinEnabled`.
- `stdout` and `stderr` take a `DataStreamParser`: `StdioCollector` or `SplitParser`.
- Signals: `started`, `exited(exitCode, exitStatus)`.
- Methods: `write(data)`, `signal(int)`, `startDetached()`, static `exec(command)`.

Stdout collection as used by weather (`plugins/panels/weather/Panel.qml:331-355`):

```qml
  Process {
    id: forecastProc
    command: ["curl", "-fsS", "--max-time", "10", "https://wttr.in/" + root.locationQuery + "?format=j1"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var raw = String(text || "").trim()
        ...
        try {
          var parsed = JSON.parse(raw)
```

Stdin write as used by network (`plugins/panels/network/Panel.qml:775-784`):

```qml
  Process {
    id: enterpriseConnect
    property string secret: ""
    stdinEnabled: true
    onStarted: {
      write(secret + "\n")
      secret = ""
    }
  }
```

Minimal combined snippet for a Check:

```qml
import QtQuick
import Quickshell.Io

Item {
  id: root
  property string pendingText: ""
  property var issues: []

  function check(text) {
    pendingText = text
    checker.running = false
    checker.running = true
  }

  Process {
    id: checker
    command: ["harper-cli", "--json"]
    stdinEnabled: true
    onStarted: {
      write(root.pendingText)
      // Closing stdin signals end of input; Quickshell has no closeStdin(),
      // so toggle stdinEnabled after the write.
      stdinEnabled = false
    }
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try { root.issues = JSON.parse(String(text || "[]")) }
        catch (e) { console.warn("check: bad JSON", e) }
      }
    }
    stderr: StdioCollector { onStreamFinished: if (text) console.warn("check:", text) }
    onExited: function(code) { if (code !== 0) console.warn("check exit", code) }
  }
}
```

Setting `stdinEnabled = false` closes the write channel.
The Quickshell docs say "If false, stdin is closed".
This snippet was not run against a real engine on this machine.
`harper-cli` is not installed.
`qmllint -I /usr/share/omarchy/shell` accepts the same imports as the built-ins.

Fire-and-forget commands use `Quickshell.execDetached([...])` (`plugins/emojis/Emojis.qml:151`) or `bar.run("cmd string")` (`plugins/bar/Bar.qml:611-615`, routed through `Util.execDetached`).

## 5. Panel focus, Escape, Tab, list navigation, Ui components

Popup window base: `Ui/KeyboardPanel.qml`, a `PanelWindow` layer-shell surface on `WlrLayer.Overlay`.
It needs two properties: `anchorItem` (the bar button) and `bar`.
Other properties: `owner`, `open`, `focusTarget`, `contentWidth`, `contentHeight`, `centerOnBar`, `padding`, `margin`.
Helpers: `fittedContentWidth()`, `fittedContentHeight()`.

Focus (`Ui/KeyboardPanel.qml:98-100`):

```qml
  WlrLayershell.keyboardFocus: open
    ? (focusPrimed ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.Exclusive)
    : WlrKeyboardFocus.None
```

On open it primes Exclusive for 75 ms and then drops to OnDemand (`focusPrimeTimer`, lines 251-259).
It also calls `focusTarget.forceActiveFocus()` (lines 230-232).
An outside click closes the panel via `dismissArea` (lines 280-333) and via transparent twins on other monitors (343-375).
`close()` calls `owner.close()` if the owner exposes one (69-72).

Key dispatch: `Ui/PanelKeyCatcher.qml` wraps the content and emits signals (lines 38-44):

```qml
  signal moveRequested(int dx, int dy)
  signal activateRequested()
  signal returnRequested()
  signal closeRequested()
  signal deleteRequested()
  signal tabRequested(int direction)
  signal textKey(string text)
```

Mapping (lines 48-84):

- Escape: `closeRequested`.
- Tab or Backtab: `tabRequested(+1 or -1)`.
- Down or `j`: `moveRequested(0, 1)`. Up or `k`: `(0, -1)`.
- Right or `l`: `moveRequested(1, 0)`. Left or `h`: `(-1, 0)`.
- Return or Enter: `returnRequested`, then `activateRequested`.
- Space: `activateRequested`.
- `x` or `X`: `deleteRequested`.
- Any other single character: `textKey`.

It uses `Keys.priority: Keys.BeforeItem`.
Set `blocked: editor.activeFocus` while an inline `TextField` or `Dropdown` popup owns the keys (comment at lines 24-32, gallery text at `plugins/dev-gallery/GalleryPanel.qml:426`).

The Tab convention in first-party panels is to switch to the neighbouring bar panel.
Weather does `onTabRequested: function(direction) { root.switchPanel(direction) }` (`plugins/panels/weather/Panel.qml:504`).
`Panel.switchPanel` calls `bar.switchPanelFrom` (`Ui/Panel.qml:32-35`).
A plugin may instead use Tab to move between its own sections.

Usage pattern (`plugins/panels/weather/Panel.qml:487-512`):

```qml
  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root.barIdentity
    bar: root.bar
    open: root.opened
    centerOnBar: true
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(480))
    contentHeight: panel.fittedContentHeight(weatherColumn.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      blocked: root.editingLocation
      onReturnRequested: root.startEditingLocation()
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }

      Flickable {
```

List navigation is not a component.
Each panel keeps `property int selectedIndex` and clamps it in `onMoveRequested` (`plugins/panels/network/Panel.qml:117, 394-396, 998`).
It binds `ListView.currentIndex: root.selectedIndex` with `onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)` (`Panel.qml:1474-1488`).
Rows compute `isSelected` from `selectedIndex === index` (`Panel.qml:1605`) and set `selectedIndex` on hover (`Panel.qml:1681`).

Open state base: `Ui/Panel.qml`.
It has `moduleName`, `settings`, `ipcTarget`, `manageIpc`, `controller`, `opened`, `open()`, `close()`, `toggle()`, and `setting()`.
It is backed by `Ui/PanelController.qml`, an `open` bool with `show()`, `hide()`, and `toggle()`.
The clock overrides `open()` and `close()` on the Panel and forwards them from the BarWidget root (`plugins/panels/clock/Panel.qml:83-107`, `BarWidget.qml:62-74`).

Components exported by `Ui/qmldir` (`import qs.Ui`):

- Bar side: `BarWidget`, `BarIconButton`, `BarIndicator`, `WidgetButton`.
- Surfaces: `KeyboardPanel`, `Panel`, `PanelController`, `PanelKeyCatcher`, `PopupCard`, `BorderSurface`, `BorderOverlay`.
- Buttons: `Button` (`text`, `iconText`, `selected`, `hasCursor`, `focusable`, signals `clicked` and `rightClicked`), `PanelActionButton` (icon button, `clicked`, `hovered`), `ButtonGroup`, `Toggle`, `ToggleSwitch`.
- Text input: `TextField` (QtQuick Controls TextField subclass with `password` and `hasCursor`), `NumberField`.
- Choice: `Dropdown` (`label`, `value`, `options` as strings or `{value,label}`, signal `changed(string)`, `open()`, `close()`, `toggle()`, `popupOpen`), `SearchableDropdown`, `MultiSelect`.
- Text and layout: `PanelHero` (`title`, `meta`, `detail`, `iconComponent`), `PanelSectionHeader` (a `Text`), `PanelSeparator`, `PanelSlider`, `PanelToolTip`, `OpticalGlyph`, `ConfirmDialog`.

There is no list or row component.
Panels build rows from `Repeater` or `ListView` with `Rectangle` and `Text`, and use `Color.*` and `Style.*` from `import qs.Commons`.
The bar button is `WidgetButton` (`Ui/WidgetButton.qml`) with `text`, `tooltipText`, `active`, `dimmed`, and signals `pressed(int button)` and `wheelMoved(int delta)`.
It registers itself as a click target so clicks on the bar reach it while a panel is open.
`plugins/dev-gallery/GalleryPanel.qml` is a live demo of every component.

## 6. Validate and reload loop

### Validate

- `omarchy plugin validate <dir>`: bash script at `/usr/share/omarchy/bin/omarchy-plugin-validate`, exit 0 when valid, mirrors `PluginRegistry.validateManifest`.
- `qmllint -I /usr/share/omarchy/shell BarWidget.qml Panel.qml`.
  The `-I` flag adds a qmltypes and qmldir search directory so `import qs.Ui` and `import qs.Commons` resolve.
- Verified: `qmllint -I /usr/share/omarchy/shell plugins/panels/weather/Panel.qml` exits 0 with no warnings on this machine.
  Adding `-I /usr/lib/qt6/qml` is not needed.
  `$OMARCHY_PATH` is `/usr/share/omarchy`.

### Hot reload

- `PluginRegistry.qml:636-655` runs `inotifywait -m -r -q -e close_write,create,delete,move --format %w%f ~/.config/omarchy/plugins`.
  Each event maps to a plugin id and emits `localPluginChanged`.
- `shell.qml:761-766` restarts a 150 ms timer that calls `shell.reloadPlugins()`.
  That function unloads panels, calls `Qt.clearComponentCache()`, and rescans (`shell.qml:739-759`).
- Any saved file under the plugin dir triggers it, except hidden entries and `.git` (`PluginRegistry.qml:701-713`).
  Manual: `omarchy-shell shell rescanPlugins`.
- `shell.json` edits reload separately via the `FileView` watcher and patch settings in place (section 2).
- Load failures print `panel plugin <id> failed to load:` or `Plugin <id> has no barWidget entry point` to the shell's stderr.
  Run `omarchy restart shell` from a terminal to see them.

### Clone a built-in

- `omarchy plugin clone omarchy.clock [--edit]` (`/usr/share/omarchy/bin/omarchy-plugin-clone`).
- It copies the source directory to `~/.config/omarchy/plugins/<username>.clock/`.
  It rewrites `id`, sets `name` to `My Clock`, sets `barWidget.displayName`, and adds `omarchy.clonedFrom`.
- It then rescans and swaps the bar entry to the clone with settings preserved (`PluginRegistry.setEnabled` lines 503-509).
  IPC calls to `omarchy.clock` are routed to the enabled clone (`resolveEnabledId`, lines 146-157).
- For a new plugin, rename the id to a third-party id such as `jyooi.grammachy` before publishing.
  Ids that start with `omarchy.` are rejected (`PluginRegistry.qml:602-607`).

### Enable and test

- Enable and place: `omarchy plugin enable <id>` or IPC `enablePlugin <id> '{"section":"right"}'`.
  `barWidget.defaultSection` sets the default section.
- Test lifecycle from develop.html: `omarchy-shell shell summon "$PLUGIN_ID" '{}'`, then `omarchy-shell shell hide "$PLUGIN_ID"`.
  Then test click, Escape, disable, re-enable, `omarchy restart shell`, and remove.

## 7. Limits

- Open the panel without a click: yes.
  `omarchy-shell shell summon <id>` or `toggle <id>` from a Hyprland bind calls the widget's `open()` on the focused monitor.
  The widget root must expose `open`, `close`, and `opened` (`Bar.qml:501-523`).
  The widget may also call its own `open()` from any code path, for example after a `Process` finishes.
  Example bind in `~/.config/hypr/bindings.lua:28`: `o.bind("SUPER + PERIOD", nil, "omarchy-shell shell toggle omarchy.emojis")`.
- Notifications: no in-process API.
  First-party code shells out to `omarchy-notification-send "<title>" "<body>"` (`plugins/reminders/ReminderFlow.qml:77`) or `notify-send`.
  Both arrive through the shell's own notification service over D-Bus.
- Clipboard: `Quickshell.clipboardText` is a read and write property with `clipboardTextChanged` (`/usr/lib/qt6/qml/Quickshell/quickshell-core.qmltypes:983-987`).
  It covers the regular clipboard only.
  No primary-selection property exists in the installed qmltypes.
  First-party plugins do not use it.
  They run `wl-paste` in a `Process` (`plugins/clipboard/Clipboard.qml:283-296`) and write with `printf %s ... | wl-copy` via `execDetached` (`plugins/panels/network/Panel.qml:450`).
  So the Selection must come from `wl-paste --primary` inside a `Process`.
- Payload: bar widgets receive none on `summon` (section 3).
- Settings UI: nothing installed renders `schema`.
  The plugin cannot rely on a system settings form.
  Build the Native language dropdown in the panel with `Ui/Dropdown`, or read the value from `shell.json` only.
- Multi-monitor: one widget instance per bar surface.
  `IpcHandler` targets bind to one instance.
  Use `broadcast()` for shared state or keep state in a `Process`-backed file.
- Focus: `KeyboardPanel` takes Exclusive focus for 75 ms on open, which routes the pointer compositor-wide for that window.
  This is by design (`KeyboardPanel.qml:87-100`).
- Sandbox: none.
  The plugin runs as the user inside `omarchy-shell`.
  A crash or infinite loop takes the whole bar down (`README.md:107-110`).
- Nested Quickshell: do not spawn a second `quickshell` instance (develop.html "No nested shell processes").
