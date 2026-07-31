# Dataset selection — large-schema variant

Facts considered when choosing a dataset for a second, deliberately harder
version of db-llm-bench built on a much larger schema. Companion to
[dataset-selection.md](dataset-selection.md), which covers the original
(small-schema) selection; the hard requirements there still apply.

This is a neutral reference — the inputs to the decision, not the decision
itself.

## How the criteria differ

The original selection treated a 50–200+ table schema as a cost to be avoided:
it exceeds prompt context and makes reference-query authoring expensive and
error-prone. This variant inverts that. Navigating a schema too large to hold
comfortably in context is part of what we want to measure, so scale becomes a
target rather than a penalty.

Target profile:

1. **A few hundred tables.** Large enough that schema navigation and
   disambiguation are themselves difficult.
2. **Rich relationships, preferably n-ary.** Where SQL, Cypher and TypeQL are
   forced into genuinely different shapes — a junction table, a reified node, a
   native n-ary relation.
3. **Opportunities for polymorphic modelling.** Discriminator columns, EAV
   tables, type-erased supertables, and generic link tables: places where the
   relational schema flattens a type hierarchy that TypeQL would express
   natively.
4. **A limited corpus of published queries.** Same contamination concern as the
   original doc — a schema heavily queried in one language advantages that
   language even with freshly authored reference queries.

Criteria 1 and 4 are in tension with most well-known datasets: schemas get big
when they are real production systems, and real production systems with public
query corpora are usually benchmarks, which is exactly what we need to avoid.
The datasets that survive tend to be ones where users reach the data through an
API or ORM rather than by writing SQL.

## Candidate datasets

Table counts below were counted from the projects' own DDL, not taken from
documentation.

### GMOD Chado (212 tables)

Genomics / model-organism biology schema; the reference deployment is FlyBase.
Counted from `chado/modules/default_schema.sql`.

- **Pros:** polymorphism is the schema's organising principle — genes, exons,
  transcripts, chromosomes and proteins all share one `feature` table, typed by a
  foreign key into the `cvterm` ontology table (the relational answer to a type
  hierarchy: erase it and carry the type as data, where TypeQL would model a real
  hierarchy with inherited roles and Cypher would use labels — three sharply
  divergent answers to the same question). Genuine n-ary that is then reified:
  `feature_cvterm` is already ternary (feature × cvterm × pub) and is extended by
  `feature_cvterm_prop`, `feature_cvterm_pub` and `feature_cvterm_dbxref`, giving
  a relation that itself carries attributes and further participants; likewise
  `feature_relationship` → `feature_relationshipprop` → `feature_relationship_pub`.
  Recursive typed relations appear in five separate tables
  (`feature_relationship`, `cvterm_relationship`, `organism_relationship`,
  `pub_relationship`, `analysis_relationship`), each with a `type_id` naming the
  edge kind — an extension of the recursion questions already authored against
  the party-defection graph. Query corpus is thin: users go through the GMOD
  Perl/Python APIs, GBrowse or the FlyBase web UI rather than raw SQL, and what
  exists is a handful of files in the `FlyBase/chado` repo plus GMOD wiki
  snippets. FlyBase runs a public read-only Postgres instance
  (`chado.flybase.org:5432`, user `flybase`), so questions can be screened
  against real data before committing to any load.
- **Cons:** the full FlyBase dump
  (`s3ftp.flybase.org/releases/FB2026_02/psql/FB2026_02.sql.gz`) needs roughly
  200 GB loaded, so subsetting is mandatory. Domain vocabulary (gene, transcript,
  ontology term) is well represented in model training even though schema-specific
  queries are not.

### Ensembl (269 tables across four schemas)

Genomics. Counted from each project's `sql/table.sql`: core 77, variation 64,
funcgen 72, compara 56.

- **Pros:** public MySQL dumps per species per schema at
  `ftp.ensembl.org/pub/current/mysql/`; choosing a small species (e.g.
  `saccharomyces_cerevisiae_core_63_116_4`) gives the full schema shape at a data
  volume that will not fight the 90s query cap. Polymorphism via `object_xref`,
  which carries an `ensembl_object_type` discriminator over Gene/Transcript/
  Translation, and via the `*_attrib` EAV pattern repeated across a dozen entity
  types. Compara's homology and gene-tree tables are genuinely n-ary. Thin query
  corpus for the same reason as Chado — the Perl API and BioMart are the usual
  access paths.
- **Cons:** polymorphism is narrower and more localised than Chado's; the four
  schemas are separate databases, so combining them into one benchmark schema is
  a deliberate act rather than something the project ships.

### Dolibarr ERP/CRM (~410 tables)

Business/accounting. Counted as table-creating `.sql` files under
`htdocs/install/mysql/tables/` (749 files total, 410 excluding the separate
`.key.sql` constraint files).

- **Pros:** `llx_element_element` is a fully polymorphic n-ary link table
  (`sourcetype`, `fk_source`, `targettype`, `fk_target`) — one conceptual relation
  covering every pairing of business objects, the exact construct that relational
  modelling handles badly. `llx_extrafields` is EAV. Published SQL is close to
  nil because all access goes through the PHP ORM. Table count comfortably in
  range, and the domain reads as serious in an enterprise sense.
- **Cons:** an ORM artifact rather than a designed schema; demo data is thin, so
  most instance data would have to be generated, which weakens the "real,
  correlated data" property the original selection valued.

### OpenMRS (114 tables in core)

Clinical EMR. Counted from
`liquibase/snapshots/schema-only/liquibase-schema-only-2.6.x.xml`.

- **Pros:** attractive polymorphism — `person`/`patient`/`user` subtyping, the
  `obs` table as EAV over `value_coded`/`value_numeric`/`value_datetime`/
  `value_text`, and a concept dictionary; `encounter` is naturally n-ary
  (patient × provider × location × encounter_type × visit × form). Public demo
  database available.
- **Cons:** 114 tables in core is under the target; reaching a few hundred means
  installing modules, which makes the schema deployment-specific rather than
  canonical.

## Datasets sourced from the graph-vs-relational comparison literature

A second search axis: datasets that Neo4j and other graph vendors have used to
compare themselves against SQL, on the assumption that they deliberately chose
data that models poorly in relational form.

The assumption holds, but it selects on a different dimension than schema size.
Of the 32 repositories in Neo4j's official `neo4j-graph-examples` gallery, nearly
all have fewer than 15 node types: movies, recommendations, fraud-detection
(PaySim), POLE crime data, ICIJ Panama/Paradise/Offshore Leaks, FinCEN,
network-management, cybersecurity (BloodHound), stackoverflow, airport-routes,
legis-graph. Graph vendors pick datasets that are **edge-dense and schema-narrow**,
because their claimed advantage is traversal depth and not schema breadth. So the
axis yields question archetypes far more readily than it yields large schemas.

There is also a contamination trap specific to this axis, and it runs against the
criteria: precisely because a vendor built a showcase around a dataset, that
dataset ships a browser guide full of published Cypher. Northwind is the worst
case (heavily published in both SQL and Cypher). Mining this axis for datasets
therefore tends to advantage Cypher asymmetrically — the opposite of what the
contamination condition wants.

Two candidates do combine edge-density with schema breadth.

### Reactome (242 tables)

Biological pathway/reaction knowledgebase. Counted by streaming the public
MySQL dump `gk_current.sql.gz` (115 MB compressed) from
`reactome.org/download/current/databases/` and counting `CREATE TABLE`.

- **Pros:** the strongest instance of the hypothesis behind this axis — Reactome
  maintains a relational curation database *and* an official Neo4j graph database,
  regenerating the latter from the former at each quarterly release, with the
  migration documented in PLOS Computational Biology (reporting a ~93% reduction
  in average query time as the motivation). The relational schema being a poor fit
  for the data is the stated reason the graph version exists, so the two
  representations are both real and both maintained. Data volume is very
  manageable (115 MB compressed) compared with the FlyBase Chado dump. Schema is
  auto-generated from a class model, giving class-table inheritance and a
  `DatabaseObject` supertype — genuine polymorphism rather than the emulated kind.
- **Cons:** contamination is asymmetric in the *unfavourable* direction. There is
  no cookbook document as such, but a substantial first-party Cypher corpus is
  published as source: ~365 `MATCH` clauses in `reactome/graph-core` (74 in Spring
  Data `@Query` annotations), ~216 in `reactome/graph-qa`, two `.cyp` files in
  `reactome/statistics-generator`, and ~15 worked examples on the
  [extract-participating-molecules](https://reactome.org/dev/graph-database/extract-participating-molecules)
  doc page. The relational side has no comparable published SQL, so a Cypher
  advantage is baked in. The generated schema also has a meta/generic flavour
  (`_displayName`, class-per-table) that reads less like a designed domain model.

### iTop CMDB/ITSM (197 classes)

IT service management and configuration management database. Counted as classes
carrying `_delta="define"` across the datamodel XMLs in `Combodo/iTop` (the
`itop-profiles-itil` module's `<class id="..."/>` entries are permission grants,
not definitions, and are excluded); iTop maps classes to MySQL tables with
class-table inheritance, so the table count tracks closely.

- **Pros:** the domain Neo4j showcases as `network-management` — dependency and
  root-cause analysis over IT infrastructure — but with a schema two orders of
  magnitude broader than Neo4j's example. Deep literal inheritance chains
  (`FunctionalCI` → `PhysicalDevice` → `ConnectableCI` → `DatacenterDevice` →
  `NetworkDevice`) rendered as class-table inheritance: the canonical relational
  polymorphism pain point, expressible natively in TypeQL and via labels in
  Cypher. Impact-analysis questions over the CI dependency graph are naturally
  recursive. No meaningful published query corpus in any language.
- **Cons:** it is a product, not a dataset — there is no real instance data, so
  everything would have to be generated, forfeiting real correlated data.

### Question archetypes from this axis

Useful independently of dataset choice — the query shapes graph vendors
consistently choose to contrast against SQL:

- Variable-length and unbounded-depth traversal (friends-of-friends; ICIJ
  beneficial-ownership chains).
- Recursive hierarchies (bill of materials, org charts, CI dependency trees).
- Polymorphic edges over polymorphic endpoints (BloodHound ACLs, where
  User/Group/Computer/GPO are interchangeable principals).
- Root-cause and blast-radius/impact analysis.
- Entity resolution and record linkage.

LDBC's choke-point methodology is the formalised version of this: a catalogue of
technical difficulties known to be hard for current DBMSs, with SNB BI explicitly
targeting relational, RDF and property-graph systems side by side. It is a good
source of question *design* and unusable as a dataset — its queries are published,
which is the contamination condition's exact exclusion.

## Ruled out

- **OMOP CDM (OHDSI)** — ~40 tables, under target, and fails contamination badly:
  the OHDSI cookbook, ATLAS and Achilles constitute exactly the kind of large
  published SQL corpus the criteria exclude.
- **MIMIC-III / M![img.png](img.png)IMIC-IV** — ~40 tables, and the `mimic-code` repository is a
  large published query corpus.
- **TPC-DS, Spider 2.0, BIRD** — self-defeating; their query sets *are* the
  published corpus.

## Practical note

At 200+ tables the schema must be ported to TypeQL and Cypher by hand, plus
loaders written — an order of magnitude more work than the candidates dataset's
8 tables. One mitigation is to keep the full schema in the prompt (that difficulty
is part of the point) while loading only a coherent subset. For Chado, the
sequence + cv + pub + organism modules are roughly 80 tables and retain every
construct that produces cross-language divergence.
