//! Sass `map.*` builtins. Keys/values yield a comma-separated list (Sass `map.keys`/
//! `map.values` are always comma lists); `map.get` returns `Value::Null` when absent.

use crate::value::{Sep, Value};

/// `Some` when this domain owns `name`.
pub fn call(name: &str, args: &[Value]) -> Option<Result<Value, String>> {
    match (name, args) {
        ("map.get" | "map.has-key" | "map.keys" | "map.values" | "map.merge" | "map.set"
        | "map.remove", _) => Some(run(name, args)),
        _ => None,
    }
}

fn run(name: &str, args: &[Value]) -> Result<Value, String> {
    let pairs = |v: &Value| -> Result<Vec<(Value, Value)>, String> { Ok(as_map(v)?.clone()) };
    match (name, args) {
        ("map.get", [m, key]) => Ok(pairs(m)?
            .into_iter()
            .find(|(k, _)| k == key)
            .map_or(Value::Null, |(_, v)| v)),
        ("map.has-key", [m, key]) => {
            Ok(Value::Bool(pairs(m)?.into_iter().any(|(k, _)| k == *key)))
        }
        ("map.keys", [m]) => {
            Ok(Value::List(pairs(m)?.into_iter().map(|(k, _)| k).collect(), Sep::Comma))
        }
        ("map.values", [m]) => {
            Ok(Value::List(pairs(m)?.into_iter().map(|(_, v)| v).collect(), Sep::Comma))
        }
        ("map.merge", [a, b]) => {
            let merged = pairs(b)?.into_iter().fold(pairs(a)?, |mut acc, (k, v)| {
                match acc.iter_mut().find(|(ek, _)| *ek == k) {
                    Some(slot) => slot.1 = v,
                    None => acc.push((k, v)),
                }
                acc
            });
            Ok(Value::Map(merged))
        }
        ("map.set", [m, k, v]) => {
            let mut ps = pairs(m)?;
            match ps.iter_mut().find(|(ek, _)| *ek == *k) {
                Some(slot) => slot.1 = v.clone(),
                None => ps.push((k.clone(), v.clone())),
            }
            Ok(Value::Map(ps))
        }
        ("map.remove", [m, k]) => {
            let ps = pairs(m)?.into_iter().filter(|(ek, _)| *ek != *k).collect();
            Ok(Value::Map(ps))
        }
        _ => Err(format!("{name}: bad arguments")),
    }
}

/// View a value as a map's pairs (an empty map `()` / `null` is no pairs).
fn as_map(v: &Value) -> Result<&Vec<(Value, Value)>, String> {
    match v {
        Value::Map(pairs) => Ok(pairs),
        Value::Null => Ok(&EMPTY),
        _ => Err("expected a map".to_string()),
    }
}

static EMPTY: Vec<(Value, Value)> = Vec::new();
