use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Canonical result representation shared by all DB packages.
///
/// Every DB package coerces its driver-native results into this enum, so
/// that all cross-type comparison rules live in exactly one place:
/// [`Value::matches`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged, from = "serde_json::Value")]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// Compare a coerced query result against a question's expected value.
    ///
    /// Every cross-type rule here is a deliberate decision; combinations not
    /// listed are unequal by design.
    pub fn matches(&self, expected: &Value) -> bool {
        match (self, expected) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            // TODO: settle the float tolerance rule
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => *a as f64 == *b,
            (Value::String(a), Value::String(b)) => a == b,
            // TODO: settle ordering (bag vs set) rules for lists
            (Value::List(a), Value::List(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.matches(y))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| v.matches(w)))
            }
            _ => false,
        }
    }
}

impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => match n.as_i64() {
                Some(i) => Value::Int(i),
                None => Value::Float(n.as_f64().unwrap_or(f64::NAN)),
            },
            serde_json::Value::String(s) => Value::String(s),
            serde_json::Value::Array(a) => Value::List(a.into_iter().map(Value::from).collect()),
            serde_json::Value::Object(o) => {
                Value::Object(o.into_iter().map(|(k, v)| (k, Value::from(v))).collect())
            }
        }
    }
}
