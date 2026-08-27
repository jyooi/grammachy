# Grammachy

Grammachy is an Omarchy plugin that checks the grammar and spelling of text on demand.

Highlight text in any application and press SUPER + G.
A popup marks every Issue on the Selection.
Accept or Skip each Fix, then Apply the Corrected text through the clipboard or, if you opt in, straight back into the Selection.
For longer text, press SUPER + SHIFT + G to open the Compose window, which checks a Draft in Chunks.

Grammachy is offline by default.
Text leaves the machine only through the opt-in cloud engine, only to `openrouter.ai`, and only after you give consent.
Grammachy never checks while you type.
Every Check is an explicit Trigger.

## Engines

| Engine | What it runs | Text leaves the machine |
|---|---|---|
| LanguageTool | LanguageTool 6.6 from pacman, the default | No |
| Local LLM | Any OpenAI-compatible endpoint on loopback, normally llama.cpp | No |
| Harper | `harper-core` in process, no server and no download | No |
| Cloud LLM (OpenRouter) | Any model on OpenRouter, opt in | Yes, to `openrouter.ai` only |

Pick one in Settings.
There is no automatic fallback: an engine that cannot answer says so, and you switch.

## Recommended models

The benchmark files under `docs/benchmarks/` decide these two lines, and every tag decides them again.
`docs/spec/evals.md` section 5 is the rule and `cli/src/bench/weights.rs` is the code that holds it.

- **Local LLM model**: `qwen3.8-4b`, with thinking on.
  It is the Settings default.
  Download it in Settings, Models, or run `grammachy model download qwen3.8-4b`.
- **Cloud LLM model**: `google/gemini-3.7-flash`.
  It is the `openrouterModel` default.
  The cloud engine is never the default engine.

### How a model earns those lines

Rows are ranked by exact fix rate on the eval set, then by F0.5, then by lower p50 latency.
When the judge of `docs/spec/evals.md` section 4.4 clears its gate, exact fix plus useful non-exact fixes ranks instead.

Two floors apply to every row.
A row with more false positives than the default engine is never recommended.
Neither is a row whose validity is under 95%.

The recommended local model clears four more bars.

- Its weights are Apache-2.0 or MIT.
- Its weights file on disk is at or under 4 GB, the on-device target.
- Its measured resident memory fits the 8 GB tier.
- It ran with thinking on, the product default.

A larger model such as `gemma-4-e4b-it` at 4.98 GB stays in the catalogue and in the benchmark tables as a reference result.
You may pick it in Settings, and the rules never make it the default.

The recommended cloud model is the best `openrouter` row with no cost ceiling.
Beside it, a benchmark file names a value cloud model when one exists: the cheapest row within 10 points of exact fix of the recommended one.
That keeps the cost trade-off visible.
A run that names no value line says why, so a reader can tell "nothing was cheaper" from "nothing was priced".

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
- `docs/spec/evals.md`: the eval sets, the metrics, the `bench` runner, and the recommendation rules.
- `docs/doctor.md`: the `doctor` envelope, exit code, and hardware tiers.
- `docs/benchmarks/`: one file per run, printed by `grammachy bench` and never edited by hand.
- `docs/adr/`: the settled decisions.
- `CONTEXT.md`: the domain glossary.

## Licence

MIT. See `LICENSE`.

The eval set is the CLC FCE corpus under its own non-commercial licence.
This repository redistributes no corpus text: `grammachy bench --eval-set` fetches it at run time.
ADR 0003 records that stance.
