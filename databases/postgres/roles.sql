-- The benchmark connects as bench_ro: SELECT-only grants are the hard
-- read-only guarantee (the package's default_transaction_read_only session
-- option is defense-in-depth on top — a generated SET can turn the session
-- option off, but grants can't be talked out of by query text).
CREATE ROLE bench_ro LOGIN PASSWORD 'bench_ro';
GRANT SELECT ON ALL TABLES IN SCHEMA public TO bench_ro;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO bench_ro;
