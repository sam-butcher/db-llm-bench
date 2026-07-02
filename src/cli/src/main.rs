use std::env;
use std::path::PathBuf;

use bench_config::{Config, DbConfig, load_questions};
use bench_core::{Database, ModelProvider};

/// Each valid DB ID gets its own package; adding a DB means adding a crate
/// and an arm here.
fn build_db(id: &str, cfg: &DbConfig) -> anyhow::Result<Box<dyn Database>> {
    Ok(match id {
        "typedb" => Box::new(db_typedb::TypeDb::new(cfg.url.clone())),
        "neo4j" => Box::new(db_neo4j::Neo4j::new(cfg.url.clone())),
        "sql" => Box::new(db_sql::Sql::new(cfg.url.clone())),
        other => anyhow::bail!("unknown DB id: {other}"),
    })
}

/// Each valid model ID gets its own package; the provider package interprets
/// its own config entry.
fn build_model(id: &str, cfg: &serde_yaml::Value) -> anyhow::Result<Box<dyn ModelProvider>> {
    Ok(match id {
        "claude" => Box::new(provider_claude::Claude::new(serde_yaml::from_value(
            cfg.clone(),
        )?)),
        "llama" => Box::new(provider_llama::Llama::new(serde_yaml::from_value(
            cfg.clone(),
        )?)),
        "chatgpt" => Box::new(provider_chatgpt::ChatGpt::new(serde_yaml::from_value(
            cfg.clone(),
        )?)),
        other => anyhow::bail!("unknown model id: {other}"),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.yml"));
    let config = Config::load(&config_path)?;
    let questions = load_questions(&config.questions_path)?;

    let dbs = config
        .db_entries()
        .map(|(id, cfg)| build_db(id, cfg))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let models = config
        .model_entries()
        .map(|(id, cfg)| build_model(id, cfg))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let max_retries = config.max_retry_counts.iter().max().copied().unwrap_or(0);
    println!(
        "{} DBs x {} models x {} example counts x skills on/off, {} questions, \
         {} repetitions per cell; running at {} max retries (levels {:?} derived from trace)",
        dbs.len(),
        models.len(),
        config.example_counts.len(),
        questions.questions.len(),
        bench_runner::REPETITIONS,
        max_retries,
        config.max_retry_counts,
    );

    // TODO: build one BenchmarkRunner per DB/model/example-count/skills
    // setup, execute, and marshal via bench_output::write_output.
    anyhow::bail!("benchmark execution not implemented yet")
}
