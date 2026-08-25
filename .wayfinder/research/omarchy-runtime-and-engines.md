# Research: Omarchy plugin runtime, Wayland text access, grammar engines

Collected 2026-08-25 during map charting.

## Omarchy plugin runtime

- One long-running Quickshell process. Plugins run inside it, unsandboxed.
- Kinds: bar-widget, panel, overlay, menu, service, bar.
- Third-party plugins live in `~/.config/omarchy/plugins/<id>/` and hot-reload on save.
- Only bar widgets have a declarative settings `schema` in the manifest. Settings are stored in `~/.config/omarchy/shell.json`.
- Services get the full Quickshell API: `Process`, `Timer`, `FileView`, `Quickshell.execDetached`, `IpcHandler`.
- No in-process notification API. Built-ins shell out to `notify-send`.
- No global hotkey API in use. Hyprland binds call `omarchy-shell shell summon|toggle|hide|call <id> ...`.
- Shell source: `/usr/share/omarchy/shell/`. Built-in plugins under `plugins/`. Full manifest schema in `services/PluginRegistry.qml`.
- Marketplace has zero community plugins and no grammar tool.

## Local machine

- Hyprland 0.56.2. Present: wl-paste, wtype, hyprctl, fcitx5, notify-send. Absent: ydotool, languagetool.
- Pacman: `extra/languagetool 6.6-2` (386 MiB, needs java-runtime-headless), `extra/harper 2.8.0-1`.

## Wayland text access

- Reading another app's input field: not possible in general.
- `wl-paste --primary`: reads the current selection with no copy keystroke. Weak in Chromium and Electron.
- `wl-paste`: reliable after a copy.
- Input method (text-input-v3, fcitx5): only near-cursor context, single active IM. Not usable for whole-text checks.
- `wtype`: feasible for paste-back (Ctrl+V) or typing text. Overwrite by selection is fragile.
- AT-SPI: partial for GTK and Qt, poor for Electron and terminals.

## Grammar engines

| Engine | Offline | Latency | Native-language aware | Variants | License |
|---|---|---|---|---|---|
| LanguageTool server | Yes | ~1 s per sentence, 1 to 2 GB JVM | `motherTongue` false-friend rules | en-US, GB, AU, CA, NZ, ZA | LGPL 2.1 |
| Harper | Yes | milliseconds | No | US, GB, CA, AU, IN dialects | Apache 2.0 |
| Claude API | No | 1 to 5 s | Yes, by prompt | Any | Pay per token |
