-- Load the cleaned candidates CSV into the normalised schema.
-- Staging holds the flat file as text; each entity is de-duplicated into
-- its table (DISTINCT ON key), then candidacy links them. Pass the CSV
-- path as :data_path, e.g.:
--   psql -v data_path=/path/cleaned.csv -f schema.sql -f load.sql

DROP TABLE IF EXISTS staging;
CREATE TABLE staging (
    person_id TEXT,
    person_name TEXT,
    election_id TEXT,
    ballot_paper_id TEXT,
    election_date TEXT,
    election_current TEXT,
    party_name TEXT,
    party_id TEXT,
    post_label TEXT,
    cancelled_poll TEXT,
    seats_contested TEXT,
    honorific_prefix TEXT,
    honorific_suffix TEXT,
    by_election TEXT,
    by_election_reason TEXT,
    party_description_text TEXT,
    sopn_last_name TEXT,
    sopn_first_names TEXT,
    legacy_party_id TEXT,
    party_list_position TEXT,
    party_lists_in_use TEXT,
    gss TEXT,
    post_id TEXT,
    candidates_locked TEXT,
    nuts1 TEXT,
    organisation_name TEXT,
    previous_party_affiliations TEXT,
    votes_cast TEXT,
    elected TEXT,
    tied_vote_winner TEXT,
    rank TEXT,
    turnout_reported TEXT,
    spoilt_ballots TEXT,
    total_electorate TEXT,
    turnout_percentage TEXT,
    results_source TEXT,
    email TEXT,
    facebook_page_url TEXT,
    facebook_personal_url TEXT,
    homepage_url TEXT,
    blog_url TEXT,
    linkedin_url TEXT,
    party_ppc_page_url TEXT,
    twitter_username TEXT,
    mastodon_username TEXT,
    wikipedia_url TEXT,
    wikidata_id TEXT,
    youtube_profile TEXT,
    instagram_url TEXT,
    blue_sky_url TEXT,
    threads_url TEXT,
    tiktok_url TEXT,
    other_url TEXT,
    mnis_id TEXT,
    twfy_id TEXT,
    gender TEXT,
    birth_date TEXT,
    death_date TEXT,
    favourite_biscuit TEXT,
    statement_to_voters TEXT,
    statement_last_updated TEXT,
    person_last_updated TEXT,
    image TEXT
);
COPY staging FROM :'data_path' WITH (FORMAT csv, HEADER true);

INSERT INTO party (party_id, party_name, legacy_party_id)
SELECT DISTINCT ON (party_id)
    NULLIF(party_id,''),
    NULLIF(party_name,''),
    NULLIF(legacy_party_id,'')
FROM staging
WHERE NULLIF(party_id,'') IS NOT NULL
ORDER BY party_id
ON CONFLICT (party_id) DO NOTHING;

INSERT INTO organisation (organisation_name)
SELECT DISTINCT ON (organisation_name)
    NULLIF(organisation_name,'')
FROM staging
WHERE NULLIF(organisation_name,'') IS NOT NULL
ORDER BY organisation_name
ON CONFLICT (organisation_name) DO NOTHING;

INSERT INTO election (election_id, election_date, election_current)
SELECT DISTINCT ON (election_id)
    NULLIF(election_id,''),
    NULLIF(election_date,'')::date,
    NULLIF(election_current,'')::boolean
FROM staging
WHERE NULLIF(election_id,'') IS NOT NULL
ORDER BY election_id
ON CONFLICT (election_id) DO NOTHING;

INSERT INTO person (person_id, person_name, honorific_prefix, honorific_suffix, gender, birth_date, death_date, favourite_biscuit, email, facebook_page_url, facebook_personal_url, homepage_url, blog_url, linkedin_url, party_ppc_page_url, twitter_username, mastodon_username, wikipedia_url, wikidata_id, youtube_profile, instagram_url, blue_sky_url, threads_url, tiktok_url, other_url, mnis_id, twfy_id, image, person_last_updated)
SELECT DISTINCT ON (person_id)
    NULLIF(person_id,'')::integer,
    NULLIF(person_name,''),
    NULLIF(honorific_prefix,''),
    NULLIF(honorific_suffix,''),
    NULLIF(gender,''),
    NULLIF(birth_date,'')::date,
    NULLIF(death_date,'')::date,
    NULLIF(favourite_biscuit,''),
    NULLIF(email,''),
    NULLIF(facebook_page_url,''),
    NULLIF(facebook_personal_url,''),
    NULLIF(homepage_url,''),
    NULLIF(blog_url,''),
    NULLIF(linkedin_url,''),
    NULLIF(party_ppc_page_url,''),
    NULLIF(twitter_username,''),
    NULLIF(mastodon_username,''),
    NULLIF(wikipedia_url,''),
    NULLIF(wikidata_id,''),
    NULLIF(youtube_profile,''),
    NULLIF(instagram_url,''),
    NULLIF(blue_sky_url,''),
    NULLIF(threads_url,''),
    NULLIF(tiktok_url,''),
    NULLIF(other_url,''),
    NULLIF(mnis_id,''),
    NULLIF(twfy_id,''),
    NULLIF(image,''),
    NULLIF(person_last_updated,'')::timestamp
FROM staging
WHERE NULLIF(person_id,'') IS NOT NULL
ORDER BY person_id
ON CONFLICT (person_id) DO NOTHING;

INSERT INTO post (post_id, post_label, organisation_name, gss, nuts1)
SELECT DISTINCT ON (post_id)
    NULLIF(post_id,''),
    NULLIF(post_label,''),
    NULLIF(organisation_name,''),
    NULLIF(gss,''),
    NULLIF(nuts1,'')
FROM staging
WHERE NULLIF(post_id,'') IS NOT NULL
ORDER BY post_id
ON CONFLICT (post_id) DO NOTHING;

INSERT INTO ballot (ballot_paper_id, election_id, post_id, seats_contested, cancelled_poll, by_election, by_election_reason, party_lists_in_use, candidates_locked, total_electorate, turnout_reported, turnout_percentage, spoilt_ballots, results_source)
SELECT DISTINCT ON (ballot_paper_id)
    NULLIF(ballot_paper_id,''),
    NULLIF(election_id,''),
    NULLIF(post_id,''),
    NULLIF(seats_contested,'')::integer,
    NULLIF(cancelled_poll,'')::boolean,
    NULLIF(by_election,'')::boolean,
    NULLIF(by_election_reason,''),
    NULLIF(party_lists_in_use,'')::boolean,
    NULLIF(candidates_locked,'')::boolean,
    NULLIF(total_electorate,'')::integer,
    NULLIF(turnout_reported,'')::integer,
    NULLIF(turnout_percentage,'')::numeric,
    NULLIF(spoilt_ballots,'')::integer,
    NULLIF(results_source,'')
FROM staging
WHERE NULLIF(ballot_paper_id,'') IS NOT NULL
ORDER BY ballot_paper_id
ON CONFLICT (ballot_paper_id) DO NOTHING;

INSERT INTO candidacy (person_id, ballot_paper_id, party_id, party_description_text, party_list_position, previous_party_affiliations, sopn_first_names, sopn_last_name, votes_cast, elected, tied_vote_winner, rank, statement_to_voters, statement_last_updated)
SELECT DISTINCT ON (person_id, ballot_paper_id)
    NULLIF(person_id,'')::integer,
    NULLIF(ballot_paper_id,''),
    NULLIF(party_id,''),
    NULLIF(party_description_text,''),
    NULLIF(party_list_position,'')::integer,
    NULLIF(previous_party_affiliations,''),
    NULLIF(sopn_first_names,''),
    NULLIF(sopn_last_name,''),
    NULLIF(votes_cast,'')::integer,
    NULLIF(elected,'')::boolean,
    NULLIF(tied_vote_winner,'')::boolean,
    NULLIF(rank,'')::integer,
    NULLIF(statement_to_voters,''),
    NULLIF(statement_last_updated,'')::timestamp
FROM staging
WHERE NULLIF(person_id,'') IS NOT NULL AND NULLIF(ballot_paper_id,'') IS NOT NULL
ORDER BY person_id, ballot_paper_id
ON CONFLICT (person_id, ballot_paper_id) DO NOTHING;

DROP TABLE staging;
