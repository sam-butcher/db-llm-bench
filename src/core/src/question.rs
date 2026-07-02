use std::collections::BTreeMap;

use serde::Deserialize;

use crate::Value;

#[derive(Debug, Deserialize)]
pub struct QuestionFile {
    pub questions: Vec<Question>,
}

#[derive(Debug, Deserialize)]
pub struct Question {
    pub question: String,
    pub difficulty: String,
    pub expected: Value,
    /// Correct query per language key ("typeql", "sql", "cypher", ...).
    #[serde(flatten)]
    pub correct: BTreeMap<String, String>,
}
