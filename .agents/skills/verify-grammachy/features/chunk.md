# Chunk

Chunk splits a Draft on stdin into spans that each fit one Check.
The Compose card runs it before a chunked Check, and then checks each span in turn.

## Sub-features

- `chunk-split` packs a long Draft into spans of at most 5,000 UTF-16 code units each.
- `chunk-single` returns one span for a short Draft.
- `chunk-empty` refuses an empty Draft with `empty_selection`.
- `chunk-engine` packs to the limit of the engine from the flag, then the stored entry, then `harper`.

## How to get to it (user POV)

- Paste a long Draft into Compose with SUPER + ALT + Q and press `Check`.
- Run `bin/grammachy chunk` in a terminal with the Draft on stdin, as section 9 of `docs/dev.md` shows.

## Driving it with vg

Preconditions:

- `vg doctor` exits 0.

- **Long Draft.** Make the 20,000 unit Draft of smoke item 5. Run `python3 -I -c "print(('I has two book and she go home every day. ' * 500)[:20000], end='')" > "$(vg path scratch)/draft.txt"` and `vg run chunk --stdin "$(vg path scratch)/draft.txt" -- chunk`. Exit code `0` and five spans: `0-4998`, `4998-9996`, `9996-14994`, `14994-19992`, `19992-20000`.
- **Contiguous spans.** Read the spans back. Run `vg run chunk --stdin "$(vg path scratch)/draft.txt" -- chunk | jq -c '[.chunks[] | [.start, .end]]'`. Each `start` equals the previous `end`, the first is `0`, and the last `end` is `20000`.
- **Short Draft.** Run `vg run chunk --text "I has two book." -- chunk`. One span, `0-15`.
- **Empty.** Run `vg run chunk --text "" -- chunk`. Exit code `1` and `"code":"empty_selection"` with the message `The Draft is empty.`.
- **Engine flag.** Run `vg run chunk --stdin "$(vg path scratch)/draft.txt" -- chunk --engine languagetool`. The same five spans, because both engines take 5,000 units.

## Gotchas

- `chunk` prints spans only, with no text. Slice the Draft by the spans to read a Chunk.
- `chunk --engine languagetool` is safe. It reads only the limit and never starts the shared unit.
- A span ends at a word or sentence boundary, so most spans are shorter than 5,000 units.
