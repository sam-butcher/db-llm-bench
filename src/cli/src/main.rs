use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use bench_config::{Config, DbConfig, load_questions};
use bench_core::{
    BenchmarkOutput, Database, DbOutput, ModelProvider, QuestionFile, QuestionOutput,
};
use bench_output::derive_retry_level;
use bench_runner::{BenchmarkRunner, QuestionRun};

/// Fold question runs into the output structure, expanding each record into
/// one entry per configured retry level.
fn append_run_records(
    outputs: &mut [QuestionOutput],
    db_id: &str,
    retry_levels: &[u32],
    runs: Vec<QuestionRun>,
) {
    for run in runs {
        let results = &mut outputs[run.question_index]
            .dbs
            .get_mut(db_id)
            .expect("seeded before running")
            .results;
        for record in run.records {
            results.extend(
                retry_levels
                    .iter()
                    .map(|&level| derive_retry_level(&record, level)),
            );
        }
    }
}

/// `[config path] [output path]`, defaulting to config.yml and results.json.
fn parse_args() -> (PathBuf, PathBuf) {
    let mut args = env::args().skip(1);
    let config_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.yml"));
    let output_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("results.json"));
    (config_path, output_path)
}

/// Give every question an (empty) result slot for this DB before running,
/// so partial-failure marshalling always has somewhere to land.
fn seed_db_slots(
    outputs: &mut [QuestionOutput],
    questions: &QuestionFile,
    db_id: &str,
    language: &str,
) {
    for (question, output) in questions.questions.iter().zip(outputs) {
        output.dbs.insert(
            db_id.to_string(),
            DbOutput {
                language: language.to_string(),
                correct: question.queries.get(language).cloned().unwrap_or_default(),
                results: Vec::new(),
            },
        );
    }
}

/// Skills on/off is a test dimension; DBs without a skills folder only run
/// with skills off.
fn skills_variants(skills: &Option<Vec<String>>) -> Vec<Option<Vec<String>>> {
    match skills {
        Some(skills) => vec![None, Some(skills.clone())],
        None => vec![None],
    }
}

/// (total, accurate) across every record in the output.
fn count_records(output: &BenchmarkOutput) -> (usize, usize) {
    let mut total = 0;
    let mut accurate = 0;
    for record in output
        .questions
        .iter()
        .flat_map(|q| q.dbs.values())
        .flat_map(|db| &db.results)
    {
        total += 1;
        if record.accurate {
            accurate += 1;
        }
    }
    (total, accurate)
}

/// Parse a DB's free-form auth block into the package's typed auth struct.
fn parse_auth<T: serde::de::DeserializeOwned>(
    cfg: &DbConfig,
    db_id: &str,
) -> anyhow::Result<Option<T>> {
    cfg.auth
        .as_ref()
        .map(|value| {
            serde_json::from_value(value.clone())
                .with_context(|| format!("parsing {db_id} auth (expects username/password)"))
        })
        .transpose()
}

/// Each valid DB ID gets its own package; adding a DB means adding a crate
/// and an arm here.
fn build_db(id: &str, cfg: &DbConfig) -> anyhow::Result<Box<dyn Database>> {
    Ok(match id {
        "dummy" => Box::new(db_dummy::DummyDb::new()),
        "typedb" => {
            let database = cfg
                .database
                .clone()
                .ok_or_else(|| anyhow::anyhow!("typedb requires `database` in its config"))?;
            let auth = parse_auth::<db_typedb::TypeDbAuth>(cfg, "typedb")?.unwrap_or_default();
            Box::new(
                db_typedb::TypeDb::new(cfg.url.clone(), database, auth)
                    .map_err(anyhow::Error::msg)?,
            )
        }
        "neo4j" => {
            let auth = parse_auth::<db_neo4j::Neo4jAuth>(cfg, "neo4j")?;
            Box::new(
                db_neo4j::Neo4j::new(&cfg.url, cfg.database.as_deref(), auth.as_ref())
                    .map_err(anyhow::Error::msg)?,
            )
        }
        "sql" => {
            let auth = parse_auth::<db_sql::SqlAuth>(cfg, "sql")?;
            Box::new(
                db_sql::Sql::new(&cfg.url, cfg.database.as_deref(), auth.as_ref())
                    .map_err(anyhow::Error::msg)?,
            )
        }
        other => anyhow::bail!("unknown DB id: {other}"),
    })
}

/// Each valid model ID gets its own package; the provider package interprets
/// its own config entry.
fn build_model(id: &str, cfg: &serde_json::Value) -> anyhow::Result<Box<dyn ModelProvider>> {
    Ok(match id {
        "claude" => {
            let config: provider_claude::ClaudeConfig = serde_json::from_value(cfg.clone())
                .context("parsing claude model config (expects model, optional api_key/max_tokens/thinking/effort)")?;
            let api_key = config.resolve_api_key().map_err(anyhow::Error::msg)?;
            Box::new(provider_claude::Claude::new(config, api_key).map_err(anyhow::Error::msg)?)
        }
        "openai-compatible" => {
            let config: provider_openai_compatible::OpenAiCompatibleConfig =
                serde_json::from_value(cfg.clone())
                    .context("parsing openai-compatible model config (expects model, base_url, optional label/api_key_env/max_tokens/max_tokens_field)")?;
            let api_key = config.resolve_api_key().map_err(anyhow::Error::msg)?;
            Box::new(
                provider_openai_compatible::OpenAiCompatible::new(config, api_key)
                    .map_err(anyhow::Error::msg)?,
            )
        }
        "dummy" => Box::new(provider_dummy::DummyProvider::from(
            serde_json::from_value::<provider_dummy::DummyConfig>(cfg.clone())?,
        )),
        other => anyhow::bail!("unknown model id: {other}"),
    })
}

struct DbAssets {
    prompt_template: String,
    schema: String,
    examples: Vec<String>,
    skills: Option<Vec<String>>,
}

/// The prompt folder holds `prompt.txt` plus `example-1.txt`, `example-2.txt`,
/// ... (contiguous from 1).
fn load_db_assets(cfg: &DbConfig) -> anyhow::Result<DbAssets> {
    let template_path = cfg.prompts.join("prompt.txt");
    let prompt_template = fs::read_to_string(&template_path)
        .with_context(|| format!("reading prompt template {}", template_path.display()))?;
    let schema = fs::read_to_string(&cfg.schema)
        .with_context(|| format!("reading schema {}", cfg.schema.display()))?;
    let mut examples = Vec::new();
    loop {
        let path = cfg
            .prompts
            .join(format!("example-{}.txt", examples.len() + 1));
        if !path.exists() {
            break;
        }
        examples.push(
            fs::read_to_string(&path)
                .with_context(|| format!("reading example {}", path.display()))?,
        );
    }
    let skills = cfg.skills.as_deref().map(load_skills).transpose()?;
    Ok(DbAssets {
        prompt_template,
        schema,
        examples,
        skills,
    })
}

fn load_skills(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading skills folder {}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            fs::read_to_string(&path).with_context(|| format!("reading skill {}", path.display()))
        })
        .collect()
}

/// Run every DB x model x example-count x skills combination, folding
/// records into `outputs`. On an unrecoverable failure the completed work
/// is still marshalled, and the abort message is returned for reporting
/// after the partial results are written.
async fn run_benchmarks(
    config: &Config,
    questions: &QuestionFile,
    models: &[Box<dyn ModelProvider>],
    retry_levels: &[u32],
    max_retries: u32,
    outputs: &mut [QuestionOutput],
) -> anyhow::Result<Option<String>> {
    for (db_id, db_cfg) in config.db_entries() {
        let db = build_db(db_id, db_cfg)?;
        let assets = load_db_assets(db_cfg)?;
        seed_db_slots(outputs, questions, db_id, db.query_language());

        for model in models {
            for &example_count in &config.example_counts {
                let count = example_count as usize;
                anyhow::ensure!(
                    count <= assets.examples.len(),
                    "example count {count} exceeds the {} example files in {}",
                    assets.examples.len(),
                    db_cfg.prompts.display()
                );
                for skills in skills_variants(&assets.skills) {
                    let runner = BenchmarkRunner {
                        db: db.as_ref(),
                        model: model.as_ref(),
                        prompt_template: assets.prompt_template.clone(),
                        schema: assets.schema.clone(),
                        examples: assets.examples[..count].to_vec(),
                        skills,
                        max_retries,
                    };
                    match runner.run(&questions.questions).await {
                        Ok(runs) => append_run_records(outputs, db_id, retry_levels, runs),
                        Err(failure) => {
                            let message = format!("aborted on DB {db_id}: {failure}");
                            append_run_records(outputs, db_id, retry_levels, failure.completed);
                            return Ok(Some(message));
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (config_path, output_path) = parse_args();
    let config = Config::load(&config_path)?;
    let questions = load_questions(&config.questions_path)?;
    // Run only at the highest level; every configured level is derived from
    // the attempt trace afterwards.
    let mut retry_levels = config.max_retry_counts.clone();
    retry_levels.sort_unstable();
    retry_levels.dedup();
    let max_retries = *retry_levels.last().expect("validated non-empty");

    let mut outputs: Vec<QuestionOutput> = questions
        .questions
        .iter()
        .map(|q| QuestionOutput {
            question: q.question.clone(),
            difficulty: q.difficulty.clone(),
            expected: q.expected.clone(),
            dbs: BTreeMap::new(),
        })
        .collect();

    let models: Vec<Box<dyn ModelProvider>> = config
        .model_entries()
        .map(|(id, cfg)| build_model(id, cfg))
        .collect::<anyhow::Result<_>>()?;
    // Provider IDs may repeat in config; the record-identifying label must
    // not, or their results become indistinguishable.
    let mut labels = std::collections::BTreeSet::new();
    for model in &models {
        let label = model.model_id();
        anyhow::ensure!(
            labels.insert(label.clone()),
            "two model entries share the label `{label}`; set a distinct `label` on one"
        );
    }

    let abort = run_benchmarks(
        &config,
        &questions,
        &models,
        &retry_levels,
        max_retries,
        &mut outputs,
    )
    .await?;

    let output = BenchmarkOutput { questions: outputs };
    bench_output::write_output(&output_path, &output)
        .with_context(|| format!("writing {}", output_path.display()))?;

    let (total, accurate) = count_records(&output);
    println!(
        "Wrote {total} records to {} ({accurate}/{total} accurate)",
        output_path.display(),
    );
    if let Some(message) = abort {
        anyhow::bail!("{message}; partial results written");
    }
    Ok(())
}
