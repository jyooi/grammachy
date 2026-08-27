# Grammachy evals spec

Settled on the evals map [HUF-202](https://linear.app/huffman/issue/HUF-202) between 2026-08-25 and 2026-08-26.
Each section names the ticket that holds its detail.
This spec fixes the eval set, the metrics, the `bench` runner, the `openrouter` cloud engine, the local engine changes the pilot forced, the model shortlist, and the milestones.
It amends `docs/spec/v1.md` sections 1, 4, 5.2, 6, 7, 10, 11, and 13.1 as stated in section 10.

## 1. Purpose and the two sets

Only the Rust `bench` produces numbers, through the real adapters and `issues::normalise`.
`run.ts` under `.wayfinder/research/` is retired.

There are two sets, and they answer two different questions.

| Set | File | Size | Question | Rule |
|---|---|---|---|---|
| Fixture | `cli/tests/fixtures/interference-30.json` | 40 items (30 interference, 10 correct) | Did a release regress? | The regression gate of v1 13.1, unchanged: the default engine must not drop its catch rate or raise its false positives against the previous benchmark file. It grows only by the 13.1 growth rule. It is never a ranking. |
| Eval set | Fetched at run time, selection committed as a sidecar | 365 items | Which model is recommended? | Ranks models by the rule of section 5. |
| Chunk fixture | `cli/tests/fixtures/chunks/<native>.json` | One Draft of a few paragraphs per native language | Does the local engine finish a full Chunk? | A local-engine regression gate: wall time, validity, recall on a whole Chunk. [HUF-219](https://linear.app/huffman/issue/HUF-219) |

Thirty sentences vary by 13 points of exact fix between identical runs at temperature 0 ([HUF-209](https://linear.app/huffman/issue/HUF-209)).
No ranking claim is made on the fixture.

## 2. Eval set

Source and composition: [HUF-203](https://linear.app/huffman/issue/HUF-203).
Licence stance: [HUF-212](https://linear.app/huffman/issue/HUF-212).

- Source: the CLC FCE dataset, BEA-2019 release `fce_v2.1.bea19`, the only public GEC corpus with a per-writer native-language label, span edits, and a download.
  Citation: Yannakoudakis, Briscoe, Medlock, "A New Dataset and Method for Automatically Grading ESOL Texts", ACL 2011.
- Languages: zh, es, fr, de, pt, ja from FCE.
  `ms` has no public source and stays on the real-user route of the fixture.
- Composition, 365 items: 300 FCE error sentences (50 per language, single-sentence, exactly one edit), 25 error-free FCE sentences as false-positive controls, plus the 40-item fixture.
  Drawn by `cli/src/bench/evalset/` with a fixed seed, at most one item per essay.
  A spelling, orthography, punctuation, or unclassified edit names no mistake this set measures, so a sentence that carries one is dropped whole.
  Keeping the sentence and dropping the edit would leave a mistake that scores a false positive against every engine that finds it.
- Conversion: FCE offsets are character offsets with no astral characters, so they equal UTF-16 units.
  Sentences are split through the M2 alignment, and a sentence is kept when all its edits lie inside it.
  A zero-width missing-word edit is widened onto a word, because the envelope of `docs/spec/v1.md` section 5.1 has no zero-width span.
  Punctuation is not a word, so an edit in front of a stop or a comma widens onto the word before it.
  The ERRANT code of the M2 file becomes the item's `type`.

### 2.1 Licence: fetch, never commit

The FCE licence is non-commercial research use with a 100-word excerpt cap.
The stance, recorded in ADR 0003: the bench is research into which engine to recommend; Grammachy is free software under MIT and is not sold, offered for sale, licensed for money, leased, or rented; the repo redistributes no corpus text.
A commercial fork must drop the fetch step.

- The bench fetches the tarball at run time into a gitignored cache, pinned by sha256 the way `cli/src/model/` pins the weights.
  The first fill prints the licence path and its non-commercial line to stderr once.
  With the cache absent the eval tables are skipped with a reason, never an error.
- The committed sidecar `cli/tests/fixtures/eval-set.sidecar.json` holds ids, document and sentence index, offsets, and error codes only.
  Sentence text and fixes are read from the fetched M2 file at run time.
  No committed file contains FCE text.
- The benchmark file prints tables plus, per model, the ids of missed items.
  Sentence, fix, and model output text live only in the gitignored record file (section 4.3).
- The benchmark file header carries one line: `Eval set: CLC FCE (BEA-2019 v2.1), CLC FCE Dataset Licence, fetched at run time, not redistributed.`
- Fallback if the stance is withdrawn: delete the fetch step and replace the sidecar with hand-written items in the fixture shape.
  Nothing else changes, because both sets share one item shape.

## 3. Item shape and metrics

Definitions: [HUF-205](https://linear.app/huffman/issue/HUF-205).
Every metric is computed from the Issues of one Check and the edits of one item, so two implementations agree to the digit.

Item shape, both sets: `{ id, native, text, edits: [{ start, end, text, fix, type }], expected_text }`.
Offsets are UTF-16 code units into `text`.
A correct sentence has `edits: []` and `expected_text == text`.
The fixture migrates to one-element `edits` arrays.
One loader, one metrics module.

| Metric | Rule |
|---|---|
| Catch rate | An interference sentence is caught when at least one Issue overlaps an expected edit. Per sentence, plain overlap, unchanged from v1 13.1. |
| Pairing | Walk Issues and edits by `start`. An Issue pairs with the first unpaired edit it overlaps, provided the Issue span extends no more than three whitespace-delimited words past the edit on either side. One-to-one. |
| Precision, recall, F0.5 | pairs / Issues, pairs / edits, 1.25PR / (0.25P + R), micro-averaged over the set. |
| Exact fix | Apply every Fix of the Check to `text`; exact when the result equals `expected_text` after collapsing runs of whitespace. Rate over interference sentences. |
| False positives | Correct sentences that earned at least one Issue, counted once per sentence. |
| Style creep | Unpaired Issues on interference sentences, per 100 interference sentences. |
| Valid | A Check is valid when it returned a result envelope. Invalid counts as zero Issues (a miss for catch rate and recall) and is excluded from precision, exact fix, and latency. An Issue dropped for a bad substring lowers validity by 1 / Issues of that Check. |
| p50, p95 latency | Nearest rank over valid Checks: sort ascending, take element ceil(p x n), no interpolation. |
| Resident memory | Measured on the device for a llama-server row: the DRM fdinfo of that process, which reports the memory one DRM client holds. A card names its card memory, an integrated processor names the system memory it maps, and the two pools are never added together. A server with no DRM client, such as a CPU-only build, keeps the RSS of its process, and so does every other server engine. RSS alone is wrong for GPU rows (HUF-209). llama-server `/metrics` is not the source: it is off unless the server runs with `--metrics`, and it carries no memory gauge. The report names the source of every measured row under the table, and a skipped row names none. |
| Cost per 1,000 Checks | Sum of `usage.cost` / priced Checks x 1,000, USD to two decimals. Local rows print `0.00 (local)`; a cloud answer without `usage.cost` prints `n/a` and is logged. |
| Recall by native language | A separate table, one column per language present; a language with fewer than 10 edits prints the raw count. |
| Useful fix | From the judgements file (section 4.4): useful / judged non-exact hits. Printed only with `--judgements`. A thinking-off local row prints `not the product default`, because the judge grades the product default alone. |
| Thinking | Local rows only: `on` or `off`, the mode the row ran under (section 4.1). Cloud rows print `-`. |

Rounding: rates to one decimal, counts as `n of m (rate)`, latency integer ms, memory whole MB below a gigabyte and one decimal above it, cost two decimals.

## 4. The `bench` runner

### 4.1 Flags

| Flag | Meaning |
|---|---|
| `--engine <slug>` | Repeatable. `openrouter` rows require `--max-cost`. |
| `--model <name>` | Repeatable, for `openai`. |
| `--cloud-model <id>` | Repeatable, for `openrouter`. |
| `--max-cost <usd>` | Whole-run cap on the sum of `usage.cost`; required when any `openrouter` row runs, refused otherwise. When the next Check would pass it, the current row ends as `skipped: cost cap <usd> USD reached after N sentences` and the remaining rows skip with the same reason. Cloud rows run beside each other, so a run may pass the cap by at most one Check for each cloud row in flight. |
| `--thinking off\|on\|both` | Local rows only. Default `on`, the product default. The flag decides every local row, so the stored `localThinking` never moves the numbers. `both` runs every local row twice and prints both with a Thinking column. The eval run uses `both`. [HUF-217](https://linear.app/huffman/issue/HUF-217) |
| `--record <dir>` | Writes every Check's answer to `<dir>/checks.json` (section 4.3). |
| `--judgements <file>` | Adds the Useful fix column from a judgements file (section 4.4). |
| `--eval-set` | Runs the eval set tables beside the fixture tables; skipped with a reason when the cache is absent. |

Runner behaviour the pilot fixed or required ([HUF-209](https://linear.app/huffman/issue/HUF-209)):

- One stderr progress line per sentence; a silent 40-minute command is unacceptable.
- Cloud rows run in parallel with each other and with local rows.
- HTTP 429 and 5xx on cloud rows retry once before counting as invalid.
- A llama-server HTTP 503 while loading means the server is still starting, not an engine error.
- A per-Check output stop rule so one capped answer does not own p95: the answer cap of section 6.
- `weights.rs` maps `gemma-4` to Apache-2.0, matches `qwen3.5-*`, has rows for ministral, granite, and smollm3, and a `hosted` class for cloud rows.
- Every engine and model row that is unreachable before its first sentence is skipped with a reason, never an error, as today.

### 4.2 Tables

The Engines table keeps its four columns.
Models splits into three tables per set: Quality (Catch, Precision, Recall, F0.5, Exact fix, FP, Creep, Valid, Useful fix when present), Cost (Thinking, p50, p95, Memory, Cost / 1k, Licence, Recommended), and Recall by native language.
A Throughput table follows for local rows: time to first token p50, output tokens per second, output tokens per Check p50, and output tokens per Issue.
Cloud rows print whole-request rates, because providers report no timings.
Output tokens per Issue is the number section 6 halves, so the file shows whether the compact answer landed.
The Chunk table (section 1) prints wall time, validity, and recall per local row.
Wall time per row and the run's cloud spend print under the tables.

### 4.3 Record file

`--record <dir>` writes `checks.json`: one entry per (engine, model, thinking, item id) with validity, latency, cost, token counts, server timings, and the normalised Issues.
Every entry also carries the item and the sentence after Accept, because that pair is the whole input of the judge.
The directory is gitignored, and so is the `judgements.json` the judge writes beside the record.
Those two files are the only place model output text and eval-set text ever land.

### 4.4 Judge

Decision and gate: [HUF-210](https://linear.app/huffman/issue/HUF-210).

- `cli/bench/judge.py` reads `checks.json`, selects every non-exact hit under the product default (thinking on for local rows), folds identical (item id, result text) pairs, and sends each to Claude Fable 5 with the sentence, the native language, the reference correction, the edits, and the sentence after Accept.
  The question: would a writer be helped by accepting these edits, where useful means correct English, or clearly better than the original and not broken.
  The call is lean: no tools, no MCP, a minimal system prompt, through `claude -p --model claude-fable-5 --output-format json` or the API directly; the full Claude Code session costs about 0.25 USD notional per item.
- Output `judgements.json`, keyed by (item id, result text), value `{ useful, reason }`.
- Hand labels live in `cli/tests/fixtures/judge-labels.json` in the same key shape, labelled by one criterion: is the sentence after Accept grammatically correct wording.
- Gate: the judge column counts in the ranking only when it agrees with the hand labels on at least 80% of the labelled items of that set.
  The gate is measured per set, because the column sits beside cells measured over that set alone.
  The gate also needs a sample of at least 5 matched labels, because a result text must match a label verbatim.
  A set under that sample leaves the judge unproven, so the file names the count and keeps the raw ranking.
  Below the gate the column still prints and the file says it is excluded.
  The pilot measured 15 of 17 (88%, kappa 0.76).
- Caveat on record: a Claude judge grading Claude rows is untested because the shortlist has no Claude rows.

## 5. Recommendation rules

Two lines, re-decided from the eval-set tables on every tag ([HUF-205](https://linear.app/huffman/issue/HUF-205), [HUF-217](https://linear.app/huffman/issue/HUF-217)).

- Ranking: exact fix rate on the eval set; ties by F0.5, then lower p50.
  When the judge gate passes, exact fix rate is replaced by exact fix plus useful non-exact fixes over interference sentences.
  The swap also needs the judgements file to cover every measured row that produced a non-exact hit.
  One uncovered row would compete on a smaller measure than a graded one, so the whole table keeps the raw ranking.
  It also needs one measured row the file grades a hit of, so a table of skipped rows never claims a measure that ranked nothing.
  The file states the gate result and the ranking result in two sentences, and the second one names why the column does not rank.
  A run that measured a thinking-off row adds one sentence, because the judge never grades that row.
- Floors: a row with more false positives than the default engine, or validity under 95%, is never recommended.
- Recommended local model, the Settings default and the README line: the best local row that is Apache-2.0 or MIT and fits the 8 GB tier by measured resident memory.
  Any thinking mode may win; the README names the mode the row ran under.
- Recommended cloud model, the `openrouterModel` line of the README: the best `openrouter` row by the ranking above, with no cost ceiling.
  A second line names the value cloud model: the cheapest row within 10 points of exact fix of the recommended one, when one exists, so the cost trade-off stays visible.
- Cloud is never the default engine.

### 5.1 Cloud placeholder

No cost ceiling: the captain chose quality over cost for the cloud line on 2026-08-26.
On the pilot numbers Gemini 3.7 Flash (0.34 USD per 1,000 Checks, 90% exact fix) is the recommended line and DeepSeek V4 Flash (0.02 USD, 70 to 83%) is the value line.
The `openrouterModel` placeholder in Settings is `google/gemini-3.7-flash` until the first full run replaces it.

## 6. Local engine changes

Decisions: [HUF-217](https://linear.app/huffman/issue/HUF-217), [HUF-218](https://linear.app/huffman/issue/HUF-218), [HUF-219](https://linear.app/huffman/issue/HUF-219).
Measured on the 890M: gemma-4-E4B-it writes 25 tokens per second; thinking raises exact fix from 33% to 87% at 17 s per sentence; a 5,000-unit Chunk with many errors cannot finish in either mode because the answer alone is about 3,400 tokens.

- Thinking is on by default for the local engine, everywhere.
  One boolean Setting `localThinking`, default `true`, shown when the engine is `openai`; the CLI flag `--thinking on|off` wins over it.
  Per request the adapter sends `chat_template_kwargs.enable_thinking` from the Setting; no unit restart on change.
- The transient unit adds `--reasoning-budget 1024 --reasoning-budget-message "Answer now."` to the command of v1 section 4.
  `--ctx-size` stays 4096.
  The request sends `max_tokens` 2048: 1,024 for thinking, 1,024 for the answer.
- The Check size limit belongs to the Engine: 2,000 UTF-16 units for `openai`, 5,000 for `languagetool`, `harper`, and `openrouter`.
  `grammachy chunk` packs to the selected engine's limit, and the Quick popup's too-long card fires at it.
- One prompt for every engine: compact JSON, and a `reason` of at most six words.
  On llama-server with thinking off, the request sends a raw `grammar` with no whitespace between tokens in place of the `json_schema` response format, so compactness is forced.
  A raw grammar bounds the whole generation, so thinking on keeps `json_schema` and the wording alone, and so do cloud rows.
  About 30 tokens per Issue against 56 before, on the thinking-off route.
- The Check timeout stays 90 s for the local engine, on every surface.
  Compose is no exception, and thinking is no exception.
  Every other engine keeps its own timeout from the v1 section 4 table, where `openrouter` is 30 s.
  Section 7 of this spec fixes that 30 s value for the cloud engine.
  The heavy 2,000-unit case with thinking finishes near 50 s on the 890M.
- `doctor` checks `ggml-cpu` plus one backend package (`ggml-vulkan` or nothing more on the CPU tier), not only `/usr/bin/llama-server`.
  A missing `ggml-cpu` fails the check, and a missing `ggml-vulkan` on a GPU tier is a note, because the server still runs on the CPU.

## 7. Cloud engine `openrouter`

Contract: [HUF-206](https://linear.app/huffman/issue/HUF-206); key placement: [HUF-208](https://linear.app/huffman/issue/HUF-208).

- Fourth `EngineSlug`, reusing the `openai` request, prompt, mapping, and `issues::normalise`, on the constant endpoint `https://openrouter.ai/api/v1/chat/completions`.
  No base URL setting; `endpoint::parse` and the loopback rule for `openai` are untouched.
  A Check leaves the machine only with this engine, and only to openrouter.ai.
- Request additions: `usage.include`, `reasoning.enabled=false` (`effort: minimal` for `google/` ids, which reject off), header `X-Title: Grammachy`, Bearer key.
  No `temperature` for ids that reject it.
  No `chat_template_kwargs`, because the local thinking key of section 6 is a llama.cpp chat-template argument.
  A cloud row bounds its thinking through `reasoning` alone.
  Timeout 30 s, kept equal in Rust and `ui/errors.js` by test.
  Cost stays inside Rust as `cost: Option<f64>` on the engine result; the 5.1 envelope is unchanged.
- Settings: `openrouterModel` (text, placeholder of section 5.1) and file-only `cloudConsent`; dropdown label "Cloud LLM (OpenRouter)"; empty model id is `bad_arguments`.
- Key: `~/.config/grammachy/openrouter-key`, directory 0700, file 0600, written by `printf '%s' "$KEY" | grammachy setup --openrouter-key`, removed by `setup --remove`, never in `shell.json`, never through QML.
- Consent card "Send text to OpenRouter?" gates the first Check in `Overlay`; Continue stores `cloudConsent`, Cancel sends nothing.
  The bar widget draws the cloud glyph with the tooltip "Grammachy: cloud engine, text is sent to OpenRouter".
- Failures map onto the six codes with a `reason` word (`no_key`, `unreachable`, `rejected_key`, `no_credit`, `rate_limited`) and the card shows the envelope message.
- `doctor` gains offline `binary` and `key` checks and a `docs/doctor.md` row.
- TLS is in the binary (`ureq` with `rustls`).
- ADR 0002, one opt-in cloud engine through OpenRouter only, as drafted on HUF-206.

## 8. Model shortlist

Research: [HUF-204](https://linear.app/huffman/issue/HUF-204); pilot: [HUF-209](https://linear.app/huffman/issue/HUF-209).

Local rows, Q4_K_M GGUF, sha256 pinned in the runner:

| Row | Size | Licence | Note |
|---|---|---|---|
| gemma-4-e4b-it | 4.98 GB | Apache-2.0 | Shipped default; 87% exact fix with thinking on the fixture |
| qwen3.5-9b | 5.68 GB | Apache-2.0 | |
| qwen3.5-4b | 2.74 GB | Apache-2.0 | Needs the answer cap of section 6 to score with thinking |
| phi-4-mini-instruct | 2.49 GB | MIT | No successor |
| gemma-4-e2b-it | 3.11 GB | Apache-2.0 | |
| ministral-3-3b-instruct-2512 | 2.15 GB | Apache-2.0 | |
| granite-4.1-3b | 2.10 GB | Apache-2.0 | |
| smollm3-3b | 1.92 GB | Apache-2.0 | |

Cloud rows: `deepseek/deepseek-v4-flash-0731` and `google/gemini-3.7-flash` only.
Gemini's reasoning cannot be disabled and is noted in the table.

Out: Muse Glimmer (16.8 GB, over the tier and the device), LFM2.5 (restricted licence), other cloud models, seq2seq T5 rows (a later effort).

## 9. Eval machine and run plan

- Tier machine: 27 GB RAM, AMD Radeon 890M, `ggml-vulkan` plus `ggml-cpu`, llama-cpp 0.2.0.
  Latency, memory, and throughput columns come from it only.
- A second machine (RTX 4070, 12 GB, over Tailscale) may run quality-only sweeps.
- Cloud spend cap for the full run: 10 USD, the hard limit on the key and `--max-cost 10`.
  The pilot spent about 0.045 USD over three runs.
- The full run: `grammachy bench --eval-set --engine openai --model <each local row> --thinking both --cloud-model deepseek/deepseek-v4-flash-0731 --cloud-model google/gemini-3.7-flash --max-cost 10 --record <dir>`, then `judge.py`, then the same command with `--judgements`.
  Its output is `docs/benchmarks/<version>.md`, unedited.

## 10. Amendments to `docs/spec/v1.md`

- Section 1: the offline standing rule and the out-of-scope line carve out the opt-in `openrouter` engine.
- Section 4: the `openrouter` row and rules of HUF-206; the `openai` row gains the reasoning flags, `max_tokens` 2048, the thinking Setting, and the 2,000-unit limit; TLS note; `doctor` backend-package check.
- Section 5.2: the Check size limit is a property of the Engine; `chunk` packs to the selected engine's limit.
- Section 6: the too-long card fires at the selected engine's limit.
- Section 7: `localThinking`, `openrouterModel`, `cloudConsent`; the dropdown label.
- Section 10: `setup --openrouter-key` joins the subcommand list and writes the key file; `setup --remove` deletes it.
- Section 11: the layout tree gains `spec/evals.md`, the two ADRs, `LICENSE`, and the `openrouter` engine.
- Section 13.1: replaced by a pointer to this spec for sets, metrics, tables, flags, and recommendation rules; the regression rule sentence stays.
- `CONTEXT.md`: Check size limit belongs to the Engine; terms Edit, Pair, Exact fix, Style creep, Valid Check, Eval set, Record file, Cloud engine, Consent (from the glossary branches).

## 11. Milestones

One PR each, in order.

1. **Runner and metrics.** Land branch `huf-209-pilot`: `openrouter` bench rows, `--cloud-model`, `--max-cost`, `--record`, the metrics module, the item shape migration, the tables, `weights.rs`, the 503 and 429 rules, progress lines, parallel cloud rows, `--thinking`, the Thinking column, device-aware memory.
2. **Eval set and licence.** `cli/src/bench/evalset/`, the fetch step with sha256 and the stderr notice, the sidecar, `--eval-set`, ADR 0003, the MIT `LICENSE` file, the benchmark header line.
3. **Local engine.** `localThinking` Setting and view, unit flags, `max_tokens` 2048, per-engine Check size limit in `check.rs`, `chunk.rs`, and the too-long card, compact grammar and six-word reason, `doctor` backend check, the Chunk fixture and table, v1 amendments of section 10.
4. **Cloud engine in the product.** `openrouter` slug for `check`, Settings entries, key file and `setup`, consent card, bar glyph, error cards, `doctor` checks, ADR 0002.
5. **Judge and first full run.** `judge.py`, hand labels, `--judgements`, the full run on the tier machine, `docs/benchmarks/<version>.md`, the README recommendation lines, the `openrouterModel` placeholder replaced.

## 12. Deferred

Prompt variants (`bench --prompt <file>` and a prompt column), seq2seq rows, L1 rule packs, the tagger, inline checking, a CI gate on model rows.
