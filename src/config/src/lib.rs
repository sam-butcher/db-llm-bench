//! Config file processor: parses `config.yml` and loads the questions file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use bench_core::question::QuestionFile;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// The YAML's list-of-single-key-maps shape is a serialization artifact;
    /// access goes through [`Config::db_entries`].
    dbs: Vec<BTreeMap<String, DbConfig>>,
    /// Each entry maps a provider ID to its provider-specific config, which
    /// the matching provider package interprets (extension stays localised).
    /// Access goes through [`Config::model_entries`].
    models: Vec<BTreeMap<String, serde_yaml::Value>>,
    pub questions_path: PathBuf,
    pub example_counts: Vec<u32>,
    /// Runs only execute at the highest count; lower levels are derived from
    /// the attempt trace.
    pub max_retry_counts: Vec<u32>,
}

#[derive(Debug, Deserialize)]
pub struct DbConfig {
    /// Prompt folder: template plus optional example-N.txt files.
    pub prompts: PathBuf,
    pub url: String,
    /// DB-specific auth, interpreted by the matching DB package.
    pub auth: Option<serde_yaml::Value>,
    /// Optional folder of .md skills loaded when generating queries.
    pub skills: Option<PathBuf>,
    /// Schema description inserted into the prompt's schema slot.
    pub schema: PathBuf,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {source_name}: {message}")]
    Parse { source_name: String, message: String },
}

impl FromStr for Config {
    type Err = ConfigError;

    fn from_str(yaml: &str) -> Result<Config, ConfigError> {
        serde_yaml::from_str(yaml).map_err(|e| ConfigError::Parse {
            source_name: "<string>".to_string(),
            message: e.to_string(),
        })
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let raw = fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        raw.parse::<Config>().map_err(|e| match e {
            ConfigError::Parse { message, .. } => ConfigError::Parse {
                source_name: path.display().to_string(),
                message,
            },
            other => other,
        })
    }

    pub fn db_entries(&self) -> impl Iterator<Item = (&str, &DbConfig)> {
        self.dbs
            .iter()
            .flat_map(|entry| entry.iter().map(|(id, cfg)| (id.as_str(), cfg)))
    }

    pub fn model_entries(&self) -> impl Iterator<Item = (&str, &serde_yaml::Value)> {
        self.models
            .iter()
            .flat_map(|entry| entry.iter().map(|(id, cfg)| (id.as_str(), cfg)))
    }
}

pub fn load_questions(path: &Path) -> Result<QuestionFile, ConfigError> {
    let raw = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&raw).map_err(|e| ConfigError::Parse {
        source_name: path.display().to_string(),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_flattens_entries() {
        let config: Config = "\
dbs:
  - dummy:
      prompts: prompts/dummy
      url: unused
      schema: schema.txt
models:
  - dummy:
      responses: [\"```\\n3\\n```\"]
questionsPath: questions.json
exampleCounts: [0, 1]
maxRetryCounts: [0, 2]
"
        .parse()
        .unwrap();

        let dbs: Vec<_> = config.db_entries().collect();
        assert_eq!(dbs.len(), 1);
        assert_eq!(dbs[0].0, "dummy");
        assert_eq!(dbs[0].1.url, "unused");
        assert!(dbs[0].1.skills.is_none());

        let models: Vec<_> = config.model_entries().collect();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].0, "dummy");
    }

    #[test]
    fn invalid_yaml_is_a_parse_error() {
        assert!(matches!(
            "not: [valid".parse::<Config>(),
            Err(ConfigError::Parse { .. })
        ));
    }
}
