#!/usr/bin/env python3
"""Generate the pilot question subset from the full Reactome question set.

The pilot exists to decide which models to keep for a full run, so the subset
is chosen to spread the axes a model can fail on rather than to sample evenly:

  * every difficulty category (expert, argmax, recursion, polymorphism) plus an
    unanswerable, since refusing correctly is its own failure mode;
  * absolute query complexity, from the shortest reference SQL in the set to
    the longest;
  * the TypeQL-to-SQL length ratio, from 0.78 to 1.98. That ratio is the
    closest cheap proxy for "this question is much harder in TypeQL than in
    SQL", which is the gradient the pilot is meant to measure — a subset where
    the three languages are uniformly easy would rank every model the same.

Index 23 is deliberately included: it is one of the four questions with a
per-store `expected_by_db`, so the pilot also exercises that path.

Regenerate rather than hand-editing the output, or it drifts from the source
whenever a question is reworded.

Usage: make_pilot.py [questions.json] [out.json]
"""

import json
import pathlib
import sys

# Indices into data/reactome/questions.json. See the module docstring for how
# these were chosen; `analysis/format_questions.py` renders them for review.
PILOT = [1, 2, 4, 10, 12, 18, 23, 32]


def main() -> None:
    src = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "data/reactome/questions.json")
    dst = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else "data/reactome/questions-pilot.json")
    questions = json.loads(src.read_text(encoding="utf8"))["questions"]

    picked = [questions[i] for i in PILOT]
    dst.write_text(
        json.dumps({"questions": picked}, indent=2, ensure_ascii=False) + "\n",
        encoding="utf8",
    )
    answerable = sum(1 for q in picked if not q.get("unanswerable"))
    print(f"wrote {dst} ({len(picked)} questions, {answerable} answerable) from {src}")


if __name__ == "__main__":
    main()
