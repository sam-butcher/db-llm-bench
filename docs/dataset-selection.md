# Dataset selection

Facts considered when choosing a benchmark dataset for db-llm-bench (which
measures how well LLMs generate TypeQL, SQL, and Cypher queries). This is a
neutral reference — the inputs to the decision, not the decision itself.

## Hard requirements

A candidate must provide, or let us build:

1. A schema expressible in TypeQL, SQL, and Cypher.
2. Populated instance data loadable into all three engines.
3. Natural-language questions answerable with a per-language reference query and
   a known expected result. (The dataset need not ship these; we author them.)

A dataset failing any of these is unusable regardless of other merits.

## Conditions

Weighted factors used to compare candidates that meet the hard requirements.

### Cross-language fairness (contamination)

- The framework compares a model's query *result* to an expected value; scoring
  is per language, and comparing across languages is a core output.
- Published query sets (benchmark or tutorial queries) appear in LLM training
  corpora. Reusing them as reference queries advantages the languages they exist
  in — asymmetrically, since the others are hand-authored.
- The advantage attaches to the (schema, language) *pairing*, not only to
  individual queries: a schema heavily queried in one language (e.g. IMDB in SQL)
  is easier for a model in that language even with freshly written queries.
  Supplying the schema in-context does not remove this.
- Two distinct effects: (a) a general base-rate — SQL > Cypher > TypeQL in total
  training volume — present for any dataset; (b) a dataset-specific pairing,
  present only for datasets famous in a particular language. (b) is avoidable by
  choosing a dataset with no large query corpus in any language.
- Contamination attaches to queries and their schema pairing, not to schema/data
  alone. A real-but-not-benchmark dataset with reference queries authored fresh
  minimizes it; a novel purpose-built schema eliminates it.

### Schema scale

- Entity count drives navigation/disambiguation difficulty; relationship
  depth/variety drives structural query difficulty; the two are largely
  independent.
- Authoring cost, and the risk of non-equivalent cross-language reference
  queries, rise super-linearly with schema size.
- The full schema is inserted into every prompt; beyond a size it no longer fits,
  forcing schema subsetting (which changes the task into retrieval).
- Schema scale and data volume are independent: sparse data keeps expected
  answers exact regardless of schema size.

| Bracket | Entities | Effect                                                                                                                          |
|---------|----------|---------------------------------------------------------------------------------------------------------------------------------|
| Tiny    | 1–5      | Trivial, trustworthy reference queries; little navigation difficulty; poor model discrimination                                 |
| Small   | 6–12     | Manageable authoring; some multi-hop depth if graph-shaped; model still fits the whole schema                                   |
| Medium  | 15–40    | Real navigation + structural difficulty; good discrimination; still fits the prompt; authoring needs execution-based validation |
| Large   | 50–200+  | Maximal difficulty; exceeds prompt context; authoring expensive and error-prone                                                 |

### Relationship richness

- n-ary relationships and inheritance are where the three languages diverge and
  where TypeDB differs from SQL/Cypher.
- An n-ary fact has no single cross-model form: SQL uses a junction table; TypeQL
  a native n-ary relation; Cypher must reify it into a node (an edge property is a
  less-expressive, non-equivalent alternative). Equivalence across the three must
  be validated by execution, not inspection.

### Sourcing effort

Whether real data and a loadable schema are readily available, versus requiring
reshaping or subsetting.

## Candidate datasets

Approximate entity counts refer to a benchmark-relevant core, not every table.

### Biolink via LinkML (generation approach)
- **Pros:** first-party SQL DDL generator (mature); first-party TypeDB generator
  (`gen-typedb` / `TypeDBGenerator`) emitting TypeQL 3.x `define` blocks; Biolink
  is large and richly modeled.
- **Cons:** no first-party Cypher/Neo4j generator (design-pattern guidance only);
  all LinkML generators produce *schema only* — not instance data, questions, or
  reference queries (the expensive parts); the TypeDB generator has documented
  limitations (enum values recorded as comments, not enforced; `xsd:duration`
  stored as string; partial mixin/multiple-inheritance support; auto-renames
  reserved-keyword collisions); Biolink ships no instance data and is very large
  (hundreds of classes, reification-heavy associations).

### IMDB / JOB (~21 entities)
- **Pros:** real, correlated data; genuine n-ary (cast: person/movie/character);
  naturally graph-shaped; subsettable.
- **Cons:** famous SQL benchmark with 113 published queries and a heavily-practiced
  IMDB+SQL pairing; no comparable Cypher/TypeQL corpus (asymmetric); raw data is
  large (~74M rows) and needs subsetting.

### TPC-DS (~24 entities)
- **Pros:** within target size; scalable data generator.
- **Cons:** SQL-only; snowflake shape is artificial as a graph; generated
  (uniform) data; established benchmark.

### Sakila / Pagila (~16 entities)
- **Pros:** real data included; exists as SQL (MySQL/Postgres) plus a community
  Neo4j port; within target size.
- **Cons:** classic tutorial database with abundant published SQL and Cypher.

### LDBC SNB (~10 entities)
- **Pros:** deterministic data generator; validated SQL and Cypher workloads;
  graph-shaped with recursive relationships.
- **Cons:** reference queries are published (contaminated); small entity count;
  binary-relationship-heavy (thin on n-ary and inheritance).

### Northwind (~13 entities)
- **Pros:** real data; official SQL and Neo4j ports.
- **Cons:** among the most-tutorialized schemas in existence (heavy SQL and
  Cypher corpus).

### Veekun Pokédex (~20–30 entities)
- **Pros:** fully normalized real data; downloadable SQLite/CSV; genuine
  multi-way n-ary (`pokemon_moves`: pokémon + move + method + level); reflexive
  type-efficacy; category/lookup tables; no canonical query workload in any
  language.
- **Cons:** a simplified "learn SQL with Pokémon" tutorial exists; models hold
  factual Pokémon priors (domain knowledge, not query knowledge); "toy" domain
  perception; upstream stores display names in i18n tables (simplified on load).

### MusicBrainz subset (12 core entities + link tables)
- **Pros:** systematic, richly-typed relationship model (`l_<entity1>_<entity2>`
  tables plus hierarchical `link_type` / `link_attribute_type` taxonomies —
  thousands of relationship types in a tree); attribute qualifiers on
  relationships can express ternary facts (e.g. an instrument on a performance);
  real downloadable Postgres dumps; no canonical query workload.
- **Cons:** relationships are binary-with-attributes, not native n-ary relations;
  no entity inheritance (the hierarchy is in the type taxonomies, not the
  entities); full schema is 100+ tables and requires deliberate subsetting; some
  self-hosted SQL exists in the wild.

### TMDB (movie domain)
- **Pros:** same n-ary cast structure as IMDB without the JOB query corpus; real
  data.
- **Cons:** no official bulk dump (API, or flat Kaggle CSV/JSON blobs) requiring
  heavy reshaping into a relational/graph schema; movie-analysis notebooks exist.

### GTFS / transit (~15–20 entities)
- **Pros:** real data (every transit agency publishes it); graph-natural network;
  ternary-ish `stop_times`.
- **Cons:** published Neo4j projects/blogs and SQL/PostGIS content — contaminated
  in two languages; little inheritance.

### European Soccer Database (7 entities)
- **Pros:** real data; downloadable SQLite.
- **Cons:** abundant Kaggle SQL notebooks (contaminated); few entities;
  denormalized (wide `Match` table).
