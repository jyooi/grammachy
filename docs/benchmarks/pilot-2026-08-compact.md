# Grammachy benchmark 0.1.0

Fixture: 30 interference sentences and 10 correct ones, `cli/tests/fixtures/interference-30.json`.
Machine: 16 GB tier, 24 CPUs, 27 GB RAM.
Command: `grammachy bench --engine openai --model gemma-4-e4b-it --model qwen3.5-4b --cloud-model deepseek/deepseek-v4-flash-0731 --cloud-model google/gemini-3.7-flash --max-cost 0.5`.

## Engines

| Engine | Catch rate | False positives | p50 latency | Resident memory |
|---|---|---|---|---|
| `languagetool` | 10 of 30 (33.3%) | 0 of 10 | 18 ms | 441 MB |
| `harper` | 5 of 30 (16.7%) | 0 of 10 | 62 ms | 312 MB |
| `openai` | 28 of 30 (93.3%) | 0 of 10 | 1552 ms | 964 MB |

Resident memory of `languagetool` is the RSS of its server process.
Resident memory of `harper` is the growth of this process's own peak RSS, because it runs in process.
Resident memory of `openai` is the RSS of its server process.

## Models

### Quality

| Model | Catch rate | Precision | Recall | F0.5 | Exact fix | False positives | Style creep | Valid |
|---|---|---|---|---|---|---|---|---|
| `gemma-4-e4b-it` | 29 of 30 (96.7%) | 29 of 30 (96.7%) | 29 of 30 (96.7%) | 96.7% | 24 of 30 (80.0%) | 0 of 10 | 3.3 | 40 of 40 (100.0%) |
| `qwen3.5-4b` | 28 of 30 (93.3%) | 27 of 29 (93.1%) | 27 of 30 (90.0%) | 92.5% | 19 of 30 (63.3%) | 0 of 10 | 6.7 | 40 of 40 (100.0%) |
| `deepseek/deepseek-v4-flash-0731` | 29 of 30 (96.7%) | 28 of 29 (96.6%) | 28 of 30 (93.3%) | 95.9% | 25 of 30 (83.3%) | 0 of 10 | 3.3 | 40 of 40 (100.0%) |
| `google/gemini-3.7-flash` | 30 of 30 (100.0%) | 30 of 31 (96.8%) | 30 of 30 (100.0%) | 97.4% | 28 of 30 (93.3%) | 0 of 10 | 3.3 | 40 of 40 (100.0%) |

### Cost

| Model | p50 latency | p95 latency | Resident memory | Cost per 1,000 Checks | Weights license | Recommended |
|---|---|---|---|---|---|---|
| `gemma-4-e4b-it` | 1358 ms | 1521 ms | 262 MB | 0.00 (local) | Apache-2.0 | recommended |
| `qwen3.5-4b` | 1509 ms | 2162 ms | 656 MB | 0.00 (local) | Apache-2.0 | eligible |
| `deepseek/deepseek-v4-flash-0731` | 6133 ms | 16741 ms | not measured | 0.02 USD | hosted | eligible |
| `google/gemini-3.7-flash` | 2525 ms | 3707 ms | not measured | 0.37 USD | hosted | recommended cloud model |

Wall time of `gemma-4-e4b-it`: 52 s for the whole fixture, server start included.
Wall time of `qwen3.5-4b`: 59 s for the whole fixture, server start included.
Wall time of `deepseek/deepseek-v4-flash-0731`: 328 s for the whole fixture.
Wall time of `google/gemini-3.7-flash`: 106 s for the whole fixture.
Cloud spend of this run: 0.0155 USD of the 0.5 USD cap, summed over the answers that reported a cost.
An answer that reported no cost stays out of that sum, so the figure is a lower bound.

### Throughput

| Model | Time to first token (p50) | Output tokens per second | Output tokens per Check (p50) | Output tokens per Issue |
|---|---|---|---|---|
| `gemma-4-e4b-it` | 448 ms | 27.2 | 24 | 26.9 |
| `qwen3.5-4b` | 635 ms | 28.9 | 25 | 28.7 |
| `deepseek/deepseek-v4-flash-0731` | not measured | 3.2 (whole request) | 28 | 36.8 |
| `google/gemini-3.7-flash` | not measured | 47.3 (whole request) | 103 | 163.1 |

Time to first token and the token rate come from the model server's own timings. A rate marked `whole request` is output tokens over the request time as seen from this machine, network included, because the provider reports no timings.
Output tokens per Issue is the output tokens of the row over the Issues the same Checks answered, so it prices one Issue rather than one Check.
The grammar bounded every local row of this run, so no model thought first.
A reader must set `localThinking` to false to reproduce these local rows, because the grammar now takes that route alone.
The default is true and `bench` reads the Setting, so a default run takes the `json_schema` route instead.
A comparison against `pilot-2026-08-thinking-on.md` is not like for like.

### Recall by native language

| Model | zh | ms | fr | es |
|---|---|---|---|---|
| `gemma-4-e4b-it` | 8 of 8 | 8 of 8 | 6 of 7 | 7 of 7 |
| `qwen3.5-4b` | 7 of 8 | 8 of 8 | 6 of 7 | 6 of 7 |
| `deepseek/deepseek-v4-flash-0731` | 7 of 8 | 8 of 8 | 7 of 7 | 6 of 7 |
| `google/gemini-3.7-flash` | 8 of 8 | 8 of 8 | 7 of 7 | 7 of 7 |

Recommended local model, the Settings default and the README line: `gemma-4-e4b-it`.
Recommended cloud model, the `openrouterModel` line of the README: `google/gemini-3.7-flash`. Cloud is never the default engine.
Ranking: exact fix rate, then F0.5, then lower p50 (HUF-205). Floors: validity at least 95% and no more false positives than the default engine, `languagetool`, which earned 0. A recommended local model must also fit the machine tier above (spec section 13.1).

## Skipped

Every engine and model of this run was reachable.

## Regression rule

A release must not drop the catch rate of the default engine, `languagetool`, and must not raise its false positives, against the previous file in `docs/benchmarks/`. A row that was skipped in one file and measured in the next is a new measurement, not a regression.

## How the numbers are measured

- Catch rate: an interference sentence is caught when at least one Issue overlaps a span the fixture expects. A right span with a wrong Fix still counts, because the Panel shows the span and lets the user Skip the Fix.
- Precision, recall, F0.5: an Issue pairs with the first unpaired expected edit it overlaps, provided it reaches no more than three words past the edit on either side. Precision is pairs over Issues, recall is pairs over expected edits, both over the whole fixture.
- Exact fix: every Fix of the Check applied to the sentence equals the corrected sentence the fixture holds, after collapsing runs of whitespace.
- False positives: correct sentences that earned at least one Issue. One sentence counts once, however many Issues it earned.
- Style creep: unpaired Issues on interference sentences, per 100 interference sentences.
- Valid: Checks that returned a result. An invalid Check counts as zero Issues, so a miss, and stays out of precision, exact fix, and latency.
- p50 and p95 latency: nearest rank over the valid Checks of the fixture, correct sentences included, measured in process around one Check.
- Cost per 1,000 Checks: the sum of `usage.cost` over the row divided by the number of Checks that reported a cost, times 1,000. A cloud answer that reports no cost ends its row as skipped, because the run cannot then measure what it spends. A cloud row where no Check answered prints `n/a`. Local rows cost nothing per Check.
- Every sentence is checked with the Native language the fixture records for it, which is what the shell passes on a real Check.

