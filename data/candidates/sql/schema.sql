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
    gss TEXT,
    nuts1 TEXT
);

CREATE TABLE election_kind (
    kind TEXT PRIMARY KEY,
    parent_kind TEXT REFERENCES election_kind(kind)
);

CREATE TABLE election (
    election_id TEXT PRIMARY KEY,
    election_date DATE,
    election_current BOOLEAN,
    kind TEXT REFERENCES election_kind(kind)
);

CREATE TABLE ballot (
    ballot_paper_id TEXT PRIMARY KEY,
    election_id TEXT REFERENCES election(election_id),
    post_id TEXT REFERENCES post(post_id),
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
    mnis_id TEXT,
    twfy_id TEXT,
    image TEXT,
    person_last_updated TIMESTAMP
);

CREATE TABLE candidacy (
    person_id INTEGER REFERENCES person(person_id),
    ballot_paper_id TEXT REFERENCES ballot(ballot_paper_id),
    party_id TEXT REFERENCES party(party_id),
    party_description_text TEXT,
    party_list_position INTEGER,
    previous_party_affiliations TEXT,
    sopn_first_names TEXT,
    sopn_last_name TEXT,
    votes_cast INTEGER,
    elected BOOLEAN,
    tied_vote_winner BOOLEAN,
    rank INTEGER,
    statement_to_voters TEXT,
    statement_last_updated TIMESTAMP,
    PRIMARY KEY (person_id, ballot_paper_id)
);

CREATE TABLE defection (
    from_party_id TEXT REFERENCES party(party_id),
    to_party_id TEXT REFERENCES party(party_id),
    defectors INTEGER,
    PRIMARY KEY (from_party_id, to_party_id)
);
