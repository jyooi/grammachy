#!/usr/bin/env python3
"""Grade the non-exact hits of a recorded bench run, evals spec section 4.4.

One run of `grammachy bench --record <dir>` writes `<dir>/checks.json`, one
entry per engine, model, and item, carrying the item and the answer. This
script reads that file, keeps every non-exact hit, folds the identical answers
of two models onto one item, and asks Claude Fable 5 one question per folded
item. It writes `judgements.json` beside the record, keyed by item id and then
result text, which is what `grammachy bench --judgements <file>` reads.

A non-exact hit is a valid Check on an item that carries a mistake, where at
least one Issue touches a span the item expects and applying every Fix does not
reproduce the expected sentence. An item nothing touched is a plain miss: the
writer is offered nothing to accept, so there is nothing to grade.

The call is lean, which is the build item HUF-210 recorded. A full Claude Code
session cost about 0.25 USD of notional spend per item on the pilot, which does
not scale to the 365-item eval set. So every call disables the tools, the MCP
servers, the skills, and the project settings, and replaces the default system
prompt with one sentence.

A judgements file costs real spend, so a later run adds to the file rather than
replacing it.
This run folds its own answers over the ones already there, by item id and
result text.
The write lands on a `.pending` sibling and is renamed, the rule `checks.json`
follows.
So a run that dies part way leaves the earlier file whole.
A run that graded nothing refuses to touch a file that exists.
Pass `--replace` to overwrite the file wholesale instead.

Usage:

    cli/bench/judge.py <dir-or-checks.json> [--out FILE] [--jobs N]
                       [--model NAME] [--limit N] [--dry-run] [--replace]

`--dry-run` prints the folded items and the first prompt and calls nothing, so
the selection can be read without spending anything.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import subprocess
import sys
from pathlib import Path

# The judge of spec section 4.4. Fable 5 is what HUF-210 measured against the
# hand labels, so a different model invalidates the 88% the gate was set from.
MODEL = "claude-fable-5"

# One sentence in place of the whole Claude Code system prompt.
SYSTEM_PROMPT = (
    "You grade one grammar-checker suggestion for usefulness to the writer. "
    "Answer with one JSON object and nothing else."
)

# The question HUF-210 settled, meaning clause included: the judge's stricter
# calls on the pilot were a lost object and a missing preposition, both real
# faults, so "correct English, or clearly better and not broken" stays.
QUESTION = (
    "Question: would a writer be helped by accepting these edits? "
    "Useful means the result is correct English, or clearly better than the "
    "original and not broken. Not useful means the result is wrong, broken, or "
    "worse than the original. The result need not equal the reference.\n"
    'Answer with only a JSON object: {"useful": true or false, "reason": "one sentence"}'
)

# How long one call may take before it is counted as a failure.
CALL_TIMEOUT_S = 300


def overlaps(a_start: int, a_end: int, b_start: int, b_end: int) -> bool:
    """Whether two half-open spans share at least one code unit."""
    return a_start < b_end and b_start < a_end


def is_caught(check: dict) -> bool:
    """Whether at least one Issue touches a span the item expects."""
    return any(
        overlaps(issue["start"], issue["end"], edit["start"], edit["end"])
        for issue in check["issues"]
        for edit in check["edits"]
    )


def collapse(text: str) -> str:
    """Runs of whitespace collapsed, the comparison of spec section 3."""
    return " ".join(text.split())


def non_exact_hits(checks: list[dict]) -> list[dict]:
    """Every Check of the record that is a non-exact hit under the default.

    A record entry that carries a `thinking` field is kept only when it ran
    under the product default: thinking on for a local row, and whatever the
    provider pins for a cloud row. A record written before that field existed
    ran under the default of its own run, so it is kept as it stands.
    """
    kept = []
    for check in checks:
        if not check.get("valid") or not check.get("edits"):
            continue
        if check.get("result_text") is None:
            continue
        if check.get("thinking") is False and check["engine"] != "openrouter":
            continue
        if not is_caught(check):
            continue
        if collapse(check["result_text"]) == collapse(check["expected_text"]):
            continue
        kept.append(check)
    return kept


def fold(hits: list[dict]) -> list[dict]:
    """One item per (item id, result text), with the rows that produced it.

    Two models that answer one sentence the same way are one judgement and one
    call, which is what keeps the eval set affordable.
    """
    folded: dict[tuple[str, str], dict] = {}
    for hit in hits:
        key = (hit["id"], hit["result_text"])
        item = folded.get(key)
        if item is None:
            item = {
                "id": hit["id"],
                "native": hit["native"],
                "text": hit["text"],
                "expected_text": hit["expected_text"],
                "edits": hit["edits"],
                "result_text": hit["result_text"],
                "issues": hit["issues"],
                "rows": [],
            }
            folded[key] = item
        row = f"{hit['engine']}:{hit['model']}"
        if row not in item["rows"]:
            item["rows"].append(row)
    return list(folded.values())


def prompt_for(item: dict) -> str:
    """The one prompt of spec section 4.4, in the wording HUF-210 measured."""
    suggested = [
        {
            "original": issue["original"],
            "fix": issue["fix"],
            "reason": issue["reason"],
        }
        for issue in item["issues"]
    ]
    reference = [{"text": edit["text"], "fix": edit["fix"]} for edit in item["edits"]]
    return (
        f"The writer's native language: {item['native']}.\n"
        f"Original sentence: {json.dumps(item['text'])}\n"
        f"Reference correction: {json.dumps(item['expected_text'])}\n"
        f"The edits the reference makes: {json.dumps(reference)}\n"
        f"The checker's suggested edits: {json.dumps(suggested)}\n"
        "The sentence the writer gets after accepting every edit: "
        f"{json.dumps(item['result_text'])}\n\n"
        f"{QUESTION}"
    )


def command(model: str, prompt: str) -> list[str]:
    """The lean `claude -p` invocation of spec section 4.4.

    No tools, no MCP server, no skill, no project or local settings file, and
    one sentence of system prompt. The session is not persisted, because a
    judgement run of the eval set would otherwise leave hundreds of them.
    """
    return [
        "claude",
        "-p",
        "--model",
        model,
        "--output-format",
        "json",
        "--system-prompt",
        SYSTEM_PROMPT,
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        '{"mcpServers": {}}',
        "--setting-sources",
        "",
        "--disable-slash-commands",
        "--no-session-persistence",
        prompt,
    ]


def read_answer(stdout: str) -> tuple[bool, str, float | None]:
    """The judgement inside one `--output-format json` answer.

    `--output-format json` prints either one result object or the whole event
    array, depending on the version, and a wrapper such as `mise` may print a
    line of its own first. So the parse starts at the first bracket of either
    kind rather than at the first character. The model's own text is the JSON
    object the prompt asked for, found by its braces, because a model may still
    wrap its object in a sentence.

    `useful` must be a JSON boolean. A model asked for `true or false` may write
    the string `"false"`, which is truthy, so a coercion would record the
    opposite label and hand it to the gate with no error at all.
    """
    starts = [at for at in (stdout.find("["), stdout.find("{")) if at >= 0]
    if not starts:
        raise ValueError(f"no JSON in the answer: {stdout[:200]!r}")
    envelope = json.loads(stdout[min(starts) :])
    if isinstance(envelope, list):
        envelope = [entry for entry in envelope if entry.get("type") == "result"][-1]
    text = envelope.get("result", "")
    answer = json.loads(text[text.index("{") : text.rindex("}") + 1])
    useful = answer["useful"]
    if not isinstance(useful, bool):
        raise ValueError(f"useful is not a boolean: {useful!r}")
    return useful, str(answer.get("reason", "")), envelope.get("total_cost_usd")


def call_context(finished: subprocess.CompletedProcess | None) -> str:
    """What a failed `claude` call left behind, for the summary `main` prints.

    A bad model name, an expired login, and a rate limit all reach the parser as
    an empty stdout, so the exit code and the tail of stderr are the only way to
    tell them apart without running the whole grading again.
    """
    if finished is None:
        return ""
    return f" (exit {finished.returncode}, stderr: {finished.stderr.strip()[:200]!r})"


def judge(item: dict, model: str) -> dict:
    """Grade one folded item, or carry the reason it could not be graded."""
    finished = None
    try:
        finished = subprocess.run(
            command(model, prompt_for(item)),
            capture_output=True,
            text=True,
            timeout=CALL_TIMEOUT_S,
        )
        useful, reason, cost = read_answer(finished.stdout)
        return {"item": item, "useful": useful, "reason": reason, "cost_usd": cost}
    except Exception as failure:  # noqa: BLE001 - one bad call must not end the run
        error = f"{type(failure).__name__}: {failure}"[:300] + call_context(finished)
        return {"item": item, "error": error}


def read_judgements(out: Path) -> dict[str, dict[str, dict]]:
    """The judgements already in the file, or nothing when it is not one.

    A truncated write, an empty file, or a hand edit all leave something this
    script cannot merge. That is a file with nothing to keep rather than a
    reason to end the run, so the operator is told and the run writes over it.
    """
    try:
        existing = json.loads(out.read_text())
    except (OSError, ValueError) as failure:
        print(f"{out} cannot be read, so it is replaced: {failure}", file=sys.stderr)
        return {}
    if not isinstance(existing, dict) or not all(
        isinstance(answers, dict) for answers in existing.values()
    ):
        print(f"{out} is not a judgements file, so it is replaced.", file=sys.stderr)
        return {}
    return existing


def write_judgements(out: Path, judgements: dict, replace: bool) -> int:
    """Fold this run's judgements over the file already there, and say how many.

    A judgement was paid for, so a run that graded a different slice of the
    record adds to the file rather than emptying it. `--replace` is the one way
    to drop what is there, for the run that means to.

    The write lands on a `.pending` sibling and is renamed, so a run that dies
    part way leaves the earlier file whole.

    A file this script cannot read is merged as if it were absent. There is
    nothing to keep from it, and refusing would throw away the calls this run
    just paid for, so it says so on stderr and writes anyway.
    """
    merged: dict[str, dict[str, dict]] = {}
    if out.exists() and not replace:
        merged = read_judgements(out)
    kept = sum(len(answers) for answers in merged.values())
    for item_id, answers in judgements.items():
        merged.setdefault(item_id, {}).update(answers)

    pending = out.with_name(out.name + ".pending")
    pending.write_text(json.dumps(merged, indent=2, ensure_ascii=False) + "\n")
    os.replace(pending, out)
    return kept


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "record",
        type=Path,
        help="the --record directory of a bench run, or its checks.json",
    )
    parser.add_argument(
        "--out",
        type=Path,
        help="where to write judgements.json (default: beside checks.json)",
    )
    parser.add_argument("--model", default=MODEL, help=f"the judge (default {MODEL})")
    parser.add_argument("--jobs", type=int, default=4, help="calls in flight at once")
    parser.add_argument("--limit", type=int, help="grade only the first N items")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the selection and the first prompt, and call nothing",
    )
    parser.add_argument(
        "--replace",
        action="store_true",
        help="overwrite the output file rather than adding this run to it",
    )
    arguments = parser.parse_args()

    checks_path = arguments.record
    if checks_path.is_dir():
        checks_path = checks_path / "checks.json"
    checks = json.loads(checks_path.read_text())

    items = fold(non_exact_hits(checks))
    items.sort(key=lambda item: (item["id"], item["result_text"]))
    if arguments.limit is not None:
        items = items[: arguments.limit]
    print(
        f"{len(checks)} Checks, {len(items)} folded non-exact hits to judge.",
        file=sys.stderr,
    )

    if arguments.dry_run:
        for item in items:
            print(f"{item['id']}\t{item['rows']}\t{item['result_text']}")
        if items:
            print("\n--- the first prompt ---\n" + prompt_for(items[0]))
        return 0

    with concurrent.futures.ThreadPoolExecutor(arguments.jobs) as pool:
        graded = list(pool.map(lambda item: judge(item, arguments.model), items))

    judgements: dict[str, dict[str, dict]] = {}
    failures = []
    spend = 0.0
    for answer in graded:
        item = answer["item"]
        if "error" in answer:
            failures.append(f"{item['id']}: {answer['error']}")
            continue
        judgements.setdefault(item["id"], {})[item["result_text"]] = {
            "useful": answer["useful"],
            "reason": answer["reason"],
        }
        spend += answer["cost_usd"] or 0.0

    out = arguments.out or checks_path.with_name("judgements.json")
    refused = not judgements and out.exists() and not arguments.replace
    if refused:
        print(
            f"Nothing was graded, so {out} keeps every judgement it already holds.\n"
            "Pass --replace to empty it on purpose.",
            file=sys.stderr,
        )
    else:
        kept = write_judgements(out, judgements, arguments.replace)
        print(f"{out} now holds this run's judgements over {kept} earlier ones.", file=sys.stderr)
    print(
        f"{len(items) - len(failures)} judged, {len(failures)} failed, "
        f"{spend:.2f} USD notional.",
        file=sys.stderr,
    )
    for failure in failures:
        print(f"  {failure}", file=sys.stderr)
    return 1 if failures or refused else 0


if __name__ == "__main__":
    sys.exit(main())
