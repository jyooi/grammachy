# `grammachy doctor`

The install check of spec sections 4, 8, 10, and 12.
It checks the binary, LanguageTool, the Java runtime, and its transient unit.
It prints one line per piece.
A missing package carries the exact command that installs it.
Under the pieces it prints the dependency table: every system package the plugin leans on, with its state.

Doctor never installs anything.
The plugin runs no `sudo` and no `pacman` itself.
Every system package goes through `omarchy pkg add`, which the setup card and the Engines page launch in a visible terminal.

A piece the machine simply has not added reads `optional` rather than `missing`.
LanguageTool is the one such piece today (HUF-237): it is an engine the user adds in Settings, Engines, so a fresh install that never asked for it is not a broken install.
Its line names `grammachy engine install languagetool`, which writes one directory under HOME and needs no password.
`ok` still answers the engine question, so `doctor --engine languagetool` on such a machine still exits 1.

```
grammachy doctor [--engine <slug>] [--json]
```

`--engine` picks the engine the diagnosis is about.
Omitted, it resolves the same way a Check does: the flag, then the plugin entry in `shell.json`, then the default `harper` (spec section 7).

## Exit code

Exit 0 when every piece the chosen engine needs is in place.
Exit 1 when one is missing.

A piece another engine needs never fails the run.
A user who checks with Harper owes nothing to LanguageTool, so a missing LanguageTool is a line in the report and not a failure.

## Text output

```
Grammachy doctor

  ok       Grammachy CLI       grammachy 0.1.0 at /home/u/plugin/bin/grammachy
  optional LanguageTool        LanguageTool is optional and is not installed. Add it in Settings, Engines. Run: grammachy engine install languagetool
  optional Java runtime        No Java runtime: JAVA_HOME is not set and no default JVM is installed. Run: omarchy pkg add jre-openjdk
  ok       LanguageTool unit   grammachy-languagetool is not running. The next Check starts it.

Dependencies

  ok       curl                bin/bootstrap.sh downloads the pinned companion binary with it.
  missing  wl-clipboard        Capture, paste, and the restored Selection all go through wl-copy and wl-paste. Run: omarchy pkg add wl-clipboard
  ok       libarchive          grammachy engine install unpacks the LanguageTool release with bsdtar.
  optional jre-openjdk         LanguageTool runs on it, and Harper needs none. Run: omarchy pkg add jre-openjdk

Engine harper is ready.
  Harper runs inside the companion binary and needs nothing installed.

Run the commands above yourself. Doctor installs nothing.
```

A required package that is absent reads `missing`, and an optional one reads `optional`.
Neither moves the engine answer or the exit code: `ready` is about the engine, and the setup card is what refuses a bootstrap without `curl`.

A stopped unit is not a fault.
The transient unit dies with the session and the next Check starts it again (spec section 4), so `doctor` reports the state and moves on.
A `systemctl --user` that does not answer at all is a fault, because then nothing can start a unit.

## The envelope

`--json` prints the same report as one JSON object on one line, which is what the shell calls.
Spec section 8 puts the `diagnosis` line under the body of the `engine_unavailable` card.

```json
{
  "contractVersion": 1,
  "engine": "languagetool",
  "ready": false,
  "diagnosis": "LanguageTool is optional and is not installed. Add it in Settings, Engines. Run: grammachy engine install languagetool",
  "checks": [
    {
      "id": "languagetool",
      "name": "LanguageTool",
      "ok": false,
      "optional": true,
      "detail": "LanguageTool is optional and is not installed. Add it in Settings, Engines.",
      "remedy": "grammachy engine install languagetool",
      "state": "absent",
      "engines": ["languagetool"]
    }
  ],
  "dependencies": [
    {
      "name": "curl",
      "package": "curl",
      "purpose": "bin/bootstrap.sh downloads the pinned companion binary with it.",
      "required": true,
      "present": true,
      "installCommand": "omarchy pkg add curl",
      "usedBy": ["bootstrap"]
    }
  ]
}
```

Fields:

- `contractVersion`: the same `1` every envelope of spec section 5 carries.
- `engine`: the slug the diagnosis is about.
- `ready`: whether every piece that engine needs is in place. It matches the exit code.
- `diagnosis`: the one line the error card shows. It is the first missing piece of that engine, or a sentence saying the engine can run.
- `checks`: one entry per piece, in the order the text report prints them.
- `dependencies`: one entry per system package, in the order the text report prints them.

Check fields:

- `id`: stable across releases, never shown to a user. The ids are `binary`, `languagetool`, `java`, and `unit:languagetool`.
- `name`: the display name.
- `ok`: whether the piece is in place.
- `optional`: whether a piece that is not `ok` is one the machine simply has not added rather than one it is missing. Only the `languagetool` and `java` checks are ever `true`, and only while LanguageTool is absent.
- `detail`: one sentence saying what was found, or what is missing.
- `remedy`: the exact command that fixes it. The key is absent when there is nothing to run.
- `state`: the stable word for which state that piece is in. Only the `languagetool` check carries one, and the field is absent everywhere else.
- `engines`: the slugs that need this piece. `harper` needs only `binary`, because it runs in process.

The `languagetool` state word says which of the two routes put LanguageTool on this machine, because only one of them is one `grammachy engine remove` can take away again.

| `state` | `ok` | `optional` | What it says |
|---|---|---|---|
| `installed` | true | false | `grammachy engine install languagetool` unpacked it under `~/.local/share/grammachy/engines/languagetool/`. This is the one the adapter runs and the one Remove deletes. |
| `package` | true | false | The Arch `languagetool` package supplies it. Grammachy never installs it and never removes it. |
| `absent` | false | true | Neither is here. The remedy adds it without a password. |

## The dependency table

`dependencies` is the one list of system packages the plugin leans on.
The shell reads it for the setup card and the Engines page, and `ui/deps.js` carries the same rows for the moment before `bin/grammachy` exists.
`cli/tests/overlay_deps.rs` keeps the two equal, and `cli/tests/readme_dependencies.rs` keeps the README section equal to both.

| `package` | `name` | `required` | `usedBy` | What says it is present |
|---|---|---|---|---|
| `curl` | curl | true | `bootstrap` | `curl` on `PATH` |
| `wl-clipboard` | wl-clipboard | true | `capture` | `wl-copy` on `PATH` |
| `libarchive` | libarchive | false | `languagetool` | `bsdtar` on `PATH` |
| `jre-openjdk` | Java runtime | false | `languagetool` | the same runtime the `java` check finds, through `JAVA_HOME` or the default JVM |

Dependency fields:

- `name`: the display name.
- `package`: the Arch package name.
- `purpose`: one sentence saying what the plugin does with it.
- `required`: whether the plugin cannot work without it. `libarchive` and `jre-openjdk` are optional, because only LanguageTool needs them.
- `present`: whether it is on this machine.
- `installCommand`: exactly `omarchy pkg add <package>`.
- `usedBy`: which parts need it, from `bootstrap`, `capture`, and `languagetool`.

## The engine diagnosis

| Slug | Pieces it needs |
|---|---|
| `languagetool` | `binary`, `languagetool`, `java`, `unit:languagetool` |
| `harper` | `binary` |

The first missing piece in that order is the diagnosis.
When nothing is missing, the diagnosis says the engine can run.
For `languagetool` it also names the address its unit answers on.

## Testing

Detection is injectable.
`doctor::facts::Facts` is a plain value and the report is a pure function of it, so every test writes the machine it wants and reads the exact lines back.
`Facts::collect` is the only function that touches the real machine, and no test calls it.
