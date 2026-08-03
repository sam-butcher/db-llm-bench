-- The benchmark connects as bench_ro: SELECT-only grants are the hard
-- read-only guarantee, mirroring databases/postgres/roles.sql. MySQL has no
-- session-level read-only equivalent to Postgres's
-- default_transaction_read_only, so here the grant is the only layer.
CREATE USER IF NOT EXISTS 'bench_ro'@'%' IDENTIFIED BY 'bench_ro';
GRANT SELECT ON reactome.* TO 'bench_ro'@'%';
