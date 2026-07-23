//! Reference-query checker. Runs each answerable question's pre-written
//! ground-truth query against the live DBs and reports every result that does
//! not match the question's `expected` value.
//!
//! It reuses the benchmark's own DB packages and [`Value::matches_question`],
//! so it exercises the identical execute → coerce → compare path the benchmark
//! scores with: a query that passes here is one the benchmark would count
//! accurate, and a mismatch here is a genuine bug in the query or `expected`.
//!
//! Usage: `verify <config.yml> [questions.json]`
//! Questions default to the config's `questionsPath`. The DBs named in the
//! config must be up (e.g. via `databases/candidates/docker-compose.yml`).

use std::path::PathBuf;
use std::process::ExitCode;

use bench_cli::build_db;
use bench_config::{Config, load_questions};

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let mut args = std::env::args().skip(1);
    let config_path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("usage: verify <config.yml> [questions.json]"))?;
    let config = Config::load(&config_path)?;
    let questions_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| config.questions_path.clone());
    let questions = load_questions(&questions_path)?;
    println!(
        "Verifying {} against {}",
        questions_path.display(),
        config_path.display()
    );

    let mut total_checked = 0usize;
    let mut total_failed = 0usize;
    for (db_id, db_cfg) in config.db_entries() {
        let db = build_db(db_id, db_cfg)?;
        let lang = db.query_language();
        db.health_check()
            .await
            .map_err(|e| anyhow::anyhow!("DB {db_id} failed its health check: {e}"))?;

        let mut checked = 0usize;
        let mut failed = 0usize;
        for question in &questions.questions {
            if question.unanswerable {
                continue;
            }
            let Some(query) = question.queries.get(lang) else {
                println!(
                    "  [MISSING] {db_id}: no {lang} query — {}",
                    question.question
                );
                failed += 1;
                continue;
            };
            let expected = question
                .expected
                .as_ref()
                .expect("answerable question has an expected value (enforced at load)");
            checked += 1;
            match db.send_query(query).await {
                Ok(value) if value.matches_question(expected, question.ordered) => {}
                Ok(value) => {
                    failed += 1;
                    println!(
                        "  [FAIL] {db_id}: {}\n         expected: {expected:?}\n         got:      {value:?}",
                        question.question
                    );
                }
                Err(e) => {
                    failed += 1;
                    println!("  [ERROR] {db_id}: {}\n         {e}", question.question);
                }
            }
        }
        println!("{db_id}: {}/{checked} matched", checked - failed);
        total_checked += checked;
        total_failed += failed;
    }

    if total_failed == 0 {
        println!("\nAll {total_checked} checks passed.");
        Ok(ExitCode::SUCCESS)
    } else {
        println!("\n{total_failed} of {total_checked} checks FAILED.");
        Ok(ExitCode::FAILURE)
    }
}
