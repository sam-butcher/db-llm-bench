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
    /// Whether the order of a top-level list result is part of correctness
    /// (i.e. the question demands an ordering). Defaults to unordered: rows
    /// compare as a bag. Nested lists always compare ordered, as tuples.
    #[serde(default)]
    pub ordered: bool,
    /// Correct query per language key ("typeql", "sql", "cypher", ...).
    pub queries: BTreeMap<String, String>,
}
