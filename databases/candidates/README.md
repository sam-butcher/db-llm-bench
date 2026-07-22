# Candidates DBs

`docker compose` boots the three benchmark DBs and loads the Democracy Club
candidates dataset from [`../../data/candidates`](../../data/candidates):

```sh
cd databases/candidates
docker compose up -d
```

| DB       | Endpoint                    | Credentials                                                                     | Seeding                                              |
| -------- | --------------------------- | ------------------------------------------------------------------------------- | --------------------------------------------------- |
| Postgres | `localhost:5432` (`candidates`) | benchmark: `bench_ro` / `bench_ro` (SELECT-only); admin: `postgres` / `postgres` | one-shot `postgres-seed` (schema → server-side COPY → role) |
| Neo4j    | `bolt://localhost:7687`     | `neo4j` / `password`                                                             | one-shot `neo4j-seed` via cypher-shell (`LOAD CSV` MERGE, idempotent) |
| TypeDB   | `localhost:1729`            | `admin` / `password`                                                            | **not in compose** — run the host multi-pass loader (below) |

A shared `prep` service cleans the raw CSV once (`../../data/candidates/clean.py`)
and fans the result out to two volumes — Postgres and Neo4j both read the file
on the *server*, and Neo4j chowns its import dir to `700 neo4j`, so it needs its
own copy rather than sharing Postgres's.

## TypeDB: the multi-pass loader

TypeDB's bulk load uses the `typedb loader` binary, a host tool, and it's the
thing under test — so it isn't a compose service. With the servers up, load it
against the `typedb` service:

```sh
data/candidates/typedb/passes/load.sh
```

See [`../../data/candidates/typedb/passes/README.md`](../../data/candidates/typedb/passes/README.md).
The compose `typedb` image tag **must match the host loader version** (3.12.1) —
a protocol-version mismatch panics the server on the handshake.

## Notes

- A distinct compose project name (`candidates`) keeps this isolated from the
  sample DBs compose. They expose the same ports, so run one dataset at a time.
- Re-running `docker compose up` re-seeds Neo4j idempotently (MERGE); Postgres's
  load uses `ON CONFLICT DO NOTHING`. To reset everything including volumes:
  `docker compose down -v && docker compose up -d`.
- All three loads were verified to produce identical counts: person 119,686 ·
  party 668 · organisation 486 · post 17,772 · election 3,927 · ballot 39,271 ·
  candidacy 217,872, with each ballot relation (`at_election`/`for_post`/
  `elects_to`) at 39,271.
