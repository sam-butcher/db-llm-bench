//! Benchmark runner: one runner per DB + model + example count + skills
//! setup. Runs each question at the highest configured retry count; lower
//! retry levels are derived from the attempt trace during output marshalling.

use std::time::{Duration, Instant};

use bench_core::{
    Attempt, Database, Message, ModelProvider, ModelResponse, ProviderError, QueryError, Question,
    RecordResult, ResultRecord, TokenUsage, Value, attempt_totals,
};
use thiserror::Error;

/// Repetitions per question/setup cell, to account for LLM non-determinism.
pub const REPETITIONS: u32 = 1;

/// Harness-level retries for transient provider errors (rate limits, network
/// blips). These never count against the model's retry budget. Doubling from
/// a 1s base gives ~31s of total patience — enough to ride out a real 429.
pub const TRANSIENT_RETRIES: u32 = 5;
const TRANSIENT_BACKOFF: Duration = Duration::from_secs(1);

/// Harness-level retries for DB infrastructure errors (dropped connections,
/// restarts) — the DB-side mirror of the transient provider policy.
pub const INFRA_RETRIES: u32 = 3;
const INFRA_BACKOFF: Duration = Duration::from_millis(500);

/// Defensive ceilings only: packages own the real timeouts. These convert a
/// hung backend into a diagnosable infrastructure failure instead of a
/// frozen run.
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(600);
const DB_TIMEOUT: Duration = Duration::from_secs(120);

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
    // An unterminated trailing fence still counts: a truncated response
    // yields its partial query (and a real execution error to iterate on)
    // rather than "no query found".
    if let Some(lines) = current {
        blocks.push(lines.join("\n"));
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
/// with skills off. The examples slot brings its own "Examples:" heading so
/// that a zero-example prompt doesn't contain a heading with nothing under
/// it.
pub fn assemble_prompt(
    template: &str,
    question: &str,
    schema: &str,
    examples: &[String],
    skills: &[String],
) -> String {
    let examples_section = if examples.is_empty() {
        String::new()
    } else {
        format!("Examples:\n{}", examples.join("\n\n"))
    };
    template
        .replace("{{question}}", question)
        .replace("{{schema}}", schema)
        .replace("{{examples}}", &examples_section)
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

#[derive(Debug)]
pub struct QuestionRun {
    pub question_index: usize,
    pub records: Vec<ResultRecord>,
}

/// Errors that abort the run — never the model's fault, so nothing here is
/// recorded as a result. Both transports get harness-level backoff first;
/// reaching this type means the backoff budget was spent (or the error was
/// fatal).
#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("infrastructure error: {0}")]
    Infrastructure(String),
}

/// An aborted run, carrying everything completed before the failure so the
/// tokens already spent aren't discarded.
#[derive(Debug, Error)]
#[error("question {question_index} repetition {repetition}: {error}")]
pub struct RunFailure {
    pub error: RunError,
    pub question_index: usize,
    pub repetition: u32,
    /// Fully completed question runs, plus a partial run for the failing
    /// question if any of its repetitions finished.
    pub completed: Vec<QuestionRun>,
}

/// A model-fault outcome: the error message that goes in the attempt trace,
/// tagged by whether the model gets another try.
enum Fault {
    Retryable(String),
    Terminal(String),
}

impl Fault {
    fn message(&self) -> &str {
        match self {
            Fault::Retryable(message) | Fault::Terminal(message) => message,
        }
    }
}

/// Everything one prompt-extract-execute round trip produced.
struct AttemptOutcome {
    query: Option<String>,
    tokens: TokenUsage,
    latency_ms: u64,
    response_text: String,
    outcome: Result<Value, Fault>,
}

/// Feedback for a retryable fault: honest about whether a query ran and
/// failed, or no query could be extracted at all.
fn retry_feedback(query_ran: bool, message: &str) -> String {
    if query_ran {
        format!(
            "The query failed with the following error:\n\n{message}\n\n\
             Please respond with a corrected query."
        )
    } else {
        "Your response did not contain a fenced code block. Respond with \
         exactly one fenced code block containing only the query, or the \
         literal token UNANSWERABLE alone on its own line."
            .to_string()
    }
}

impl BenchmarkRunner<'_> {
    /// For each question, REPETITIONS times: assemble the prompt, send it to
    /// the model, extract and execute the query, feeding model-fault errors
    /// back until success or the retry budget is spent, then compare against
    /// the expected result.
    pub async fn run(&self, questions: &[Question]) -> Result<Vec<QuestionRun>, RunFailure> {
        let mut runs = Vec::with_capacity(questions.len());
        for (question_index, question) in questions.iter().enumerate() {
            let mut records = Vec::with_capacity(REPETITIONS as usize);
            for repetition in 1..=REPETITIONS {
                match self.run_once(question, repetition).await {
                    Ok(record) => records.push(record),
                    Err(error) => {
                        let mut completed = runs;
                        if !records.is_empty() {
                            completed.push(QuestionRun {
                                question_index,
                                records,
                            });
                        }
                        return Err(RunFailure {
                            error,
                            question_index,
                            repetition,
                            completed,
                        });
                    }
                }
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
        let mut conversation = vec![Message::user(self.assemble(question))];
        let mut attempts: Vec<Attempt> = Vec::new();

        loop {
            let AttemptOutcome {
                query,
                tokens,
                latency_ms,
                response_text,
                outcome,
            } = self.attempt(&conversation).await?;
            attempts.push(Attempt {
                query: query.clone(),
                tokens,
                latency_ms,
                error: outcome
                    .as_ref()
                    .err()
                    .map(|fault| fault.message().to_string()),
            });

            match outcome {
                Ok(value) => {
                    let accurate = value.matches_question(&question.expected, question.ordered);
                    return Ok(self.build_record(
                        repetition,
                        attempts,
                        RecordResult::Value(value),
                        accurate,
                    ));
                }
                Err(Fault::Terminal(_)) => {
                    return Ok(self.build_record(repetition, attempts, RecordResult::Error, false));
                }
                Err(Fault::Retryable(message)) => {
                    if attempts.len() > self.max_retries as usize {
                        return Ok(self.build_record(
                            repetition,
                            attempts,
                            RecordResult::Error,
                            false,
                        ));
                    }
                    conversation.push(Message::assistant(response_text));
                    conversation.push(Message::user(retry_feedback(query.is_some(), &message)));
                }
            }
        }
    }

    fn assemble(&self, question: &Question) -> String {
        assemble_prompt(
            &self.prompt_template,
            &question.question,
            &self.schema,
            &self.examples,
            self.skills.as_deref().unwrap_or_default(),
        )
    }

    /// One prompt-extract-execute round trip; never touches retry
    /// bookkeeping.
    async fn attempt(&self, conversation: &[Message]) -> Result<AttemptOutcome, RunError> {
        let (response, provider_latency_ms) = self.send_with_backoff(conversation).await?;
        let (query, db_latency_ms, outcome) = match extract_query(&response.text) {
            Extraction::Query(query) => {
                let (outcome, db_latency_ms) = self.query_with_backoff(&query).await?;
                (Some(query), db_latency_ms, outcome)
            }
            Extraction::Unanswerable => (
                None,
                0,
                Err(Fault::Terminal("declared UNANSWERABLE".to_string())),
            ),
            // An abnormal stop (refusal, truncation) makes the trace say
            // why there was no query, not just that there wasn't one.
            Extraction::Malformed => {
                let message = match &response.stop {
                    Some(reason) => format!("no query found in response (stop reason: {reason})"),
                    None => "no query found in response".to_string(),
                };
                (None, 0, Err(Fault::Retryable(message)))
            }
        };
        Ok(AttemptOutcome {
            query,
            tokens: response.tokens,
            // Model+DB work only: harness backoff sleeps and failed
            // transport calls are infra noise and excluded.
            latency_ms: provider_latency_ms + db_latency_ms,
            response_text: response.text,
            outcome,
        })
    }

    /// Retry transient provider errors with exponential backoff; fatal
    /// errors and exhausted retries abort the run. Returns the response and
    /// the latency of the successful call only.
    async fn send_with_backoff(
        &self,
        conversation: &[Message],
    ) -> Result<(ModelResponse, u64), RunError> {
        let mut transient_failures = 0;
        loop {
            let started = Instant::now();
            let outcome =
                match tokio::time::timeout(PROVIDER_TIMEOUT, self.model.send_prompt(conversation))
                    .await
                {
                    Ok(outcome) => outcome,
                    Err(_) => Err(ProviderError::Transient(format!(
                        "provider exceeded the harness ceiling of {PROVIDER_TIMEOUT:?}"
                    ))),
                };
            let latency_ms = started.elapsed().as_millis() as u64;
            match outcome {
                Ok(response) => return Ok((response, latency_ms)),
                Err(ProviderError::Transient(_)) if transient_failures < TRANSIENT_RETRIES => {
                    tokio::time::sleep(TRANSIENT_BACKOFF * 2u32.pow(transient_failures)).await;
                    transient_failures += 1;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Execute the query, retrying infrastructure errors with exponential
    /// backoff — the DB-side mirror of `send_with_backoff`. Model-fault
    /// errors return immediately as retryable faults for the model. The
    /// latency covers the final (non-infra) call only.
    async fn query_with_backoff(
        &self,
        query: &str,
    ) -> Result<(Result<Value, Fault>, u64), RunError> {
        let mut infra_failures = 0;
        loop {
            let started = Instant::now();
            let outcome = match tokio::time::timeout(DB_TIMEOUT, self.db.send_query(query)).await {
                Ok(outcome) => outcome,
                Err(_) => Err(QueryError::Infrastructure(format!(
                    "query exceeded the harness ceiling of {DB_TIMEOUT:?}"
                ))),
            };
            let latency_ms = started.elapsed().as_millis() as u64;
            match outcome {
                Ok(value) => return Ok((Ok(value), latency_ms)),
                Err(e) if e.is_model_fault() => {
                    return Ok((Err(Fault::Retryable(e.to_string())), latency_ms));
                }
                Err(_) if infra_failures < INFRA_RETRIES => {
                    tokio::time::sleep(INFRA_BACKOFF * 2u32.pow(infra_failures)).await;
                    infra_failures += 1;
                }
                Err(e) => return Err(RunError::Infrastructure(e.to_string())),
            }
        }
    }

    fn build_record(
        &self,
        repetition: u32,
        attempts: Vec<Attempt>,
        result: RecordResult,
        accurate: bool,
    ) -> ResultRecord {
        let (tokens, latency_ms) = attempt_totals(&attempts);
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
        for (index, record) in records.iter().enumerate() {
            assert_eq!(record.repetition, index as u32 + 1);
        }
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

    #[tokio::test(start_paused = true)]
    async fn infrastructure_error_recovers_with_backoff() {
        let db = DummyDb::new();
        // One dropped connection, then the JSON-echo fallback succeeds.
        db.script(Err(QueryError::Infrastructure(
            "connection reset".to_string(),
        )));
        let provider = per_repetition("```\n3\n```");
        let runs = runner(&db, &provider)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();

        assert!(runs[0].records.iter().all(|r| r.accurate));
        // Infra retries are invisible to the trace and the model's budget.
        assert!(runs[0].records.iter().all(|r| r.attempts.len() == 1));
    }

    #[tokio::test(start_paused = true)]
    async fn persistent_infrastructure_errors_abort_with_partials() {
        // Question 1 succeeds for all repetitions; question 2 hits infra
        // errors beyond the harness budget on its first repetition.
        let db = DummyDb::new();
        for _ in 0..REPETITIONS {
            db.script(Ok(Value::Int(3)));
        }
        for _ in 0..=INFRA_RETRIES {
            db.script(Err(QueryError::Infrastructure(
                "db unreachable".to_string(),
            )));
        }
        let provider = DummyProvider::new(["```\n3\n```"]).repeating();
        let failure = runner(&db, &provider)
            .run(&[question(Value::Int(3)), question(Value::Int(3))])
            .await
            .unwrap_err();

        assert!(matches!(failure.error, RunError::Infrastructure(_)));
        assert_eq!(failure.question_index, 1);
        assert_eq!(failure.repetition, 1);
        // The completed question survives the abort.
        assert_eq!(failure.completed.len(), 1);
        assert_eq!(failure.completed[0].records.len(), REPETITIONS as usize);
        assert!(failure.completed[0].records.iter().all(|r| r.accurate));
    }

    #[tokio::test]
    async fn provider_error_aborts_the_run() {
        let db = DummyDb::new();
        let provider = DummyProvider::new(Vec::<String>::new());
        let failure = runner(&db, &provider)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap_err();
        assert!(matches!(failure.error, RunError::Provider(_)));
        assert!(failure.completed.is_empty());
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
            assert!(
                record.attempts[0]
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("syntax error")
            );
            assert!(record.attempts[1].error.is_none());
            assert_eq!(record.generated, "3");
            // Totals sum both attempts.
            let expected_output: u64 = record.attempts.iter().map(|a| a.tokens.output).sum();
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
        // Malformed responses get formatting feedback, not a bogus "query
        // failed" message.
        let feedback = &provider.conversations()[1][2].content;
        assert!(
            feedback.contains("did not contain a fenced code block"),
            "got: {feedback}"
        );
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
        assert_eq!(provider.conversations().len(), (REPETITIONS * 2) as usize);
    }

    #[tokio::test(start_paused = true)]
    async fn persistent_transient_errors_eventually_abort() {
        let db = DummyDb::new();
        let provider = DummyProvider::new(Vec::<String>::new());
        // One more than the harness's transient budget.
        for _ in 0..=TRANSIENT_RETRIES {
            provider.push_transient_error("rate limited");
        }
        let failure = runner(&db, &provider)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap_err();
        assert!(matches!(
            failure.error,
            RunError::Provider(ProviderError::Transient(_))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn hung_db_becomes_a_diagnosable_infrastructure_failure() {
        struct HangingDb;

        #[async_trait::async_trait]
        impl Database for HangingDb {
            fn query_language(&self) -> &'static str {
                "dummy"
            }

            async fn send_query(&self, _query: &str) -> Result<Value, QueryError> {
                std::future::pending().await
            }
        }

        let db = HangingDb;
        let provider = per_repetition("```\n3\n```");
        let failure = BenchmarkRunner {
            db: &db,
            model: &provider,
            prompt_template: "{{question}}".to_string(),
            schema: String::new(),
            examples: vec![],
            skills: None,
            max_retries: 0,
        }
        .run(&[question(Value::Int(3))])
        .await
        .unwrap_err();

        match failure.error {
            RunError::Infrastructure(message) => {
                assert!(message.contains("harness ceiling"), "got: {message}")
            }
            other => panic!("expected infrastructure error, got {other:?}"),
        }
    }

    /// The core claim of the run-at-max/derive-lower design: deriving a
    /// lower level from a max-level trace produces the same record an
    /// actual run at that level would (latency aside — wall-clock isn't
    /// reproducible).
    #[tokio::test]
    async fn derived_levels_match_actual_low_budget_runs() {
        let script = ["no code here", "```\nnot json\n```", "```\n3\n```"];

        let mut actual_by_level = Vec::new();
        for max_retries in [0, 2] {
            let db = DummyDb::new();
            let provider = DummyProvider::new(script).repeating();
            let runs = runner_with_retries(&db, &provider, max_retries)
                .run(&[question(Value::Int(3))])
                .await
                .unwrap();
            // Repetition 1 starts at the same script position at any level.
            actual_by_level.push(runs[0].records[0].clone());
        }

        let db = DummyDb::new();
        let provider = DummyProvider::new(script).repeating();
        let high = runner_with_retries(&db, &provider, 4)
            .run(&[question(Value::Int(3))])
            .await
            .unwrap();
        let max_record = &high[0].records[0];

        for (actual, level) in actual_by_level.iter().zip([0, 2]) {
            let derived = bench_output::derive_retry_level(max_record, level);
            assert_eq!(
                strip_latency(actual),
                strip_latency(&derived),
                "level {level} derived record diverged from an actual run"
            );
        }
    }

    fn strip_latency(record: &ResultRecord) -> serde_json::Value {
        let mut value = serde_json::to_value(record).unwrap();
        value["latencyMs"] = 0.into();
        for attempt in value["attempts"].as_array_mut().unwrap() {
            attempt["latencyMs"] = 0.into();
        }
        value
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

    #[test]
    fn unterminated_trailing_fence_still_counts() {
        assert_eq!(
            extract_query("Here you go:\n```sql\nSELECT 1;"),
            Extraction::Query("SELECT 1;".to_string())
        );
    }

    #[test]
    fn examples_heading_appears_only_with_examples() {
        let template = "{{schema}}\n{{examples}}\n{{question}}";
        let without = assemble_prompt(template, "q", "s", &[], &[]);
        assert!(!without.contains("Examples:"));
        let with = assemble_prompt(
            template,
            "q",
            "s",
            &["e1".to_string(), "e2".to_string()],
            &[],
        );
        assert!(with.contains("Examples:\ne1\n\ne2"));
    }
}
