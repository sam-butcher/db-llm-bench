#!/usr/bin/env python3
"""Emit the per-pass CSV projections for the multi-pass TypeDB load.

The single-file loader (../query.tql) has to run at `--batch-rows 1`, because a
shared @key entity appearing twice in one batch is inserted twice and only
caught at commit (the loader's `put` match phase can't see sibling rows in the
same batch). Deduplicating each entity's key column *before* the loader removes
that collision, so the multi-pass load can run with large parallel batches.

Each output CSV's header matches exactly the `given` block of the pass that
consumes it (no extra columns), so column-to-variable binding is unambiguous.
Passes 1-6 are deduplicated to one row per entity key; the ballot key column
feeds both the ballot entity pass and the ballot-relation pass. The candidacy
projection is not deduplicated - each base row is one candidacy - it just
narrows the 63-column base CSV to the candidacy columns.

Deduplication merges *per column*, keeping the first non-blank value each
column takes across the key's rows, rather than keeping the first row whole.
The rows for one key can disagree: post gss:E05008823's first row has no nuts1
while a later one does, and 10 posts carry two post_labels (ward renames).
First-row-wins dropped that nuts1, leaving TypeDB one region short of the other
two DBs, which both resolve to first-non-blank-per-column (`coalesce(node.x,
row.x)` in Neo4j, a `first_non_blank` aggregate in Postgres).

Usage: project.py CLEANED_CSV OUT_DIR
"""
import csv, os, sys

# The prefix -> kind mapping lives with the other derivation, so the TypeDB
# passes and the Postgres/Neo4j loads cannot drift apart on which election is
# which type.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
from derive_election_kinds import PREFIX_KIND

PERSON = ["person_id", "person_name", "honorific_prefix", "honorific_suffix",
          "gender", "birth_date", "death_date", "favourite_biscuit", "email",
          "facebook_page_url", "facebook_personal_url", "homepage_url",
          "blog_url", "linkedin_url", "party_ppc_page_url", "twitter_username",
          "mastodon_username", "wikipedia_url", "wikidata_id", "youtube_profile",
          "instagram_url", "blue_sky_url", "threads_url", "tiktok_url",
          "other_url", "mnis_id", "twfy_id", "image", "person_last_updated"]
PARTY = ["party_id", "party_name", "legacy_party_id"]
POST = ["post_id", "post_label", "gss", "nuts1"]
ELECTION = ["election_id", "election_date", "election_current"]
ORGANISATION = ["organisation_name"]
BALLOT = ["ballot_paper_id", "seats_contested", "cancelled_poll", "by_election",
          "by_election_reason", "party_lists_in_use", "candidates_locked",
          "total_electorate", "turnout_reported", "turnout_percentage",
          "spoilt_ballots", "results_source"]
BALLOT_LINKS = ["ballot_paper_id", "election_id", "post_id", "organisation_name"]
CANDIDACY = ["person_id", "ballot_paper_id", "party_id", "party_description_text",
             "party_list_position", "previous_party_affiliations",
             "sopn_first_names", "sopn_last_name", "votes_cast", "elected",
             "tied_vote_winner", "rank", "statement_to_voters",
             "statement_last_updated"]

# (output file, dedup key column or None, columns). A None key means every row
# is kept (candidacy is one-per-row); otherwise rows sharing a key merge into
# one, each column taking its first non-blank value.
PROJECTIONS = [
    ("person.csv", "person_id", PERSON),
    ("party.csv", "party_id", PARTY),
    ("post.csv", "post_id", POST),
    ("organisation.csv", "organisation_name", ORGANISATION),
    ("ballot.csv", "ballot_paper_id", BALLOT),
    ("ballot-links.csv", "ballot_paper_id", BALLOT_LINKS),
    ("candidacy.csv", None, CANDIDACY),
]


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: project.py CLEANED_CSV OUT_DIR")
    src, out_dir = sys.argv[1], sys.argv[2]
    os.makedirs(out_dir, exist_ok=True)

    # `election` is abstract in the TypeDB schema, so its rows go to one file
    # per leaf kind and are loaded by one pass each. Merged the same way as the
    # keyed projections below: first non-blank per column, not first row whole.
    election_files, election_writers = {}, {}
    for kind in sorted(set(PREFIX_KIND.values())):
        f = open(os.path.join(out_dir, f"election-{kind}.csv"), "w", newline="")
        election_files[kind] = f
        election_writers[kind] = csv.writer(f)
        election_writers[kind].writerow(ELECTION)
    elections = {}

    files, writers, counts = {}, {}, {}
    # One merged row per key, insertion-ordered so the output keeps
    # first-appearance order. Unkeyed projections stream straight out instead.
    merged = {}
    for name, key, cols in PROJECTIONS:
        f = open(os.path.join(out_dir, name), "w", newline="")
        files[name] = f
        writers[name] = csv.writer(f)
        writers[name].writerow(cols)
        merged[name] = {} if key else None
        counts[name] = 0

    with open(src, newline="") as fi:
        for row in csv.DictReader(fi):
            election_id = row["election_id"]
            if election_id:
                kept = elections.get(election_id)
                if kept is None:
                    kind = PREFIX_KIND.get(election_id.split(".", 1)[0])
                    if kind is None:
                        sys.exit(f"unclassified election_id: {election_id}")
                    elections[election_id] = (kind, [row[c] for c in ELECTION])
                else:
                    for i, c in enumerate(ELECTION):
                        if not kept[1][i]:
                            kept[1][i] = row[c]
            for name, key, cols in PROJECTIONS:
                if key is None:
                    writers[name].writerow([row[c] for c in cols])
                    counts[name] += 1
                    continue
                kept = merged[name].get(row[key])
                if kept is None:
                    merged[name][row[key]] = [row[c] for c in cols]
                    counts[name] += 1
                else:
                    for i, c in enumerate(cols):
                        if not kept[i]:
                            kept[i] = row[c]

    for name, key, _ in PROJECTIONS:
        if key is not None:
            writers[name].writerows(merged[name].values())
    election_counts = {kind: 0 for kind in election_writers}
    for kind, values in elections.values():
        election_writers[kind].writerow(values)
        election_counts[kind] += 1

    for f in list(files.values()) + list(election_files.values()):
        f.close()
    for name, _, _ in PROJECTIONS:
        print(f"  {name}: {counts[name]} rows")
    for kind, n in sorted(election_counts.items()):
        print(f"  election-{kind}.csv: {n} rows")


if __name__ == "__main__":
    main()
