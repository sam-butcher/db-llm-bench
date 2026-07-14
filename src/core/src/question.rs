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
    /// Deliberately unanswerable against the schema: the only correct
    /// response is the UNANSWERABLE token, so there is no expected value
    /// and there are no ground-truth queries.
    #[serde(default)]
    pub unanswerable: bool,
    /// None only for unanswerable questions (enforced when the question
    /// file is loaded).
    #[serde(default)]
    pub expected: Option<Value>,
    /// Whether the order of a top-level list result is part of correctness
    /// (i.e. the question demands an ordering). Defaults to unordered: rows
    /// compare as a bag. Nested lists always compare ordered, as tuples.
    #[serde(default)]
    pub ordered: bool,
    /// Correct query per language key ("typeql", "sql", "cypher", ...).
    /// Empty only for unanswerable questions.
    #[serde(default)]
    pub queries: BTreeMap<String, String>,
}
