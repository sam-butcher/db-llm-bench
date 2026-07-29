"""Shared helpers for the results-analysis scripts.

Both scripts read a benchmark results JSON (as written by `bench-cli`) and
report query-generation accuracy — the share of runs whose generated query
returned the expected result.

Two facts about the record model matter for correctness:

- Later tiers name the *construct* a question stresses rather than a difficulty:
  `recursion` needs transitive closure, `reification` needs the n-ary candidacy
  fact constrained on several roles at once, and `control` holds questions of the
  same aggregation shape as the `reification` ones but with no candidacy in them.
  Read those columns against each other — this benchmark exists to compare query
  languages, so a tier where all DBs score alike carries no information.
- `unanswerable` is its own difficulty tier. Those questions measure a different
  skill (emitting the UNANSWERABLE token rather than a correct query), so read
  the `unanswerable` column as detection accuracy, not query accuracy.
- The runner executes each run ONCE at the highest configured retry level and
  *derives* a record for every lower level by truncating the attempt trace.
  Records at different `maxRetries` are therefore overlapping views of the same
  execution: grouping BY maxRetries is meaningful ("accuracy within an N-retry
  budget"), but averaging ACROSS levels double-counts. `accuracy_by_db`
  collapses to the single highest level; `accuracy_by_variation` groups by it.
"""

import json

DIFF_ORDER = ["easy", "medium", "hard", "expert", "recursion", "reification",
               "control", "unanswerable"]


def load_records(path):
    """Flatten the results JSON into one dict per (question, db, run).

    Unanswerable questions are bucketed under the "unanswerable" difficulty tier
    from their `unanswerable` flag, not the stored difficulty string — so result
    files written before "unanswerable" was a difficulty label tabulate the same
    way as newer ones.
    """
    data = json.load(open(path))
    records = []
    for q in data["questions"]:
        unanswerable = q.get("unanswerable", False)
        difficulty = "unanswerable" if unanswerable else q["difficulty"]
        for db, info in q["dbs"].items():
            for r in info["results"]:
                attempts = r.get("attempts") or []
                last_error = attempts[-1].get("error") if attempts and attempts[-1] else None
                records.append({
                    "difficulty": difficulty,
                    "unanswerable": unanswerable,
                    "question": q["question"],
                    "db": db,
                    "model": r["model"],
                    "skills": r["skills"],
                    "examples": r["examples"],
                    "maxRetries": r["maxRetries"],
                    "retriesUsed": r.get("retriesUsed", 0),
                    "repetition": r["repetition"],
                    "accurate": r["accurate"],
                    # Record-level token totals (already summed over this record's
                    # attempts by the runner). Used by token_usage.py.
                    "tokens": r.get("tokens") or {"input": 0, "output": 0},
                    # For per-run inspection (incorrect_queries.py); other scripts ignore these.
                    "generated": r.get("generated"),
                    "actual": r.get("result"),
                    "correct_query": info.get("correct"),
                    "expected": q.get("expected"),
                    "error": last_error,
                    # One entry per attempt (the retry trace): the query tried and its error.
                    "attempts": [{"query": a.get("query"), "error": a.get("error")} for a in attempts],
                })
    return records


def diff_order(records):
    """Difficulties present, canonical order first then any extras."""
    present = {r["difficulty"] for r in records}
    return [d for d in DIFF_ORDER if d in present] + sorted(present - set(DIFF_ORDER))


def rate(records):
    """(accurate, total) over the given records."""
    return sum(1 for r in records if r["accurate"]), len(records)


def cell(records):
    a, n = rate(records)
    return f"{100 * a / n:.0f}% ({a}/{n})" if n else "-"


def render_table(headers, rows, label_cols=1):
    """Aligned text table; the first `label_cols` columns are left-justified,
    the rest right-justified."""
    grid = [headers] + rows
    widths = [max(len(str(row[i])) for row in grid) for i in range(len(headers))]

    def line(cells):
        return "  ".join(
            str(c).ljust(w) if i < label_cols else str(c).rjust(w)
            for i, (c, w) in enumerate(zip(cells, widths))
        )

    return "\n".join([line(headers), line(["-" * w for w in widths])] + [line(r) for r in rows])
