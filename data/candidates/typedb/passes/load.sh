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
#   ADDRESS=localhost:1729 USER=admin PASS=password DB=candidates
#   BATCH_ROWS=1000 PARALLEL=8
#   RAW=<repo>/data/candidates/data.csv   (skips clean+project if WORK already populated)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CAND="$(cd "$HERE/../.." && pwd)"          # data/candidates
ROOT="$(cd "$CAND/../.." && pwd)"          # repo root

ADDRESS="${ADDRESS:-localhost:1729}"
USER="${USER:-admin}"
PASS="${PASS:-password}"
DB="${DB:-candidates}"
BATCH_ROWS="${BATCH_ROWS:-1000}"
PARALLEL="${PARALLEL:-8}"
RAW="${RAW:-$CAND/data.csv}"

SCHEMA="$CAND/typedb/schema.tql"
WORK="$HERE/work"
CLEANED="$WORK/data.cleaned.csv"
mkdir -p "$WORK"

echo "== cleaning (typedb dialect) =="
python3 "$CAND/clean.py" "$RAW" "$CLEANED" --dialect typedb

echo "== projecting per-pass CSVs =="
python3 "$HERE/project.py" "$CLEANED" "$WORK"

run_pass() {                                # run_pass <query.tql> <data.csv> [extra args...]
    local query="$1" data="$2"; shift 2
    echo "== load: $(basename "$query") <- $(basename "$data") =="
    typedb loader \
        --query "$HERE/$query" \
        --data "$WORK/$data" \
        --header true \
        --database "$DB" \
        --address "$ADDRESS" \
        --username "$USER" \
        --password "$PASS" \
        --tls-disabled true \
        --batch-rows "$BATCH_ROWS" \
        --parallel-batches "$PARALLEL" \
        --no-checkpoint \
        "$@"
}

# Pass 1 creates the database and installs the schema; the rest load into it.
run_pass 1-person.tql       person.csv        --create-db true --schema-file "$SCHEMA"
run_pass 2-party.tql        party.csv
run_pass 3-post.tql         post.csv
run_pass 4-election.tql     election.csv
run_pass 5-organisation.tql organisation.csv
run_pass 6-ballot.tql       ballot.csv
run_pass 7-ballot-links.tql ballot-links.csv
run_pass 8-candidacy.tql    candidacy.csv

echo "== done =="
