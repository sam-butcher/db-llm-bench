# Benchmarking LLM query generation across SQL, Cypher and TypeQL

*A full report on the Claude Sonnet 5 / DeepSeek V4 Pro run of db-llm-bench*

## Summary

We asked two LLMs to answer the same 42 natural-language questions against the Reactome
curation database, loaded identically into MySQL, Neo4j and TypeDB, and scored the generated
SQL, Cypher and TypeQL by executing each query and comparing its result to a known answer.
Each question ran under every combination of two models, a query-writing skill on or off,
0 or 5 few-shot examples, and a retry budget of 0, 2 or 4, with 3 repetitions: 3,024 runs in
total.

The main results:

1. Averaged over all configurations, SQL is the most accurate target language (76–78%
   depending on model); TypeQL trails for DeepSeek (56%) and is level with Cypher for
   Sonnet (68%). The averages conceal differences in how the languages respond to
   in-context help: with skill, examples and retries all present, TypeQL is the most accurate
   language for Claude Sonnet 5 (92.9%), the most successful single configuration in the benchmark.
2. The languages fail differently. On first attempts, a failing TypeQL query surfaces as a
   visible error six times in seven; a failing SQL query returns a plausible wrong answer
   three times in five. This changes what a retry loop can achieve, and what reaches the
   application undetected.
3. Question categories separate the languages. TypeDB leads on polymorphic questions (69% vs
   48% for MySQL at the full retry budget) and trails on argmax (25% vs 96%).
4. TypeQL costs more to run: roughly twice the output tokens of SQL for both models, and
   2–2.6× those of Cypher.

Section 1 describes the benchmark, section 2 its limitations, and section 3 our
interpretation; sections 4–10 give the data.

## 1. The benchmark

[Reactome](https://reactome.org) is a professionally curated biological pathway database. Its
publishers ship each release as both a MySQL dump of the curation database and a Neo4j graph
dump; we restore both and build the TypeDB database ourselves from the Neo4j graph. The
schema is large (the MySQL DDL alone is ~62KB, or ~16k tokens) and is placed in every prompt
in full, so working with a schema too large to hold in working memory is part of the task.

Each run works as follows. The model receives a prompt containing the schema for its target
database, optional resources (below), and one question. It answers with a single query. We
execute the query and compare the result to the expected answer for that question; a run is
accurate only if the result matches. Reference queries in all three languages, and the
expected answers, were authored by us for this dataset.

A retry is triggered only by a failure the harness can observe: a syntax or execution error,
a timeout, a malformed result shape, or a response containing no query. The error is fed back
to the model with the conversation so far, up to the retry budget. A query that executes and
returns a wrong answer is terminal: the harness does not know the answer is wrong, any more
than a real application would.

### Questions

The 42 questions are split into tiers. Three are labelled by difficulty; six are labelled by
the query construct they exercise; one tier measures whether the model recognises an
unanswerable question and says so rather than guessing.

| tier         | questions | exercises                                           |
|--------------|-----------|-----------------------------------------------------|
| easy         | 4         | lookups and counts                                  |
| medium       | 5         | joins/traversals with filters                       |
| expert       | 11        | multi-hop structure, negation, subqueries           |
| recursion    | 4         | transitive closure over the pathway hierarchy       |
| reification  | 1         | n-ary facts constrained on several roles            |
| argmax       | 1         | per-group extremes (with a tie in the answer)       |
| aggregation  | 2         | stacked aggregates                                  |
| polymorphism | 11        | queries through the class hierarchy via a supertype |
| unanswerable | 3         | declining to answer                                 |

The one-question tiers (reification, argmax) are indicative only; we flag their sample sizes
wherever they appear.

### Configurations

- **Models**: Claude Sonnet 5 and DeepSeek V4 Pro.
- **Skill on/off**: whether a query-writing skill (a markdown document teaching the language)
  is included in the prompt. We use each database's official or best publicly available
  skill: TypeDB's official TypeQL skill (~33KB), the Neo4j contrib Cypher skill (~21KB), and
  a PostgreSQL best-practices skill (~11KB); no comparable query-writing skill exists for
  SQL. See §2 on how the skills compare.
- **Few-shot examples**: 0 or 5 worked question-and-query examples written for this dataset.
- **Retry budget**: 0, 2 or 4.
- **Repetitions**: 3 per combination.

Per model and database, that is 12 configurations × 42 questions × 3 repetitions = 1,512
scored runs (504 at each retry budget).

## 2. Limitations

- **The data model favours the incumbents.** Reactome designed its database for the
  relational and graph stores it publishes; our TypeDB database is a reconstruction of the
  Neo4j graph, so the structure favours Cypher. A schema designed for TypeDB first would
  likely make fuller use of its model.
- **We authored the reference queries and expected answers ourselves.** A reference query
  fixes what each question means in each language, and a mistaken or unidiomatic reference
  would bias that language's scores. All of them are published in the repository for review,
  and we would welcome critique of our SQL and Cypher references in particular from experts
  in those languages.
- **The question mix is weighted towards polymorphism.** Polymorphism has 11 of the 42
  questions, as many as the expert tier, because we expected TypeQL to do best there and
  wanted to measure it, and it does (§6). We kept the weighting moderate so the set still
  explores the languages broadly, but the mix is our choice, and a different mix would shift
  the overall numbers.
- **The skills are not equivalent.** We used the best available skill for each database
  rather than degrading any to match the weakest. The TypeQL skill is the strongest of the
  three (official, current, and written for query generation), so part of TypeDB's skill-on
  gain may reflect skill quality rather than the language's teachability. The Neo4j skill
  targets Cypher 25 and instructs a `CYPHER 25` preamble that the benchmark's Neo4j 5.26
  rejects; at a zero-retry budget this collapses one Sonnet configuration to 9% (its accuracy
  recovers fully with any retries, and the prompt names the server version). The SQL skill is
  a PostgreSQL best-practices document running against MySQL, because no
  query-writing-focused SQL skill comparable to the TypeQL and Cypher ones exists.
- **Model selection is narrow.** We paired a frontier-lab model with a strong open-weights
  model because we did not want to test only top-of-the-line frontier-lab models; no OpenAI
  or Google model is included, and results may differ across model families.
- **The unanswerable tier is saturated** (100% everywhere) and provides no discrimination in
  this run.
- **Two tiers have one question each** (argmax, reification). We report them with counts and
  treat them as indicative.

## 3. Interpretation

These are our readings of the data in sections 4–10, not measurements.

- **The TypeQL deficit is a training-data deficit, and it can be corrected.** The bare
  numbers measure what the models absorbed from pre-training; SQL's decades of corpus give it
  a 40-point head start. A 33KB skill and five examples close the gap entirely for both
  models and reverse it for one. For a team choosing a database to put behind an LLM, how
  well the language can be taught in context matters more than how much of it the model has
  already seen.
- **Failure visibility follows from where the schema is enforced.** In TypeDB the schema
  participates in query compilation, so a structurally wrong query is rejected before it
  runs. In SQL and Cypher the schema constrains much less; structurally wrong queries execute
  and produce values. This is why retries, which can only act on observable failures, buy
  TypeDB 20 points and MySQL 4.
- **Schema expressiveness converts into query accuracy.** Where the schema can state "these
  seventeen attributes are all names", one pattern covers them and the model cannot forget a
  branch. Where it cannot, every query re-encodes that knowledge, and the models drop
  branches exactly as human engineers do.
- **Reactome favours the schema-light stores.** Every label and property in it is clearly and
  consistently named, and names are the only structural guide a schema-light system gives the
  model. On data with less disciplined naming we would expect the failure-mode and
  polymorphism gaps to widen; measuring that is future work.

## 4. Fully-resourced accuracy

Accuracy with every resource present: skill, 5 examples and the full retry budget, averaged
over repetitions. All 42 questions count here, including the unanswerable tier:

| model           | MySQL | Neo4j | TypeDB |
|-----------------|-------|-------|--------|
| Claude Sonnet 5 | 84.1% | 83.3% | 92.9%  |
| DeepSeek V4 Pro | 85.7% | 89.7% | 86.5%  |

This is each language at its best in this benchmark: TypeQL is the most accurate for Sonnet
and second for DeepSeek. Averaged over every variation instead, the ordering reverses; the
averages are in §10, and §7 breaks down how the languages travel between the two points.

All three databases scored 100% on the unanswerable tier in every configuration, so that tier
contributes no signal in this run; the remaining tables in this report exclude it and use
answerable questions only (936 runs per database at each retry budget, pooling both models
and all configurations).

## 5. Token usage

Average output tokens per run at the full retry budget:

| model    | MySQL  | Neo4j | TypeDB |
|----------|--------|-------|--------|
| Sonnet   | 1,526  | 1,530 | 3,116  |
| DeepSeek | 11,150 | 8,904 | 23,300 |

TypeQL runs cost roughly twice the output tokens of SQL runs for both models, and 2.0×
(Sonnet) to 2.6× (DeepSeek) those of Cypher, a product of more retry attempts and, for
DeepSeek, longer reasoning.

## 6. Accuracy by question tier

Pooled over both models and all configurations at the full retry budget:

| tier         | MySQL           | Neo4j           | TypeDB          |
|--------------|-----------------|-----------------|-----------------|
| easy         | 94.8% (91/96)   | 100% (96/96)    | 89.6% (86/96)   |
| medium       | 95.0% (114/120) | 96.7% (116/120) | 79.2% (95/120)  |
| expert       | 82.6% (218/264) | 66.3% (175/264) | 58.7% (155/264) |
| recursion    | 92.7% (89/96)   | 79.2% (76/96)   | 56.3% (54/96)   |
| reification  | 95.8% (23/24)   | 100% (24/24)    | 83.3% (20/24)   |
| argmax       | 95.8% (23/24)   | 91.7% (22/24)   | 25.0% (6/24)    |
| aggregation  | 100% (48/48)    | 87.5% (42/48)   | 66.7% (32/48)   |
| polymorphism | 48.1% (127/264) | 60.6% (160/264) | 69.3% (183/264) |

Two tiers separate the languages in opposite directions:

- **Polymorphism** is the only tier where TypeDB leads both other databases, and the only
  tier where MySQL is under 50%. These questions range over Reactome's class hierarchy
  through a supertype. In TypeQL the supertype is a schema object (`$x has name $n` matches
  every subtype of `name`), so the query can be derived from the schema; in SQL the same
  question requires enumerating the per-class tables. On the widest such question (objects
  with any kind of name containing "PIK3" whose display name does not), the SQL reference
  query is a seventeen-branch `UNION ALL`, and no model produced a correct SQL or Cypher
  answer in any of the six best-configuration runs, against 4/6 for TypeQL.
- **Argmax** (one question; small sample) is currently TypeDB's worst tier. The expected
  answer is a two-way tie, and a tie-safe per-group extreme requires re-deriving the pipeline
  twice in TypeQL or writing a user-defined function. Models instead generate
  `sort ... limit 1` and return one of the two correct people. SQL window functions and
  Cypher's `collect` express the tie-safe version directly.

The recursion gap (56.3% for TypeDB pooled) is a resourcing effect rather than a capability
gap; see §7.

## 7. Effect of in-context resources

Accuracy pooled over both models at the full retry budget, by prompt contents:

| config                | MySQL | Neo4j | TypeDB |
|-----------------------|-------|-------|--------|
| no skill, no examples | 71.8% | 64.5% | 29.1%  |
| skill only            | 76.9% | 71.4% | 69.7%  |
| examples only         | 80.8% | 82.5% | 82.1%  |
| skill + examples      | 83.8% | 85.5% | 88.9%  |

The same split per model. Claude Sonnet 5:

| config                | MySQL | Neo4j | TypeDB |
|-----------------------|-------|-------|--------|
| no skill, no examples | 66.7% | 60.7% | 41.9%  |
| skill only            | 74.4% | 67.5% | 74.4%  |
| examples only         | 76.9% | 81.2% | 82.1%  |
| skill + examples      | 82.9% | 82.1% | 92.3%  |

DeepSeek V4 Pro:

| config                | MySQL | Neo4j | TypeDB |
|-----------------------|-------|-------|--------|
| no skill, no examples | 76.9% | 68.4% | 16.2%  |
| skill only            | 79.5% | 75.2% | 65.0%  |
| examples only         | 84.6% | 83.8% | 82.1%  |
| skill + examples      | 84.6% | 88.9% | 85.5%  |

Observations:

- The bare configuration measures what each model brings from its training data. DeepSeek
  with no resources and no retries answered 0 of 117 answerable questions in TypeQL. Both
  models have effectively no working knowledge of TypeQL 3.x from pre-training.
- The pooled skill effect is +40.6 points for TypeDB, +5.1 for MySQL and +6.8 for Neo4j. The
  examples effect is +53.0 for TypeDB, +9.0 for MySQL and +17.9 for Neo4j. TypeDB gains
  several times more than either of the other languages from the same additions.
- Fully resourced, the ordering inverts for Sonnet: TypeDB 92.3% on answerable questions
  (92.9% including the unanswerable tier), against 82.9% for MySQL and 82.1% for Neo4j. For
  DeepSeek, TypeDB (85.5%) lands between MySQL (84.6%) and Neo4j (88.9%).
- Recursion illustrates the pattern at its steepest: 0% for bare models on TypeQL's recursive
  functions, 12/12 for Sonnet fully resourced.

### Retries

With skill and examples present, accuracy by retry budget (pooled models, answerable):

| retry budget | MySQL | Neo4j | TypeDB |
|--------------|-------|-------|--------|
| 0            | 79.9% | 79.1% | 68.8%  |
| 2            | 82.9% | 84.6% | 81.2%  |
| 4            | 83.8% | 85.5% | 88.9%  |

TypeDB gains 20.1 points from retries in this configuration; MySQL gains 3.9. Of the runs
that failed at a zero-retry budget (all configurations), the share recovered by budget 4 is
35.7% for TypeDB (169/474), 36.3% for Neo4j (128/353) and 26.4% for MySQL (73/276). The
mechanism is in §8: retries act only on failures the harness can see, and the languages
differ in how visible their failures are.

## 8. Failure modes

Outcome of every first attempt (answerable questions, all configurations, share of all runs):

| first attempts                      | MySQL | Neo4j | TypeDB |
|-------------------------------------|-------|-------|--------|
| correct                             | 70.5% | 62.3% | 49.4%  |
| failed with a visible error         | 11.9% | 23.5% | 43.2%  |
| ran fine, returned the wrong answer | 17.6% | 14.2% | 7.5%   |

As a share of failures: 85.2% of failing TypeQL first attempts raise a visible error, against
62.3% for Cypher and 40.2% for SQL. The same outcome split at the full retry budget:

| after retries                       | MySQL | Neo4j | TypeDB |
|-------------------------------------|-------|-------|--------|
| correct                             | 78.3% | 76.0% | 67.4%  |
| failed with a visible error         | 0.6%  | 3.0%  | 17.2%  |
| ran fine, returned the wrong answer | 21.0% | 21.0% | 15.4%  |

Retries eliminate nearly all visible errors for MySQL and Neo4j but leave their silent
failure rates essentially unchanged (17.6% → 21.0% and 14.2% → 21.0%; the rates rise because
some retried runs land on a wrong answer). TypeDB ends with the lowest silent-failure rate
(15.4%) and the majority of its remaining failures still visible.

## 9. TypeQL error taxonomy

We classified every failing TypeQL first attempt by its TypeDB error code (script:
`analysis/typedb_errors.py`). Shares of all first attempts (234 per row, models pooled):

| skill | examples | syntax | semantic | timeout | truncated | wrongly UNANSWERABLE |
|-------|----------|--------|----------|---------|-----------|----------------------|
| off   | 0        | 32.9%  | 5.6%     | 1.3%    | 22.2%     | 13.7%                |
| off   | 5        | 15.0%  | 5.1%     | 3.4%    | 11.5%     | 0%                   |
| on    | 0        | 2.1%   | 4.3%     | 5.1%    | 21.4%     | 0.9%                 |
| on    | 5        | 2.1%   | 7.3%     | 7.3%    | 11.1%     | 0%                   |

- **Syntax errors** (the query does not parse) track language knowledge: the skill reduces
  them from 32.9% of all first attempts to 2.1%, with or without examples.
- **Semantic errors** (a type label that does not exist, a variable out of scope across
  pipeline stages, an impermissible recursion) stay in the 4–7% band in every configuration.
  These are schema-level mistakes that the compiler rejects; the equivalent mistakes in SQL
  execute and return a result.
- **Truncation** ("no query in response") means the model hit its output-token limit before
  emitting a query. 137 of the 155 truncated first attempts are DeepSeek's, whose long
  reasoning precedes its answer; this is a model artifact, not a language one.
- **Timeouts rise with resources** (1.3% → 7.3%): more queries parse and run, and some are
  too expensive to finish in time.
- Wrongly declaring an answerable question UNANSWERABLE disappears once examples are present.

## 10. Average accuracy over all variations

Accuracy averaged over every variation (skill, examples, retry budget and repetitions). As in
§4, all 42 questions count, including the unanswerable tier:

| model           | MySQL | Neo4j | TypeDB |
|-----------------|-------|-------|--------|
| Claude Sonnet 5 | 76.3% | 68.7% | 68.2%  |
| DeepSeek V4 Pro | 77.9% | 77.8% | 56.0%  |

Averaged this way, SQL leads and TypeQL trails. The average weights the resource-starved
configurations equally with the resourced ones, and TypeQL loses far more than the other
languages when resources are absent (§7).

## 11. Reproducibility

The benchmark runner, dataset build scripts, prompts, vendored skills, questions with
per-language reference queries, the full results file for this run, and the analysis scripts
that produce every table above (`accuracy_by_db.py`, `accuracy_by_variation.py`,
`failure_modes.py`, `typedb_errors.py`, `token_usage.py`) are in the db-llm-bench repository.
<!-- TODO: link the repo at its published location -->
