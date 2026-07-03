//! Benchmark runner: one runner per DB + model + example count + skills
//! setup. Runs each question at the highest configured retry count; lower
//! retry levels are derived from the attempt trace during output marshalling.

use std::time::{Duration, Instant};

use bench_core::{
    Attempt, Database, Message, ModelProvider, ModelResponse, ProviderError, Question,
    RecordResult, ResultRecord, Role, TokenUsage,
};
use thiserror::Error;

/// Repetitions per question/setup cell, to account for LLM non-determinism.
pub const REPETITIONS: u32 = 3;

/// Harness-level retries for transient provider errors (rate limits, network
/// blips). These never count against the model's retry budget.
const TRANSIENT_RETRIES: u32 = 3;
const TRANSIENT_BACKOFF: Duration = Duration::from_millis(100);

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
    /// the model, extract and execute the query, feeding model-fault errors
    /// back until success or the retry budget is spent, then compare against
    /// the expected result.
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

    /// Attempts the question up to `max_retries + 1` times, feeding each
    /// model-fault error (with the prior conversation) back to the model.
    /// UNANSWERABLE and wrong-but-valid results are terminal: only errors
    /// that can't possibly be correct earn a retry.
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
        let mut conversation = vec![Message {
            role: Role::User,
            content: prompt,
        }];
        let mut attempts: Vec<Attempt> = Vec::new();

        loop {
            let started = Instant::now();
            let response = self.send_with_backoff(&conversation).await?;

            // Ok(coerced value), or Err(model-fault message, retryable flag);
            // infrastructure failures abort instead of being recorded.
            let (query, outcome) = match extract_query(&response.text) {
                Extraction::Query(query) => match self.db.send_query(&query).await {
                    Ok(value) => (Some(query), Ok(value)),
                    Err(e) if !e.is_model_fault() => {
                        return Err(RunError::Infrastructure(e.to_string()));
                    }
                    Err(e) => (Some(query), Err((e.to_string(), true))),
                },
                Extraction::Unanswerable => {
                    (None, Err(("declared UNANSWERABLE".to_string(), false)))
                }
                Extraction::Malformed => {
                    (None, Err(("no query found in response".to_string(), true)))
                }
            };
            let latency_ms = started.elapsed().as_millis() as u64;

            match outcome {
                Ok(value) => {
                    attempts.push(Attempt {
                        query,
                        tokens: response.tokens,
                        latency_ms,
                        error: None,
                    });
                    let accurate = value.matches_question(&question.expected, question.ordered);
                    return Ok(self.record(repetition, attempts, RecordResult::Value(value), accurate));
                }
                Err((message, retryable)) => {
                    attempts.push(Attempt {
                        query,
                        tokens: response.tokens,
                        latency_ms,
                        error: Some(message.clone()),
                    });
                    let budget_left = attempts.len() <= self.max_retries as usize;
                    if !(retryable && budget_left) {
                        return Ok(self.record(repetition, attempts, RecordResult::Error, false));
                    }
                    conversation.push(Message {
                        role: Role::Assistant,
                        content: response.text,
                    });
                    conversation.push(Message {
                        role: Role::User,
                        content: format!(
                            "The query failed with the following error:\n\n{message}\n\n\
                             Please respond with a corrected query."
                        ),
                    });
                }
            }
        }
    }

    /// Retry transient provider errors with exponential backoff; fatal
    /// errors and exhausted retries abort the run.
    async fn send_with_backoff(
        &self,
        conversation: &[Message],
    ) -> Result<ModelResponse, RunError> {
        let mut transient_failures = 0;
        loop {
            match self.model.send_prompt(conversation).await {
                Ok(response) => return Ok(response),
                Err(ProviderError::Transient(_)) if transient_failures < TRANSIENT_RETRIES => {
                    tokio::time::sleep(TRANSIENT_BACKOFF * 2u32.pow(transient_failures)).await;
                    transient_failures += 1;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn record(
        &self,
        repetition: u32,
        attempts: Vec<Attempt>,
        result: RecordResult,
        accurate: bool,
    ) -> ResultRecord {
        let mut tokens = TokenUsage::default();
        let mut latency_ms = 0;
        for attempt in &attempts {
            tokens.add(attempt.tokens);
            latency_ms += attempt.latency_ms;
        }
        ResultRecord {
            model: self.model.model_id(),
            max_retries: self.max_retries,
            retries_used: attempts.len().saturating_sub(1) as u32,
            examples: self.examples.len() as u32,
            skills: self.skills.is_some(),
            repetition,
            // The last extractable query, matching derive_retry_level's rule.
            generated: attempts
                .iter()
                .rev()
                .find_map(|a| a.query.clone())
                .unwrap_or_default(),
            attempts,
            tokens,
            latency_ms,
            result,
            accurate,
        }
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
        runner_with_retries(db, provider, 0)
    }

    fn runner_with_retries<'a>(
        db: &'a DummyDb,
        provider: &'a DummyProvider,
        max_retries: u32,
    ) -> BenchmarkRunner<'a> {
        BenchmarkRunner {
            db,
            model: provider,
            prompt_template: "{{schema}}\n{{examples}}\n{{skills}}\nQ: {{question}}".to_string(),
            schema: "cars have ages".to_string(),
            examples: vec![],
            skills: None,
            max_retries,
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

    #[tokio::test]
    async fn retry_recovers_from_model_fault() {
        let db = DummyDb::new();
        // Each repetition consumes both entries: syntax error, then success.
        let provider = DummyProvider::new(["```\nnot json\n```", "```\n3\n```"]).repeating();
        let runs = runner_with_retries(&db, &provider, 2)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        for record in &runs[0].records {
            assert!(record.accurate);
            assert_eq!(record.max_retries, 2);
            assert_eq!(record.retries_used, 1);
            assert_eq!(record.attempts.len(), 2);
            assert!(record.attempts[0].error.as_deref().unwrap().contains("syntax error"));
            assert!(record.attempts[1].error.is_none());
            assert_eq!(record.generated, "3");
            // Totals sum both attempts.
            let expected_output: u64 =
                record.attempts.iter().map(|a| a.tokens.output).sum();
            assert_eq!(record.tokens.output, expected_output);
        }
        // The retry conversation carried the original prompt, the model's
        // response, and the error feedback.
        let retry_conversation = &provider.conversations()[1];
        assert_eq!(retry_conversation.len(), 3);
        assert!(retry_conversation[2].content.contains("syntax error"));
    }

    #[tokio::test]
    async fn budget_exhaustion_is_an_error_record() {
        let db = DummyDb::new();
        let provider = DummyProvider::new(["no code here"]).repeating();
        let runs = runner_with_retries(&db, &provider, 2)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        let record = &runs[0].records[0];
        assert!(!record.accurate);
        assert!(matches!(record.result, RecordResult::Error));
        assert_eq!(record.retries_used, 2);
        assert_eq!(record.attempts.len(), 3);
    }

    #[tokio::test]
    async fn unanswerable_is_never_retried() {
        let db = DummyDb::new();
        let provider = DummyProvider::new(["UNANSWERABLE"]).repeating();
        let runs = runner_with_retries(&db, &provider, 2)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        let record = &runs[0].records[0];
        assert_eq!(record.attempts.len(), 1);
        assert_eq!(record.retries_used, 0);
        assert!(!record.accurate);
    }

    #[tokio::test]
    async fn wrong_but_valid_result_is_never_retried() {
        let db = DummyDb::new();
        let provider = DummyProvider::new(["```\n4\n```"]).repeating();
        let runs = runner_with_retries(&db, &provider, 2)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        let record = &runs[0].records[0];
        assert_eq!(record.attempts.len(), 1);
        assert!(matches!(&record.result, RecordResult::Value(Value::Int(4))));
    }

    #[tokio::test(start_paused = true)]
    async fn transient_provider_errors_are_backed_off_and_retried() {
        let db = DummyDb::new();
        let provider = DummyProvider::new(Vec::<String>::new());
        for _ in 0..REPETITIONS {
            provider.push_transient_error("rate limited");
            provider.push_response("```\n3\n```");
        }
        let runs = runner(&db, &provider)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        assert!(runs[0].records.iter().all(|r| r.accurate));
        // Transient failures don't touch the model's budget or the trace.
        assert!(runs[0].records.iter().all(|r| r.retries_used == 0));
        assert_eq!(
            provider.conversations().len(),
            (REPETITIONS * 2) as usize
        );
    }

    #[tokio::test(start_paused = true)]
    async fn persistent_transient_errors_eventually_abort() {
        let db = DummyDb::new();
        let provider = DummyProvider::new(Vec::<String>::new());
        // One more than the harness's transient budget.
        for _ in 0..=TRANSIENT_RETRIES {
            provider.push_transient_error("rate limited");
        }
        let result = runner(&db, &provider).run(&[question(Value::Int(3))]).await;
        assert!(matches!(
            result,
            Err(RunError::Provider(ProviderError::Transient(_)))
        ));
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
