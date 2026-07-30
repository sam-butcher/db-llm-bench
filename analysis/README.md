# Analysis

Scripts for summarising a benchmark results JSON (as written by `bench-cli`).
Python 3, standard library only. Each takes the results path as its argument
and defaults to `results-candidates.json`.

| Script | Reports |
| ------ | ------- |
| `accuracy_by_db.py` | Accuracy per DB × difficulty, collapsing all run variations (highest retry level; averaged over example count, skills, repetitions). |
| `accuracy_by_variation.py` | Accuracy per (model, db, skills, examples, retries) × difficulty — one row per variation, so you can see the effect of skill injection, few-shot count, and retry budget. |
| `incorrect_queries.py` | Every failing run: the question, its config, the expected and generated queries, and the expected vs actual answer. For debugging *what* the model got wrong. Accepts `key=value` filters (`db=`, `difficulty=`, `model=`, `examples=`, `skills=on\|off`). |
| `token_usage.py` | Total model tokens used (input/output), broken down by model and DB, plus run and call counts. |

```sh
analysis/accuracy_by_db.py results-candidates.json
analysis/accuracy_by_variation.py results-candidates.json
analysis/incorrect_queries.py results-candidates.json db=sql difficulty=hard
analysis/token_usage.py results-candidates.json
```

Notes (see `_common.py`):

- **Accuracy** is the share of runs whose generated query returned the expected
  result. `unanswerable` is its own difficulty tier (bucketed from the question's
  `unanswerable` flag); that column measures UNANSWERABLE-detection accuracy, a
  different skill from query generation.
- **Return shape counts.** A query that computes the right answer but hands back
  the wrong shape — an extra column, say — is a miss. Producing the asked-for
  shape is part of using a language, and a language that makes it awkward should
  score worse for it: Cypher won't `ORDER BY` an aggregate that isn't projected,
  so argmax answers come back as two columns unless the model adds a further
  clause. Use `incorrect_queries.py` to see which failures are of this kind.
- **Retry levels** are derived: the runner executes each run once at the highest
  configured retry level and truncates the attempt trace for the lower ones. So
  summing/averaging across levels double-counts — `accuracy_by_db` and
  `token_usage` count at the highest level only; `accuracy_by_variation` groups
  by it; `incorrect_queries` reports the highest-level (final) outcome.
