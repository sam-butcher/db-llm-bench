//! TypeDB package. Queries will go through the official `typedb-driver` crate.

use async_trait::async_trait;
use bench_core::{Database, QueryError, Value};

pub struct TypeDb {
    pub url: String,
}

impl TypeDb {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

#[async_trait]
impl Database for TypeDb {
    fn query_language(&self) -> &'static str {
        "typeql"
    }

    async fn send_query(&self, _query: &str) -> Result<Value, QueryError> {
        // TODO: connect via typedb-driver, run in a read transaction with a
        // timeout, and coerce concept rows into Value.
        todo!("wire up typedb-driver")
    }
}
