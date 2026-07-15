# Dataset selection: findings and considerations

Rationale behind choosing a benchmark dataset for db-llm-bench (which measures
how well LLMs generate database queries across TypeQL, SQL, and Cypher). This
records the options weighed and why, so the reasoning survives beyond the
conversation that produced it.

## TL;DR

- **Chosen for the pilot: the Veekun Pokédex** — a real, normalized, relationship-rich
  dataset (~15–30 core entities, a genuine n-ary relationship, downloadable data)
  that is *not* a query benchmark, so no query language gets a training-corpus
  head start.
- **Rejected the LinkML/Biolink generation route** — LinkML reliably generates
  only SQL DDL; it has no Cypher generator and only a dead TypeQL one, and it
  produces schemas, not the data + questions + gold queries the benchmark needs.
- **Rejected all famous query benchmarks / sample DBs** (IMDB/JOB, TPC-DS,
  Sakila, LDBC SNB, Northwind) on **cross-language-fairness** grounds: their
  published queries and heavily-practiced (schema, language) pairings advantage
  some languages over others, which biases the exact comparison the benchmark
  exists to make.

## What the benchmark needs from a dataset

The framework scores by executing the model's generated query and comparing the
**result** to an expected value. That imposes requirements many datasets don't meet:

1. **A schema expressible in all three languages** (TypeQL, SQL, Cypher).
2. **Populated instance data**, loadable into all three engines.
3. **Natural-language questions**, each with a hand-authored reference ("gold")
   query per language and a known expected result.
4. **Relationship richness** — ideally genuine n-ary relationships and some
   inheritance, since those are where the three languages diverge and where
   TypeDB differentiates.
5. **Cross-language fairness** — no language should be advantaged by the dataset
   choice itself (see contamination, below).

Translating a schema is the cheap part. The expensive, error-prone parts are the
data and a validated question set with equivalent gold across three languages —
and those are exactly what a schema-only tool like LinkML cannot provide.

## Schema scale: the trade-offs

"Scale" is a proxy for four things that don't move together:

- **Vocabulary breadth** (entity count) → drives *navigation/disambiguation*
  difficulty (which types are relevant, which join path connects two concepts).
- **Relationship depth/variety** → drives *structural* query complexity (hops,
  aggregation, paths, n-ary). Rises with relationship richness, only loosely
  with entity count.
- **Authoring cost and correctness risk** → every question needs equivalent gold
  in three languages; keeping them semantically identical gets harder
  super-linearly with schema size. This is the binding constraint — wrong or
  non-equivalent gold means the benchmark measures its own mistakes.
- **Prompt footprint** → the schema goes in every prompt; past some size it stops
  fitting, forcing schema-subsetting that silently changes the task into a
  retrieval problem.

Adding entities mainly buys the *first* while taxing the *third* hardest.

| Bracket | Entities | Pros | Cons |
|---|---|---|---|
| Tiny | 1–5 | Trivial, trustworthy gold; great for validating the *framework* | No navigation difficulty; poor model discrimination; no n-ary |
| Small | 6–12 | Manageable gold; some multi-hop depth if graph-shaped | Model still fits whole schema; famous ones contaminated |
| **Medium** | **15–40** | **Real navigation + structural difficulty; good discrimination; still fits the prompt** | **Authoring cost climbs; needs execution-based validation** |
| Large | 50–200+ | Maximal difficulty; hard to saturate | Exceeds prompt context; gold authoring expensive/error-prone; slow |

Two cross-cutting points:

- **Decouple schema scale from data scale.** For result-comparison scoring you
  want a large-ish schema with *small* data — sparse population keeps expected
  answers exact and cheap to verify. "Big schema" does not force "big data".
- **The prompt-context ceiling is the real upper bound.** The largest schema you
  can benchmark cleanly is the largest whose full DDL fits comfortably in the
  prompt while still leaving the model genuine work. Past that you're measuring
  retrieval, not query generation.

**Conclusion:** target ~15–30 entities, chosen for *relationship richness*
(n-ary, inheritance) rather than raw entity count, with sparsely-populated data.

## Contamination and cross-language fairness

This was the decisive consideration, and it evolved in two steps:

1. **Don't reuse published query sets.** Any dataset that ships a canonical query
   workload (JOB's 113 queries, LDBC SNB's reference queries, Sakila/Northwind
   tutorial queries) has those queries in training corpora. Reusing them as gold
   inflates the languages they exist in — *asymmetrically*, since we'd hand-author
   the others — which corrupts the cross-language comparison specifically.

2. **Even fresh queries don't fix a famous dataset.** The advantage lives in the
   **(schema, language) pairing**, not individual queries. "IMDB + SQL" is a
   combination models have practiced thousands of times (join paths, idioms), in
   a way reading the schema in-context doesn't replicate — and there's no
   comparable corpus for IMDB + Cypher or IMDB + TypeQL. So a famous SQL benchmark
   hands SQL a schema-specific head start on top of the general base-rate.

Two confounds, only one of which is fixable:

- **(a) General language base-rate** — SQL >> Cypher >> TypeQL in total training
  representation, for *any* schema. Unavoidable, and arguably part of what we're
  honestly measuring.
- **(b) Dataset-specific pairing** — a famous dataset over-practiced in one
  language. Avoidable, stacks on top of (a), and for a TypeDB-oriented benchmark
  it's self-defeating (it handicaps TypeQL).

**Key reframe:** contamination lives in the *queries and their schema pairing*,
not in the *schema and data* themselves. So the correct posture is:

- Pick a dataset for schema + data quality.
- Author **all** gold fresh in three languages; never import a published set.
- Prefer a **real but non-benchmark, non-tutorialized** dataset, so all three
  languages start from equally low schema-specific familiarity.

A useful corollary: **"comes with ready-made queries" is a liability, not an
asset**, for a query-generation benchmark. And running three languages gives a
free contamination diagnostic — if SQL scores were inflated by dataset-specific
practice, you'd see it as an SQL-vs-graph gap beyond the base-rate.

The cleanest option of all would be a novel, purpose-built schema (zero corpus
in every language), but that was ruled out on effort grounds. "Real but obscure"
is the pragmatic compromise: real data (credible, less work) with most of the
neutrality of a novel schema.

## Candidates surveyed

### Famous benchmarks / sample DBs — rejected on fairness

| Dataset | Entities | Why rejected |
|---|---|---|
| IMDB / JOB | 21 | Real data + genuine n-ary (cast), but a famous SQL benchmark: 113 published queries and heavily-practiced IMDB+SQL pairing |
| TPC-DS | 24 | SQL-only snowflake; artificial as a graph; generated data |
| Sakila / Pagila | 16 | Kaggle is full of "explore Sakila with SQL" notebooks; also a classic tutorial DB |
| LDBC SNB | ~10 | Validated SQL + Cypher workloads — but *published*, hence contaminated; also small |
| Northwind | ~13 | The most-tutorialized schema in existence (SQL and Neo4j) |

### Real, non-benchmark datasets — the actual shortlist

| Dataset | Entities | Verdict |
|---|---|---|
| **Veekun Pokédex** | ~20–30 | **Chosen.** Fully normalized, real data, downloadable SQLite/CSV; genuine multi-way n-ary (`pokemon_moves`: pokémon + move + method + level); reflexive type-efficacy; minimal query corpus in any language |
| MusicBrainz (subset) | 12 core + link model | Strong: rich `l_*` relationship model + type hierarchy (excellent TypeDB fit), real dumps, low query corpus. Cost: 100+ tables, must subset |
| TMDB | movie domain | Fallback: same n-ary cast as IMDB without the JOB corpus, but no official bulk dump (needs heavy reshaping) |
| GTFS (transit) | ~15–20 | Rejected: published Neo4j *and* SQL query corpus (routing is a classic graph-DB demo) |
| European Soccer DB | 7 | Rejected: Kaggle SQL notebooks everywhere; thin and denormalized |

## The LinkML / Biolink route

Biolink was attractive because LinkML *seemed* able to generate the three
translations from one model. It can't, for our targets:

- **SQL DDL** — first-party, mature generator. Works.
- **Cypher / Neo4j** — *no generator exists*; only design-pattern guidance.
- **TypeQL** — only `typedb-osi/typeql-linkml`, a 4-commit, no-release
  proof-of-concept predating TypeDB 3.x.

Even if all three worked, LinkML produces **schema**, not the populated data,
questions, or gold queries — the expensive parts. Biolink itself is also
oversized (hundreds of classes, reification-heavy associations) and ships no
instance data. Ruled out.

## Cross-model design consideration: n-ary relationships

The reason relationship richness matters (and why medium scale needs
execution-based validation, not eyeballing) is that an n-ary fact has no single
canonical cross-model form:

- **SQL** — a junction table with N foreign keys.
- **TypeQL** — a native n-ary relation with named roles (its differentiator).
- **Cypher** — a property graph has only binary edges, so an n-ary fact must be
  **reified** into a node (e.g. `(:Pokemon)-[:LEARNS]->(:Learning)-[:OF_MOVE]->(:Move)`,
  `(:Learning)-[:VIA]->(:MoveMethod)`). Collapsing it onto an edge property is a
  *different, less expressive* schema and breaks equivalence with the other two.

That reification choice is a deliberate, documented design decision the dataset
author owns — and the safeguard is running all three gold queries against the
shared data and reconciling results, since equivalence can no longer be confirmed
by inspection at this scale.

## Open items when scaling beyond the pilot

- Even a real-but-obscure schema carries *some* model familiarity; mitigate with
  natural (non-canonical) question phrasings and by watching the SQL-vs-graph gap
  as a contamination signal.
- MusicBrainz remains the strongest candidate if the pilot outgrows the Pokédex
  and a richer relationship/type model is wanted (at the cost of subsetting).
- Keep data sparse and expected answers execution-validated as the question set
  grows; hand-authored equivalence is not trustworthy by inspection at medium scale.
