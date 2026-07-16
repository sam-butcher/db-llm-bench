// Load the cleaned candidates CSV into the property graph. Nodes are
// MERGEd on their key (deduping shared entities); Candidacy is reified.
// Pass the CSV as a param, e.g.:
//   cypher-shell -u neo4j -p password --param "csv => 'file:///cleaned.csv'" -f load.cypher

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
    SET person.person_name = CASE row.person_name WHEN '' THEN null ELSE row.person_name END,
      person.honorific_prefix = CASE row.honorific_prefix WHEN '' THEN null ELSE row.honorific_prefix END,
      person.honorific_suffix = CASE row.honorific_suffix WHEN '' THEN null ELSE row.honorific_suffix END,
      person.gender = CASE row.gender WHEN '' THEN null ELSE row.gender END,
      person.birth_date = CASE row.birth_date WHEN '' THEN null ELSE date(row.birth_date) END,
      person.death_date = CASE row.death_date WHEN '' THEN null ELSE date(row.death_date) END,
      person.favourite_biscuit = CASE row.favourite_biscuit WHEN '' THEN null ELSE row.favourite_biscuit END,
      person.email = CASE row.email WHEN '' THEN null ELSE row.email END,
      person.facebook_page_url = CASE row.facebook_page_url WHEN '' THEN null ELSE row.facebook_page_url END,
      person.facebook_personal_url = CASE row.facebook_personal_url WHEN '' THEN null ELSE row.facebook_personal_url END,
      person.homepage_url = CASE row.homepage_url WHEN '' THEN null ELSE row.homepage_url END,
      person.blog_url = CASE row.blog_url WHEN '' THEN null ELSE row.blog_url END,
      person.linkedin_url = CASE row.linkedin_url WHEN '' THEN null ELSE row.linkedin_url END,
      person.party_ppc_page_url = CASE row.party_ppc_page_url WHEN '' THEN null ELSE row.party_ppc_page_url END,
      person.twitter_username = CASE row.twitter_username WHEN '' THEN null ELSE row.twitter_username END,
      person.mastodon_username = CASE row.mastodon_username WHEN '' THEN null ELSE row.mastodon_username END,
      person.wikipedia_url = CASE row.wikipedia_url WHEN '' THEN null ELSE row.wikipedia_url END,
      person.wikidata_id = CASE row.wikidata_id WHEN '' THEN null ELSE row.wikidata_id END,
      person.youtube_profile = CASE row.youtube_profile WHEN '' THEN null ELSE row.youtube_profile END,
      person.instagram_url = CASE row.instagram_url WHEN '' THEN null ELSE row.instagram_url END,
      person.blue_sky_url = CASE row.blue_sky_url WHEN '' THEN null ELSE row.blue_sky_url END,
      person.threads_url = CASE row.threads_url WHEN '' THEN null ELSE row.threads_url END,
      person.tiktok_url = CASE row.tiktok_url WHEN '' THEN null ELSE row.tiktok_url END,
      person.other_url = CASE row.other_url WHEN '' THEN null ELSE row.other_url END,
      person.mnis_id = CASE row.mnis_id WHEN '' THEN null ELSE row.mnis_id END,
      person.twfy_id = CASE row.twfy_id WHEN '' THEN null ELSE row.twfy_id END,
      person.image = CASE row.image WHEN '' THEN null ELSE row.image END,
      person.person_last_updated = CASE row.person_last_updated WHEN '' THEN null ELSE datetime(row.person_last_updated) END
  MERGE (party:Party {party_id: row.party_id})
    SET party.party_name = CASE row.party_name WHEN '' THEN null ELSE row.party_name END,
      party.legacy_party_id = CASE row.legacy_party_id WHEN '' THEN null ELSE row.legacy_party_id END
  MERGE (org:Organisation {organisation_name: row.organisation_name})
  MERGE (post:Post {post_id: row.post_id})
    SET post.post_label = CASE row.post_label WHEN '' THEN null ELSE row.post_label END,
      post.gss = CASE row.gss WHEN '' THEN null ELSE row.gss END,
      post.nuts1 = CASE row.nuts1 WHEN '' THEN null ELSE row.nuts1 END
  MERGE (election:Election {election_id: row.election_id})
    SET election.election_date = CASE row.election_date WHEN '' THEN null ELSE date(row.election_date) END,
      election.election_current = toBoolean(row.election_current)
  MERGE (ballot:Ballot {ballot_paper_id: row.ballot_paper_id})
    SET ballot.seats_contested = toInteger(row.seats_contested),
      ballot.cancelled_poll = toBoolean(row.cancelled_poll),
      ballot.by_election = toBoolean(row.by_election),
      ballot.by_election_reason = CASE row.by_election_reason WHEN '' THEN null ELSE row.by_election_reason END,
      ballot.party_lists_in_use = toBoolean(row.party_lists_in_use),
      ballot.candidates_locked = toBoolean(row.candidates_locked),
      ballot.total_electorate = toInteger(row.total_electorate),
      ballot.turnout_reported = toInteger(row.turnout_reported),
      ballot.turnout_percentage = toFloat(row.turnout_percentage),
      ballot.spoilt_ballots = toInteger(row.spoilt_ballots),
      ballot.results_source = CASE row.results_source WHEN '' THEN null ELSE row.results_source END
  MERGE (ballot)-[:AT_ELECTION]->(election)
  MERGE (ballot)-[:FOR_POST]->(post)
  MERGE (post)-[:IN_ORGANISATION]->(org)
  MERGE (person)-[:STOOD]->(cand:Candidacy)-[:IN_BALLOT]->(ballot)
  MERGE (cand)-[:FOR_PARTY]->(party)
    SET cand.party_description_text = CASE row.party_description_text WHEN '' THEN null ELSE row.party_description_text END,
      cand.party_list_position = toInteger(row.party_list_position),
      cand.previous_party_affiliations = CASE row.previous_party_affiliations WHEN '' THEN null ELSE row.previous_party_affiliations END,
      cand.sopn_first_names = CASE row.sopn_first_names WHEN '' THEN null ELSE row.sopn_first_names END,
      cand.sopn_last_name = CASE row.sopn_last_name WHEN '' THEN null ELSE row.sopn_last_name END,
      cand.votes_cast = toInteger(row.votes_cast),
      cand.elected = toBoolean(row.elected),
      cand.tied_vote_winner = toBoolean(row.tied_vote_winner),
      cand.rank = toInteger(row.rank),
      cand.statement_to_voters = CASE row.statement_to_voters WHEN '' THEN null ELSE row.statement_to_voters END,
      cand.statement_last_updated = CASE row.statement_last_updated WHEN '' THEN null ELSE datetime(row.statement_last_updated) END
} IN TRANSACTIONS OF 1000 ROWS
;
