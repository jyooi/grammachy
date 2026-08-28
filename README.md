# Grammachy

Grammachy is an Omarchy plugin that checks the grammar and spelling of text on demand.

Highlight text in any application and press SUPER + SHIFT + Q.
A popup marks every Issue on the Selection.
Accept or Skip each Fix, then Apply the Corrected text through the clipboard or, if you opt in, straight back into the Selection.
For longer text, press SUPER + ALT + Q to open the Compose window, which checks a Draft in Chunks.
Change either key in Settings.

Grammachy is offline.
No engine sends the text of a Check off the machine.
Grammachy never checks while you type.
Every Check is an explicit Trigger.

## Engines

| Engine | What it runs | Text leaves the machine |
|---|---|---|
| Harper | `harper-core` in process, no server and no download. The default | No |
| LanguageTool | LanguageTool 6.6, added from Settings, Engines | No |

Pick one in Settings.
There is no automatic fallback: an engine that cannot answer says so, and you switch.

A fresh install checks with Harper.
It is compiled into the binary, so nothing is downloaded and no `pacman` command is needed to run the first check.

The honest cost: Harper catches about one in eight learner errors with zero false positives, in process, in about 14 MB.
LanguageTool catches about one in seven, for a 250 MB download, a Java runtime, and about 1 GB resident.
That gap keeps LanguageTool opt in.

### Adding LanguageTool

Open Settings, Engines and press Install beside LanguageTool.
It unpacks the upstream release into `~/.local/share/grammachy/engines/languagetool/`, so no password is asked for.
Remove takes that directory away again, and the engine falls back to Harper.
`grammachy engine list | install languagetool | remove languagetool` does the same from a terminal.

It needs a Java runtime beside it: `sudo pacman -S jre-openjdk`.
If you already installed the Arch `languagetool` package, Grammachy uses that and offers no Install; `grammachy engine remove` never touches a pacman package.

## Install

1. Add the plugin.
   This clones the repository and runs `omarchy-plugin-validate`, and nothing else.

   ```bash
   omarchy plugin add <repo-url>
   ```

2. Click the Grammachy bar widget.
   The setup card names the pinned binary and its sha256.
   Click Install, and watch `bin/bootstrap.sh` fetch and verify it.
3. Highlight text and press SUPER + SHIFT + Q.
   The first Check runs on Harper, compiled into the binary, so nothing downloads and no `pacman` command runs.
4. Run `bin/grammachy setup` from the plugin folder.
   This writes the two hotkeys and the menu entry, then reloads Hyprland.
   `bin/grammachy setup --remove` takes them out again.
   An install that already ran `setup` keeps its old keys until it runs `setup` again.
5. Optional: add LanguageTool.
   Open Settings, Engines and press Install beside LanguageTool.
   It needs a Java runtime: `sudo pacman -S jre-openjdk`.

`grammachy doctor` reports what each engine still needs and names the exact command that installs it.
Doctor installs nothing: pacman steps stay manual.

`docs/dev.md` is the full walkthrough, including the manual smoke items.

### Setting the hotkeys by hand

`bin/grammachy setup` writes the trigger hotkeys into `~/.config/hypr/bindings.lua`, between `-- grammachy begin` and `-- grammachy end`, then reloads Hyprland.
The keys are the Settings values `quickHotkey` and `composeHotkey`.
This is the default block.
Paste it yourself if you do not run the command:

```lua
hl.unbind("SUPER + SHIFT + Q")
o.bind("SUPER + SHIFT + Q", "Grammachy", [[omarchy-shell shell summon io.github.jyooi.grammachy '{"mode":"quick"}']])
hl.unbind("SUPER + ALT + Q")
o.bind("SUPER + ALT + Q", "Grammachy compose", [[omarchy-shell shell summon io.github.jyooi.grammachy '{"mode":"compose"}']])
```

### Developer path

Build the companion binary from source instead of downloading the pinned release, then copy it in.

```bash
git clone <repo-url> ~/.config/omarchy/plugins/io.github.jyooi.grammachy
cd ~/.config/omarchy/plugins/io.github.jyooi.grammachy/cli
cargo build --release
mkdir -p ../bin && cp target/release/grammachy ../bin/grammachy
../bin/grammachy setup
omarchy-shell shell rescanPlugins
omarchy plugin enable io.github.jyooi.grammachy
```

The setup card offers this path too whenever `cli.lock` carries no pinned release.
See `docs/dev.md` for cutting a release and pinning `cli.lock`.

## Documentation

- `docs/spec/v1.md`: the v1 contract for every surface, engine, and envelope.
- `docs/doctor.md`: the `doctor` envelope and exit code.
- `CONTEXT.md`: the domain glossary.

## Licence

MIT. See `LICENSE`.
