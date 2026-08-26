---
status: accepted
---

# One opt-in cloud engine, through OpenRouter only

v1 promised that a Check never leaves the machine, and every engine was local.
Users who want a stronger model than a 4 GB local one asked for a hosted option, and the evals need cloud rows to rank against.
We add one slug, openrouter, that sends the openai request to a constant endpoint on openrouter.ai with a key from a 0600 file.
It is opt in: never the default, gated by a one-time consent card, marked on the bar widget, and the loopback rule for openai is untouched.

## Considered options

- No cloud engine: keeps the v1 promise whole, but leaves the product without a route to a stronger model and the evals without cloud rows.
- Per-vendor slugs with direct keys (anthropic, google, openai-cloud): one adapter and one key flow per vendor, three consent stories, three doctor checks.
- A settable base URL on the openai slug: any host works, but the consent card cannot say where the text goes and the loopback guarantee is lost.
- One openrouter slug with a constant endpoint: one key, one adapter reuse, one host the consent card can name.

## Consequences

- The product promise changes from "never online" to "online only with openrouter, only to openrouter.ai, only after consent".
- A key file lives under ~/.config/grammachy/; setup writes and removes it, doctor checks its permissions.
- The cost of a Check becomes a metric; bench needs --max-cost and the Engine result carries usage.cost inside Rust.
- The recommended cloud model is a benchmark output and changes with each benchmark file.
