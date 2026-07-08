# Databases

One `docker compose` manifest boots all three benchmark DBs and seeds each
with the car dataset from [`../data`](../data):

```sh
cd databases
docker compose up -d
cd .. && cargo run -p db-typedb --example seed   # TypeDB seeding (idempotent)
```

| DB       | Endpoint                 | Credentials         | Seeding                                     |
| -------- | ------------------------ | ------------------- | ------------------------------------------- |
| TypeDB   | `localhost:1729`         | `admin` / `password` | `cargo run -p db-typedb --example seed` (the server image ships no console; the seeder drops and recreates `bench` via typedb-driver) |
| Postgres | `localhost:5432` (`bench`) | `postgres` / `postgres` | native `initdb.d` hook on first boot    |
| Neo4j    | `bolt://localhost:7687`  | `neo4j` / `password` | one-shot `neo4j-seed` via cypher-shell (idempotent) |

Credentials and endpoints match the defaults in `src/config.yml` and the DB
packages' `#[ignore]`d live tests (`cargo test -- --ignored`).

To reseed everything from scratch: `docker compose down -v && docker compose up -d`,
then re-run the TypeDB seeder.
