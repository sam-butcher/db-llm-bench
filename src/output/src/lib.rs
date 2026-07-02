//! Output marshalling: writing the result file, and deriving lower
//! retry-level records from a run at the highest level.

use std::fs::File;
use std::io::{self, BufWriter};
use std::path::Path;

use bench_core::{Attempt, BenchmarkOutput, RecordResult, ResultRecord, TokenUsage};

/// Derive the record for a lower retry level from a run at the highest
/// level, by replaying the attempt trace as if it had been cut off after
/// `max_retries` retries (i.e. `max_retries + 1` attempts).
pub fn derive_retry_level(record: &ResultRecord, max_retries: u32) -> ResultRecord {
    let attempts: Vec<Attempt> = record
        .attempts
        .iter()
        .take(max_retries as usize + 1)
        .cloned()
        .collect();
    // Only the last attempt of a run can succeed, so success survives the
    // cut iff it happened within the kept attempts.
    let succeeded = attempts.iter().any(|a| a.error.is_none());
    let mut tokens = TokenUsage::default();
    let mut latency_ms = 0;
    for attempt in &attempts {
        tokens.add(attempt.tokens);
        latency_ms += attempt.latency_ms;
    }
    ResultRecord {
        model: record.model.clone(),
        max_retries,
        retries_used: attempts.len().saturating_sub(1) as u32,
        examples: record.examples,
        skills: record.skills,
        repetition: record.repetition,
        generated: attempts
            .iter()
            .rev()
            .find_map(|a| a.query.clone())
            .unwrap_or_default(),
        tokens,
        latency_ms,
        result: if succeeded {
            record.result.clone()
        } else {
            RecordResult::Error
        },
        accurate: succeeded && record.accurate,
        attempts,
    }
}

pub fn write_output(path: &Path, output: &BenchmarkOutput) -> io::Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(BufWriter::new(file), output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bench_core::Value;

    fn attempt(query: &str, error: Option<&str>) -> Attempt {
        Attempt {
            query: Some(query.to_string()),
            tokens: TokenUsage {
                input: 100,
                output: 10,
            },
            latency_ms: 50,
            error: error.map(String::from),
        }
    }

    /// A run at maxRetries=4 that failed twice then succeeded.
    fn full_record() -> ResultRecord {
        ResultRecord {
            model: "dummy".to_string(),
            max_retries: 4,
            retries_used: 2,
            examples: 0,
            skills: false,
            repetition: 1,
            generated: "third".to_string(),
            attempts: vec![
                attempt("first", Some("syntax error")),
                attempt("second", Some("syntax error")),
                attempt("third", None),
            ],
            tokens: TokenUsage {
                input: 300,
                output: 30,
            },
            latency_ms: 150,
            result: RecordResult::Value(Value::Int(3)),
            accurate: true,
        }
    }

    #[test]
    fn cutting_before_success_becomes_an_error() {
        let derived = derive_retry_level(&full_record(), 0);
        assert_eq!(derived.max_retries, 0);
        assert_eq!(derived.retries_used, 0);
        assert_eq!(derived.attempts.len(), 1);
        assert_eq!(derived.generated, "first");
        assert_eq!(derived.tokens.input, 100);
        assert_eq!(derived.latency_ms, 50);
        assert!(matches!(derived.result, RecordResult::Error));
        assert!(!derived.accurate);
    }

    #[test]
    fn cutting_after_success_keeps_the_outcome() {
        let derived = derive_retry_level(&full_record(), 2);
        assert_eq!(derived.retries_used, 2);
        assert_eq!(derived.attempts.len(), 3);
        assert_eq!(derived.generated, "third");
        assert_eq!(derived.tokens.input, 300);
        assert!(matches!(derived.result, RecordResult::Value(_)));
        assert!(derived.accurate);
    }
}
