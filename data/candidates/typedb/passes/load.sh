#!/usr/bin/env bash
# Multi-pass TypeDB load for the candidates dataset.
#
# Deduplicating each entity's key column up front (project.py) means no shared
# @key entity ever recurs within a batch, so unlike the single-file ../query.tql
# this can run with large, parallel batches instead of --batch-rows 1.
#
# Pass order matters: entities (1-6) before the relations that reference them
# (7 ballot links, 8 candidacy).
#
# Config via env vars (defaults shown):
#   ADDRESS=localhost:1729 DB_USER=admin DB_PASS=password DB=candidates
#   BATCH_ROWS=1000 PARALLEL=1
#   RAW=<repo>/data/candidates/data.csv.gz  (plain .csv also accepted)
#   WORK=<here>/work            writable dir for the cleaned CSV + projections
#   CLEANED_CSV=<unset>         if set, use this pre-cleaned CSV and skip cleaning
# (DB_USER/DB_PASS, not USER/PASS: USER is a standard shell variable and would
# shadow the default, authenticating as the login user.)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CAND="$(cd "$HERE/../.." && pwd)"          # data/candidates
ROOT="$(cd "$CAND/../.." && pwd)"          # repo root

ADDRESS="${ADDRESS:-localhost:1729}"
DB_USER="${DB_USER:-admin}"
DB_PASS="${DB_PASS:-password}"
DB="${DB:-candidates}"
BATCH_ROWS="${BATCH_ROWS:-1000}"
# Sequential batches: TypeDB attributes are global value objects, so concurrent
# batches that insert shared values (gender "Male", honorific "Mr", a shared
# election on a relation) conflict on the attribute/instance lock and the losing
# batch is rejected. Dedup buys large batches, not parallel commits. Large
# batches alone are already ~1000x fewer transactions than --batch-rows 1.
PARALLEL="${PARALLEL:-1}"
RAW="${RAW:-$CAND/data.csv.gz}"

SCHEMA="$CAND/typedb/schema.tql"
WORK="${WORK:-$HERE/work}"
CLEANED="${CLEANED_CSV:-$WORK/data.cleaned.csv}"
mkdir -p "$WORK"

if [ -n "${CLEANED_CSV:-}" ]; then
    echo "== using pre-cleaned CSV: $CLEANED =="
else
    echo "== cleaning (typedb dialect) =="
    python3 "$CAND/clean.py" "$RAW" "$CLEANED" --dialect typedb
fi

echo "== projecting per-pass CSVs =="
python3 "$HERE/project.py" "$CLEANED" "$WORK"

# Derived, not projected: the defection edges roll up across rows rather than
# narrowing them, and the same script feeds the Postgres and Neo4j loads.
echo "== deriving defection edges =="
python3 "$CAND/derive_defections.py" "$CLEANED" "$WORK/defection.csv"

run_pass() {                                # run_pass <query.tql> <data.csv> [extra args...]
    local query="$1" data="$2"; shift 2
    echo "== load: $(basename "$query") <- $(basename "$data") =="
    typedb loader \
        --query "$HERE/$query" \
        --data "$WORK/$data" \
        --header true \
        --database "$DB" \
        --address "$ADDRESS" \
        --username "$DB_USER" \
        --password "$DB_PASS" \
        --tls-disabled true \
        --batch-rows "$BATCH_ROWS" \
        --parallel-batches "$PARALLEL" \
        --no-checkpoint \
        "$@"
}

console() {                                 # console <console args...>
    typedb console \
        --address "$ADDRESS" \
        --username "$DB_USER" \
        --password "$DB_PASS" \
        --tls-disabled \
        "$@"
}

# `--create-db true` creates the database only when it is absent, and it is
# absent that keeps a re-run honest: entities dedupe on their @key, but
# candidacy and the three ballot links are relations with no key, so loading
# over existing data inserts a second copy of every one of them (a silent
# doubling the loader reports as a clean run). Drop first, so re-running this
# reloads rather than duplicates.
if console --command "database list" | grep -qx "$DB"; then
    echo "== dropping existing database: $DB =="
    console --command "database delete $DB"
fi

# Pass 1 creates the database and installs the schema; the rest load into it.
run_pass 1-person.tql       person.csv        --create-db true --schema-file "$SCHEMA"
run_pass 2-party.tql        party.csv
run_pass 3-post.tql         post.csv
run_pass 4-election.tql     election.csv
run_pass 5-organisation.tql organisation.csv
run_pass 6-ballot.tql       ballot.csv
run_pass 7-ballot-links.tql ballot-links.csv
run_pass 8-candidacy.tql    candidacy.csv
run_pass 9-defection.tql    defection.csv

echo "== done =="
