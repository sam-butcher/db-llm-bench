//! Config file processor: parses `config.yml` and loads the questions file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use bench_core::question::QuestionFile;
use serde::{Deserialize, Deserializer};
use serde_yaml2::wrapper::YamlNodeWrapper;
use thiserror::Error;

/// serde_yaml2 can't deserialize a YAML mapping directly into
/// `serde_json::Value`, so free-form fields deserialize as its
/// `YamlNodeWrapper` and are transcoded through Serialize.
fn transcode(node: YamlNodeWrapper) -> Result<serde_json::Value, String> {
    serde_json::to_value(node).map_err(|e| e.to_string())
}

fn yaml_any_opt<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<serde_json::Value>, D::Error> {
    Option::<YamlNodeWrapper>::deserialize(deserializer)?
        .map(|node| transcode(node).map_err(serde::de::Error::custom))
        .transpose()
}

fn yaml_any_entries<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BTreeMap<String, serde_json::Value>>, D::Error> {
    Vec::<BTreeMap<String, YamlNodeWrapper>>::deserialize(deserializer)?
        .into_iter()
        .map(|entry| {
            entry
                .into_iter()
                .map(|(id, node)| Ok((id, transcode(node).map_err(serde::de::Error::custom)?)))
                .collect()
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// The YAML's list-of-single-key-maps shape is a serialization artifact;
    /// access goes through [`Config::db_entries`].
    dbs: Vec<BTreeMap<String, DbConfig>>,
    /// Each entry maps a provider ID to its provider-specific config, which
    /// the matching provider package interprets (extension stays localised).
    /// Held as `serde_json::Value` so the YAML library never appears in this
    /// crate's API. Access goes through [`Config::model_entries`].
    #[serde(deserialize_with = "yaml_any_entries")]
    models: Vec<BTreeMap<String, serde_json::Value>>,
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
    /// Database name within the server, for DBs that namespace by database
    /// (e.g. TypeDB, Neo4j).
    pub database: Option<String>,
    /// DB-specific auth, interpreted by the matching DB package.
    #[serde(default, deserialize_with = "yaml_any_opt")]
    pub auth: Option<serde_json::Value>,
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
    #[error("invalid config: {0}")]
    Invalid(String),
}

impl FromStr for Config {
    type Err = ConfigError;

    fn from_str(yaml: &str) -> Result<Config, ConfigError> {
        let config: Config = serde_yaml2::from_str(yaml).map_err(|e| ConfigError::Parse {
            source_name: "<string>".to_string(),
            message: e.to_string(),
        })?;
        config.validate()?;
        Ok(config)
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?.parse::<Config>().map_err(|e| match e {
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

    pub fn model_entries(&self) -> impl Iterator<Item = (&str, &serde_json::Value)> {
        self.models
            .iter()
            .flat_map(|entry| entry.iter().map(|(id, cfg)| (id.as_str(), cfg)))
    }

    fn validate(&self) -> Result<(), ConfigError> {
        fn no_duplicates<'a>(
            kind: &str,
            ids: impl Iterator<Item = &'a str>,
        ) -> Result<usize, ConfigError> {
            let mut seen = std::collections::BTreeSet::new();
            let mut count = 0;
            for id in ids {
                if !seen.insert(id) {
                    return Err(ConfigError::Invalid(format!("duplicate {kind} id: {id}")));
                }
                count += 1;
            }
            if count == 0 {
                return Err(ConfigError::Invalid(format!("no {kind}s configured")));
            }
            Ok(count)
        }
        no_duplicates("DB", self.db_entries().map(|(id, _)| id))?;
        no_duplicates("model", self.model_entries().map(|(id, _)| id))?;
        if self.example_counts.is_empty() {
            return Err(ConfigError::Invalid("exampleCounts is empty".to_string()));
        }
        if self.max_retry_counts.is_empty() {
            return Err(ConfigError::Invalid("maxRetryCounts is empty".to_string()));
        }
        Ok(())
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
      auth:
        user: admin
      schema: schema.txt
models:
  - dummy:
      responses: [\"select 1\"]
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
        assert_eq!(dbs[0].1.auth, Some(serde_json::json!({"user": "admin"})));
        assert!(dbs[0].1.skills.is_none());

        let models: Vec<_> = config.model_entries().collect();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].0, "dummy");
        assert_eq!(
            models[0].1,
            &serde_json::json!({"responses": ["select 1"]})
        );
    }

    #[test]
    fn invalid_yaml_is_a_parse_error() {
        assert!(matches!(
            "not: [valid".parse::<Config>(),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn duplicate_db_ids_are_rejected() {
        let yaml = "\
dbs:
  - dummy:
      prompts: p
      url: u
      schema: s
  - dummy:
      prompts: p
      url: u
      schema: s
models:
  - dummy:
      responses: []
questionsPath: q.json
exampleCounts: [0]
maxRetryCounts: [0]
";
        assert!(matches!(
            yaml.parse::<Config>(),
            Err(ConfigError::Invalid(msg)) if msg.contains("duplicate DB id")
        ));
    }

    #[test]
    fn empty_example_counts_are_rejected() {
        let yaml = "\
dbs:
  - dummy:
      prompts: p
      url: u
      schema: s
models:
  - dummy:
      responses: []
questionsPath: q.json
exampleCounts: []
maxRetryCounts: [0]
";
        assert!(matches!(
            yaml.parse::<Config>(),
            Err(ConfigError::Invalid(msg)) if msg.contains("exampleCounts")
        ));
    }
}
