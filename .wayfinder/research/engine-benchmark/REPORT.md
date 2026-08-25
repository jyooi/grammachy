# HUF-171: Engine benchmark for Grammachy v1

Question: which engine catches the English mistakes of a Mandarin, Malay, French, or Spanish native speaker best, at what cost, so v1 can pick an offline default engine.

Collected 2026-08-25 on Arch Linux, 24 CPUs, 27 GB RAM, CPU only, no GPU.
Raw data: `testset.json`, `results-<engine>.json`, runner `run.ts` (bun).

## Setup

All engines were installed in user space under `~/.cache/grammachy-bench/`. No sudo, no pacman.

- LanguageTool 6.6 standalone zip from languagetool.org, run as `java -cp languagetool-server.jar org.languagetool.server.HTTPServer --port 8081` on OpenJDK 26. Requests set `language=en-US` and `motherTongue` (zh-CN, fr, es). Malay is not a LanguageTool language, so Malay items ran without `motherTongue`.
- Harper 2.8.0 `harper-cli` x86_64 release binary from GitHub (`cargo install harper-cli` fails: the crate is not on crates.io). Run as `harper-cli lint --format json --dialect us <file>`, one process per sentence.
- llama.cpp b10615 ubuntu-x64 release binary, `llama-server` with 12 threads, context 2048, temperature 0, and a JSON schema `response_format` so the output is always a well-formed array.
  - Qwen2.5-3B-Instruct Q4_K_M (2.1 GB)
  - Qwen2.5-7B-Instruct Q4_K_M (4.7 GB, two shards)
- Claude API: skipped. `ANTHROPIC_API_KEY` is not set on this machine.

## Method

- Test set: 30 sentences with one interference error each (8 Mandarin, 8 Malay, 7 French, 7 Spanish) plus 10 correct sentences. Error types: article, tense, plural, gerund, false friend, word order, preposition, agreement, comparative, conjunction, existential, double subject, missing subject.
- Each sentence is one Check. The runner sends every sentence to each engine in sequence and records latency.
- `caught`: an engine Issue overlaps the expected span.
- `precise`: the overlapping Issue covers at most half of the sentence. A whole-sentence Issue does not localize the mistake for the Panel.
- `exact fix`: applying the engine Fix gives the same sentence as applying the expected Fix.
- `useful fix`: `exact fix` plus hits whose Fix I judged correct by hand (listed below).
- `false positives`: correct sentences with at least one Issue.
- LLM prompt (same for both models): en-US, the native language stated, grammar and spelling only, return only a JSON array of `{original, fix, reason}` where `original` is the shortest substring with the mistake. Issues with `fix == original` are dropped as noise and counted.
- Cold start: LanguageTool, time from process start until `/v2/languages` answers (first real check answers after about 3.1 s); llama.cpp, until `/health` answers with the model file already in page cache; Harper, one full process run.
- RSS: `ps` RSS of the server process after the run. For llama.cpp this includes the memory-mapped model file.

## Results

| Engine | Caught (of 30) | Precise | Exact fix | Useful fix | False positives (of 10) | Median latency | p90 latency | RSS | Cold start | Offline | License |
|---|---|---|---|---|---|---|---|---|---|---|---|
| LanguageTool 6.6 | 10 (33%) | 10 | 7 | 10 | 0 | 20 ms | 51 ms | 731 MB | 0.9 s (3.1 s to first check) | Yes | LGPL-2.1 |
| Harper 2.8.0 | 6 (20%) | 5 | 3 | 5 | 0 | 410 ms (process spawn; the library itself is milliseconds) | 428 ms | 141 MB | 0.44 s | Yes | Apache-2.0 |
| Qwen2.5-3B Q4_K_M (llama.cpp) | 17 (57%) | 17 | 8 | 9 | 0 | 1.7 s | 4.9 s | 3.6 GB | 1.3 s | Yes | MIT + Apache-2.0 |
| Qwen2.5-7B Q4_K_M (llama.cpp) | 24 (80%) | 20 | 16 | 20 | 0 | 1.8 s | 2.4 s | 7.7 GB | 2.5 s | Yes | MIT + Apache-2.0 |
| Claude API | not run | | | | | | | 0 | | No | Commercial, pay per token |

Catch rate by native language (caught of 8, 8, 7, 7):

| Engine | zh | ms | fr | es |
|---|---|---|---|---|
| LanguageTool | 1 | 5 | 4 | 0 |
| Harper | 0 | 4 | 2 | 0 |
| Qwen2.5-3B | 6 | 4 | 3 | 4 |
| Qwen2.5-7B | 7 | 7 | 6 | 4 |

Per item (hit = overlapping Issue; exact = Fix equals the expected Fix):

| id | type | languagetool | harper | qwen3b | qwen7b |
|---|---|---|---|---|---|
| zh-01 | tense | miss | miss | hit, exact fix | hit, exact fix |
| zh-02 | plural | hit, exact fix | miss | hit, exact fix | hit, exact fix |
| zh-03 | gerund | miss | miss | hit, exact fix | hit, exact fix |
| zh-04 | article | miss | miss | miss | hit, exact fix |
| zh-05 | conjunction | miss | miss | hit | hit, exact fix |
| zh-06 | word_order | miss | miss | hit | hit |
| zh-07 | false_friend | miss | miss | miss | miss |
| zh-08 | existential | miss | miss | hit | hit, exact fix |
| ms-01 | double_subject | miss | miss | miss | miss |
| ms-02 | tense | hit | miss | hit, exact fix | hit |
| ms-03 | preposition | hit, exact fix | hit, exact fix | miss | hit, exact fix |
| ms-04 | preposition | hit, exact fix | hit, exact fix | hit | hit, exact fix |
| ms-05 | comparative | hit, exact fix | hit, exact fix | miss | hit, exact fix |
| ms-06 | plural | hit, exact fix | hit | hit, exact fix | hit, exact fix |
| ms-07 | word_order | miss | miss | hit | hit, exact fix |
| ms-08 | agreement | miss | miss | miss | hit, exact fix |
| fr-01 | false_friend | hit, exact fix | miss | miss | hit, exact fix |
| fr-02 | false_friend | hit | miss | miss | miss |
| fr-03 | false_friend | miss | miss | hit, exact fix | hit |
| fr-04 | tense | hit | hit, whole sentence | miss | hit, whole sentence |
| fr-05 | preposition | hit, exact fix | miss | hit | hit, exact fix |
| fr-06 | word_order | miss | miss | hit | hit |
| fr-07 | article | miss | hit | miss | hit, whole sentence |
| es-01 | false_friend | miss | miss | hit | miss |
| es-02 | agreement | miss | miss | hit, exact fix | hit, exact fix |
| es-03 | false_friend | miss | miss | hit, exact fix | hit, exact fix |
| es-04 | missing_subject | miss | miss | miss | miss |
| es-05 | preposition | miss | miss | hit | hit |
| es-06 | preposition | miss | miss | miss | hit |
| es-07 | article | miss | miss | miss | miss |
| ok-01 to ok-10 | none | clean | clean | clean | clean |

### Hand judgement of non-exact hits

- LanguageTool: ms-02 `finish -> finishes` (wrong tense, but the flag is right), fr-02 `have thirty years old -> are thirty years old` (right idea, wrong person), fr-04 `am -> have been` plus `since -> for` (correct as two Issues). Useful: 10.
- Harper: ms-06 `informations -> in formations` (wrong), fr-04 `since two hours -> for two hours` (correct), fr-07 `an information -> information` (correct). Useful: 5.
- Qwen2.5-3B: zh-06 `I very -> I` (acceptable). All other non-exact hits are wrong fixes, for example zh-08 `have -> is`, ms-07 `car red -> car`, fr-05 `me -> her`, es-05 `to -> at`. It also produced 42 no-op issues (`fix == original`) that the runner dropped. Useful: 9.
- Qwen2.5-7B: zh-06 `I very like -> I like`, ms-02 `already finish -> has finished`, fr-07 `received an information -> received the information`, es-05 `saw to -> met with` (acceptable). fr-03 `assisted to -> assisted at`, fr-04 `since two hours -> for two hours` (leaves `am here`), fr-06 `like very much -> like this`, es-06 `waiting the -> waiting for` (drops `the`) are wrong. Useful: 20.

A first run with a looser prompt (no "shortest substring" rule) let the 7B model quote the whole sentence and rewrite it. That run caught 27 of 30 with 21 exact fixes and 0 false positives, but every Issue covered the full sentence. Full-sentence rewrites fit the Corrected text flow but do not fit a per-Issue Accept and Skip Panel.

## One sample per engine

Item ms-04, "We discussed about the new project for hours." (Malay native, expected `about -> ` removed):

- LanguageTool: span `discussed about the`, fix `discussed the`, reason `MENTION_ABOUT: Did you mean simply "discussed the"? You do not need the word "about" here.`
- Harper (item ms-03, since Harper also caught it): span `since ten years`, fix `for ten years`, reason `SinceDuration: For a duration, use 'for' instead of 'since'. Or for a point in time, add 'ago' at the end.`
- Qwen2.5-3B: span `about`, fix `on`, reason `correct preposition for discussion` (wrong fix, right span).
- Qwen2.5-7B: span `discussed about`, fix `discussed`, reason `Redundant preposition`.

## Recommendation

v1 default engine: LanguageTool 6.6 local server, en-US with `motherTongue`.
Fallback order: LanguageTool, then Qwen2.5-7B-Instruct via llama.cpp when the user opts in and has 8 GB free, then Harper as a last resort, then the Claude API as an online opt-in.

Reasons:

- The 7B model has the best catch rate (80%) and the best useful fixes (20 of 30) with zero false positives, but it costs 7.7 GB RSS, about 2 s per sentence, and needs a 4.7 GB download. It is a good opt-in "deep" engine, not a safe default for a shell plugin that shares a machine with a browser and an editor.
- LanguageTool catches only a third of the set, but it never fires on correct text, answers in 20 ms, explains each Issue in one sentence, starts in about 3 s, and needs 0.7 GB. Its rules are strong on Malay and French patterns (uncountable nouns, `since` and `for`, `discuss about`, `I am agree`, `have N years old`) and weak on Mandarin and Spanish patterns (missing articles, tense with a time adverb, `very like`, `borrow me`, missing subject).
- Harper is small and fast but caught 6 of 30, and one of its fixes (`in formations`) is harmful. It is worth a place as the zero-dependency fallback only.
- Qwen2.5-3B is not usable. More than half of its hits carry a wrong fix, and it emitted 42 no-op issues. A 3B model does not save enough memory over 7B to justify the quality loss.
- No engine caught zh-07 `borrow me`, ms-01 double subject, or es-04 missing subject. The v1 Panel should not promise coverage of these.

Practical notes for the Engine adapter:

- LanguageTool: keep one server process alive, warm it with one check at start, pass `motherTongue` from the Native language setting. Malay has no `motherTongue` code.
- llama.cpp: use the JSON schema `response_format`, temperature 0, the "shortest substring" prompt, drop issues whose fix equals the original, and drop issues whose `original` is not a substring of the text. Use `-c 2048` and a single slot to keep RSS low.
- Harper: use `harper-ls` or the library in process. The 410 ms here is process spawn plus dictionary load.

## Known limitations of the method

- 40 sentences, one error each, written by one person. This is a smoke test, not a corpus study. Rank order is credible; percentages are not.
- Mandarin and Malay items lean on tense, article, and word order; French and Spanish items lean on false friends. Engine ranking per language reflects that mix.
- `found` is span overlap. An engine that flags the right words with a wrong fix still counts as caught. The `useful fix` column corrects for this by hand and is subjective.
- LLM outputs vary with the prompt. Two prompts gave 24 and 27 caught for the same 7B model. Results for a different chat template or quantization will differ.
- Latency is sequential, single request, on a 24-core machine with 12 llama.cpp threads. A 4-core laptop will see roughly 3x the LLM latency.
- RSS for llama.cpp includes the memory-mapped model file, which the kernel can evict. Real private memory is lower.
- LanguageTool ran in default (non-picky) mode. A picky probe on five missed items found nothing extra. Premium rules were not tested.
- Harper per-sentence latency is dominated by process spawn. Library latency is under 10 ms.
- Claude API was not measured because no key was present. Expect about 250 input and 40 output tokens per Check with this prompt.
