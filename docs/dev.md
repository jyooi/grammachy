# Running Grammachy in a live Omarchy shell

This page is for a developer or a reviewer who wants the plugin on a real desktop.
The automated checks in spec section 13 run in CI.
They cannot cover capture, because capture needs a compositor, a focused window, and a real selection.
Smoke items 1 and 2 of spec section 13 are therefore run by hand, and this page is how.

## What you need

- Omarchy 4.0.0 or later, because the plugin imports `qs.Ui` and `qs.Commons` from `/usr/share/omarchy/shell`.
- A Rust toolchain, to build the companion CLI.
- `languagetool` from pacman, for a Check that finds anything: `sudo pacman -S languagetool`.
  Without it the popup shows the notice card instead of marks, which still proves capture works.

`wl-clipboard` and `wtype` already ship with Omarchy.

## 1. Install the plugin

The shell loads third-party plugins from `~/.config/omarchy/plugins/<plugin-id>/`.
A plain clone is the recommended way, because the shell watches that directory and reloads plugin code when a file changes.

```bash
git clone --branch fm/grammachy-huf-190 <repo-url> ~/.config/omarchy/plugins/io.github.jyooi.grammachy
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

## 6. Smoke item 2: an Electron text field

Electron applications do not fill the primary selection, so this item exercises the Ctrl + C fallback in spec section 3.

1. Open an Electron application with a text field, for example VS Code, Slack, or Obsidian.
2. Type a sentence with a mistake into the field and select it with the mouse.
3. Click the `G` button on the bar.

Expected: the same popup as smoke item 1.
The plugin saves the clipboard, sends Ctrl + C to the field, reads the result, and puts the old clipboard back.
Check the restore: copy some other text first, then run the item, then paste somewhere.
The paste must give the text you copied first, not the sentence, until you press `Copy corrected text`.

## 7. Running the automated checks

The same three checks CI runs, against the shell installed on this machine:

```bash
for file in $(find . -name '*.qml' | sort); do qmllint -I /usr/share/omarchy/shell "$file" || echo "FAILED $file"; done
omarchy-plugin-validate .
node --test ui/splice.test.js ui/tokens.test.js
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

## 8. Removing it

```bash
omarchy plugin disable io.github.jyooi.grammachy
rm -rf ~/.config/omarchy/plugins/io.github.jyooi.grammachy
omarchy-shell shell rescanPlugins
```
