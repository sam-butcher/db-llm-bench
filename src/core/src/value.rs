use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Canonical result representation shared by all DB packages.
///
/// Every DB package coerces its driver-native results into this enum, so
/// that all cross-type comparison rules live in exactly one place:
/// [`Value::matches_expected`]. The derived `==` is strict structural
/// equality (`Int(3) != Float(3.0)`) — use it in tests, never for scoring
/// benchmark accuracy.
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
    /// This is the benchmark's accuracy rule — distinct from `==`.
    ///
    /// Every cross-type rule here is a deliberate decision; combinations not
    /// listed are unequal by design.
    pub fn matches_expected(&self, expected: &Value) -> bool {
        match (self, expected) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            // TODO: settle the float tolerance rule
            (Value::Float(a), Value::Float(b)) => a == b,
            // Compared in the integer domain: the float must be a whole
            // number strictly inside i64 range (so the cast below is exact,
            // never saturating) whose value is `a`. Int-to-float casts can't
            // be trusted here — they round at the extremes.
            (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => {
                *b >= -(2f64.powi(63)) && *b < 2f64.powi(63) && b.fract() == 0.0 && *b as i64 == *a
            }
            (Value::String(a), Value::String(b)) => a == b,
            // TODO: settle ordering (bag vs set) rules for lists
            (Value::List(a), Value::List(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.matches_expected(y))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len()
                    && a
                        .iter()
                        .all(|(k, v)| b.get(k).is_some_and(|w| v.matches_expected(w)))
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_and_float_match_when_the_int_round_trips() {
        assert!(Value::Int(3).matches_expected(&Value::Float(3.0)));
        assert!(Value::Float(3.0).matches_expected(&Value::Int(3)));
        assert!(!Value::Int(3).matches_expected(&Value::Float(3.5)));
        // i64::MAX is not representable in f64; the cast rounds, so this
        // must not compare equal.
        assert!(!Value::Int(i64::MAX).matches_expected(&Value::Float(i64::MAX as f64)));
    }

    #[test]
    fn structural_equality_stays_strict() {
        assert_ne!(Value::Int(3), Value::Float(3.0));
        assert!(!Value::String("3".into()).matches_expected(&Value::Int(3)));
        assert!(!Value::Bool(true).matches_expected(&Value::Int(1)));
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
