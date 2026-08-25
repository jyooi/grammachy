# Running Grammachy in a live Omarchy shell

This page is for a developer or a reviewer who wants the plugin on a real desktop.
The automated checks in spec section 13 run in CI.
They cannot cover capture, because capture needs a compositor, a focused window, and a real selection.
Smoke items 1, 2, 3, 4, 7, 8, and 9 of spec section 13 are therefore run by hand, and this page is how.

## What you need

- Omarchy 4.0.0 or later, because the plugin imports `qs.Ui` and `qs.Commons` from `/usr/share/omarchy/shell`.
- A Rust toolchain, to build the companion CLI.
- `languagetool` from pacman, for a Check that finds anything: `sudo pacman -S languagetool`.
  Without it the popup shows the notice card instead of marks, which still proves capture works.
  The `harper` engine needs no package and no server, so smoke item 7 works on a bare machine.

`wl-clipboard` and `wtype` already ship with Omarchy.

## 1. Install the plugin

The shell loads third-party plugins from `~/.config/omarchy/plugins/<plugin-id>/`.
A plain clone is the recommended way, because the shell watches that directory and reloads plugin code when a file changes.

```bash
git clone --branch fm/grammachy-huf-193 <repo-url> ~/.config/omarchy/plugins/io.github.jyooi.grammachy
```

To work from an existing checkout instead, link it:

```bash
ln -s /path/to/grammachy ~/.config/omarchy/plugins/io.github.jyooi.grammachy
```

The link works, but the shell's file watcher does not follow it.
After every edit, force the reload by hand with the command in step 4.

## 2. Build the companion binary

Spec section 10 says the overlay runs `bin/grammachy` from the plugin folder and never assumes a `PATH`.
Nothing downloads it yet, so build it and copy it in.

```bash
cd ~/.config/omarchy/plugins/io.github.jyooi.grammachy/cli
cargo build --release
mkdir -p ../bin
cp target/release/grammachy ../bin/grammachy
```

`bin/grammachy` is gitignored, so a fresh clone never carries a stale binary.

Then write the hotkeys and the menu entry (spec section 10):

```bash
../bin/grammachy setup
```

The command edits `~/.config/hypr/bindings.lua` and the Omarchy menu extension.
It then reloads Hyprland.
Press SUPER + G on a selection to confirm the hotkeys.

## 3. Enable it and put the button on the bar

```bash
omarchy plugin validate ~/.config/omarchy/plugins/io.github.jyooi.grammachy
omarchy-shell shell rescanPlugins
omarchy plugin enable io.github.jyooi.grammachy
```

`enable` writes one entry in `~/.config/omarchy/shell.json`, which turns on both declared kinds at once.
The button lands on the right of the bar and reads `G`.
Move it with `omarchy bar move io.github.jyooi.grammachy --section right`.

## 4. Reload after an edit

A saved file under `~/.config/omarchy/plugins/` reloads by itself.
Force it when the plugin folder is a link, or when a reload does not seem to happen:

```bash
omarchy-shell shell rescanPlugins
```

The shell prints QML errors on its own stderr.
Read them with `journalctl --user -f` or from the terminal that started the shell.

## 5. Smoke item 1: a terminal primary selection

1. Open a terminal and print a sentence with a mistake, for example `echo "I has two book."`.
2. Highlight the sentence with the mouse.
   The highlight alone is the primary selection; no copy is needed.
3. Click the `G` button on the bar.

Expected: the popup opens under the bar on the trailing edge.
The whole sentence shows, `has` and `book` carry a solid underline, and the hero reads `2 issues, 0 accepted, languagetool, <n> ms`.
Accept moves the mark to green and shows the Fix in place.
Skip dims the mark and drops its underline.
Both advance the focus to the next open Issue.
`Accept all open` settles every mark that is still open.
`Copy corrected text` turns on only after one Accept, and reads `Copied` once it runs.
Paste into the terminal to confirm the clipboard holds the corrected sentence.

Run the whole key map of spec section 6 on the same card:

| Key | What it does |
|---|---|
| Enter | Accept the focused Issue, then move to the next open one |
| Space | Skip the focused Issue, then move to the next open one |
| Up, Down | Move the focus over every Issue, wrapping at both ends |
| A | Accept every open Issue |
| Ctrl + C | Copy the Corrected text |
| Ctrl + Enter | Apply: copy, or replace when auto-replace is on |
| Esc | Close the popup |

Ctrl + C and Ctrl + Enter stay off until one Fix is accepted, the same as the Apply button.

## 6. Smoke item 2: an Electron text field

Electron applications do not fill the primary selection, so this item exercises the Ctrl + C fallback in spec section 3.

1. Open an Electron application with a text field, for example VS Code, Slack, or Obsidian.
2. Type a sentence with a mistake into the field and select it with the mouse.
3. Click the `G` button on the bar.

Expected: the same popup as smoke item 1.
The plugin saves the clipboard, sends Ctrl + C to the field, reads the result, and puts the old clipboard back.
Check the restore: copy some other text first, then run the item, then paste somewhere.
The paste must give the text you copied first, not the sentence, until you press `Copy corrected text`.

## 7. Smoke item 3: nothing selected

1. Click somewhere with no text selected, for example the desktop background.
2. Clear the primary selection and the clipboard: `wl-copy --clear; wl-copy --primary --clear`.
3. Click the `G` button on the bar.

Expected: the popup opens with the `empty_selection` card.
It reads `Nothing selected` over `Highlight some text, then press SUPER + G.`, with a Close button.
Esc closes it, and so does a click outside the card.

The capture tries the primary selection, then the Ctrl + C fallback, so this item takes about 150 ms longer than the others.

## 8. Smoke item 4: a 6,000 unit selection

One check takes 5,000 UTF-16 code units, so a longer selection earns the too-long card.

1. Make a file of about 6,000 characters and open it in a terminal pager or an editor:

   ```bash
   python3 -c "print(('The quick brown fox jumps over the lazy dog and keeps running. ' * 100)[:6000])" > /tmp/grammachy-6000.txt
   wc -m /tmp/grammachy-6000.txt
   cat /tmp/grammachy-6000.txt
   ```

2. Select the whole text with the mouse.
3. Click the `G` button on the bar.

Expected: the too-long card.
A size bar shows the 5,000 unit limit against the 6,000 units selected, with `5,000 units per check` under the filled part and `6,000 units selected` under the end.
The CLI message shows below the body in monospace.
The buttons read Close, `Check the first 5,000 only`, and `Open in Compose`.

Press `Check the first 5,000 only`.
The card runs a new Check on the first 5,000 units and shows the marked text.
The hero adds `First 5,000 of 6,000 units checked` under the meta line.

Check the two bounds that this selection is here to prove:

- The card fits on the screen. Its bottom edge stays above the screen edge, and the whole footer is visible.
- The text region scrolls. Drag inside it, or press Down until the focus passes the bottom of the region.
  The focused mark scrolls into view; the card does not grow.

`Open in Compose` shows the Compose notice until the Compose window lands.

## 9. Smoke item 7: switch to Harper, Check, switch back

This item proves the Settings view of spec section 7: the gear, the storage, and that a change applies to the next Check.

1. Run smoke item 1 so the popup is open on a sentence with a mistake.
   Read the hero meta line and note the engine it names, which is `languagetool` by default.
2. Click the gear on the trailing edge of the hero.
   The card flips to Settings; the Issues stay behind it.
3. Set `Engine` to `Harper`.
   Nothing is saved by hand: the choice is in `~/.config/omarchy/shell.json` the moment the row is picked.
   Confirm it with `jq '.bar.layout.right[] | select(.id == "io.github.jyooi.grammachy")' ~/.config/omarchy/shell.json`.
4. Click `Back`, or the gear again.
   The same Issues are still on screen with the same accepted and skipped marks, and the meta line still names `languagetool`, because a change applies to the next Check only.
5. Highlight the sentence again and click `G`.

Expected: the new Check runs through Harper, and the meta line now names `harper`.
Switch back to `LanguageTool` in Settings and run one more Check; the meta line names `languagetool` again.

Three more Settings checks belong to the same session:

- **The openai fields.** Set `Engine` to `Local LLM`.
  The `Local LLM server` row appears with the base URL and the model.
  Set it back to `Harper` and the row goes away.
- **Scripting.** With the popup open on Settings, run `omarchy-shell shell setBarWidget io.github.jyooi.grammachy engine '"harper"'` from a terminal.
  The Engine dropdown moves to `Harper` without a click.
- **An unknown stored value.** Stop the shell, hand-edit the entry to `"engine": "claude"`, and start it again.
  The dropdown shows `LanguageTool`, the default.
  Open Settings, change nothing, and close the popup: `"engine": "claude"` is still in the file, because nothing is rewritten until the user changes it.
  Pick `Harper` and the file finally reads `"engine": "harper"`.

## 10. Smoke item 8: auto-replace in a terminal and a browser field

Auto-replace copies the Corrected text, closes the popup, and pastes over the still-highlighted Selection.
It only works while the source window still holds that highlight.

1. Turn the toggle on: click `Auto-replace` in the popup hero.
   The Apply button then reads `Replace selection` instead of `Copy corrected text`.
2. In a terminal, type a sentence with a mistake at a shell prompt, for example `echo I has two book.`, and select the text with the mouse.
   Do not press Enter.
3. Click the `G` button, accept one Fix, then press `Replace selection` or Ctrl + Enter.

Expected: the popup closes, and the terminal line becomes the corrected sentence.
The button state reads `Replaced`.
The Corrected text stays in the clipboard, so a further Ctrl + Shift + V pastes it again.

4. Repeat in a browser text field, for example the search box of any page.
   Type the sentence, select it with the mouse, then run the same steps.

Expected: the same replacement in the field.

If nothing pastes, the source window lost the highlight before the paste landed.
That is the documented limit of spec section 6, and the hint under the toggle says so.

## 11. Smoke item 9: settings persist across a shell restart

1. Open Settings and set `Native language` to `Malay`, `Engine` to `Harper`, and `Auto-replace` on.
2. Restart the shell:

```bash
omarchy restart shell
```

3. Click `G` on a selection and open Settings again.

Expected: `Malay`, `Harper`, and `Auto-replace` on, all as they were left.
The plugin keeps no state of its own: the values come back because `~/.config/omarchy/shell.json` holds them, and the shell reads that file at start.
`targetEnglish` and `openaiApiKey` have no control, so check by hand that an edit of those two keys in the file also survives a round trip through the Settings view.

## 12. Running the automated checks

The same three checks CI runs, against the shell installed on this machine:

```bash
for file in $(find . -name '*.qml' | sort); do qmllint -I /usr/share/omarchy/shell "$file" || echo "FAILED $file"; done
omarchy-plugin-validate .
node --test ui/splice.test.js ui/tokens.test.js ui/settings.test.js ui/keymap.test.js
```

The `qmllint` on `PATH` reports a syntax error through its exit status alone and prints nothing.
For the diagnosis, run the Qt 6 build over the same file:

```bash
/usr/lib/qt6/bin/qmllint <file>.qml
```

It prints line and column for a syntax error.
Ignore its import and unqualified-access warnings; the shell's own plugins raise the same ones.

The CLI keeps its own checks, run from `cli/`:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## 13. Removing it

Remove the hotkeys and the menu entry first (spec section 10):

```bash
~/.config/omarchy/plugins/io.github.jyooi.grammachy/bin/grammachy setup --remove
omarchy plugin disable io.github.jyooi.grammachy
rm -rf ~/.config/omarchy/plugins/io.github.jyooi.grammachy
omarchy-shell shell rescanPlugins
```
