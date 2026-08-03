# Reactome DBs

`docker compose` boots the two Reactome benchmark DBs and restores the dumps
from [`../../data/reactome`](../../data/reactome):

```sh
cd databases/reactome
docker compose up -d --wait
```

`--wait` blocks until both loads have finished — neither service reports healthy
before then. Without it, `up -d` returns while the data is still loading.

| DB    | Endpoint                       | Credentials                                                                | Seeding                                                              |
| ----- | ------------------------------ | -------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| MySQL | `localhost:3306` (`reactome`)  | benchmark: `bench_ro` / `bench_ro` (SELECT-only); admin: `root` / `password` | self-seeding via `/docker-entrypoint-initdb.d` on first init          |
| Neo4j | `bolt://localhost:7687`        | `neo4j` / `password`                                                        | one-shot `neo4j-load` → `neo4j-migrate`, then the server starts       |

## Why this stack differs from the candidates one

The candidates dataset derives every DB's data from one cleaned CSV, so it needs
a shared `prep` service and a per-DB seed step. Reactome instead publishes each
release as two native dumps — a MySQL dump of the curation database and a Neo4j
dump generated from it — so each DB restores its own and the loads are
independent.

That also means the two DBs are not loaded from a common source here. They are
both derived from the same Reactome release upstream, but the graph is produced
by Reactome's own `graph-importer`, so cross-DB count parity is not something
this stack guarantees — and the loaded data confirms it does not hold:

| | MySQL | Neo4j |
| --- | --- | --- |
| `DatabaseObject` | 1,871,599 | 2,958,129 |
| `PhysicalEntity` | 410,334 | 410,271 |

The `graph-importer` is a transformation, not a mirror. **Any question authored
against this dataset needs its expected answer established per DB rather than
assumed shared**, and the divergence should be characterised before questions
are written — the candidates dataset's verified-identical-counts property does
not carry over here.

## MySQL

The dump is MySQL, not Postgres — backtick quoting, `ENGINE=MyISAM`,
`int unsigned`, `utf8mb3` collations. It will not load into Postgres, which is
why this stack runs `mysql:8.0` rather than reusing the candidates stack's
Postgres service. Pinned to 8.0 deliberately: all 242 tables are MyISAM, and
MySQL 9.x drops MyISAM support.

The dump carries no `CREATE DATABASE` or `USE` statement, so what places it in
the `reactome` schema is the entrypoint running it with
`--database=$MYSQL_DATABASE`. It contains no views, stored routines, triggers,
or `DEFINER` clauses, so no extra privileges are needed at load time.

Verified after a from-scratch `up`: all **242 tables** loaded, `DatabaseObject`
1,871,599 rows, `PhysicalEntity` 410,334, `ReactionlikeEvent` 95,780, `Pathway`
23,604. `bench_ro` reads and is refused `CREATE`.

**The benchmark cannot query this DB yet.** `src/dbs/sql` is Postgres-only —
`PgConnectOptions`/`PgPool`/`PgRow` in `src/dbs/sql/src/lib.rs`. Running the
benchmark against Reactome needs that package extended to MySQL; `sqlx` is
already the dependency and supports MySQL, so it is a `MySqlPool` branch rather
than a new dependency.

## Neo4j

Restoring takes **two** offline steps before the server can start, so both run
as one-shot containers sharing the `neo4j_data` volume:

1. `neo4j-load` — `neo4j-admin database load`.
2. `neo4j-migrate` — `neo4j-admin database migrate --force-btree-indexes-to-range`.

The migrate step is not optional. Reactome's dump is written in the **AF4.3.0**
store format (introduced in Neo4j 4.3); `database load` restores it happily, but
the server refuses to start on it. Migration in turn aborts by default because
Neo4j 5 removed BTREE indexes and the dump carries 13 BTREE indexes plus 23
uniqueness constraints backed by BTREE indexes — hence
`--force-btree-indexes-to-range`, which rebuilds them as RANGE. Reactome's are
all single-property exact-match lookups (`dbId`, `stId`, `identifier`), which
RANGE serves well, but they are repopulated from scratch, so the first queries
after a fresh load are slower until that settles.

Two details worth knowing if you change the file layout:

- `neo4j-admin` resolves the dump as `<database>.dump` inside `--from-path`, so
  the bind mount renames `reactome.graphdb.dump` to `neo4j.dump`.
- Community edition serves exactly one user database, always named `neo4j`, so
  that is the load target.

Verified after a from-scratch `up`: **2,958,129 nodes** and **11,535,888
relationships**, with 36 RANGE indexes and 23 uniqueness constraints. Every node
carries the `DatabaseObject` label (count equals the total), which is the
polymorphic supertype — labels encode the class hierarchy
(`DatabaseObject` → `PhysicalEntity` → `GenomeEncodedEntity` →
`EntityWithAccessionedSequence`).

## Notes

- A distinct compose project name (`reactome`) keeps this isolated from the
  sample and candidates stacks. It shares their Neo4j ports, so run one dataset
  at a time.
- Re-running `docker compose up` does **not** reload MySQL: the init hook fires
  only when the data directory is empty. Neo4j does reload, because
  `neo4j-load` passes `--overwrite-destination=true`. To reset both:
  `docker compose down -v && docker compose up -d --wait`.
