# Grammachy

Grammachy is an Omarchy plugin that checks the grammar and spelling of text on demand.

Highlight text in any application and press SUPER + G.
A popup marks every Issue on the Selection.
Accept or Skip each Fix, then Apply the Corrected text through the clipboard or, if you opt in, straight back into the Selection.
For longer text, press SUPER + SHIFT + G to open the Compose window, which checks a Draft in Chunks.

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

### Adding LanguageTool

LanguageTool catches a little more than Harper and costs a great deal more: about 250 MB to download, a Java runtime, and about 1 GB of memory while it runs.
So it is something you add rather than something you get.

Open Settings, Engines and press Install beside LanguageTool.
It unpacks the upstream release into `~/.local/share/grammachy/engines/languagetool/`, so no password is asked for.
Remove takes that directory away again, and the engine falls back to Harper.
`grammachy engine list | install languagetool | remove languagetool` does the same from a terminal.

It needs a Java runtime beside it: `sudo pacman -S jre-openjdk`.
If you already installed the Arch `languagetool` package, Grammachy uses that and offers no Install; `grammachy engine remove` never touches a pacman package.

## Install

Clone into the Omarchy plugin directory, build the companion binary, write the hotkeys, and enable the plugin.

```bash
git clone <repo-url> ~/.config/omarchy/plugins/io.github.jyooi.grammachy
cd ~/.config/omarchy/plugins/io.github.jyooi.grammachy/cli
cargo build --release
mkdir -p ../bin && cp target/release/grammachy ../bin/grammachy
../bin/grammachy setup
omarchy-shell shell rescanPlugins
omarchy plugin enable io.github.jyooi.grammachy
```

`grammachy setup` writes the two hotkeys and the menu entry, then reloads Hyprland.
`omarchy plugin enable` turns on the bar button and the overlay.
`grammachy doctor` reports what each engine still needs and names the exact command that installs it.
Doctor installs nothing: pacman steps stay manual.

`docs/dev.md` is the full walkthrough, including the manual smoke items.

## Documentation

- `docs/spec/v1.md`: the v1 contract for every surface, engine, and envelope.
- `docs/doctor.md`: the `doctor` envelope and exit code.
- `docs/adr/`: the settled decisions.
- `CONTEXT.md`: the domain glossary.

## Licence

MIT. See `LICENSE`.
