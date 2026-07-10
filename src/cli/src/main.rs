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

/// A DB whose client, prompt assets, and cross-checks against the run's
/// questions and example counts have all passed validation.
struct PreparedDb<'a> {
    id: &'a str,
    db: Box<dyn Database>,
    assets: DbAssets,
}

/// Build and validate every configured DB up-front, so a bad entry fails
/// the run before any benchmarking (and any spend) begins.
fn prepare_dbs<'a>(
    config: &'a Config,
    questions: &QuestionFile,
) -> anyhow::Result<Vec<PreparedDb<'a>>> {
    let max_examples = *config
        .example_counts
        .iter()
        .max()
        .expect("validated non-empty") as usize;
    config
        .db_entries()
        .map(|(db_id, db_cfg)| {
            let db = build_db(db_id, db_cfg)?;
            let assets = load_db_assets(db_cfg)?;
            validate_db_inputs(
                db_id,
                db_cfg,
                &assets,
                db.query_language(),
                questions,
                max_examples,
            )?;
            Ok(PreparedDb { id: db_id, db, assets })
        })
        .collect()
}

/// The cross-checks between a DB's assets and the rest of the run's
/// configuration; each of these would otherwise only fail mid-run, once
/// the combination (or output record) that needs it is reached.
fn validate_db_inputs(
    db_id: &str,
    db_cfg: &DbConfig,
    assets: &DbAssets,
    language: &str,
    questions: &QuestionFile,
    max_examples: usize,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        max_examples <= assets.examples.len(),
        "example count {max_examples} exceeds the {} example files in {}",
        assets.examples.len(),
        db_cfg.prompts.display()
    );
    let mut required_slots = vec!["{{question}}", "{{schema}}"];
    if max_examples > 0 {
        required_slots.push("{{examples}}");
    }
    if assets.skills.is_some() {
        required_slots.push("{{skills}}");
    }
    for slot in required_slots {
        anyhow::ensure!(
            assets.prompt_template.contains(slot),
            "prompt template {} is missing its {slot} slot",
            db_cfg.prompts.join("prompt.txt").display()
        );
    }
    if let Some(skills) = &assets.skills {
        anyhow::ensure!(
            !skills.is_empty(),
            "skills folder {} contains no .md files",
            db_cfg
                .skills
                .as_ref()
                .expect("skills were loaded from this path")
                .display()
        );
    }
    for question in &questions.questions {
        anyhow::ensure!(
            question.queries.contains_key(language),
            "question `{}` has no ground-truth {language} query for DB {db_id}",
            question.question
        );
    }
    Ok(())
}

/// Build every configured provider and require the record-identifying
/// labels to be unique: provider IDs may repeat in config, but same-label
/// results would be indistinguishable in the output.
fn build_models(config: &Config) -> anyhow::Result<Vec<Box<dyn ModelProvider>>> {
    let models: Vec<Box<dyn ModelProvider>> = config
        .model_entries()
        .map(|(id, cfg)| build_model(id, cfg))
        .collect::<anyhow::Result<_>>()?;
    let mut labels = std::collections::BTreeSet::new();
    for model in &models {
        let label = model.model_id();
        anyhow::ensure!(
            labels.insert(label.clone()),
            "two model entries share the label `{label}`; set a distinct `label` on one"
        );
    }
    Ok(models)
}

/// Run every DB x model x example-count x skills combination, folding
/// records into `outputs`. All inputs were validated by `prepare_dbs` and
/// `build_models`, so only runtime failures remain; on an unrecoverable
/// one the completed work is still marshalled, and the abort message is
/// returned for reporting after the partial results are written.
async fn run_benchmarks(
    config: &Config,
    questions: &QuestionFile,
    dbs: &[PreparedDb<'_>],
    models: &[Box<dyn ModelProvider>],
    retry_levels: &[u32],
    max_retries: u32,
    outputs: &mut [QuestionOutput],
) -> Option<String> {
    for prepared in dbs {
        for model in models {
            for &example_count in &config.example_counts {
                for skills in skills_variants(&prepared.assets.skills) {
                    let runner = BenchmarkRunner {
                        db: prepared.db.as_ref(),
                        model: model.as_ref(),
                        prompt_template: prepared.assets.prompt_template.clone(),
                        schema: prepared.assets.schema.clone(),
                        examples: prepared.assets.examples[..example_count as usize].to_vec(),
                        skills,
                        max_retries,
                    };
                    match runner.run(&questions.questions).await {
                        Ok(runs) => append_run_records(outputs, prepared.id, retry_levels, runs),
                        Err(failure) => {
                            let message = format!("aborted on DB {}: {failure}", prepared.id);
                            append_run_records(outputs, prepared.id, retry_levels, failure.completed);
                            return Some(message);
                        }
                    }
                }
            }
        }
    }
    None
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

    // Validate the whole run up-front: any provider, DB, or asset problem
    // should fail here, before any benchmarking begins.
    let models = build_models(&config)?;
    let dbs = prepare_dbs(&config, &questions)?;
    for prepared in &dbs {
        seed_db_slots(&mut outputs, &questions, prepared.id, prepared.db.query_language());
    }

    let abort = run_benchmarks(
        &config,
        &questions,
        &dbs,
        &models,
        &retry_levels,
        max_retries,
        &mut outputs,
    )
    .await;

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

#[cfg(test)]
mod tests {
    use super::*;
    use bench_core::{Question, Value};

    const FULL_TEMPLATE: &str = "{{schema}} {{examples}} {{skills}} {{question}}";

    fn db_cfg(skills: Option<PathBuf>) -> DbConfig {
        DbConfig {
            prompts: PathBuf::from("prompts"),
            url: "http://localhost".to_string(),
            database: None,
            auth: None,
            skills,
            schema: PathBuf::from("schema.txt"),
        }
    }

    fn assets(template: &str, examples: usize, skills: Option<Vec<String>>) -> DbAssets {
        DbAssets {
            prompt_template: template.to_string(),
            schema: "schema".to_string(),
            examples: vec!["example".to_string(); examples],
            skills,
        }
    }

    fn questions() -> QuestionFile {
        QuestionFile {
            questions: vec![Question {
                question: "How many cars are there?".to_string(),
                difficulty: "easy".to_string(),
                expected: Value::Int(3),
                ordered: false,
                queries: BTreeMap::from([("sql".to_string(), "SELECT 1".to_string())]),
            }],
        }
    }

    #[test]
    fn valid_inputs_pass() {
        let cfg = db_cfg(None);
        let full = assets(FULL_TEMPLATE, 3, None);
        assert!(validate_db_inputs("sql", &cfg, &full, "sql", &questions(), 3).is_ok());
    }

    #[test]
    fn insufficient_examples_are_rejected() {
        let cfg = db_cfg(None);
        let two = assets(FULL_TEMPLATE, 2, None);
        let err = validate_db_inputs("sql", &cfg, &two, "sql", &questions(), 3).unwrap_err();
        assert!(err.to_string().contains("example count 3 exceeds"));
    }

    #[test]
    fn missing_template_slots_are_rejected() {
        let cfg = db_cfg(None);
        let no_question = assets("{{schema}}", 0, None);
        let err = validate_db_inputs("sql", &cfg, &no_question, "sql", &questions(), 0).unwrap_err();
        assert!(err.to_string().contains("{{question}}"));

        // The examples slot is only required once examples are in play.
        let no_examples_slot = assets("{{schema}} {{question}}", 3, None);
        assert!(validate_db_inputs("sql", &cfg, &no_examples_slot, "sql", &questions(), 0).is_ok());
        let err =
            validate_db_inputs("sql", &cfg, &no_examples_slot, "sql", &questions(), 3).unwrap_err();
        assert!(err.to_string().contains("{{examples}}"));

        // The skills slot is only required when a skills folder is configured
        // (otherwise the skills-on variant would silently equal skills-off).
        let with_skills = db_cfg(Some(PathBuf::from("skills")));
        let no_skills_slot = assets("{{schema}} {{question}}", 0, Some(vec!["skill".to_string()]));
        let err = validate_db_inputs("sql", &with_skills, &no_skills_slot, "sql", &questions(), 0)
            .unwrap_err();
        assert!(err.to_string().contains("{{skills}}"));
    }

    #[test]
    fn empty_skills_folder_is_rejected() {
        let cfg = db_cfg(Some(PathBuf::from("skills")));
        let empty_skills = assets(FULL_TEMPLATE, 0, Some(Vec::new()));
        let err =
            validate_db_inputs("sql", &cfg, &empty_skills, "sql", &questions(), 0).unwrap_err();
        assert!(err.to_string().contains("no .md files"));
    }

    #[test]
    fn every_question_needs_a_query_in_the_dbs_language() {
        let cfg = db_cfg(None);
        let full = assets(FULL_TEMPLATE, 3, None);
        let err = validate_db_inputs("neo4j", &cfg, &full, "cypher", &questions(), 3).unwrap_err();
        assert!(err.to_string().contains("no ground-truth cypher query"));
    }
}
