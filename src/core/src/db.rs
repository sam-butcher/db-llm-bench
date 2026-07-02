use async_trait::async_trait;
use thiserror::Error;

use crate::Value;

/// Anything that can't possibly be a correct answer: wrong result shape,
/// syntax error, timeout. Empty results are NOT errors — they come back as a
/// normal [`Value`]. All variants are retryable; the display message is what
/// gets returned to the LLM for iteration.
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("syntax error: {0}")]
    Syntax(String),
    #[error("query timed out")]
    Timeout,
    #[error("result had the wrong shape: {0}")]
    WrongShape(String),
    #[error("connection error: {0}")]
    Connection(String),
}

/// Unified interface implemented by each DB package.
///
/// Implementations are responsible for query safety (read-only transactions
/// and query timeouts where viable) and for coercing driver-native results
/// into the canonical [`Value`].
#[async_trait]
pub trait Database: Send + Sync {
    /// The query language this DB is benchmarked under, matching the
    /// per-language keys in the questions file (e.g. "typeql", "cypher", "sql").
    fn query_language(&self) -> &'static str;

    async fn send_query(&self, query: &str) -> Result<Value, QueryError>;
}
