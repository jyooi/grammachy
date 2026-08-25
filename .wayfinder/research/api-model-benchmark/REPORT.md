# API model benchmark for the `openai` adapter (HUF-183)

Date: 2026-08-25.
Wayfinder map: HUF-170.
Depends on: HUF-171 engine benchmark (`../engine-benchmark/`), same test set, same prompt, same scorer.

## Question

Which hosted model through OpenRouter gives the best grammar and spelling catch rate on the HUF-171 test set at the lowest cost per Check?
The answer becomes the recommended default model for the `openai` adapter.

## Method

- Branch `research/api-model-benchmark`, created from `research/engine-benchmark`.
- Runner: `../engine-benchmark/run.ts`, new `openrouter` engine.
  It calls `POST https://openrouter.ai/api/v1/chat/completions` (OpenAI chat completions shape, `Authorization: Bearer $OPENROUTER_API_KEY`).
  Source: https://openrouter.ai/docs/api-reference/overview.
- Same prompt as HUF-171 (`llmPrompt`), `temperature: 0`, `max_tokens: 600`, `response_format: {type: "json_object"}`.
- `reasoning: {enabled: false}` on every request, because OpenRouter bills reasoning tokens as output tokens and a Check is a one shot classification.
  If a model rejects the field with HTTP 400, the runner retries without it and records "reasoning provider default" in the version string.
  Source: https://openrouter.ai/docs/use-cases/reasoning-tokens.
- Actual tokens and cost come from the `usage` object of each response (`prompt_tokens`, `completion_tokens`, `completion_tokens_details.reasoning_tokens`, `cost`).
  OpenRouter counts tokens with the model's native tokenizer and always returns `usage` on non streaming requests.
- Test set: `../engine-benchmark/testset.json`, 30 error items (zh, ms, fr, es native writers) and 10 correct items.
- Scoring: `caught` = any issue overlaps the expected span, `precise` = a hit no longer than half the sentence, `exact fix` = applying the engine fix yields the expected sentence.
  `useful fix` is a hand judgement of non exact hits and needs a run.
- Run: `cd .wayfinder/research/engine-benchmark && OPENROUTER_API_KEY=... bun run.ts openrouter`.
  `OPENROUTER_MODELS=a,b` overrides the model list.
  Results land in this directory as `results-<model>.json`.

## Model list and prices

All ids and prices come from the live catalogue `GET https://openrouter.ai/api/v1/models`, fetched 2026-08-25 (418 models).
Raw extract: `prices.json`.
Prices are USD per million tokens.
Estimated cost uses the HUF-171 note: about 250 input and 40 output tokens per Check.

| Model id (OpenRouter) | Released | Input $/M | Output $/M | Temp param | Est. $/Check | Est. $/1,000 Checks |
|---|---|---|---|---|---|---|
| `google/gemini-3.7-flash` | 2026-08-13 | 0.375 | 1.875 | yes | 0.000169 | 0.169 |
| `deepseek/deepseek-v4-flash-0731` | 2026-07-31 | 0.140 | 0.280 | yes | 0.000046 | 0.046 |
| `~deepseek/deepseek-v4-flash-latest` (alias) | 2026-08-01 | 0.035 | 0.075 | yes | 0.000012 | 0.012 |
| `anthropic/claude-haiku-4.5` | 2025-10-15 | 1.000 | 5.000 | yes | 0.000450 | 0.450 |
| `openai/gpt-5.4-nano` | 2026-03-17 | 0.200 | 1.250 | no | 0.000100 | 0.100 |
| `qwen/qwen3.7-flash` | 2026-07-27 | 0.030 | 0.130 | yes | 0.000013 | 0.013 |
| `mistralai/mistral-small-2603` (Mistral Small 4) | 2026-03-16 | 0.150 | 0.600 | yes | 0.000062 | 0.062 |
| `google/gemini-3.1-flash-lite` | 2026-05-07 | 0.250 | 1.500 | yes | 0.000122 | 0.122 |

Notes on the list:

- Latest Gemini Flash is `google/gemini-3.7-flash` (canonical `google/gemini-3.7-flash-20260813`).
  The alias `~google/gemini-flash-latest` resolves to it at the same price.
- Latest DeepSeek Flash is `deepseek/deepseek-v4-flash-0731` (canonical `deepseek-v4-flash-20260731`).
  The alias `~deepseek/deepseek-v4-flash-latest` lists a price four times lower (0.035 / 0.075).
  The catalogue does not explain the gap.
  A run with the key will show the real billed cost in `usage.cost`.
  Do not hard code the alias price until that run confirms it.
- Latest Claude Haiku is `anthropic/claude-haiku-4.5` (canonical `claude-4.5-haiku-20251001`).
  `~anthropic/claude-haiku-latest` resolves to it.
  No newer Haiku exists in the OpenRouter catalogue or on the Anthropic pricing page.
- `openai/gpt-5.4-nano` does not list `temperature` in `supported_parameters`.
  OpenRouter drops unsupported parameters, so the run is not at temperature 0 for that model.
  This is a fairness caveat, not a blocker.
- `qwen/qwen3.7-flash` lists `response_format` but not `structured_outputs`.
  The runner uses `json_object` mode, which the model supports.
- Batch variants (`:batch`, half price) exist for Gemini, Haiku, and GPT.
  Grammachy Checks are interactive, so batch prices are not used here.

## Measured results

No `OPENROUTER_API_KEY` was present in the environment at the start or at the end of this task.
Every measured column is pending a key.

| Model | Caught /30 | Precise /30 | Exact fix /30 | Useful fix /30 | False pos /10 | p50 ms | p90 ms | Tokens in/out per Check | Actual $/1,000 Checks |
|---|---|---|---|---|---|---|---|---|---|
| `google/gemini-3.7-flash` | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key |
| `deepseek/deepseek-v4-flash-0731` | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key |
| `anthropic/claude-haiku-4.5` | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key |
| `openai/gpt-5.4-nano` | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key |
| `qwen/qwen3.7-flash` | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key |
| `mistralai/mistral-small-2603` | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key |
| `google/gemini-3.1-flash-lite` | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key | pending key |

Reference points from HUF-171 on the same test set: LanguageTool, Harper, Qwen2.5-3B, and Qwen2.5-7B results are in `../engine-benchmark/REPORT.md`.

## Claude direct or via OpenRouter

Anthropic first party price for Claude Haiku 4.5 is $1.00 input and $5.00 output per million tokens.
Source: https://platform.claude.com/docs/en/about-claude/pricing (fetched 2026-08-25).
OpenRouter lists `anthropic/claude-haiku-4.5` at the same $1.00 / $5.00.
OpenRouter adds a fee when you buy credits, not per token, so the per token price is identical.
Prompt cache reads are also identical (0.10 per million on both).

Value verdict: for Haiku alone, direct is marginally better (no credit purchase fee, one less hop of latency, first party rate limits and data handling).
For Grammachy, OpenRouter is still the better integration point.
One `openai` adapter with a `model` field covers Claude, Gemini, DeepSeek, and every other candidate through one API shape and one key.
Users who already hold an Anthropic key can point the same adapter at `https://api.anthropic.com/v1` only if Grammachy also ships an Anthropic Messages adapter, which HUF-171 already sketched as `runClaude`.

## Recommendation

Default model for the `openai` adapter via OpenRouter: `deepseek/deepseek-v4-flash-0731`, pending measurement.
Reasoning from the price data alone: it is the newest cheap general model (2026-07-31), costs about $0.046 per 1,000 Checks, supports `temperature` and `structured_outputs`, and is ten times cheaper than Haiku 4.5.
`google/gemini-3.7-flash` is the quality fallback at about $0.17 per 1,000 Checks, and is the pick if DeepSeek's catch rate falls below the HUF-171 Qwen2.5-7B baseline.
`anthropic/claude-haiku-4.5` at $0.45 per 1,000 Checks is only worth the default slot if it catches clearly more than the two models above, which the measured run must show.
`qwen/qwen3.7-flash` is the cheapest option on paper ($0.013 per 1,000) and is worth keeping in the run as the price floor.

The final call needs the measured columns.
The decision rule: pick the cheapest model whose `caught` count is within 2 of the best model and whose false positives are at most 1 of 10.

## Limitations

- No key was available, so no model was run.
  Every number above is a catalogue price times an estimated token count.
- Token estimates (250 in, 40 out) come from the HUF-171 note and one tokenizer.
  Gemini, DeepSeek, and Qwen tokenizers differ, so actual counts will vary by roughly 10 to 30 percent.
- The test set has 30 error items and 10 correct items.
  Differences of one or two catches are noise.
- OpenRouter prices change without notice.
  Re-fetch `https://openrouter.ai/api/v1/models` before shipping a default.
- The `~deepseek/deepseek-v4-flash-latest` alias price is unexplained and unverified.
- `gpt-5.4-nano` ignores `temperature`, so its run is not fully comparable.
- Provider routing on OpenRouter can send one model id to several upstream hosts with different latency and quantisation.
  Latency numbers from a run reflect the router's choice on that day.
