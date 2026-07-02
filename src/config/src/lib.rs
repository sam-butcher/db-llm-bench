//! Config file processor: parses `config.yml` and loads the questions file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use bench_core::question::QuestionFile;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub dbs: Vec<BTreeMap<String, DbConfig>>,
    /// Each entry maps a provider ID to its provider-specific config, which
    /// the matching provider package interprets (extension stays localised).
    pub models: Vec<BTreeMap<String, serde_yaml::Value>>,
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
    #[error("failed to parse {path}: {message}")]
    Parse { path: PathBuf, message: String },
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let raw = fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        serde_yaml::from_str(&raw).map_err(|e| ConfigError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
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
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}
