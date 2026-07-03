//! TypeDB package: queries go through the official `typedb-driver`, run in
//! read transactions (generated queries cannot mutate the dataset) with a
//! package-level query timeout and result-size cap.

use std::time::Duration;

use async_trait::async_trait;
use bench_core::value::shape_rows;
use bench_core::{Database, QueryError, Value};
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::OnceCell;
use typedb_driver::answer::QueryAnswer;
use typedb_driver::concept::Concept;
use typedb_driver::concept::value::Value as TypeDbValue;
use typedb_driver::{Addresses, Credentials, DriverOptions, TransactionType, TypeDBDriver};

/// Pathological queries surface as model-fault timeouts rather than hanging
/// the run (the runner's 120s ceiling stays a last resort).
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ROWS: usize = 10_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypeDbAuth {
    pub username: String,
    pub password: String,
}

impl Default for TypeDbAuth {
    fn default() -> Self {
        Self {
            username: "admin".to_string(),
            password: "password".to_string(),
        }
    }
}

pub struct TypeDb {
    address: String,
    database: String,
    auth: TypeDbAuth,
    driver: OnceCell<TypeDBDriver>,
}

impl TypeDb {
    pub fn new(
        address: impl Into<String>,
        database: impl Into<String>,
        auth: TypeDbAuth,
    ) -> Self {
        Self {
            address: address.into(),
            database: database.into(),
            auth,
            driver: OnceCell::new(),
        }
    }

    /// Connect lazily so constructing the package (before any question runs)
    /// can't fail.
    async fn driver(&self) -> Result<&TypeDBDriver, QueryError> {
        self.driver
            .get_or_try_init(|| async {
                TypeDBDriver::new(
                    Addresses::try_from_address_str(&self.address)?,
                    Credentials::new(&self.auth.username, &self.auth.password),
                    DriverOptions::default(),
                )
                .await
            })
            .await
            .map_err(|e| QueryError::Infrastructure(e.to_string()))
    }
}

#[async_trait]
impl Database for TypeDb {
    fn query_language(&self) -> &'static str {
        "typeql"
    }

    async fn send_query(&self, query: &str) -> Result<Value, QueryError> {
        let driver = self.driver().await?;
        let transaction = driver
            .transaction(&self.database, TransactionType::Read)
            .await
            .map_err(|e| QueryError::Infrastructure(e.to_string()))?;
        let answer = tokio::time::timeout(QUERY_TIMEOUT, transaction.query(query))
            .await
            .map_err(|_| QueryError::Timeout)?
            .map_err(map_driver_error)?;
        coerce_answer(answer).await
    }
}

/// Transport-level failures are the harness's problem; anything the server
/// said about the query itself is the model's.
fn map_driver_error(error: typedb_driver::Error) -> QueryError {
    match &error {
        typedb_driver::Error::Connection(_) => QueryError::Infrastructure(error.to_string()),
        _ => QueryError::Syntax(error.to_string()),
    }
}

async fn coerce_answer(answer: QueryAnswer) -> Result<Value, QueryError> {
    match answer {
        QueryAnswer::Ok(_) => Ok(Value::Null),
        QueryAnswer::ConceptRowStream(_, mut stream) => {
            let mut rows = Vec::new();
            while let Some(row) = stream.next().await {
                let row = row.map_err(map_driver_error)?;
                if rows.len() >= MAX_ROWS {
                    return Err(QueryError::WrongShape(format!(
                        "result exceeded {MAX_ROWS} rows"
                    )));
                }
                rows.push(row);
            }
            let columns: Vec<String> = rows
                .first()
                .map(|row| row.get_column_names().to_vec())
                .unwrap_or_default();
            let mut table = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut cells = Vec::with_capacity(columns.len());
                for name in &columns {
                    let concept = row.get(name).map_err(map_driver_error)?;
                    cells.push(coerce_concept(name, concept)?);
                }
                table.push(cells);
            }
            Ok(shape_rows(&columns, table))
        }
        QueryAnswer::ConceptDocumentStream(_, mut stream) => {
            let mut docs = Vec::new();
            while let Some(document) = stream.next().await {
                let document = document.map_err(map_driver_error)?;
                if docs.len() >= MAX_ROWS {
                    return Err(QueryError::WrongShape(format!(
                        "result exceeded {MAX_ROWS} documents"
                    )));
                }
                // The driver's JSON type round-trips through its string form
                // into our canonical Value.
                let json: serde_json::Value =
                    serde_json::from_str(&document.into_json().to_string()).map_err(|e| {
                        QueryError::WrongShape(format!("undecodable document: {e}"))
                    })?;
                docs.push(Value::from(json));
            }
            Ok(if docs.len() == 1 {
                docs.pop().unwrap()
            } else {
                Value::List(docs)
            })
        }
    }
}

/// Only values and attributes are comparable benchmark results; a variable
/// bound to an entity/relation/type is a wrong shape, and the message tells
/// the model how to fix it.
fn coerce_concept(name: &str, concept: Option<&Concept>) -> Result<Value, QueryError> {
    let Some(concept) = concept else {
        return Ok(Value::Null);
    };
    match concept.try_get_value() {
        Some(value) => Ok(coerce_value(value)),
        None => Err(QueryError::WrongShape(format!(
            "variable ${name} is bound to {}, which is not a value; \
             return attributes or values instead",
            concept.get_label()
        ))),
    }
}

fn coerce_value(value: &TypeDbValue) -> Value {
    match value {
        TypeDbValue::Boolean(b) => Value::Bool(*b),
        TypeDbValue::Integer(i) => Value::Int(*i),
        TypeDbValue::Double(d) => Value::Float(*d),
        TypeDbValue::Decimal(d) => d
            .to_string()
            .parse::<f64>()
            .map(Value::Float)
            .unwrap_or_else(|_| Value::String(d.to_string())),
        TypeDbValue::String(s) => Value::String(s.clone()),
        // Temporal and structured values compare as their canonical string
        // forms; questions with these answers author `expected` as strings.
        other => Value::String(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires a running TypeDB server (TYPEDB_ADDRESS, TYPEDB_DATABASE)"]
    async fn connects_to_a_live_server() {
        let address =
            std::env::var("TYPEDB_ADDRESS").unwrap_or_else(|_| "127.0.0.1:1729".to_string());
        let database = std::env::var("TYPEDB_DATABASE").unwrap_or_else(|_| "test".to_string());
        let db = TypeDb::new(address, database, TypeDbAuth::default());
        db.driver().await.expect("driver should connect");
    }
}
