//! Neo4j package. Queries will go through the community `neo4rs` crate.

use async_trait::async_trait;
use bench_core::{Database, QueryError, Value};

pub struct Neo4j {
    pub url: String,
}

impl Neo4j {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

#[async_trait]
impl Database for Neo4j {
    fn query_language(&self) -> &'static str {
        "cypher"
    }

    async fn send_query(&self, _query: &str) -> Result<Value, QueryError> {
        // TODO: connect via neo4rs, run in a read transaction with a timeout,
        // and coerce Bolt records into Value.
        todo!("wire up neo4rs")
    }
}
