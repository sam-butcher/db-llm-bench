-- Load the cleaned candidates CSV into the normalised schema.
-- Staging holds the flat file as text; each entity is de-duplicated into
-- its table, then candidacy links them. Pass the CSV paths as :data_path and
-- :defection_path, e.g.:
--   psql -v data_path=/path/cleaned.csv -v defection_path=/path/defection.csv \
--        -v election_kind_path=/path/election_kind.csv -f schema.sql -f load.sql
--
-- A key repeats across rows, and those rows can disagree: a cell may be blank
-- in one and filled in another (post gss:E05001147's last row has no nuts1),
-- or hold different values (ward renames give 10 posts two post_labels). Each
-- entity column therefore takes the FIRST NON-BLANK value in CSV row order,
-- per column -- `first_non_blank(col ORDER BY row_no)`. `DISTINCT ON (key)` with
-- no tiebreaker used to pick an arbitrary whole row instead, which disagreed
-- with the other two loaders on four post_labels and was not stable across
-- reloads. Neo4j's `coalesce(node.x, row.x)` and TypeDB's `put` + `try has`
-- both resolve to this same first-non-blank-per-column rule.

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

-- CSV row order. COPY appends rows in file order into a freshly created table,
-- so the identity values the rewrite assigns follow the file.
ALTER TABLE staging ADD COLUMN row_no bigint GENERATED ALWAYS AS IDENTITY;

-- Keeps the first non-null input; `ORDER BY row_no` at the call site makes
-- "first" mean the earliest CSV row rather than whatever order the scan yields.
CREATE FUNCTION keep_first(anyelement, anyelement) RETURNS anyelement
    LANGUAGE sql IMMUTABLE PARALLEL SAFE AS 'SELECT coalesce($1, $2)';
CREATE AGGREGATE first_non_blank(anyelement) (SFUNC = keep_first, STYPE = anyelement);

INSERT INTO party (party_id, party_name, legacy_party_id)
SELECT
    NULLIF(party_id,''),
    first_non_blank(NULLIF(party_name,'') ORDER BY row_no),
    first_non_blank(NULLIF(legacy_party_id,'') ORDER BY row_no)
FROM staging
WHERE NULLIF(party_id,'') IS NOT NULL
GROUP BY 1
ON CONFLICT (party_id) DO NOTHING;

INSERT INTO organisation (organisation_name)
SELECT
    NULLIF(organisation_name,'')
FROM staging
WHERE NULLIF(organisation_name,'') IS NOT NULL
GROUP BY 1
ON CONFLICT (organisation_name) DO NOTHING;

DROP TABLE IF EXISTS staging_election_kind;
CREATE TABLE staging_election_kind (election_id TEXT, kind TEXT);
COPY staging_election_kind FROM :'election_kind_path' WITH (FORMAT csv, HEADER true);

INSERT INTO election (election_id, election_date, election_current)
SELECT
    NULLIF(election_id,''),
    first_non_blank(NULLIF(election_date,'') ORDER BY row_no)::date,
    first_non_blank(NULLIF(election_current,'') ORDER BY row_no)::boolean
FROM staging
WHERE NULLIF(election_id,'') IS NOT NULL
GROUP BY 1
ON CONFLICT (election_id) DO NOTHING;

INSERT INTO person (person_id, person_name, honorific_prefix, honorific_suffix, gender, birth_date, death_date, favourite_biscuit, email, facebook_page_url, facebook_personal_url, homepage_url, blog_url, linkedin_url, party_ppc_page_url, twitter_username, mastodon_username, wikipedia_url, wikidata_id, youtube_profile, instagram_url, blue_sky_url, threads_url, tiktok_url, other_url, mnis_id, twfy_id, image, person_last_updated)
SELECT
    NULLIF(person_id,'')::integer,
    first_non_blank(NULLIF(person_name,'') ORDER BY row_no),
    first_non_blank(NULLIF(honorific_prefix,'') ORDER BY row_no),
    first_non_blank(NULLIF(honorific_suffix,'') ORDER BY row_no),
    first_non_blank(NULLIF(gender,'') ORDER BY row_no),
    first_non_blank(NULLIF(birth_date,'') ORDER BY row_no)::date,
    first_non_blank(NULLIF(death_date,'') ORDER BY row_no)::date,
    first_non_blank(NULLIF(favourite_biscuit,'') ORDER BY row_no),
    first_non_blank(NULLIF(email,'') ORDER BY row_no),
    first_non_blank(NULLIF(facebook_page_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(facebook_personal_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(homepage_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(blog_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(linkedin_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(party_ppc_page_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(twitter_username,'') ORDER BY row_no),
    first_non_blank(NULLIF(mastodon_username,'') ORDER BY row_no),
    first_non_blank(NULLIF(wikipedia_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(wikidata_id,'') ORDER BY row_no),
    first_non_blank(NULLIF(youtube_profile,'') ORDER BY row_no),
    first_non_blank(NULLIF(instagram_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(blue_sky_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(threads_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(tiktok_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(other_url,'') ORDER BY row_no),
    first_non_blank(NULLIF(mnis_id,'') ORDER BY row_no),
    first_non_blank(NULLIF(twfy_id,'') ORDER BY row_no),
    first_non_blank(NULLIF(image,'') ORDER BY row_no),
    first_non_blank(NULLIF(person_last_updated,'') ORDER BY row_no)::timestamp
FROM staging
WHERE NULLIF(person_id,'') IS NOT NULL
GROUP BY 1
ON CONFLICT (person_id) DO NOTHING;

INSERT INTO post (post_id, post_label, gss, nuts1)
SELECT
    NULLIF(post_id,''),
    first_non_blank(NULLIF(post_label,'') ORDER BY row_no),
    first_non_blank(NULLIF(gss,'') ORDER BY row_no),
    first_non_blank(NULLIF(nuts1,'') ORDER BY row_no)
FROM staging
WHERE NULLIF(post_id,'') IS NOT NULL
GROUP BY 1
ON CONFLICT (post_id) DO NOTHING;

INSERT INTO ballot (ballot_paper_id, election_id, post_id, organisation_name, seats_contested, cancelled_poll, by_election, by_election_reason, party_lists_in_use, candidates_locked, total_electorate, turnout_reported, turnout_percentage, spoilt_ballots, results_source)
SELECT
    NULLIF(ballot_paper_id,''),
    first_non_blank(NULLIF(election_id,'') ORDER BY row_no),
    first_non_blank(NULLIF(post_id,'') ORDER BY row_no),
    first_non_blank(NULLIF(organisation_name,'') ORDER BY row_no),
    first_non_blank(NULLIF(seats_contested,'') ORDER BY row_no)::integer,
    first_non_blank(NULLIF(cancelled_poll,'') ORDER BY row_no)::boolean,
    first_non_blank(NULLIF(by_election,'') ORDER BY row_no)::boolean,
    first_non_blank(NULLIF(by_election_reason,'') ORDER BY row_no),
    first_non_blank(NULLIF(party_lists_in_use,'') ORDER BY row_no)::boolean,
    first_non_blank(NULLIF(candidates_locked,'') ORDER BY row_no)::boolean,
    first_non_blank(NULLIF(total_electorate,'') ORDER BY row_no)::integer,
    first_non_blank(NULLIF(turnout_reported,'') ORDER BY row_no)::integer,
    first_non_blank(NULLIF(turnout_percentage,'') ORDER BY row_no)::numeric,
    first_non_blank(NULLIF(spoilt_ballots,'') ORDER BY row_no)::integer,
    first_non_blank(NULLIF(results_source,'') ORDER BY row_no)
FROM staging
WHERE NULLIF(ballot_paper_id,'') IS NOT NULL
GROUP BY 1
ON CONFLICT (ballot_paper_id) DO NOTHING;

-- The CSV holds exactly one row per (person_id, ballot_paper_id), so there is
-- nothing to merge here; DISTINCT ON keeps that assumption from turning a
-- duplicate into a load failure, and row_no makes which row wins deterministic.
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
ORDER BY person_id, ballot_paper_id, row_no
ON CONFLICT (person_id, ballot_paper_id) DO NOTHING;

UPDATE election e SET kind = k.kind FROM staging_election_kind k
WHERE k.election_id = e.election_id;

DROP TABLE staging_election_kind;

-- The defection edges arrive pre-derived (../derive_defections.py) so that all
-- three loaders hold the identical graph; staging only exists to make a re-run
-- idempotent, since COPY has no ON CONFLICT.
DROP TABLE IF EXISTS staging_defection;
CREATE TABLE staging_defection (
    from_party_id TEXT,
    to_party_id TEXT,
    defectors TEXT
);
COPY staging_defection FROM :'defection_path' WITH (FORMAT csv, HEADER true);

INSERT INTO defection (from_party_id, to_party_id, defectors)
SELECT from_party_id, to_party_id, NULLIF(defectors,'')::integer
FROM staging_defection
ON CONFLICT (from_party_id, to_party_id) DO NOTHING;

DROP TABLE staging_defection;

DROP AGGREGATE first_non_blank(anyelement);
DROP FUNCTION keep_first(anyelement, anyelement);
DROP TABLE staging;
