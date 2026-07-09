# Databases

One `docker compose` manifest boots all three benchmark DBs and seeds each
with the car dataset from [`../data`](../data):

```sh
cd databases
docker compose up -d --build
```

| DB       | Endpoint                 | Credentials         | Seeding                                     |
| -------- | ------------------------ | ------------------- | ------------------------------------------- |
| TypeDB   | `localhost:1729`         | `admin` / `password` | one-shot `typedb-seed` (builds a console image, drops + recreates `bench`; idempotent) |
| Postgres | `localhost:5432` (`bench`) | benchmark: `bench_ro` / `bench_ro` (SELECT-only); admin: `postgres` / `postgres` | native `initdb.d` hook on first boot |
| Neo4j    | `bolt://localhost:7687`  | `neo4j` / `password` | one-shot `neo4j-seed` via cypher-shell (idempotent) |

Credentials and endpoints match the defaults in `src/config.yml` and the DB
packages' `#[ignore]`d live tests (`cargo test -- --ignored`).

Re-running `docker compose up` reseeds TypeDB and Neo4j in place. To reset
everything including Postgres: `docker compose down -v && docker compose up -d --build`.
Keep the console version in `typedb/Dockerfile` aligned with the `typedb/typedb`
image tag.
