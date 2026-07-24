# Analysis

Scripts for summarising a benchmark results JSON (as written by `bench-cli`).
Python 3, standard library only. Each takes the results path as its argument
and defaults to `results-candidates.json`.

| Script | Reports |
| ------ | ------- |
| `accuracy_by_db.py` | Accuracy per DB × difficulty, collapsing all run variations (highest retry level; averaged over example count, skills, repetitions). |
| `accuracy_by_variation.py` | Accuracy per (model, db, skills, examples, retries) × difficulty — one row per variation, so you can see the effect of skill injection, few-shot count, and retry budget. |

```sh
analysis/accuracy_by_db.py results-candidates.json
analysis/accuracy_by_variation.py results-candidates.json
```

Notes (see `_common.py`):

- **Accuracy** is the share of runs whose generated query returned the expected
  result. `unanswerable` is its own difficulty tier (bucketed from the question's
  `unanswerable` flag); that column measures UNANSWERABLE-detection accuracy, a
  different skill from query generation.
- **Retry levels** are derived: the runner executes each run once at the highest
  configured retry level and truncates the attempt trace for the lower ones. So
  averaging across levels double-counts — `accuracy_by_db` collapses to the
  highest level, `accuracy_by_variation` groups by it.
