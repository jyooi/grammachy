# `grammachy doctor`

The install check of spec sections 4, 8, 10, and 12.
It checks the binary, LanguageTool, llama.cpp, the model file, and the two transient units.
It also checks the Java runtime and the local LLM endpoint.
It prints one line per piece.
A missing package carries the exact command that installs it.

Doctor never installs anything.
pacman steps stay manual.

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
  missing  llama.cpp server    llama.cpp is not installed: /usr/bin/llama-server does not exist. Run: sudo pacman -S llama-cpp ggml-vulkan
  missing  Model weights       No weights for gemma-4-e4b-it in /home/u/.local/share/grammachy/models. Run: grammachy setup
  ok       Local LLM endpoint  127.0.0.1:8080
  ok       LanguageTool unit   grammachy-languagetool is not running. The next Check starts it.
  ok       llama.cpp unit      grammachy-llama is not running. The next Check starts it.

Hardware tier discrete-gpu, so llama.cpp wants ggml-vulkan.
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
  "diagnosis": "llama.cpp is not installed: /usr/bin/llama-server does not exist. Run: sudo pacman -S llama-cpp ggml-vulkan",
  "hardwareTier": "discrete-gpu",
  "backendPackage": "ggml-vulkan",
  "checks": [
    {
      "id": "llama.cpp",
      "name": "llama.cpp server",
      "ok": false,
      "detail": "llama.cpp is not installed: /usr/bin/llama-server does not exist.",
      "remedy": "sudo pacman -S llama-cpp ggml-vulkan",
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
- `backendPackage`: the ggml package that tier wants beside `llama-cpp`.
- `checks`: one entry per piece, in the order the text report prints them.

Check fields:

- `id`: stable across releases, never shown to a user. The ids are `binary`, `languagetool`, `java`, `llama.cpp`, `model`, `endpoint`, `unit:languagetool`, and `unit:llama`.
- `name`: the display name.
- `ok`: whether the piece is in place.
- `detail`: one sentence saying what was found, or what is missing.
- `remedy`: the exact command that fixes it. The key is absent when there is nothing to run.
- `engines`: the slugs that need this piece. `harper` needs only `binary`, because it runs in process.

## The engine diagnosis

| Slug | Pieces it needs |
|---|---|
| `languagetool` | `binary`, `languagetool`, `java`, `unit:languagetool` |
| `openai` | `binary`, `llama.cpp`, `model`, `endpoint`, `unit:llama` |
| `harper` | `binary` |
| `openrouter` | none that `doctor` reads yet |

The first missing piece in that order is the diagnosis.
When nothing is missing, the diagnosis says the engine can run.
`doctor` cannot read the cloud key yet, so `openrouter` always reports `ready: false` and names the key file.
For `languagetool` and `openai` it also names the address its unit answers on.

## Hardware tiers

Spec section 4: hardware tiers affect only the install step.
The `llama-cpp` package carries no compute backend, so the tier decides the second package on its install line.

The tier is read from the graphics devices under `/sys/class/drm`:

| Tier | Machine | Backend package |
|---|---|---|
| `discrete-gpu` | A graphics card on its own PCIe bus | `ggml-vulkan` |
| `integrated-gpu` | A graphics processor on the CPU package, which answers PCI bus `00` | `ggml-vulkan` |
| `cpu` | Only a framebuffer or a virtual device, or no device at all | `ggml-cpu` |

NPU use stays a documented manual FastFlowLM setup reached through the `openai` adapter, so no tier names it.

## Testing

Detection is injectable.
`doctor::facts::Facts` is a plain value and the report is a pure function of it, so every test writes the machine it wants and reads the exact lines back.
`Facts::collect` is the only function that touches the real machine, and no test calls it.
