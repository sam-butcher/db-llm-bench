//! Shared types for the benchmark framework: the canonical result value,
//! the `Database` and `ModelProvider` traits that DB/provider packages
//! implement, and the question definitions.

pub mod db;
pub mod model;
pub mod question;
pub mod value;

pub use db::{Database, QueryError};
pub use model::{Message, ModelProvider, ModelResponse, ProviderError, Role};
pub use question::{Question, QuestionFile};
pub use value::Value;
