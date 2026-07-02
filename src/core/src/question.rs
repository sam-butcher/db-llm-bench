use std::collections::BTreeMap;

use serde::Deserialize;

use crate::Value;

#[derive(Debug, Deserialize)]
pub struct QuestionFile {
    pub questions: Vec<Question>,
}

/// `deny_unknown_fields` so a typo'd or unexpected key is a parse error
/// rather than silently ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub question: String,
    pub difficulty: String,
    pub expected: Value,
    /// Correct query per language key ("typeql", "sql", "cypher", ...).
    pub queries: BTreeMap<String, String>,
}
