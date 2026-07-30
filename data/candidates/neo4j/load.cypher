// Load the cleaned candidates CSV into the property graph. Nodes are
// MERGEd on their key (deduping shared entities); Candidacy is reified.
// Pass both CSVs as params, e.g.:
//   cypher-shell -u neo4j -p password --param "csv => 'file:///cleaned.csv'" \
//     --param "defection_csv => 'file:///defection.csv'" \
//     --param "election_kind_csv => 'file:///election_kind.csv'" -f load.cypher
//
// LOAD CSV yields null for a blank cell, never '' (verified: no column in the
// cleaned CSV ever produces an empty string), and the conversion functions
// propagate null, so a bare `row.x` is already the "value or null" the schema
// wants.
//
// Person/Party/Post/Election/Ballot are written once per CSV row that mentions
// them, so plain assignment made the *last* row win — losing `post.nuts1` for
// gss:E05001147, whose last row is blank, and disagreeing with the other two
// DBs on four post_labels that differ across rows (ward renames). Postgres takes
// one whole row per key with `DISTINCT ON` and TypeDB's `put` keeps what it
// first stored, so both effectively hold the earliest value; `coalesce(node.x,
// row.x)` matches them — first non-blank per property wins, and a blank never
// overwrites. Candidacy needs no guard: the CSV holds exactly one row per
// (person, ballot).

CREATE CONSTRAINT person_key IF NOT EXISTS FOR (n:Person) REQUIRE n.person_id IS UNIQUE;
CREATE CONSTRAINT party_key IF NOT EXISTS FOR (n:Party) REQUIRE n.party_id IS UNIQUE;
CREATE CONSTRAINT organisation_key IF NOT EXISTS FOR (n:Organisation) REQUIRE n.organisation_name IS UNIQUE;
CREATE CONSTRAINT post_key IF NOT EXISTS FOR (n:Post) REQUIRE n.post_id IS UNIQUE;
CREATE CONSTRAINT election_key IF NOT EXISTS FOR (n:Election) REQUIRE n.election_id IS UNIQUE;
CREATE CONSTRAINT ballot_key IF NOT EXISTS FOR (n:Ballot) REQUIRE n.ballot_paper_id IS UNIQUE;

LOAD CSV WITH HEADERS FROM $csv AS row
CALL {
  WITH row
  MERGE (person:Person {person_id: toInteger(row.person_id)})
    SET person.person_name = coalesce(person.person_name, row.person_name),
      person.honorific_prefix = coalesce(person.honorific_prefix, row.honorific_prefix),
      person.honorific_suffix = coalesce(person.honorific_suffix, row.honorific_suffix),
      person.gender = coalesce(person.gender, row.gender),
      person.birth_date = coalesce(person.birth_date, date(row.birth_date)),
      person.death_date = coalesce(person.death_date, date(row.death_date)),
      person.favourite_biscuit = coalesce(person.favourite_biscuit, row.favourite_biscuit),
      person.email = coalesce(person.email, row.email),
      person.facebook_page_url = coalesce(person.facebook_page_url, row.facebook_page_url),
      person.facebook_personal_url = coalesce(person.facebook_personal_url, row.facebook_personal_url),
      person.homepage_url = coalesce(person.homepage_url, row.homepage_url),
      person.blog_url = coalesce(person.blog_url, row.blog_url),
      person.linkedin_url = coalesce(person.linkedin_url, row.linkedin_url),
      person.party_ppc_page_url = coalesce(person.party_ppc_page_url, row.party_ppc_page_url),
      person.twitter_username = coalesce(person.twitter_username, row.twitter_username),
      person.mastodon_username = coalesce(person.mastodon_username, row.mastodon_username),
      person.wikipedia_url = coalesce(person.wikipedia_url, row.wikipedia_url),
      person.wikidata_id = coalesce(person.wikidata_id, row.wikidata_id),
      person.youtube_profile = coalesce(person.youtube_profile, row.youtube_profile),
      person.instagram_url = coalesce(person.instagram_url, row.instagram_url),
      person.blue_sky_url = coalesce(person.blue_sky_url, row.blue_sky_url),
      person.threads_url = coalesce(person.threads_url, row.threads_url),
      person.tiktok_url = coalesce(person.tiktok_url, row.tiktok_url),
      person.other_url = coalesce(person.other_url, row.other_url),
      person.mnis_id = coalesce(person.mnis_id, row.mnis_id),
      person.twfy_id = coalesce(person.twfy_id, row.twfy_id),
      person.image = coalesce(person.image, row.image),
      person.person_last_updated = coalesce(person.person_last_updated, datetime(row.person_last_updated))
  MERGE (party:Party {party_id: row.party_id})
    SET party.party_name = coalesce(party.party_name, row.party_name),
      party.legacy_party_id = coalesce(party.legacy_party_id, row.legacy_party_id)
  MERGE (org:Organisation {organisation_name: row.organisation_name})
  MERGE (post:Post {post_id: row.post_id})
    SET post.post_label = coalesce(post.post_label, row.post_label),
      post.gss = coalesce(post.gss, row.gss),
      post.nuts1 = coalesce(post.nuts1, row.nuts1)
  MERGE (election:Election {election_id: row.election_id})
    SET election.election_date = coalesce(election.election_date, date(row.election_date)),
      election.election_current = coalesce(election.election_current, toBoolean(row.election_current))
  MERGE (ballot:Ballot {ballot_paper_id: row.ballot_paper_id})
    SET ballot.seats_contested = coalesce(ballot.seats_contested, toInteger(row.seats_contested)),
      ballot.cancelled_poll = coalesce(ballot.cancelled_poll, toBoolean(row.cancelled_poll)),
      ballot.by_election = coalesce(ballot.by_election, toBoolean(row.by_election)),
      ballot.by_election_reason = coalesce(ballot.by_election_reason, row.by_election_reason),
      ballot.party_lists_in_use = coalesce(ballot.party_lists_in_use, toBoolean(row.party_lists_in_use)),
      ballot.candidates_locked = coalesce(ballot.candidates_locked, toBoolean(row.candidates_locked)),
      ballot.total_electorate = coalesce(ballot.total_electorate, toInteger(row.total_electorate)),
      ballot.turnout_reported = coalesce(ballot.turnout_reported, toInteger(row.turnout_reported)),
      ballot.turnout_percentage = coalesce(ballot.turnout_percentage, toFloat(row.turnout_percentage)),
      ballot.spoilt_ballots = coalesce(ballot.spoilt_ballots, toInteger(row.spoilt_ballots)),
      ballot.results_source = coalesce(ballot.results_source, row.results_source)
  MERGE (ballot)-[:AT_ELECTION]->(election)
  MERGE (ballot)-[:FOR_POST]->(post)
  MERGE (ballot)-[:ELECTS_TO]->(org)
  MERGE (person)-[:STOOD]->(cand:Candidacy)-[:IN_BALLOT]->(ballot)
  MERGE (cand)-[:FOR_PARTY]->(party)
    SET cand.party_description_text = row.party_description_text,
      cand.party_list_position = toInteger(row.party_list_position),
      cand.previous_party_affiliations = row.previous_party_affiliations,
      cand.sopn_first_names = row.sopn_first_names,
      cand.sopn_last_name = row.sopn_last_name,
      cand.votes_cast = toInteger(row.votes_cast),
      cand.elected = toBoolean(row.elected),
      cand.tied_vote_winner = toBoolean(row.tied_vote_winner),
      cand.rank = toInteger(row.rank),
      cand.statement_to_voters = row.statement_to_voters,
      cand.statement_last_updated = datetime(row.statement_last_updated)
} IN TRANSACTIONS OF 1000 ROWS
;

// Party-to-party defections, pre-derived by ../derive_defections.py so that all
// three loaders hold the identical graph. Both endpoints were created above, so
// the MATCH always resolves; MERGE keeps a re-run idempotent.
LOAD CSV WITH HEADERS FROM $defection_csv AS row
CALL {
  WITH row
  MATCH (from_party:Party {party_id: row.from_party_id})
  MATCH (to_party:Party {party_id: row.to_party_id})
  MERGE (from_party)-[defection:DEFECTED_TO]->(to_party)
    SET defection.defectors = toInteger(row.defectors)
} IN TRANSACTIONS OF 1000 ROWS
;

// Election kinds, pre-derived by ../derive_election_kinds.py. Neo4j has no label
// inheritance, so each node carries its leaf label and every ancestor label —
// that is what makes (:DevolvedElection) match all four devolved kinds. Labels
// cannot be parameterised, so each kind needs its own statement.

LOAD CSV WITH HEADERS FROM $election_kind_csv AS row
CALL {
  WITH row
  WITH row WHERE row.kind = 'parliamentary_election'
  MATCH (e:Election {election_id: row.election_id})
  SET e:ParliamentaryElection
} IN TRANSACTIONS OF 1000 ROWS
;

LOAD CSV WITH HEADERS FROM $election_kind_csv AS row
CALL {
  WITH row
  WITH row WHERE row.kind = 'european_election'
  MATCH (e:Election {election_id: row.election_id})
  SET e:EuropeanElection
} IN TRANSACTIONS OF 1000 ROWS
;

LOAD CSV WITH HEADERS FROM $election_kind_csv AS row
CALL {
  WITH row
  WITH row WHERE row.kind = 'scottish_parliament_election'
  MATCH (e:Election {election_id: row.election_id})
  SET e:DevolvedElection:ScottishParliamentElection
} IN TRANSACTIONS OF 1000 ROWS
;

LOAD CSV WITH HEADERS FROM $election_kind_csv AS row
CALL {
  WITH row
  WITH row WHERE row.kind = 'senedd_election'
  MATCH (e:Election {election_id: row.election_id})
  SET e:DevolvedElection:SeneddElection
} IN TRANSACTIONS OF 1000 ROWS
;

LOAD CSV WITH HEADERS FROM $election_kind_csv AS row
CALL {
  WITH row
  WITH row WHERE row.kind = 'ni_assembly_election'
  MATCH (e:Election {election_id: row.election_id})
  SET e:DevolvedElection:NiAssemblyElection
} IN TRANSACTIONS OF 1000 ROWS
;

LOAD CSV WITH HEADERS FROM $election_kind_csv AS row
CALL {
  WITH row
  WITH row WHERE row.kind = 'london_assembly_election'
  MATCH (e:Election {election_id: row.election_id})
  SET e:DevolvedElection:LondonAssemblyElection
} IN TRANSACTIONS OF 1000 ROWS
;

LOAD CSV WITH HEADERS FROM $election_kind_csv AS row
CALL {
  WITH row
  WITH row WHERE row.kind = 'council_election'
  MATCH (e:Election {election_id: row.election_id})
  SET e:LocalElection:CouncilElection
} IN TRANSACTIONS OF 1000 ROWS
;

LOAD CSV WITH HEADERS FROM $election_kind_csv AS row
CALL {
  WITH row
  WITH row WHERE row.kind = 'mayoral_election'
  MATCH (e:Election {election_id: row.election_id})
  SET e:LocalElection:MayoralElection
} IN TRANSACTIONS OF 1000 ROWS
;

LOAD CSV WITH HEADERS FROM $election_kind_csv AS row
CALL {
  WITH row
  WITH row WHERE row.kind = 'pcc_election'
  MATCH (e:Election {election_id: row.election_id})
  SET e:LocalElection:PccElection
} IN TRANSACTIONS OF 1000 ROWS
;
