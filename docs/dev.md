# Running Grammachy in a live Omarchy shell

This page is for a developer or a reviewer who wants the plugin on a real desktop.
The automated checks in spec section 13 run in CI.
They cannot cover capture, because capture needs a compositor, a focused window, and a real selection.
Every smoke item of spec section 13 but item 10 is therefore run by hand, and this page is how.
The Compose walkthrough later on this page is here for the same reason: spec section 13 lists no smoke item for the card itself, only for the chunked Draft of item 5.

## What you need

- Omarchy 4.0.0 or later, because the plugin imports `qs.Ui` and `qs.Commons` from `/usr/share/omarchy/shell`.
- A Rust toolchain, to build the companion CLI.
- `languagetool` from pacman, for a Check that finds anything: `sudo pacman -S languagetool`.
  Without it the popup shows the `engine_unavailable` card instead of marks, which still proves capture works.
  The `harper` engine needs no package and no server, so smoke item 7 works on a bare machine.

`wl-clipboard` and `wtype` already ship with Omarchy.

## 1. Install the plugin

The shell loads third-party plugins from `~/.config/omarchy/plugins/<plugin-id>/`.
A plain clone is the recommended way, because the shell watches that directory and reloads plugin code when a file changes.

```bash
git clone --branch fm/grammachy-huf-198 <repo-url> ~/.config/omarchy/plugins/io.github.jyooi.grammachy
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

Expected: the popup opens with the `empty_selection` card of spec section 8.
It reads `Nothing selected` over `Highlight some text, then press SUPER + G.`, with `Close` and `Open Compose`.
`Open Compose` opens the Compose card on the kept Draft.
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

Now finish the item on the handover, which is the half spec section 2 puts on this card.

Press `Open in Compose`.

Expected: the Compose card opens with the whole 6,000 unit selection as the Draft, not the first 5,000.
The hero reads `draft, 6,000 units`.
Nothing under the text area refuses the Check, because a Draft over one Check is checked in Chunks.

Press `Check`.

Expected: the hero counts the Chunks, then review mode with the marked text.
Spot-check one Issue in the second half of the text: click its mark and read the inspector.
The original in the inspector is the text under the mark, which is what proves the span of a later Chunk moved by that Chunk's start.

Run it once more with a Draft already in Compose, to see the confirm of spec section 2:

1. Open Compose, type `keep me`, and press Esc.
2. Select the 6,000 unit text again and press `Open in Compose` from the too-long card.

Expected: the confirm card rather than the new Draft.
It reads `Replace the draft?`, names both sizes, and offers `Keep the draft` and `Replace it`.
`Keep the draft` leaves `keep me` in the text area; `Replace it` puts the selection there.
Clear the Draft and repeat: with an empty Draft the selection lands straight away and no confirm appears.

The Compose button in the popup header carries the selection the same way, even a short one.

## 9. Smoke item 5: a 20,000 unit Draft, with progress, Cancel, and a failure

This is the item chunked checking exists for: a Draft that takes several Chunks, spec section 9.

1. Put a long Draft in the clipboard and paste it into Compose:

   ```bash
   python3 -c "print(('I has two book and she go home every day. ' * 500)[:20000], end='')" | wl-copy
   ```

   Open Compose with SUPER + SHIFT + G, clear whatever is there, and paste with Ctrl + V.

Expected: the hero reads `draft, 20,000 units` and `Check` is on.

2. Press `Check`.

Expected: the hero meta line reads `Checking 1 of n, languagetool, <elapsed>`, with a `Cancel` button beside it.
The number climbs, the elapsed time counts up, and the bar under `Checking chunk k of n...` fills as each Chunk lands.
`n` is what `grammachy chunk` answered; check it by hand with the same Draft:

```bash
python3 -c "print(('I has two book and she go home every day. ' * 500)[:20000], end='')" | bin/grammachy chunk | jq '.chunks | length'
```

3. Let it finish.

Expected: review mode with every Chunk's Issues in one list, sorted, with no gap at a Chunk boundary.
The hero has no `Checked k of n chunks` note, because every Chunk finished.
Walk to the last Issue with the End of the key map (Down until it wraps) and confirm the inspector's original matches the mark under it.

4. Press `Back to edit`, then `Check` again, and press `Cancel` part way through.

Expected: the run stops after the Chunk it was on rather than at once, then review mode opens.
The hero note reads `Checked k of n chunks`, and the marked text carries only the Issues of those k Chunks.
Nothing after the last checked Chunk is marked.

5. The failure path. Stop the LanguageTool unit while a run is walking:

   ```bash
   # In a second terminal, while the progress line is climbing.
   systemctl --user stop grammachy-languagetool
   ```

   The transient unit dies with the session, so nothing here is permanent.
   Stop it with `systemctl --user`, never from a test: only this manual run may touch the unit the live shell uses.

Expected: the run stops on the Chunk that failed and shows the failure inline over what it has.
The card reads `LanguageTool is not running` with the `grammachy doctor` line and the CLI message under it.
The hero note reads `Checked k of n chunks, m issues so far`.
The two buttons are `Retry remaining` and `Review what we have`.

Press `Review what we have`.

Expected: review mode on the Issues of the finished Chunks, with the same `Checked k of n chunks` note.
`Back to edit` returns the Corrected text of those Chunks only, and the rest of the Draft is untouched.

Now run the Check again, stop the unit again, and this time let it start back up:

```bash
systemctl --user start grammachy-languagetool
```

Press `Retry remaining`.

Expected: the run resumes at the Chunk that failed.
The progress line picks up at that number rather than at `1`, no Chunk before it is checked twice, and the finished run has the whole Draft's Issues.

## 10. Smoke item 6: the engine_unavailable card, then Retry

This item proves the error cards of spec section 8: the card names the engine, the `grammachy doctor` line names the missing piece, and Retry re-runs the Check on the same Selection.

The transient unit dies with the session, so nothing here is permanent.
Stop it with `systemctl --user`, never from a test: only this manual run may touch the unit the live shell uses.

1. Make sure `Engine` reads `LanguageTool` in Settings, which is the default.
2. Run one Check so the unit is up, then stop it:

```bash
systemctl --user stop grammachy-languagetool
systemctl --user is-active grammachy-languagetool
```

3. Highlight a sentence with a mistake and click `G`.

Expected: the card reads `LanguageTool is not running` over `Grammachy could not reach it on this machine.`.
Under that comes the one-line `grammachy doctor` diagnosis, and under that the CLI message in monospace, which names the address that stayed silent.
The hero meta line reads `engine not reachable`.
The buttons are `Close`, `Retry`, and `Settings`, with `Retry` in the accent colour.
Compare the diagnosis with the line the CLI prints for itself:

```bash
bin/grammachy doctor --engine languagetool --json | jq -r .diagnosis
```

4. Leave the popup open and highlight a **different** sentence in the terminal.
5. Click `Retry`.

Expected: the popup checks the sentence from step 3, not the one highlighted in step 4.
Spec section 8 says Retry re-runs the Check with the same Selection and never captures again.
The unit is still down, so the same card comes back.

6. Start the unit again and click `Retry` once more:

```bash
systemctl --user start grammachy-languagetool
```

Expected: the Check succeeds and the marked text of smoke item 1 replaces the card.
A first start takes a moment, so a `LanguageTool took too long` card on the first Retry is the `engine_timeout` card doing its job; Retry again once the port answers.

Two more cards belong to the same session:

- **Settings from a card.** Bring the `engine_unavailable` card back, click the gear or `Settings`, then click `Back`.
  The same card is still behind the Settings view, because Settings opens the Settings view of the same card.
  Switch `Engine` to `Harper` and click `Retry`: the Check now runs in process and succeeds.
- **No companion binary.** Move `bin/grammachy` aside, reload the plugin, and click `G` on a selection.
  The card reads `Grammachy could not run the check` with `Close` and `Setup`.
  `Setup` shows the setup notice until the setup card lands.
  Put the binary back and reload.

## 11. Smoke item 7: switch to Harper, Check, switch back

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

## 12. Smoke item 8: auto-replace in a terminal and a browser field

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

## 13. Smoke item 9: settings persist across a shell restart

1. Open Settings and set `Native language` to `Malay`, `Engine` to `Harper`, and `Auto-replace` on.
2. Restart the shell:

```bash
omarchy restart shell
```

3. Click `G` on a selection and open Settings again.

Expected: `Malay`, `Harper`, and `Auto-replace` on, all as they were left.
The plugin keeps no state of its own: the values come back because `~/.config/omarchy/shell.json` holds them, and the shell reads that file at start.
`targetEnglish` and `openaiApiKey` have no control, so check by hand that an edit of those two keys in the file also survives a round trip through the Settings view.

## 14. The Compose card

Compose is spec section 9.
It captures nothing: it holds a Draft, checks it only when you ask, and reviews the answer with the same hero, inspector, footer, and keys as the popup.
Smoke item 5 above covers the chunked Draft; this walkthrough is the manual check of everything else on the card.

Every trigger of spec section 2 opens it.
The four to try are:

```bash
omarchy-shell shell summon io.github.jyooi.grammachy '{"mode":"compose"}'
omarchy-shell shell summon io.github.jyooi.grammachy '{"mode":"compose","text":"I has two book."}'
```

SUPER + SHIFT + G once `grammachy setup` has written the bindings, and the `Grammachy compose` row of the Omarchy menu.
The first two commands and both of those open the kept Draft; only the payload with a `text` brings its own, and only after the confirm when a Draft is already there.

### The Draft and the Check

1. Type `I has two book. She dont like it.` into the text area.

Expected: the card is centred over a dimmed desktop, about 900 px wide and 80 percent of the screen high.
The hero reads `draft, 33 units` and counts up as you type.
There is no auto-replace toggle, because auto-replace never applies here.
`Clear` and `Check` turn on the moment the Draft is not empty.

2. Press `Check`, or Ctrl + Enter.

Expected: the hero shows `Checking 1 of 1` for a moment, then the card switches to review mode.
A Draft this short is one Chunk, so the progress goes by too fast to read; smoke item 5 is where it is worth watching.
The marked text, the inspector strip, the counts, and `Accept all open` all behave as they do in the popup, and the whole key map of smoke item 1 works on them.
The Apply button reads `Copy corrected text` and never `Replace selection`, whatever the auto-replace setting says.

3. Accept every Issue, press `Copy corrected text`, and paste somewhere.

Expected: the clipboard holds `I have two books. She does not like it.`

4. Press `Back to edit`, or Esc.

Expected: edit mode again, with the Corrected text as the new Draft.
The unit count in the hero matches the new length.

### The Draft survives a close

1. Type a Draft and press Esc.

Expected: the card closes and nothing is written anywhere.
Confirm that with `git status` in the plugin folder and with `jq . ~/.config/omarchy/shell.json`; neither changes.

2. Open Compose again.

Expected: the same Draft, with the caret in the text area.
`Clear` empties it, and the count returns to `0 units`.

3. Restart the shell with `omarchy restart shell`, then open Compose again.

Expected: an empty Draft.
The Draft lives in memory for one shell run only, spec section 9.

### The cap

The cap of spec section 9 is the one size Compose refuses.

1. Paste a Draft of 50,001 units:

   ```bash
   python3 -c "print('x' * 50001, end='')" | wl-copy
   ```

   Open Compose and paste with Ctrl + V.

Expected: the hero reads `draft, 50,001 units`, the line under the text area reads `The draft is 50,001 units, over the cap of 50,000.`, and `Check` is off.
Trim the Draft and `Check` turns back on.

2. Paste a Draft of about 6,000 units, over what one Check takes.

Expected: no refusal at all, and `Check` is on.
Anything under the cap is checked in Chunks, which is smoke item 5.

### The replace confirm

Spec section 2: a trigger that carries a text replaces a non-empty Draft only after a confirm.

1. Open Compose, clear the Draft, and close it.
2. Run the payload command above with a `text`.

Expected: the text lands as the Draft straight away, with no confirm, because there was nothing to lose.

3. Run the same command a second time, with that Draft still in place.

Expected: the confirm card.
It reads `Replace the draft?`, names the size of each, and offers `Keep the draft` and `Replace it`.
Esc closes the card and keeps the Draft, the same as `Keep the draft`.

### Esc, the backdrop, and the gear

| Where | Esc | A click outside the card |
|---|---|---|
| Edit mode | Closes and keeps the Draft | Closes and keeps the Draft |
| The replace confirm | Closes and keeps the Draft | Closes and keeps the Draft |
| A chunked run | Closes and stops after the Chunk in flight | The same |
| An inline Chunk failure | Closes and keeps the Draft | Closes |
| Review mode | Back to edit, with the Corrected text | Closes |

The gear flips Compose to the same Settings view as the popup, and `Back` returns to whichever mode was on screen.

## 15. Running the automated checks

The same three plugin checks CI runs, against the shell installed on this machine:

```bash
for file in $(find . -name '*.qml' | sort); do qmllint -I /usr/share/omarchy/shell "$file" || echo "FAILED $file"; done
omarchy-plugin-validate .
node --test ui/splice.test.js ui/tokens.test.js ui/settings.test.js ui/keymap.test.js ui/errors.test.js ui/format.test.js
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

## 16. Removing it

Remove the hotkeys and the menu entry first (spec section 10):

```bash
~/.config/omarchy/plugins/io.github.jyooi.grammachy/bin/grammachy setup --remove
omarchy plugin disable io.github.jyooi.grammachy
rm -rf ~/.config/omarchy/plugins/io.github.jyooi.grammachy
omarchy-shell shell rescanPlugins
```
