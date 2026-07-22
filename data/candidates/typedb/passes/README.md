# Multi-pass TypeDB load

The single-file loader (`../query.tql`) inserts every entity and relation from
each base row. Because many entities recur across rows (one party appears on
hundreds of ballots), it can only run at `--batch-rows 1`: within a larger
batch the loader's `put` match phase doesn't see sibling rows, so a shared
`@key` entity is inserted twice and rejected at commit. Correct, but slow — the
full ~218k-row load takes about an hour.

This directory loads the same data in several passes, each over a CSV that has
been **deduplicated on the relevant key first**, so no key ever recurs within a
batch. That removes the collision entirely, so the passes run with large,
parallel batches. `project.py` derives the projections; `load.sh` runs the
passes in order.

## Why it's split this way

Each fact is inserted at the granularity where it's actually unique:

| Pass | Input (deduped on) | Inserts |
|------|--------------------|---------|
| 1 `person`       | `person_id`          | person entity + attributes |
| 2 `party`        | `party_id`           | party entity + attributes |
| 3 `post`         | `post_id`            | post entity + attributes |
| 4 `election`     | `election_id`        | election entity + attributes |
| 5 `organisation` | `organisation_name`  | organisation entity |
| 6 `ballot`       | `ballot_paper_id`    | ballot entity + attributes |
| 7 `ballot-links` | `ballot_paper_id`    | `at_election` / `for_post` / `elects_to` relations |
| 8 `candidacy`    | *(none — one per row)* | `candidacy` relation + attributes |

Entities come before the relations that reference them (7 and 8 `match` players
that passes 1–6 created).

The ballot relations **cannot** go in the final base-CSV pass: a ballot appears
in many consecutive rows, so a per-row insert would create one duplicate
`for_post`/`at_election`/`elects_to` per candidate on that ballot. A relation
has no `@key` to catch it, so that duplication is silent. Using `put` instead of
`insert` doesn't help — the same intra-batch blindness that forces
`--batch-rows 1` means `put` wouldn't dedupe within a batch either; it would just
silently duplicate instead of erroring. So the relations get their own
ballot-deduplicated pass (7), where each ballot appears exactly once and a plain
`insert` is guaranteed to run once. `candidacy` is genuinely one-per-row, so
pass 8 needs no dedup.

Attributes live in their entity's pass (not the final pass) so each is inserted
once, rather than re-inserted on every base row the entity appears in (which for
a low-cardinality entity like organisation would be hundreds of redundant
inserts).

Each projection's CSV header matches its pass's `given` block exactly, so
column-to-variable binding is unambiguous.

## Running

```bash
data/candidates/typedb/passes/load.sh
```

Config via env vars (defaults): `ADDRESS=localhost:1729`, `USER=admin`,
`PASS=password`, `DB=candidates`, `BATCH_ROWS=1000`, `PARALLEL=8`,
`RAW=data/candidates/data.csv`. Pass 1 creates the database and installs
`../schema.tql`; the rest load into it. Derived CSVs are written to `work/`
(git-ignored).

`../query.tql` remains the simple single-pass fallback (run it at
`--batch-rows 1`).
