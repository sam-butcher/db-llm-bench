//! Neo4j package via the community `neo4rs` crate.
//!
//! Mutation safety: neo4rs 0.8 exposes no read-access-mode, so every query
//! runs in an explicit transaction that is ALWAYS rolled back. A generated
//! write query therefore executes but can never change the dataset — note
//! that unlike the SQL/TypeDB packages it does not produce an error, just
//! results that won't match the expected value.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use bench_core::temporal::{canonical_date, canonical_datetime, canonical_datetime_utc};
use bench_core::value::shape_rows;
use bench_core::{Database, QueryError, Value};
use neo4rs::{BoltType, Graph, Neo4jClientErrorKind, Neo4jErrorKind, Txn};
use serde::Deserialize;
use tokio::sync::OnceCell;

/// Pathological queries surface as model-fault timeouts rather than hanging
/// the run (the runner's 120s ceiling stays a last resort).
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ROWS: usize = 10_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Neo4jAuth {
    pub username: String,
    pub password: String,
}

impl Default for Neo4jAuth {
    fn default() -> Self {
        Self {
            username: "neo4j".to_string(),
            password: "password".to_string(),
        }
    }
}

pub struct Neo4j {
    config: neo4rs::Config,
    graph: OnceCell<Graph>,
}

impl Neo4j {
    /// Unlike the SQL/TypeDB packages, neo4rs only parses the URI when
    /// connecting, so a malformed URI surfaces as infrastructure at the
    /// first query rather than at startup.
    pub fn new(
        uri: &str,
        database: Option<&str>,
        auth: Option<&Neo4jAuth>,
    ) -> Result<Self, String> {
        let default_auth = Neo4jAuth::default();
        let auth = auth.unwrap_or(&default_auth);
        let mut builder = neo4rs::ConfigBuilder::default()
            .uri(uri)
            .user(&auth.username)
            .password(&auth.password)
            .max_connections(2);
        if let Some(database) = database {
            builder = builder.db(database);
        }
        let config = builder
            .build()
            .map_err(|e| format!("invalid Neo4j config for `{uri}`: {e}"))?;
        Ok(Self {
            config,
            graph: OnceCell::new(),
        })
    }

    /// Connect lazily so constructing the package can't fail; connection
    /// problems surface as infrastructure at the first query.
    async fn graph(&self) -> Result<&Graph, QueryError> {
        self.graph
            .get_or_try_init(|| async { Graph::connect(self.config.clone()).await })
            .await
            .map_err(|e| QueryError::Infrastructure(e.to_string()))
    }
}

#[async_trait]
impl Database for Neo4j {
    fn query_language(&self) -> &'static str {
        "cypher"
    }

    async fn send_query(&self, query: &str) -> Result<Value, QueryError> {
        let graph = self.graph().await?;
        tokio::time::timeout(QUERY_TIMEOUT, run_query(graph, query))
            .await
            .map_err(|_| QueryError::Timeout)?
    }
}

async fn run_query(graph: &Graph, query: &str) -> Result<Value, QueryError> {
    let mut txn = graph
        .start_txn()
        .await
        .map_err(|e| QueryError::Infrastructure(e.to_string()))?;
    let result = collect_rows(&mut txn, query).await;
    // Always roll back — this is the mutation guard. On a query error the
    // query's own failure is the informative one; the rollback result only
    // matters when the query succeeded.
    let rollback = txn.rollback().await;
    let value = result?;
    rollback.map_err(|e| QueryError::Infrastructure(format!("rollback failed: {e}")))?;
    Ok(value)
}

async fn collect_rows(txn: &mut Txn, query: &str) -> Result<Value, QueryError> {
    let mut stream = txn
        .execute(neo4rs::query(query))
        .await
        .map_err(map_neo4j_error)?;
    let mut columns: Vec<String> = Vec::new();
    let mut table: Vec<Vec<Value>> = Vec::new();
    while let Some(row) = stream
        .next(txn.handle())
        .await
        .map_err(map_neo4j_error)?
    {
        if table.len() >= MAX_ROWS {
            return Err(QueryError::WrongShape(format!(
                "result exceeded {MAX_ROWS} rows"
            )));
        }
        let cells: HashMap<String, BoltType> = row
            .to_strict()
            .map_err(|e| QueryError::WrongShape(format!("undecodable row: {e}")))?;
        if columns.is_empty() {
            columns = cells.keys().cloned().collect();
            // HashMap order is arbitrary; sorted keys keep row shaping
            // deterministic.
            columns.sort();
        }
        let mut row_values = Vec::with_capacity(columns.len());
        for name in &columns {
            let cell = cells.get(name).ok_or_else(|| {
                QueryError::Infrastructure(format!("row missing column `{name}`"))
            })?;
            row_values.push(coerce_bolt(name, cell)?);
        }
        table.push(row_values);
    }
    Ok(shape_rows(&columns, table))
}

/// Scalars, lists, maps, and temporals coerce; graph entities (nodes,
/// relationships, paths) are a wrong shape, and the message tells the model
/// how to fix it.
fn coerce_bolt(name: &str, value: &BoltType) -> Result<Value, QueryError> {
    Ok(match value {
        BoltType::Null(_) => Value::Null,
        BoltType::Boolean(b) => Value::Bool(b.value),
        BoltType::Integer(i) => Value::Int(i.value),
        BoltType::Float(f) => Value::Float(f.value),
        BoltType::String(s) => Value::String(s.value.clone()),
        BoltType::List(list) => Value::List(
            list.value
                .iter()
                .map(|item| coerce_bolt(name, item))
                .collect::<Result<_, _>>()?,
        ),
        BoltType::Map(map) => Value::Object(
            map.value
                .iter()
                .map(|(key, item)| Ok((key.value.clone(), coerce_bolt(name, item)?)))
                .collect::<Result<_, _>>()?,
        ),
        BoltType::Date(date) => {
            let date: chrono::NaiveDate =
                date.try_into().map_err(|_| temporal_error(name, "date"))?;
            Value::String(canonical_date(date))
        }
        BoltType::LocalDateTime(datetime) => {
            let datetime: chrono::NaiveDateTime = datetime
                .try_into()
                .map_err(|_| temporal_error(name, "datetime"))?;
            Value::String(canonical_datetime(datetime))
        }
        BoltType::DateTime(datetime) => {
            let datetime: chrono::DateTime<chrono::FixedOffset> = datetime
                .try_into()
                .map_err(|_| temporal_error(name, "zoned datetime"))?;
            Value::String(canonical_datetime_utc(datetime.with_timezone(&chrono::Utc)))
        }
        other => {
            return Err(QueryError::WrongShape(format!(
                "variable `{name}` is bound to {}, which is not a supported value; \
                 return properties or scalar values instead",
                bolt_kind(other)
            )));
        }
    })
}

fn temporal_error(name: &str, kind: &str) -> QueryError {
    QueryError::WrongShape(format!("variable `{name}`: out-of-range {kind}"))
}

fn bolt_kind(value: &BoltType) -> &'static str {
    match value {
        BoltType::Node(_) => "a node",
        BoltType::Relation(_) | BoltType::UnboundedRelation(_) => "a relationship",
        BoltType::Path(_) => "a path",
        BoltType::Point2D(_) | BoltType::Point3D(_) => "a point",
        BoltType::Bytes(_) => "bytes",
        BoltType::Duration(_) => "a duration",
        BoltType::Time(_) | BoltType::LocalTime(_) => "a time value",
        BoltType::DateTimeZoneId(_) => "a zone-id datetime",
        _ => "an unsupported value",
    }
}

fn map_neo4j_error(error: neo4rs::Error) -> QueryError {
    match &error {
        neo4rs::Error::Neo4j(e) => classify_neo4j(e.kind(), e.code(), &error.to_string()),
        _ => QueryError::Infrastructure(error.to_string()),
    }
}

/// Client-error codes describe the query — the model's fault — except the
/// security/session/protocol kinds, which are the harness's problem.
/// Transient, database, and unknown errors are infrastructure. Server-side
/// timeouts are recognised by code.
fn classify_neo4j(kind: Neo4jErrorKind, code: &str, display: &str) -> QueryError {
    if code.contains("Timeout") || code.contains("TimedOut") {
        return QueryError::Timeout;
    }
    match kind {
        Neo4jErrorKind::Client(client) => match client {
            Neo4jClientErrorKind::Security(_)
            | Neo4jClientErrorKind::SessionExpired
            | Neo4jClientErrorKind::FatalDiscovery
            | Neo4jClientErrorKind::TransactionTerminated
            | Neo4jClientErrorKind::ProtocolViolation => {
                QueryError::Infrastructure(display.to_string())
            }
            _ => QueryError::Syntax(display.to_string()),
        },
        _ => QueryError::Infrastructure(display.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerces_scalars_lists_and_temporals() {
        assert_eq!(
            coerce_bolt("x", &BoltType::from(42i64)).unwrap(),
            Value::Int(42)
        );
        assert_eq!(
            coerce_bolt("x", &BoltType::from(2.5f64)).unwrap(),
            Value::Float(2.5)
        );
        assert_eq!(
            coerce_bolt("x", &BoltType::from("ka")).unwrap(),
            Value::String("ka".to_string())
        );
        assert_eq!(
            coerce_bolt("x", &BoltType::from(vec![1i64, 2i64])).unwrap(),
            Value::List(vec![Value::Int(1), Value::Int(2)])
        );

        let date = chrono::NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        assert_eq!(
            coerce_bolt("x", &BoltType::Date(date.into())).unwrap(),
            Value::String("2024-01-15".to_string())
        );
        let datetime =
            chrono::NaiveDateTime::new(date, chrono::NaiveTime::from_hms_opt(10, 30, 0).unwrap());
        assert_eq!(
            coerce_bolt("x", &BoltType::LocalDateTime(datetime.into())).unwrap(),
            Value::String("2024-01-15T10:30:00.000000000".to_string())
        );
    }

    #[test]
    fn error_kinds_decide_fault_ownership() {
        // Statement errors (syntax, unknown labels) are the model's fault.
        assert!(matches!(
            classify_neo4j(
                Neo4jErrorKind::Client(Neo4jClientErrorKind::Other),
                "Neo.ClientError.Statement.SyntaxError",
                "Invalid input"
            ),
            QueryError::Syntax(_)
        ));
        // Auth failures are infrastructure, not the model's.
        assert!(matches!(
            classify_neo4j(
                Neo4jErrorKind::Client(Neo4jClientErrorKind::Security(
                    neo4rs::Neo4jSecurityErrorKind::Authentication
                )),
                "Neo.ClientError.Security.Unauthorized",
                "unauthorized"
            ),
            QueryError::Infrastructure(_)
        ));
        // Transient server conditions are infrastructure (harness backoff).
        assert!(matches!(
            classify_neo4j(
                Neo4jErrorKind::Transient,
                "Neo.TransientError.General.MemoryPoolOutOfMemoryError",
                "oom"
            ),
            QueryError::Infrastructure(_)
        ));
        // Server-side timeouts map to the model-fault timeout.
        assert!(matches!(
            classify_neo4j(
                Neo4jErrorKind::Client(Neo4jClientErrorKind::Other),
                "Neo.ClientError.Transaction.TransactionTimedOut",
                "timed out"
            ),
            QueryError::Timeout
        ));
    }

    #[test]
    fn config_builds_with_and_without_database() {
        assert!(Neo4j::new("bolt://localhost:7687", None, None).is_ok());
        assert!(Neo4j::new("bolt://localhost:7687", Some("bench"), None).is_ok());
    }

    #[tokio::test]
    #[ignore = "requires a running Neo4j server (NEO4J_URI, NEO4J_PASSWORD)"]
    async fn queries_a_live_server() {
        let uri =
            std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string());
        let auth = Neo4jAuth {
            username: "neo4j".to_string(),
            password: std::env::var("NEO4J_PASSWORD").unwrap_or_else(|_| "password".to_string()),
        };
        let db = Neo4j::new(&uri, None, Some(&auth)).unwrap();
        let value = db.send_query("RETURN 1 + 1 AS result").await.unwrap();
        assert_eq!(value, Value::Int(2));
        // Writes execute but are rolled back: the node must not survive.
        db.send_query("CREATE (n:BenchRollbackProbe)").await.unwrap();
        let count = db
            .send_query("MATCH (n:BenchRollbackProbe) RETURN count(n)")
            .await
            .unwrap();
        assert_eq!(count, Value::Int(0));
    }
}
