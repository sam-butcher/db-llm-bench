-- Democracy Club candidates data, normalised from data/candidates/headers.csv.
-- Each CSV row is one candidacy (a person standing in a ballot for a party);
-- the flat file is split into the entities it denormalises. Keys use the
-- identifiers the CSV already carries (person_id, party_id, election_id,
-- ballot_paper_id, post_id); organisation has no id in the CSV, so it is keyed
-- on its name.

CREATE TABLE party (
    party_id TEXT PRIMARY KEY,
    party_name TEXT,
    legacy_party_id TEXT
);

CREATE TABLE organisation (
    organisation_name TEXT PRIMARY KEY
);

CREATE TABLE post (
    post_id TEXT PRIMARY KEY,
    post_label TEXT,
    gss TEXT,                       -- ONS geography code for the area
    nuts1 TEXT                      -- statistical region
);

CREATE TABLE election (
    election_id TEXT PRIMARY KEY,
    election_date DATE,
    election_current BOOLEAN
);

CREATE TABLE ballot (
    ballot_paper_id TEXT PRIMARY KEY,
    election_id TEXT REFERENCES election(election_id),
    post_id TEXT REFERENCES post(post_id),
    -- The organisation elected to is a property of the contest (the ballot),
    -- not the post: a Welsh seat elected to "Welsh assembly" pre-2020 and
    -- "Senedd Cymru" from 2021, and the ballot's election fixes which.
    organisation_name TEXT REFERENCES organisation(organisation_name),
    seats_contested INTEGER,
    cancelled_poll BOOLEAN,
    by_election BOOLEAN,
    by_election_reason TEXT,
    party_lists_in_use BOOLEAN,
    candidates_locked BOOLEAN,
    total_electorate INTEGER,
    turnout_reported INTEGER,
    turnout_percentage NUMERIC(5, 2),
    spoilt_ballots INTEGER,
    results_source TEXT
);

CREATE TABLE person (
    person_id INTEGER PRIMARY KEY,
    person_name TEXT,
    honorific_prefix TEXT,
    honorific_suffix TEXT,
    gender TEXT,
    birth_date DATE,
    death_date DATE,
    favourite_biscuit TEXT,
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
    mnis_id TEXT,                   -- Parliament Members Names ID
    twfy_id TEXT,                   -- TheyWorkForYou ID
    image TEXT,
    person_last_updated TIMESTAMP
);

-- The n-ary fact: one person standing in one ballot for one party, with the
-- result and nomination details.
CREATE TABLE candidacy (
    person_id INTEGER REFERENCES person(person_id),
    ballot_paper_id TEXT REFERENCES ballot(ballot_paper_id),
    party_id TEXT REFERENCES party(party_id),
    party_description_text TEXT,
    party_list_position INTEGER,
    previous_party_affiliations TEXT,   -- delimited list of prior party ids
    sopn_first_names TEXT,              -- name as on the statement of persons nominated
    sopn_last_name TEXT,
    votes_cast INTEGER,
    elected BOOLEAN,
    tied_vote_winner BOOLEAN,
    rank INTEGER,                       -- finishing position in the ballot
    statement_to_voters TEXT,
    statement_last_updated TIMESTAMP,
    PRIMARY KEY (person_id, ballot_paper_id)
);
