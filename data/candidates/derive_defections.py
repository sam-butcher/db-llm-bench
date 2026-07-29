#!/usr/bin/env python3
"""Derive the party-to-party defection edges from the cleaned candidates CSV.

`candidacy.previous_party_affiliations` records, per candidacy, the parties a
candidate was affiliated with before standing for the one on that row. Rolled
up across people that becomes an edge between two parties: `from` supplied at
least one candidate who later stood for `to`. Unlike the rest of the schema
this is *derived* rather than a column of the source file, so it is computed
once, here, and the three loaders each load the identical result — the same
rule that keeps them agreeing on everything else.

Modelling decisions, all of which the loaders inherit:

- The column holds party *names*, semicolon-separated, but the edge is keyed on
  `party_id`. Names are not unique across ids (two ids share "Conservative and
  Unionist Party"), so a name resolves to whichever id carries it on the most
  candidacies, ties broken by the lower id.
- Names matching no party at all are dropped (2 of 26: parties that never
  themselves fielded a candidate in this dataset).
- Self-edges are dropped: a candidate listing their current party as a previous
  affiliation is a re-join, not a move between parties.
- `defectors` counts distinct people making that move, so one person standing
  repeatedly for the same new party counts once.

Usage: derive_defections.py [CLEANED_CSV] [OUT_CSV]
"""
import csv, sys
from collections import defaultdict


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else "data/candidates/data.cleaned.csv"
    dst = sys.argv[2] if len(sys.argv) > 2 else "data/candidates/defection.csv"

    # party name -> {party_id: rows carrying it}, to resolve names to one id.
    name_ids = defaultdict(lambda: defaultdict(int))
    # (from_name, to_party_id) -> set of people who made that move.
    movers = defaultdict(set)

    with open(src, newline="") as f:
        for row in csv.DictReader(f):
            party_id, party_name = row["party_id"], row["party_name"]
            if party_id and party_name:
                name_ids[party_name][party_id] += 1
            previous = row["previous_party_affiliations"]
            if not previous or not party_id:
                continue
            for name in previous.split(";"):
                name = name.strip()
                if name:
                    movers[(name, party_id)].add(row["person_id"])

    # Most-used id per name; `-count` first so max() picks the highest count
    # and, among equals, the lowest id.
    canonical = {
        name: min(ids.items(), key=lambda kv: (-kv[1], kv[0]))[0]
        for name, ids in name_ids.items()
    }

    edges = defaultdict(set)
    unmatched = set()
    for (from_name, to_id), people in movers.items():
        from_id = canonical.get(from_name)
        if from_id is None:
            unmatched.add(from_name)
            continue
        if from_id == to_id:
            continue
        edges[(from_id, to_id)] |= people

    with open(dst, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["from_party_id", "to_party_id", "defectors"])
        for (from_id, to_id), people in sorted(edges.items()):
            w.writerow([from_id, to_id, len(people)])

    print(f"derived {len(edges)} defection edges -> {dst}")
    if unmatched:
        print(f"  dropped {len(unmatched)} name(s) matching no party: {sorted(unmatched)}")


if __name__ == "__main__":
    main()
