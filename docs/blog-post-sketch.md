# Blog post sketch: results of the Sonnet 5 / DeepSeek V4 Pro run

A working outline for a blog post on `results-sonnet-deepseek/full.json`. Numbers below are
reproducible with `analysis/accuracy_by_db.py` and `analysis/accuracy_by_variation.py`; the
failure-mode and recovery numbers come from ad-hoc scripts over `analysis/_common.py` records.

## Working title

*"The model doesn't know your query language — and that's fine": benchmarking LLM query
generation across SQL, Cypher and TypeQL*

## 1. The benchmark, briefly

- 42 natural-language questions over [Reactome](https://reactome.org), the biological pathway
  curation database, loaded identically into MySQL, Neo4j (5.26) and TypeDB (3.x). Real,
  large-schema data (~87k characters of MySQL DDL) with no published query corpus in any of the
  three languages — chosen specifically so no language gets a training-data head start on the
  *dataset* (the general SQL > Cypher > TypeQL base rate in training corpora remains, and is
  itself one of the things measured).
- The model gets the schema and the question, writes one query, the query is executed, and the
  *result* is compared to a known expected answer. Wrong shape counts as wrong — producing the
  asked-for shape is part of using a language.
- Question tiers name the construct they stress, not just difficulty: easy / medium / expert,
  plus **recursion** (transitive closure over the pathway hierarchy), **reification** (n-ary
  facts), **argmax** (per-group extremes), **aggregation** (stacked aggregates), **polymorphism**
  (querying through the class hierarchy), and **unanswerable** (does the model admit it can't).
- This run's grid: 2 models (Claude Sonnet 5, DeepSeek V4 Pro) × 3 DBs × skills on/off (each
  DB's official/vendored query-writing skill in the prompt) × 0/5 few-shot examples × retry
  budget 0/2/4 × 3 repetitions — 3,024 top-level runs, ~68M tokens. Retries fire only on
  failures the harness can *see* (syntax error, timeout, wrong result shape); a query that runs
  and returns a wrong answer is terminal.

## 2. Headline numbers

Averaged over all variations at the full retry budget:

| model | MySQL | Neo4j | TypeDB |
|---|---|---|---|
| Claude Sonnet 5 | 77% | 75% | 75% |
| DeepSeek V4 Pro | 83% | 81% | 65% |

Reads as "SQL wins, TypeQL trails" — but the average hides the real story, which is how
differently the three languages respond to help. Every finding below is a cut of the same grid.

## 3. Finding: in-context resources flip the ranking

Accuracy pooled over both models, non-unanswerable questions, full retry budget:

| config | MySQL | Neo4j | TypeDB |
|---|---|---|---|
| no skill, 0 examples | 72% | 65% | **29%** |
| skill only | 77% | 71% | 70% |
| examples only | 81% | 83% | 82% |
| skill + examples | 84% | 86% | **89%** |

- Bare, TypeDB is by far the worst — DeepSeek with no skill, no examples and no retries scores
  **7%** on TypeQL. The models simply haven't seen much TypeQL 3.x; they have seen decades of SQL.
- Fully resourced, TypeDB is the *best* of the three, and the single best cell in the whole
  72-row variation grid is **Sonnet 5 + TypeDB + skill + examples + retries: 93%** (117/126),
  ahead of the best Neo4j cell (90%, DeepSeek) and the best MySQL cell (86%, DeepSeek).
- The skill alone is worth +41 points to TypeDB (29% → 70%) versus +5 for SQL and +6 for Cypher.
  Angle for the post: what looks like a language deficit is mostly a *training-data* deficit,
  and ~33KB of in-context documentation substantially erases it. For anyone deploying
  LLM-generated queries, "how well does the model already know the language" matters much less
  than "how well can the language be taught in context" — and a regular, composable language
  teaches well.

## 4. Finding: TypeQL fails loudly, SQL fails silently

Of the runs that failed on their first attempt (no retries, answerable questions):

| | MySQL | Neo4j | TypeDB |
|---|---|---|---|
| failed with a visible error | 40% | 62% | **85%** |
| ran fine, returned the wrong answer | 60% | 38% | 15% |

- A silent wrong answer is the worst outcome for any real application: nothing downstream can
  tell it from a right one. At the full retry budget the share of *all* runs ending in a
  silent wrong answer is 21% for MySQL and Neo4j vs 15% for TypeDB — and TypeDB's residual
  failures are mostly still loud (53% carry an error), while SQL's are almost entirely silent
  (97% of its remaining failures return a plausible wrong answer).
- Likely mechanism worth discussing: SQL happily joins any two columns and aggregates over
  duplicated rows; TypeQL's schema is part of the query semantics, so a wrong guess about
  structure (a nonexistent role, an attribute on the wrong type) is a type error, not an
  empty-ish result.
- This is also *why* retries help TypeDB so much: the retry loop can only act on failures it
  can see. TypeDB + skill + examples goes 77% → 89% → 93% (Sonnet) as the retry budget grows
  0 → 2 → 4; SQL barely moves because its failures don't announce themselves. Loud failure
  converts a fixed accuracy ceiling into an engineering knob.

## 5. Finding: polymorphism is a schema problem before it is a query problem

Best-resourced config, pooled models (33 polymorphism runs each per model×db):

| | MySQL | Neo4j | TypeDB |
|---|---|---|---|
| polymorphism accuracy | 55% | 65% | **82%** |

Showcase question: *"Ignoring case, how many database objects go by a name of some kind that
contains 'PIK3' although their display name does not?"* (expected: 189)

- The TypeQL reference query is six lines: `name` is an attribute *supertype*, so `$x has name
  $n` covers gene names, systematic names, surnames — every kind of name — in one pattern.
- The SQL reference is a 17-branch `UNION ALL` over per-class name tables; the Cypher one is a
  wall of `coalesce` over differently-typed properties. Neither model ever got the SQL right
  (0/6 in the best config); TypeDB scored 4/6.
- The point to make: this isn't the LLM being better or worse at a syntax — it's the query
  being *derivable from the schema* in one language and requiring exhaustive schema knowledge
  in the others. LLMs inherit exactly the failure mode human engineers have here: forgetting
  one of the seventeen tables.

## 6. Honest weaknesses (include these — they buy credibility)

- **Argmax is TypeQL's worst tier** (2/6 vs 6/6 for both others in the best config on the
  authors-of-most-references question). The expected answer is a *tie* (two people), and a
  per-group extreme with ties needs the pipeline re-derived twice in TypeQL; models reach for
  `sort ... limit 1` and return one of the two winners. SQL's window functions and Cypher's
  `collect` make the tie-safe version natural. This is a real language gap, currently mitigated
  only by user-defined functions.
- **Recursion is resource-hungry**: TypeQL's recursive functions are exotic enough that bare
  models score 0% on the recursion tier, but with skill + examples it recovers to 12/12
  (Sonnet) — the same in-context-learnability story as §3, just steeper.
- **Token cost**: TypeQL runs consumed roughly 2× the output tokens of SQL runs (retry loops
  plus a more verbose skill). Loud failure isn't free.

## 7. Caveats / footnotes

- The Cypher skill is written for Cypher 25 and instructs a `CYPHER 25` preamble that the
  benchmark's Neo4j 5.26 rejects, which zeroes one Sonnet cell (9%) at 0 retries; with any
  retry budget the model drops the preamble and the cell recovers to normal. Skill-on Neo4j
  numbers at 0 retries should be read with that in mind (or that cell excluded).
- The SQL "skill" is a PostgreSQL best-practices document (no comparable query-writing skill
  exists for SQL — models already write SQL well, which is itself a data point) running against
  MySQL.
- Unanswerable detection is 100% across the board — the tier is saturated and carries no
  discriminating signal in this run.
- One results quirk: Sonnet input-token counts in this file are implausibly low (hundreds);
  treat cross-model token comparisons as DeepSeek-only until that's fixed.
