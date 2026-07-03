use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use bench_config::{Config, DbConfig, load_questions};
use bench_core::{BenchmarkOutput, Database, DbOutput, ModelProvider, QuestionOutput};
use bench_runner::BenchmarkRunner;

/// Each valid DB ID gets its own package; adding a DB means adding a crate
/// and an arm here.
fn build_db(id: &str, _cfg: &DbConfig) -> anyhow::Result<Box<dyn Database>> {
    Ok(match id {
        "dummy" => Box::new(db_dummy::DummyDb::new()),
        other => anyhow::bail!("unknown DB id: {other}"),
    })
}

/// Each valid model ID gets its own package; the provider package interprets
/// its own config entry.
fn build_model(id: &str, cfg: &serde_json::Value) -> anyhow::Result<Box<dyn ModelProvider>> {
    Ok(match id {
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
        let path = cfg.prompts.join(format!("example-{}.txt", examples.len() + 1));
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1);
    let config_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.yml"));
    let output_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("results.json"));

    let config = Config::load(&config_path)?;
    let questions = load_questions(&config.questions_path)?;
    let max_retries = config.max_retry_counts.iter().max().copied().unwrap_or(0);

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

    for (db_id, db_cfg) in config.db_entries() {
        let db = build_db(db_id, db_cfg)?;
        let assets = load_db_assets(db_cfg)?;

        for (q, out) in questions.questions.iter().zip(&mut outputs) {
            out.dbs.insert(
                db_id.to_string(),
                DbOutput {
                    language: db.query_language().to_string(),
                    correct: q.queries.get(db.query_language()).cloned().unwrap_or_default(),
                    results: Vec::new(),
                },
            );
        }

        for model in &models {
            for &example_count in &config.example_counts {
                let count = example_count as usize;
                anyhow::ensure!(
                    count <= assets.examples.len(),
                    "example count {count} exceeds the {} example files in {}",
                    assets.examples.len(),
                    db_cfg.prompts.display()
                );
                // Skills on/off is a test dimension; DBs without a skills
                // folder only run with skills off.
                let skills_variants: Vec<Option<Vec<String>>> = match &assets.skills {
                    Some(skills) => vec![None, Some(skills.clone())],
                    None => vec![None],
                };
                for skills in skills_variants {
                    let runner = BenchmarkRunner {
                        db: db.as_ref(),
                        model: model.as_ref(),
                        prompt_template: assets.prompt_template.clone(),
                        schema: assets.schema.clone(),
                        examples: assets.examples[..count].to_vec(),
                        skills,
                        max_retries,
                    };
                    for run in runner.run(&questions.questions).await? {
                        outputs[run.question_index]
                            .dbs
                            .get_mut(db_id)
                            .expect("seeded above")
                            .results
                            .extend(run.records);
                    }
                }
            }
        }
    }

    let output = BenchmarkOutput { questions: outputs };
    bench_output::write_output(&output_path, &output)
        .with_context(|| format!("writing {}", output_path.display()))?;

    let records: Vec<_> = output
        .questions
        .iter()
        .flat_map(|q| q.dbs.values())
        .flat_map(|db| &db.results)
        .collect();
    let accurate = records.iter().filter(|r| r.accurate).count();
    println!(
        "Wrote {} records to {} ({accurate}/{} accurate)",
        records.len(),
        output_path.display(),
        records.len(),
    );
    Ok(())
}
