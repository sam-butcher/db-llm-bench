"""Shared helpers for the results-analysis scripts.

Both scripts read a benchmark results JSON (as written by `bench-cli`) and
report query-generation accuracy — the share of runs whose generated query
returned the expected result.

Two facts about the record model matter for correctness:

- Unanswerable questions carry a difficulty but measure a different skill
  (emitting the UNANSWERABLE token), so they are excluded from the difficulty
  tables and reported on their own.
- The runner executes each run ONCE at the highest configured retry level and
  *derives* a record for every lower level by truncating the attempt trace.
  Records at different `maxRetries` are therefore overlapping views of the same
  execution: grouping BY maxRetries is meaningful ("accuracy within an N-retry
  budget"), but averaging ACROSS levels double-counts. `accuracy_by_db`
  collapses to the single highest level; `accuracy_by_variation` groups by it.
"""

import json

DIFF_ORDER = ["easy", "medium", "hard"]


def load_records(path):
    """Flatten the results JSON into one dict per (question, db, run)."""
    data = json.load(open(path))
    records = []
    for q in data["questions"]:
        base = {"difficulty": q["difficulty"], "unanswerable": q.get("unanswerable", False)}
        for db, info in q["dbs"].items():
            for r in info["results"]:
                records.append({
                    **base,
                    "db": db,
                    "model": r["model"],
                    "skills": r["skills"],
                    "examples": r["examples"],
                    "maxRetries": r["maxRetries"],
                    "repetition": r["repetition"],
                    "accurate": r["accurate"],
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
