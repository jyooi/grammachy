# Popup

The quick popup marks the Issues of a captured Selection, and the Compose card checks a Draft in Chunks.
Both are QML surfaces of the Omarchy shell, and both call the CLI recipes in this map for every Check.
An agent does not drive them, so every step in this file is manual.

## Sub-features

- `popup-quick` captures a Selection, marks its Issues, and applies the Corrected text.
- `popup-nothing-new` opens the nothing-new state when there is no new Selection.
- `popup-too-long` offers `Check the first 5,000 only` and `Open in Compose`.
- `popup-errors` shows the `engine_unavailable` card and its Retry.
- `popup-settings` stores Engine, Target English, Native language, and Auto-replace in `shell.json`.
- `compose-draft` checks a Draft in Chunks with progress, Cancel, and Retry remaining.
- `popup-engines` installs and removes LanguageTool from Settings, Engines.

## How to get to it (user POV)

- Highlight text and press SUPER + SHIFT + Q, or click the `G` bar button.
- Press SUPER + ALT + Q, or run `omarchy-shell shell summon io.github.jyooi.grammachy '{"mode":"compose"}'`, for Compose.
- Choose Apps > Grammachy in the Omarchy menu.

## Driving it with vg

Preconditions:

- The operator runs these steps by hand, on a desktop where the operator chose to install the plugin.
- `vg` drives none of these steps.

Why an agent does not drive the popup:

- The QML plugin loads only inside the Omarchy shell loader. No second shell instance can run beside the operator shell.
- The only loaded plugin is the copy that the operator uses. A summon changes the operator Draft, focus, and clipboard.
- Capture sends Ctrl + C to the focused window and reads `wl-paste`. Auto-replace types into the source window with `wtype`. Both act on operator windows.
- A synchronous `wl-paste` or `hyprctl` call from a Hyprland keybinding callback freezes the whole desktop.
- A screenshot with `grim` captures every operator window, which is private data.

The manual recipes live in `docs/dev.md`. Run them in this order, and record a pass or a fail for each item.

- **Quick popup.** Run smoke item 1 in section 5. The card opens beside the source window and marks each Issue.
- **Electron capture.** Run smoke item 2 in section 6. The old clipboard comes back after the capture.
- **Nothing new.** Run smoke item 3 in section 7. The card reads `No new selection.` and runs no Check.
- **Too long.** Run smoke item 4 in section 8. The too-long card shows the 5,000 unit bar and both offers.
- **Chunked Draft.** Run smoke item 5 in section 9. The progress line climbs, and Cancel stops after the current Chunk.
- **Error card.** Run smoke item 6 in section 10. `Retry` checks the original Selection again.
- **Settings.** Run smoke items 7 and 9 in sections 11 and 13. A change applies to the next Check and survives a shell restart.
- **Auto-replace.** Run smoke item 8 in section 12. The corrected text lands only in the source window.
- **Compose card.** Run section 14. The Draft survives a close, and the 50,001 unit cap turns `Check` off.
- **Engines list.** Run smoke items 11 to 15 in section 15.

## Gotchas

- Do not install, link, or replace the plugin under `~/.config/omarchy/plugins/`. That copy belongs to the operator.
- Do not edit files under `~/.config/hypr` or `~/.config/omarchy` to set up a popup test.
- Smoke items 5, 6, and 13 stop the shared unit `grammachy-languagetool` by hand. Only the operator runs them.
- A CLI pass in this map does not prove a popup item. Report each popup item as manual until the operator records it.
- A popup helper that calls `hyprctl` or `wl-paste` needs a timeout, for example `timeout 2 hyprctl activewindow -j`.
