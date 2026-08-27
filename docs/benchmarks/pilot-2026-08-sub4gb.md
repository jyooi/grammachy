# Grammachy benchmark 0.1.0

Fixture: 30 interference sentences and 10 correct ones, `cli/tests/fixtures/interference-30.json`.
Machine: 16 GB tier, 24 CPUs, 27 GB RAM.
Command: `grammachy bench --engine openai --model qwen3.8-4b --model granite-4.2-3b --thinking both`.

## Engines

| Engine | Catch rate | False positives | p50 latency | Resident memory |
|---|---|---|---|---|
| `languagetool` | 10 of 30 (33.3%) | 0 of 10 | 17 ms | 454 MB |
| `harper` | 5 of 30 (16.7%) | 0 of 10 | 12 ms | 295 MB |
| `openai` | 27 of 30 (90.0%) | 1 of 10 | 29456 ms | 2.2 GB |

Resident memory of `languagetool` is the RSS of its server process.
Resident memory of `harper` is the growth of this process's own peak RSS, because it runs in process.
Resident memory of `openai` is the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS.

## Models

### Quality

| Model | Catch rate | Precision | Recall | F0.5 | Exact fix | False positives | Style creep | Valid |
|---|---|---|---|---|---|---|---|---|
| `qwen3.8-4b` | 29 of 30 (96.7%) | 27 of 29 (93.1%) | 27 of 30 (90.0%) | 92.5% | 20 of 30 (66.7%) | 0 of 10 | 6.7 | 40 of 40 (100.0%) |
| `qwen3.8-4b` | 21 of 30 (70.0%) | 20 of 21 (95.2%) | 20 of 30 (66.7%) | 87.7% | 16 of 30 (53.3%) | 0 of 10 | 3.3 | 40 of 40 (100.0%) |
| `granite-4.2-3b` | 27 of 30 (90.0%) | 26 of 27 (96.3%) | 26 of 30 (86.7%) | 94.2% | 15 of 30 (50.0%) | 0 of 10 | 3.3 | 40 of 40 (100.0%) |
| `granite-4.2-3b` | 27 of 30 (90.0%) | 20 of 32 (62.5%) | 20 of 30 (66.7%) | 63.3% | 14 of 30 (46.7%) | 1 of 10 | 36.7 | 40 of 40 (100.0%) |

Every Models table prints the rows in one order. A model that appears twice is its two Thinking modes, named in the Cost table below.

### Cost

| Model | Thinking | p50 latency | p95 latency | Resident memory | Cost per 1,000 Checks | Weights license | Recommended |
|---|---|---|---|---|---|---|---|
| `qwen3.8-4b` | on | 4772 ms | 13513 ms | 2.2 GB | 0.00 (local) | Apache-2.0 | recommended |
| `qwen3.8-4b` | off | 1813 ms | 2269 ms | 2.2 GB | 0.00 (local) | Apache-2.0 | eligible |
| `granite-4.2-3b` | on | 28935 ms | 58756 ms | 2.2 GB | 0.00 (local) | Apache-2.0 | eligible |
| `granite-4.2-3b` | off | 1159 ms | 2231 ms | 2.2 GB | 0.00 (local) | Apache-2.0 | no, more false positives than `languagetool` |

Thinking is the local mode the row ran in, from `--thinking`. A cloud row prints `-`: the mode is a llama.cpp chat-template argument and never reaches a provider.
Resident memory of `qwen3.8-4b` with thinking on is the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS.
Resident memory of `qwen3.8-4b` with thinking off is the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS.
Resident memory of `granite-4.2-3b` with thinking on is the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS.
Resident memory of `granite-4.2-3b` with thinking off is the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS.
Wall time of `qwen3.8-4b` with thinking on: 219 s for the whole set.
Wall time of `qwen3.8-4b` with thinking off: 67 s for the whole set, on the server the earlier row of this model ran on.
Wall time of `granite-4.2-3b` with thinking on: 1240 s for the whole set.
Wall time of `granite-4.2-3b` with thinking off: 49 s for the whole set, on the server the earlier row of this model ran on.

### Throughput

| Model | Time to first token (p50) | Output tokens per second | Output tokens per Check (p50) | Output tokens per Issue |
|---|---|---|---|---|
| `qwen3.8-4b` | 675 ms | 25.1 | 101 | 143.7 |
| `qwen3.8-4b` | 675 ms | 25.1 | 29 | 45.1 |
| `granite-4.2-3b` | 62 ms | 36.7 | 1058 | 1491.8 |
| `granite-4.2-3b` | 55 ms | 38.3 | 42 | 55.6 |

Time to first token and the token rate come from the model server's own timings. A rate marked `whole request` is output tokens over the request time as seen from this machine, network included, because the provider reports no timings.
Output tokens per Issue is the output tokens of the row over the Issues the same Checks answered, so it prices one Issue rather than one Check.

### Recall by native language

| Model | zh | ms | fr | es |
|---|---|---|---|---|
| `qwen3.8-4b` | 7 of 8 | 7 of 8 | 7 of 7 | 6 of 7 |
| `qwen3.8-4b` | 5 of 8 | 7 of 8 | 4 of 7 | 4 of 7 |
| `granite-4.2-3b` | 6 of 8 | 8 of 8 | 7 of 7 | 5 of 7 |
| `granite-4.2-3b` | 4 of 8 | 7 of 8 | 6 of 7 | 3 of 7 |

### Missed items

Ids only. The sentence, the fix, and the model's own answer live in the record file of `--record`, which git ignores.

- `qwen3.8-4b`: es-01
- `qwen3.8-4b`: zh-02, zh-03, zh-05, ms-03, fr-01, fr-04, fr-07, es-04, es-06
- `granite-4.2-3b`: zh-06, es-02, es-03
- `granite-4.2-3b`: zh-03, zh-07, ms-08

Recommended local model, the Settings default and the README line: `qwen3.8-4b`, with thinking on.
Ranking: exact fix rate, then F0.5, then lower p50 (HUF-205). Floors: validity at least 95% and no more false positives than the default engine, `languagetool`, which earned 0. A recommended local model must also fit the machine tier above (`docs/spec/evals.md` section 5).

## Skipped

Every engine and model of this run was reachable.

## Regression rule

A release must not drop the catch rate of the default engine, `languagetool`, and must not raise its false positives, against the previous file in `docs/benchmarks/`. A row that was skipped in one file and measured in the next is a new measurement, not a regression.

## How the numbers are measured

- Catch rate: an interference sentence is caught when at least one Issue overlaps a span the item expects. A right span with a wrong Fix still counts, because the Panel shows the span and lets the user Skip the Fix.
- Precision, recall, F0.5: an Issue pairs with the first unpaired expected edit it overlaps, provided it reaches no more than three words past the edit on either side. Precision is pairs over Issues, recall is pairs over expected edits, both over the whole set.
- Exact fix: every Fix of the Check applied to the sentence equals the corrected sentence the item holds, after collapsing runs of whitespace.
- False positives: correct sentences that earned at least one Issue. One sentence counts once, however many Issues it earned.
- Style creep: unpaired Issues on interference sentences, per 100 interference sentences.
- Valid: Checks that returned a result. An invalid Check counts as zero Issues, so a miss, and stays out of precision, exact fix, and latency.
- p50 and p95 latency: nearest rank over the valid Checks of the set, correct sentences included, measured in process around one Check.
- Cost per 1,000 Checks: the sum of `usage.cost` over the row divided by the number of Checks that reported a cost, times 1,000. A cloud answer that reports no cost ends its row as skipped, because the run cannot then measure what it spends. A cloud row where no Check answered prints `n/a`. Local rows cost nothing per Check.
- Every sentence is checked with the Native language the set records for it, which is what the shell passes on a real Check.
- Thinking: the mode `--thinking` gave the local rows. `both` runs every local model twice, once in each mode. The Engines table's `openai` row runs once, in the mode the flag names, and under `both` in the product default. A cloud row prints `-`.

