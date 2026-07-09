//! SQL package: Postgres via sqlx ("sql" is the generic language ID; the
//! engine is Postgres). Mutation safety is layered: connect as a
//! SELECT-only role (the hard guarantee — see databases/postgres/roles.sql)
//! with a read-only session default and server-side statement timeout as
//! defense-in-depth, since a generated SET can disable session defaults but
//! cannot escape grants.

use std::time::Duration;

use async_trait::async_trait;
use bench_core::temporal::{canonical_date, canonical_datetime, canonical_datetime_utc};
use bench_core::value::shape_rows;
use bench_core::{Database, QueryError, Value};
use futures::StreamExt;
use rust_decimal::prelude::ToPrimitive;
use serde::Deserialize;
use sqlx::postgres::{PgColumn, PgConnectOptions, PgPool, PgPoolOptions, PgRow};
use sqlx::{Column, Row, TypeInfo};
use tokio::sync::OnceCell;

/// Client-side ceiling, deliberately longer than the server-side
/// statement_timeout so the server cancels first — that path yields a clean
/// SQLSTATE 57014 on a still-healthy connection, instead of the client
/// dropping the stream mid-query.
const QUERY_TIMEOUT: Duration = Duration::from_secs(35);
const STATEMENT_TIMEOUT: &str = "30s";
const MAX_ROWS: usize = 10_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqlAuth {
    pub username: String,
    pub password: String,
}

pub struct Sql {
    options: PgConnectOptions,
    pool: OnceCell<PgPool>,
}

impl Sql {
    /// Validates the URL eagerly so a config typo fails at startup. The URL
    /// may embed credentials and a database (postgres://user:pass@host/db);
    /// explicit `database`/`auth` config overrides them.
    pub fn new(url: &str, database: Option<&str>, auth: Option<&SqlAuth>) -> Result<Self, String> {
        let mut options: PgConnectOptions = url
            .parse()
            .map_err(|e| format!("invalid Postgres URL `{url}`: {e}"))?;
        if let Some(database) = database {
            options = options.database(database);
        }
        if let Some(auth) = auth {
            options = options.username(&auth.username).password(&auth.password);
        }
        options = options.options([
            ("default_transaction_read_only", "on"),
            ("statement_timeout", STATEMENT_TIMEOUT),
        ]);
        Ok(Self {
            options,
            pool: OnceCell::new(),
        })
    }

    /// Connect lazily; the first connection validates host, auth, and
    /// database existence, surfacing config problems as infrastructure.
    async fn pool(&self) -> Result<&PgPool, QueryError> {
        self.pool
            .get_or_try_init(|| async {
                PgPoolOptions::new()
                    .max_connections(2)
                    .connect_with(self.options.clone())
                    .await
            })
            .await
            .map_err(|e| QueryError::Infrastructure(e.to_string()))
    }
}

#[async_trait]
impl Database for Sql {
    fn query_language(&self) -> &'static str {
        "sql"
    }

    async fn send_query(&self, query: &str) -> Result<Value, QueryError> {
        let pool = self.pool().await?;
        tokio::time::timeout(QUERY_TIMEOUT, run_query(pool, query))
            .await
            .map_err(|_| QueryError::Timeout)?
    }
}

async fn run_query(pool: &PgPool, query: &str) -> Result<Value, QueryError> {
    let mut stream = sqlx::query(query).fetch(pool);
    let mut columns: Vec<String> = Vec::new();
    let mut table: Vec<Vec<Value>> = Vec::new();
    while let Some(row) = stream.next().await {
        let row = row.map_err(map_sqlx_error)?;
        if table.len() >= MAX_ROWS {
            return Err(QueryError::WrongShape(format!(
                "result exceeded {MAX_ROWS} rows"
            )));
        }
        if columns.is_empty() {
            columns = row
                .columns()
                .iter()
                .map(|column| column.name().to_string())
                .collect();
        }
        table.push(coerce_row(&row)?);
    }
    Ok(shape_rows(&columns, table))
}

fn coerce_row(row: &PgRow) -> Result<Vec<Value>, QueryError> {
    row.columns()
        .iter()
        .map(|column| coerce_column(row, column))
        .collect()
}

fn coerce_column(row: &PgRow, column: &PgColumn) -> Result<Value, QueryError> {
    let name = column.name();
    let type_name = column.type_info().name();
    let index = column.ordinal();
    let decode_error =
        |e: sqlx::Error| QueryError::WrongShape(format!("column `{name}` ({type_name}): {e}"));

    macro_rules! cell {
        ($t:ty, $map:expr) => {
            row.try_get::<Option<$t>, _>(index)
                .map_err(decode_error)?
                .map($map)
        };
    }

    let value = match type_name {
        "BOOL" => cell!(bool, Value::Bool),
        "INT2" => cell!(i16, |v| Value::Int(v as i64)),
        "INT4" => cell!(i32, |v| Value::Int(v as i64)),
        "INT8" => cell!(i64, Value::Int),
        "FLOAT4" => cell!(f32, |v| Value::Float(v as f64)),
        "FLOAT8" => cell!(f64, Value::Float),
        // Lossy into f64 by design; the canonical Value has no decimal type.
        "NUMERIC" => cell!(rust_decimal::Decimal, |v| v
            .to_f64()
            .map(Value::Float)
            .unwrap_or_else(|| Value::String(v.to_string()))),
        "TEXT" | "VARCHAR" | "BPCHAR" | "CHAR" | "NAME" => cell!(String, Value::String),
        "DATE" => cell!(chrono::NaiveDate, |v| Value::String(canonical_date(v))),
        "TIMESTAMP" => cell!(chrono::NaiveDateTime, |v| Value::String(
            canonical_datetime(v)
        )),
        "TIMESTAMPTZ" => cell!(chrono::DateTime<chrono::Utc>, |v| Value::String(
            canonical_datetime_utc(v)
        )),
        "JSON" | "JSONB" => cell!(serde_json::Value, Value::from),
        other => {
            return Err(QueryError::WrongShape(format!(
                "column `{name}` has unsupported type {other}; \
                 select numbers, text, booleans, dates, or json instead"
            )));
        }
    };
    Ok(value.unwrap_or(Value::Null))
}

fn map_sqlx_error(error: sqlx::Error) -> QueryError {
    match error {
        sqlx::Error::Database(db) => classify_database_error(db.code().as_deref(), db.message()),
        // Decode failures mean the query selected something outside the
        // coercible types — the model can fix that.
        error @ (sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_)) => {
            QueryError::WrongShape(error.to_string())
        }
        error => QueryError::Infrastructure(error.to_string()),
    }
}

/// SQLSTATE class decides fault ownership. The model owns everything its
/// query text can cause: feature-not-supported (0A), cardinality (21), data
/// exceptions (22), constraint violations (23), invalid transaction state
/// incl. read-only violations (25), syntax/access (42), program limits
/// (54), and PL/pgSQL raises (P0). 57014 is the statement timeout;
/// everything else — connection, resource, internal — is infrastructure.
fn classify_database_error(code: Option<&str>, message: &str) -> QueryError {
    let Some(code) = code else {
        return QueryError::Infrastructure(message.to_string());
    };
    if code == "57014" {
        return QueryError::Timeout;
    }
    match code.get(..2) {
        Some("0A") | Some("21") | Some("22") | Some("23") | Some("25") | Some("42")
        | Some("54") | Some("P0") => QueryError::Syntax(message.to_string()),
        _ => QueryError::Infrastructure(message.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlstate_classes_decide_fault_ownership() {
        // Syntax error and undefined table: the model's fault.
        assert!(matches!(
            classify_database_error(Some("42601"), "syntax error at or near"),
            QueryError::Syntax(_)
        ));
        assert!(matches!(
            classify_database_error(Some("42P01"), "relation does not exist"),
            QueryError::Syntax(_)
        ));
        // A write attempt bounces off the read-only connection — also the
        // model's fault, with a message telling it what happened.
        assert!(matches!(
            classify_database_error(Some("25006"), "cannot execute INSERT in a read-only transaction"),
            QueryError::Syntax(m) if m.contains("read-only")
        ));
        // Division by zero is a data exception: model fault.
        assert!(matches!(
            classify_database_error(Some("22012"), "division by zero"),
            QueryError::Syntax(_)
        ));
        // Unsupported features and PL/pgSQL raises are things the query
        // caused — not infrastructure.
        assert!(matches!(
            classify_database_error(Some("0A000"), "feature not supported"),
            QueryError::Syntax(_)
        ));
        assert!(matches!(
            classify_database_error(Some("P0001"), "raise_exception"),
            QueryError::Syntax(_)
        ));
        // The server-side statement timeout.
        assert!(matches!(
            classify_database_error(Some("57014"), "canceling statement"),
            QueryError::Timeout
        ));
        // Connection/resource classes are infrastructure.
        assert!(matches!(
            classify_database_error(Some("08006"), "connection failure"),
            QueryError::Infrastructure(_)
        ));
        assert!(matches!(
            classify_database_error(Some("53300"), "too many connections"),
            QueryError::Infrastructure(_)
        ));
        assert!(matches!(
            classify_database_error(None, "unknown"),
            QueryError::Infrastructure(_)
        ));
    }

    #[test]
    fn invalid_url_fails_eagerly() {
        assert!(Sql::new("not a url", None, None).is_err());
        assert!(Sql::new("postgres://localhost/bench", None, None).is_ok());
    }

    #[tokio::test]
    #[ignore = "requires a running Postgres server (SQL_URL)"]
    async fn queries_a_live_server() {
        let url = std::env::var("SQL_URL")
            .unwrap_or_else(|_| "postgres://bench_ro:bench_ro@localhost/bench".to_string());
        let db = Sql::new(&url, None, None).unwrap();
        let value = db.send_query("SELECT 1 + 1").await.unwrap();
        assert_eq!(value, Value::Int(2));
        // The read-only session default rejects writes as a model fault.
        let error = db
            .send_query("CREATE TABLE nope (id INT)")
            .await
            .unwrap_err();
        assert!(matches!(error, QueryError::Syntax(m) if m.contains("read-only")));
        // Even if a generated SET disables the session default, the
        // SELECT-only role still blocks writes (grants are the hard layer).
        db.send_query("SET default_transaction_read_only = off")
            .await
            .unwrap();
        let error = db
            .send_query("INSERT INTO cars (brand, model, wheels) VALUES ('X', 'Y', 4)")
            .await
            .unwrap_err();
        assert!(matches!(error, QueryError::Syntax(_)));
    }
}
