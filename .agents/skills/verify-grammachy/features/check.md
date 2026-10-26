# Check

A Check takes the text of a Selection on stdin and prints one JSON envelope with every Issue, or one error.
The quick popup runs this command for every Selection, and the Compose card runs it once for each Chunk.

## Sub-features

- `check-issues` reports each Issue with its UTF-16 span, Fix, reason, and category.
- `check-clean` reports an empty Issue list for correct text.
- `check-empty` refuses empty text with `empty_selection`.
- `check-too-long` refuses text over 5,000 UTF-16 code units with `text_too_long`.
- `check-target` spells against American or British English.
- `check-settings` resolves flags first, then the stored entry, then the defaults.
- `check-bad-args` refuses an unknown flag value with `bad_arguments`.

## How to get to it (user POV)

- Highlight text and press SUPER + SHIFT + Q, or click the `G` bar button. The popup runs `bin/grammachy check`.
- Press `Check` on the Compose card. The card runs one Check for each Chunk.
- Run `bin/grammachy check` in a terminal with the text on stdin.

## Driving it with vg

Preconditions:

- `vg doctor` exits 0.
- The scratch entry is empty, so the Engine is `harper`.

- **Issues.** Check a sentence with a mistake. Run `vg run check --text "I has two book." -- check`. Exit code `0`, `"engine":"harper"`, and one Issue with `"start":2`, `"end":5`, `"original":"has"`, `"fix":"have"`, and `"category":"grammar"`.
- **Clean text.** Check a correct sentence. Run `vg run check --text "I have two books." -- check`. Exit code `0` and `"issues":[]`.
- **Empty.** Check nothing. Run `vg run check --text "" -- check`. Exit code `1` and `"code":"empty_selection"`.
- **Too long.** Check 6,000 units. Run `python3 -I -c "print('x ' * 3000, end='')" > "$(vg path scratch)/6000.txt"` and `vg run check --stdin "$(vg path scratch)/6000.txt" -- check`. Exit code `1`, `"code":"text_too_long"`, and a message with `6000` and `5000`.
- **Target English.** Check British spelling both ways. Run `vg run check --text "The colour of the centre" -- check`, then the same with `-- check --target en-GB`. The first envelope flags `colour` and `centre`. The second has `"issues":[]`.
- **Settings order.** Store British English, then check without a flag. Run `f=$(vg path shell-json); jq '.bar.layout.right[0].targetEnglish = "en-GB"' "$f" > "$f.new" && mv "$f.new" "$f"`, then `vg run check --text "The colour of the centre" -- check`. `"issues":[]`. Then run the same Check with `-- check --target en-US`. The flag wins and both words come back.
- **Bad arguments.** Pass an engine with no adapter. Run `vg run check --text "x" -- check --engine gector`. Exit code `1`, `"code":"bad_arguments"`, and the clap error on stderr.

## Gotchas

- Harper flags only `has` in `I has two book.`. Section 5 of `docs/dev.md` expects marks on `has` and `book`. Trust the envelope and report the difference.
- The span is in UTF-16 code units, not bytes. Use ASCII text when you compare a span by hand.
- `elapsedMs` changes on every run. Do not compare it.
- A `check` on `languagetool` uses the shared unit `grammachy-languagetool`. The helper refuses it, also when the stored entry names `languagetool`.
- `--native` changes the request only for LanguageTool. With Harper the envelope is the same for each value.
- The stored entry can sit in `bar.layout.left`, `center`, or `right`, or in `plugins`. The CLI reads the first match.
