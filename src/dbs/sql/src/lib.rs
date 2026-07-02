//! SQL package. "sql" is used generically — the specific engine doesn't
//! matter for the benchmark. Queries will go through `sqlx`.

use async_trait::async_trait;
use bench_core::{Database, QueryError, Value};

pub struct Sql {
    pub url: String,
}

impl Sql {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

#[async_trait]
impl Database for Sql {
    fn query_language(&self) -> &'static str {
        "sql"
    }

    async fn send_query(&self, _query: &str) -> Result<Value, QueryError> {
        // TODO: connect via sqlx with a read-only connection and statement
        // timeout, and coerce rows into Value.
        todo!("wire up sqlx")
    }
}
