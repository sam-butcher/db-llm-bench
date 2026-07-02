//! Output marshalling: the result-file structure described in the README,
//! plus derivation of lower retry levels from the attempt trace.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufWriter};
use std::path::Path;

use bench_core::Value;
use serde::{Serialize, Serializer};

#[derive(Debug, Serialize)]
pub struct BenchmarkOutput {
    pub questions: Vec<QuestionOutput>,
}

#[derive(Debug, Serialize)]
pub struct QuestionOutput {
    pub question: String,
    pub difficulty: String,
    pub expected: Value,
    /// Keyed by query language ("typeql", "sql", "cypher", ...).
    #[serde(flatten)]
    pub languages: BTreeMap<String, LanguageOutput>,
}

#[derive(Debug, Serialize)]
pub struct LanguageOutput {
    pub correct: String,
    pub results: Vec<ResultRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRecord {
    pub model: String,
    pub max_retries: u32,
    pub retries_used: u32,
    pub examples: u32,
    pub skills: bool,
    pub repetition: u32,
    pub generated: String,
    pub attempts: Vec<Attempt>,
    pub tokens: u64,
    pub result: RecordResult,
    pub accurate: bool,
}

/// One failed attempt in the retry loop; the final (successful or given-up)
/// query lives in `generated`.
#[derive(Debug, Serialize)]
pub struct Attempt {
    pub query: String,
    pub error: String,
}

#[derive(Debug)]
pub enum RecordResult {
    /// The coerced result of a successfully executed query.
    Value(Value),
    /// The query never produced a usable result; details are in `attempts`.
    Error,
}

impl Serialize for RecordResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            RecordResult::Value(v) => v.serialize(serializer),
            RecordResult::Error => serializer.serialize_str("error"),
        }
    }
}

/// Derive the record for a lower retry level from a run at the highest
/// level: replay the attempt trace as if it had been cut off at
/// `max_retries` retries.
pub fn derive_retry_level(_record: &ResultRecord, _max_retries: u32) -> ResultRecord {
    todo!("cut the attempt trace off at the lower level")
}

pub fn write_output(path: &Path, output: &BenchmarkOutput) -> io::Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(BufWriter::new(file), output)?;
    Ok(())
}
