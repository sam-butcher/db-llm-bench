#!/usr/bin/env python3
"""How the failing runs fail: with a visible error, or silently wrong.

A run that fails can end in two ways: the query errors (syntax error, timeout,
wrong shape — anything the harness can see and feed back for a retry), or it
executes cleanly and returns a wrong answer, which nothing downstream can tell
from a right one. This script reports that split per DB, twice:

- at retry level 0 (the first attempt), classifying only the failing runs —
  the raw failure profile of each language; and
- at the highest retry level (the final outcome), over ALL runs — correct vs
  errored vs silently wrong after the retry loop has done what it can.

Only answerable questions count: an unanswerable question has no wrong-answer
failure mode. Both tables pool models; pass model=<substring> to filter.

Usage: analysis/failure_modes.py [results.json] [model=<substring>]
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _common as C


def pct(n, total):
    return f"{100 * n / total:.0f}% ({n}/{total})" if total else "-"


def main():
    args = sys.argv[1:]
    model_filter = None
    paths = []
    for a in args:
        if a.startswith("model="):
            model_filter = a.split("=", 1)[1]
        else:
            paths.append(a)
    path = paths[0] if paths else "results-reactome.json"

    records = [r for r in C.load_records(path) if not r["unanswerable"]]
    if model_filter:
        records = [r for r in records if model_filter in r["model"]]
    if not records:
        sys.exit(f"no records in {path}")
    dbs = sorted({r["db"] for r in records})
    max_retry = max(r["maxRetries"] for r in records)

    print(f"{path}  (answerable questions; models pooled"
          + (f", filtered to *{model_filter}*" if model_filter else "") + ")\n")

    print("First attempt (retry level 0): how the failing runs fail")
    rows = []
    for db in dbs:
        fails = [r for r in records
                 if r["db"] == db and r["maxRetries"] == 0 and not r["accurate"]]
        errored = sum(1 for r in fails if r["error"])
        rows.append([db, len(fails), pct(errored, len(fails)),
                     pct(len(fails) - errored, len(fails))])
    print(C.render_table(["DB", "failures", "visible error", "silently wrong"], rows))

    print(f"\nFinal outcome (retry level {max_retry}): all runs")
    rows = []
    for db in dbs:
        runs = [r for r in records if r["db"] == db and r["maxRetries"] == max_retry]
        correct = sum(1 for r in runs if r["accurate"])
        errored = sum(1 for r in runs if not r["accurate"] and r["error"])
        silent = len(runs) - correct - errored
        fails = errored + silent
        rows.append([db, len(runs), pct(correct, len(runs)), pct(errored, len(runs)),
                     pct(silent, len(runs)), pct(errored, fails)])
    print(C.render_table(
        ["DB", "runs", "correct", "error", "silently wrong", "error share of failures"], rows))


if __name__ == "__main__":
    main()
