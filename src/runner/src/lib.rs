//! Benchmark runner: one runner per DB + model + example count + skills
//! setup. Runs each question at the highest configured retry count; lower
//! retry levels are derived from the attempt trace during output marshalling.

use std::time::Instant;

use bench_core::{
    Attempt, Database, Message, ModelProvider, ProviderError, Question, RecordResult,
    ResultRecord, Role,
};
use thiserror::Error;

/// Repetitions per question/setup cell, to account for LLM non-determinism.
pub const REPETITIONS: u32 = 3;

/// Literal token the prompt instructs the model to emit, alone on its own
/// line, when it believes the question cannot be answered against the
/// schema. The own-line rule keeps prose that merely mentions the token
/// (e.g. "I shouldn't say UNANSWERABLE here") from reading as a decline.
pub const UNANSWERABLE_TOKEN: &str = "UNANSWERABLE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extraction {
    Query(String),
    /// Terminal — never retried. Scored as a failure for answerable
    /// questions (and as correct if deliberately-unanswerable questions are
    /// added later).
    Unanswerable,
    /// Neither a code block nor the UNANSWERABLE marker. Retryable, since
    /// declining has an explicit channel — retrying doesn't pressure the
    /// model into hallucinating a query.
    Malformed,
}

/// Extract the query from a model response: the last fenced code block.
pub fn extract_query(response: &str) -> Extraction {
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in response.lines() {
        if line.trim_start().starts_with("```") {
            match current.take() {
                Some(lines) => blocks.push(lines.join("\n")),
                None => current = Some(Vec::new()),
            }
        } else if let Some(lines) = current.as_mut() {
            lines.push(line);
        }
    }
    if let Some(block) = blocks.pop() {
        let query = block.trim();
        if !query.is_empty() {
            return Extraction::Query(query.to_string());
        }
    }
    if response
        .lines()
        .any(|line| line.trim() == UNANSWERABLE_TOKEN)
    {
        Extraction::Unanswerable
    } else {
        Extraction::Malformed
    }
}

/// Fill the prompt template's slots. `skills` is empty when this setup runs
/// with skills off.
pub fn assemble_prompt(
    template: &str,
    question: &str,
    schema: &str,
    examples: &[String],
    skills: &[String],
) -> String {
    template
        .replace("{{question}}", question)
        .replace("{{schema}}", schema)
        .replace("{{examples}}", &examples.join("\n\n"))
        .replace("{{skills}}", &skills.join("\n\n"))
}

pub struct BenchmarkRunner<'a> {
    pub db: &'a dyn Database,
    pub model: &'a dyn ModelProvider,
    pub prompt_template: String,
    pub schema: String,
    /// Examples inserted into the prompt, already truncated to this setup's
    /// example count.
    pub examples: Vec<String>,
    /// Skills content when this setup runs with skills enabled.
    pub skills: Option<Vec<String>>,
    /// The highest configured retry count — the only level actually run.
    pub max_retries: u32,
}

pub struct QuestionRun {
    pub question_index: usize,
    pub records: Vec<ResultRecord>,
}

/// Errors that abort the run — never the model's fault, so nothing here is
/// recorded as a result. Harness-level backoff for transient cases comes
/// with the retry work.
#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("infrastructure error: {0}")]
    Infrastructure(String),
}

impl BenchmarkRunner<'_> {
    /// For each question, REPETITIONS times: assemble the prompt, send it to
    /// the model, extract and execute the query, and compare against the
    /// expected result.
    pub async fn run(&self, questions: &[Question]) -> Result<Vec<QuestionRun>, RunError> {
        let mut runs = Vec::with_capacity(questions.len());
        for (question_index, question) in questions.iter().enumerate() {
            let mut records = Vec::with_capacity(REPETITIONS as usize);
            for repetition in 1..=REPETITIONS {
                records.push(self.run_once(question, repetition).await?);
            }
            runs.push(QuestionRun {
                question_index,
                records,
            });
        }
        Ok(runs)
    }

    /// A single attempt per repetition; the retry loop (feeding model-fault
    /// errors back into the conversation) slots in here later.
    async fn run_once(
        &self,
        question: &Question,
        repetition: u32,
    ) -> Result<ResultRecord, RunError> {
        let prompt = assemble_prompt(
            &self.prompt_template,
            &question.question,
            &self.schema,
            &self.examples,
            self.skills.as_deref().unwrap_or_default(),
        );
        let conversation = vec![Message {
            role: Role::User,
            content: prompt,
        }];

        let started = Instant::now();
        let response = self.model.send_prompt(&conversation).await?;

        // Ok(coerced value) or Err(model-fault message fit for the record);
        // infrastructure failures abort instead of being recorded.
        let (query, outcome) = match extract_query(&response.text) {
            Extraction::Query(query) => match self.db.send_query(&query).await {
                Ok(value) => (Some(query), Ok(value)),
                Err(e) if !e.is_model_fault() => {
                    return Err(RunError::Infrastructure(e.to_string()));
                }
                Err(e) => (Some(query), Err(e.to_string())),
            },
            Extraction::Unanswerable => (None, Err("declared UNANSWERABLE".to_string())),
            Extraction::Malformed => (None, Err("no query found in response".to_string())),
        };
        let latency_ms = started.elapsed().as_millis() as u64;

        let (result, accurate, error) = match outcome {
            Ok(value) => {
                let accurate = value.matches_question(&question.expected, question.ordered);
                (RecordResult::Value(value), accurate, None)
            }
            Err(message) => (RecordResult::Error, false, Some(message)),
        };
        Ok(ResultRecord {
            model: self.model.model_id(),
            max_retries: 0,
            retries_used: 0,
            examples: self.examples.len() as u32,
            skills: self.skills.is_some(),
            repetition,
            generated: query.clone().unwrap_or_default(),
            attempts: vec![Attempt {
                query,
                tokens: response.tokens,
                latency_ms,
                error,
            }],
            tokens: response.tokens,
            latency_ms,
            result,
            accurate,
        })
    }
}

#[cfg(test)]
mod run_tests {
    use std::collections::BTreeMap;

    use bench_core::{QueryError, Value};
    use db_dummy::DummyDb;
    use provider_dummy::DummyProvider;

    use super::*;

    fn question(expected: Value) -> Question {
        Question {
            question: "How many cars are there?".to_string(),
            difficulty: "easy".to_string(),
            expected,
            ordered: false,
            queries: BTreeMap::new(),
        }
    }

    fn runner<'a>(db: &'a DummyDb, provider: &'a DummyProvider) -> BenchmarkRunner<'a> {
        BenchmarkRunner {
            db,
            model: provider,
            prompt_template: "{{schema}}\n{{examples}}\n{{skills}}\nQ: {{question}}".to_string(),
            schema: "cars have ages".to_string(),
            examples: vec![],
            skills: None,
            max_retries: 0,
        }
    }

    /// The dummy provider errors when its script runs out, so every test
    /// scripts one response per repetition.
    fn per_repetition(response: &str) -> DummyProvider {
        DummyProvider::new(vec![response; REPETITIONS as usize])
    }

    #[tokio::test]
    async fn accurate_when_result_matches_expected() {
        let db = DummyDb::new();
        let provider = per_repetition("The count:\n```\n3\n```");
        let runs = runner(&db, &provider)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        assert_eq!(runs.len(), 1);
        let records = &runs[0].records;
        assert_eq!(records.len(), REPETITIONS as usize);
        assert!(records.iter().all(|r| r.accurate));
        assert!(records.iter().all(|r| r.attempts[0].error.is_none()));
        assert_eq!(records[0].generated, "3");
        assert_eq!(records[0].model, "dummy");
        assert!(records[0].tokens.output > 0);
        assert_eq!(records[0].repetition, 1);
        assert_eq!(records[2].repetition, 3);
        // The assembled prompt carried the question and schema.
        let first_prompt = &provider.conversations()[0][0].content;
        assert!(first_prompt.contains("How many cars are there?"));
        assert!(first_prompt.contains("cars have ages"));
    }

    #[tokio::test]
    async fn wrong_result_is_recorded_but_inaccurate() {
        let db = DummyDb::new();
        let provider = per_repetition("```\n4\n```");
        let runs = runner(&db, &provider)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        let record = &runs[0].records[0];
        assert!(!record.accurate);
        // A wrong-but-valid result is not an error: the query ran fine.
        assert!(record.attempts[0].error.is_none());
        assert!(matches!(&record.result, RecordResult::Value(Value::Int(4))));
    }

    #[tokio::test]
    async fn model_fault_query_error_is_recorded() {
        let db = DummyDb::new();
        let provider = per_repetition("```\nnot valid json\n```");
        let runs = runner(&db, &provider)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        let record = &runs[0].records[0];
        assert!(!record.accurate);
        assert!(matches!(record.result, RecordResult::Error));
        let error = record.attempts[0].error.as_deref().unwrap();
        assert!(error.contains("syntax error"), "got: {error}");
        assert_eq!(record.generated, "not valid json");
    }

    #[tokio::test]
    async fn malformed_response_is_recorded_without_a_query() {
        let db = DummyDb::new();
        let provider = per_repetition("I think the answer is 3.");
        let runs = runner(&db, &provider)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        let record = &runs[0].records[0];
        assert!(!record.accurate);
        assert!(record.attempts[0].query.is_none());
        assert_eq!(record.generated, "");
        assert_eq!(
            record.attempts[0].error.as_deref(),
            Some("no query found in response")
        );
    }

    #[tokio::test]
    async fn unanswerable_is_scored_as_failure() {
        let db = DummyDb::new();
        let provider = per_repetition("UNANSWERABLE");
        let runs = runner(&db, &provider)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        let record = &runs[0].records[0];
        assert!(!record.accurate);
        assert_eq!(
            record.attempts[0].error.as_deref(),
            Some("declared UNANSWERABLE")
        );
    }

    #[tokio::test]
    async fn infrastructure_error_aborts_the_run() {
        let db = DummyDb::new();
        db.script(Err(QueryError::Infrastructure("db unreachable".to_string())));
        let provider = per_repetition("```\n3\n```");
        let result = runner(&db, &provider).run(&[question(Value::Int(3))]).await;
        assert!(matches!(result, Err(RunError::Infrastructure(_))));
    }

    #[tokio::test]
    async fn provider_error_aborts_the_run() {
        let db = DummyDb::new();
        let provider = DummyProvider::new(Vec::<String>::new());
        let result = runner(&db, &provider).run(&[question(Value::Int(3))]).await;
        assert!(matches!(result, Err(RunError::Provider(_))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_last_fenced_block() {
        let response = "First try:\n```sql\nSELECT 1;\n```\nActually:\n```sql\nSELECT 2;\n```\n";
        assert_eq!(
            extract_query(response),
            Extraction::Query("SELECT 2;".to_string())
        );
    }

    #[test]
    fn unanswerable_token_on_its_own_line_is_unanswerable() {
        assert_eq!(
            extract_query("UNANSWERABLE\nThe schema has no such attribute."),
            Extraction::Unanswerable
        );
    }

    #[test]
    fn token_mentioned_in_prose_is_malformed() {
        assert_eq!(
            extract_query("I shouldn't say UNANSWERABLE here, but there is no query."),
            Extraction::Malformed
        );
    }

    #[test]
    fn no_block_and_no_token_is_malformed() {
        assert_eq!(
            extract_query("The query you want is SELECT 1;"),
            Extraction::Malformed
        );
    }
}
