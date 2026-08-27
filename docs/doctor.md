# `grammachy doctor`

The install check of spec sections 4, 8, 10, and 12.
It checks the binary, LanguageTool, llama.cpp, the model file, and the two transient units.
It also checks the Java runtime, the llama.cpp compute backend, the local LLM endpoint, and the OpenRouter key file.
It prints one line per piece.
A missing package carries the exact command that installs it.

Doctor never installs anything.
pacman steps stay manual.

Missing weights are the one piece a user fixes without a terminal.
Settings, Models downloads, picks, and removes any catalogue model (spec section 5.3).
`grammachy model download <name>` is the same step from a shell.
Doctor names that command only for a catalogue name.
The `openaiModel` field takes any name, and a name the catalogue does not carry has no download.
For such a name the detail says to place the `.gguf` file by hand, or to pick a catalogue model.

A Check on the `openai` engine reads the model the server serves before it sends anything, and a server that holds other weights than `openaiModel` names is reloaded or refused with `bad_arguments` (HUF-236).
Doctor reports the install state and never that comparison, because the answer belongs to one Check and to one base URL.

The weights the `model` check names are the `openaiModel` setting, whose default is the recommended local model.
That name comes from the benchmark tables by the rules of `docs/spec/evals.md` section 5, which `cli/src/bench/weights.rs` holds.
The recommended local model is Apache-2.0 or MIT, its weights file is at or under 4 GB, its measured resident memory fits the 8 GB tier, and it ran with thinking on.
The catalogue keeps a larger row such as `gemma-4-e4b-it` for reference.
A user may still pick it, and the rules never make it the default.

```
grammachy doctor [--engine <slug>] [--json]
```

`--engine` picks the engine the diagnosis is about.
Omitted, it resolves the same way a Check does: the flag, then the plugin entry in `shell.json`, then the default `languagetool` (spec section 7).

## Exit code

Exit 0 when every piece the chosen engine needs is in place.
Exit 1 when one is missing.

A piece another engine needs never fails the run.
A user who checks with LanguageTool owes nothing to llama.cpp, so a missing llama.cpp is a line in the report and not a failure.

## Text output

```
Grammachy doctor

  ok       Grammachy CLI       grammachy 0.1.0 at /home/u/plugin/bin/grammachy
  ok       LanguageTool        /usr/bin/languagetool
  ok       Java runtime        /usr/lib/jvm/default/bin/java
  missing  llama.cpp server    llama.cpp is not installed: /usr/bin/llama-server does not exist. Run: sudo pacman -S llama-cpp ggml-cpu ggml-vulkan
  missing  llama.cpp backend   llama.cpp is missing the ggml-cpu and ggml-vulkan backends. It needs ggml-cpu to answer at all. Run: sudo pacman -S ggml-cpu ggml-vulkan
  missing  Model weights       No weights for qwen3.8-4b in /home/u/.local/share/grammachy/models. Run: grammachy model download qwen3.8-4b
  ok       Local LLM endpoint  127.0.0.1:8080
  missing  OpenRouter key      No OpenRouter key: /home/u/.config/grammachy/openrouter-key does not exist. Run: printf '%s' "$KEY" | grammachy setup --openrouter-key
  ok       LanguageTool unit   grammachy-languagetool is not running. The next Check starts it.
  ok       llama.cpp unit      grammachy-llama is not running. The next Check starts it.

Hardware tier discrete-gpu, so llama.cpp wants ggml-cpu and ggml-vulkan.
Engine languagetool is ready.
  LanguageTool is installed. The next Check starts it on 127.0.0.1:8081, which takes a moment.

Run the commands above yourself. Doctor installs nothing.
```

A stopped unit is not a fault.
Transient units die with the session and the next Check starts them again (spec section 4), so `doctor` reports the state and moves on.
A `systemctl --user` that does not answer at all is a fault, because then nothing can start a unit.

## The envelope

`--json` prints the same report as one JSON object on one line, which is what the shell calls.
Spec section 8 puts the `diagnosis` line under the body of the `engine_unavailable` card.

```json
{
  "contractVersion": 1,
  "engine": "openai",
  "ready": false,
  "diagnosis": "llama.cpp is not installed: /usr/bin/llama-server does not exist. Run: sudo pacman -S llama-cpp ggml-cpu ggml-vulkan",
  "hardwareTier": "discrete-gpu",
  "backendPackages": ["ggml-cpu", "ggml-vulkan"],
  "checks": [
    {
      "id": "llama.cpp",
      "name": "llama.cpp server",
      "ok": false,
      "detail": "llama.cpp is not installed: /usr/bin/llama-server does not exist.",
      "remedy": "sudo pacman -S llama-cpp ggml-cpu ggml-vulkan",
      "engines": ["openai"]
    }
  ]
}
```

Fields:

- `contractVersion`: the same `1` every envelope of spec section 5 carries.
- `engine`: the slug the diagnosis is about.
- `ready`: whether every piece that engine needs is in place. It matches the exit code.
- `diagnosis`: the one line the error card shows. It is the first missing piece of that engine, or a sentence saying the engine can run.
- `hardwareTier`: `discrete-gpu`, `integrated-gpu`, or `cpu`.
- `backendPackages`: the ggml packages that tier wants beside `llama-cpp`. Every tier wants `ggml-cpu`.
- `checks`: one entry per piece, in the order the text report prints them.

Check fields:

- `id`: stable across releases, never shown to a user. The ids are `binary`, `languagetool`, `java`, `llama.cpp`, `backend`, `model`, `endpoint`, `key`, `unit:languagetool`, and `unit:llama`.
- `name`: the display name.
- `ok`: whether the piece is in place.
- `detail`: one sentence saying what was found, or what is missing.
- `remedy`: the exact command that fixes it. The key is absent when there is nothing to run. An `ok` check carries one only as advice, as the backend check does for `ggml-vulkan`.
- `state`: the stable word for which state that piece is in. Only the `key` check carries one, and the field is absent everywhere else.
- `engines`: the slugs that need this piece. `harper` needs only `binary`, because it runs in process.

The `key` check reads the state of the OpenRouter key file and never its contents.
A file another user can read fails the check, and the remedy is a `chmod 600`.
No report line can carry the key itself.

The `key` state word is one of these five.
It is what the shell reads, because `detail` is prose and no contract.

| `state` | `ok` | What it says |
|---|---|---|
| `ready` | true | A key is stored, and no other user can read it. |
| `missing` | false | The key file does not exist. |
| `empty` | false | The key file exists and holds no key. |
| `loose` | false | The key file exists, and a group or another user can read it. |
| `noHome` | false | HOME is not set, so the key file has no path at all. |

A reader that gets no `state` word falls back to the pair `ok` names.
An older binary then degrades rather than breaks.

## The engine diagnosis

| Slug | Pieces it needs |
|---|---|
| `languagetool` | `binary`, `languagetool`, `java`, `unit:languagetool` |
| `openai` | `binary`, `llama.cpp`, `backend`, `model`, `endpoint`, `unit:llama` |
| `harper` | `binary` |
| `openrouter` | `binary`, `key` |

The first missing piece in that order is the diagnosis.
When nothing is missing, the diagnosis says the engine can run.
For `openrouter` the ready line has two forms, because `openrouterModel` has no built-in default.
With a model set it reads `The key is in place and the model is <model>. Checks send text to openrouter.ai.`
With no model set it reads `The key is in place. Set the cloud model in Settings before a Check.`
The second form names no model, because a report that named an empty one would name a Check that cannot run.
For `languagetool` and `openai` it also names the address its unit answers on.

## Hardware tiers

Spec section 4: hardware tiers affect only the install step.
The `llama-cpp` package carries no compute backend, so the tier decides the other packages on its install line.

The tier is read from the graphics devices under `/sys/class/drm`:

| Tier | Machine | Backend packages |
|---|---|---|
| `discrete-gpu` | A graphics card on its own PCIe bus | `ggml-cpu`, `ggml-vulkan` |
| `integrated-gpu` | A graphics processor on the CPU package, which answers PCI bus `00` | `ggml-cpu`, `ggml-vulkan` |
| `cpu` | Only a framebuffer or a virtual device, or no device at all | `ggml-cpu` |

NPU use stays a documented manual FastFlowLM setup reached through the `openai` adapter, so no tier names it.

## The compute backend

The `backend` check is what spec section 4 asks for beyond `/usr/bin/llama-server`.
A server with no backend starts and then answers nothing, which reads as a broken engine rather than as a missing package.

The backend libraries live in `/usr/lib/ggml`.
`ggml-cpu` installs one `libggml-cpu-<microarchitecture>.so` per microarchitecture.
`ggml-vulkan` installs `libggml-vulkan.so`.

Every tier wants `ggml-cpu`, because llama.cpp runs on the CPU the parts no other backend takes.
A GPU tier wants `ggml-vulkan` beside it.
The remedy names only the packages that are missing, and the line names only what is missing.
A machine can carry `ggml-vulkan` and still lack `ggml-cpu`, so no line claims that the machine has no backend at all.

`ggml-cpu` is the requirement and `ggml-vulkan` is the accelerator.
A missing `ggml-cpu` fails the check, because the server then answers nothing.
A GPU machine that has `ggml-cpu` alone passes the check and reads the `ggml-vulkan` line as advice.
That machine runs the engine on the CPU, so failing it would hide the real cause, such as weights that are not downloaded yet.

## Testing

Detection is injectable.
`doctor::facts::Facts` is a plain value and the report is a pure function of it, so every test writes the machine it wants and reads the exact lines back.
`Facts::collect` is the only function that touches the real machine, and no test calls it.
