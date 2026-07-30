#!/usr/bin/env python3
"""Derive each election's kind from its ID prefix, for the election type
hierarchy.

The source file has no kind column: it is carried in the `election_id` prefix
(`parl.2024-07-04`, `sp.c.2021-05-06`, ...). Every DB needs the same assignment,
so it is computed once here and the three loaders load the identical result —
the rule that keeps them agreeing on everything else.

The hierarchy itself is schema, and each DB expresses it in its own way: TypeDB
as `sub` types, Neo4j as ancestor labels on the node, Postgres as rows in
`election_kind` linking each kind to its parent. This script only says which
leaf kind an election is; PARENTS below is the shape all three encode.

Usage: derive_election_kinds.py [CLEANED_CSV] [OUT_CSV]
"""
import csv, sys

# Leaf kind per election_id prefix. Senedd and its predecessor the National
# Assembly for Wales are the same body under two names, so they share a kind.
PREFIX_KIND = {
    "parl": "parliamentary_election",
    "europarl": "european_election",
    "sp": "scottish_parliament_election",
    "senedd": "senedd_election",
    "naw": "senedd_election",
    "nia": "ni_assembly_election",
    "gla": "london_assembly_election",
    "local": "council_election",
    "mayor": "mayoral_election",
    "pcc": "pcc_election",
}

# kind -> parent kind. The two middle types group the bodies that sit between a
# single national parliament and a council: devolved legislatures, and the
# local contests. Both are abstract — no election is directly one of them.
PARENTS = {
    "parliamentary_election": "election",
    "european_election": "election",
    "devolved_election": "election",
    "local_election": "election",
    "scottish_parliament_election": "devolved_election",
    "senedd_election": "devolved_election",
    "ni_assembly_election": "devolved_election",
    "london_assembly_election": "devolved_election",
    "council_election": "local_election",
    "mayoral_election": "local_election",
    "pcc_election": "local_election",
}


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else "data/candidates/data.cleaned.csv"
    dst = sys.argv[2] if len(sys.argv) > 2 else "data/candidates/election_kind.csv"

    kinds, unknown = {}, {}
    with open(src, newline="") as f:
        for row in csv.DictReader(f):
            election_id = row["election_id"]
            if not election_id or election_id in kinds:
                continue
            prefix = election_id.split(".", 1)[0]
            kind = PREFIX_KIND.get(prefix)
            if kind is None:
                unknown[prefix] = unknown.get(prefix, 0) + 1
                continue
            kinds[election_id] = kind

    with open(dst, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["election_id", "kind"])
        for election_id, kind in sorted(kinds.items()):
            w.writerow([election_id, kind])

    print(f"assigned a kind to {len(kinds)} elections -> {dst}")
    by_kind = {}
    for kind in kinds.values():
        by_kind[kind] = by_kind.get(kind, 0) + 1
    for kind, n in sorted(by_kind.items()):
        print(f"  {kind}: {n}")
    if unknown:
        # A new prefix must be classified deliberately, not silently dropped:
        # an unclassified election would be invisible to every polymorphic
        # query while still counting as an election.
        sys.exit(f"unclassified election_id prefixes: {sorted(unknown)}")


if __name__ == "__main__":
    main()
