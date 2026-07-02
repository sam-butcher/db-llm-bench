//! Benchmark runner: one runner per DB + model + example count + skills
//! setup. Runs each question at the highest configured retry count; lower
//! retry levels are derived from the attempt trace during output marshalling.

use bench_core::{Database, ModelProvider, Question, ResultRecord};

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

impl BenchmarkRunner<'_> {
    /// For each question, REPETITIONS times: assemble the prompt, send it to
    /// the model, extract and execute the query, feeding errors back until
    /// success or max_retries is exhausted, then compare against expected.
    pub async fn run(&self, _questions: &[Question]) -> Vec<QuestionRun> {
        todo!("implement the question loop")
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
