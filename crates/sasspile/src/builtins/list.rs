//! Sass `list.*` builtins. A non-list value counts as a one-element list (Sass rule), and
//! bare `length` dispatches here on the argument's runtime type (list → item count, else
//! string character count).

use super::str_of;
use crate::expr::num;
use crate::value::{Sep, Value};

/// `Some` when this domain owns `name`.
pub fn call(name: &str, args: &[Value]) -> Option<Result<Value, String>> {
    match (name, args) {
        ("length", [a]) => Some(Ok(length_of(a))),
        ("list.length", [a]) => Some(as_list(a).map(|(items, _)| Value::Number(items.len() as f64, String::new()))),
        ("list.nth", [a, idx]) => Some(nth_of(a, idx)),
        ("list.set-nth", [a, idx, v]) => {
            Some((|| {
                let (mut items, sep) = as_list(a)?;
                let i = index_arg(idx, items.len())?;
                items[i] = v.clone();
                Ok(Value::List(items, sep))
            })())
        }
        ("list.append", [a, v]) | ("list.append", [a, v, _]) => {
            Some(as_list(a).map(|(mut items, sep)| {
                items.push(v.clone());
                Value::List(items, sep)
            }))
        }
        ("list.join", [a, b]) | ("list.join", [a, b, _]) => {
            Some(as_list(a).and_then(|(mut items, sep)| {
                items.extend(as_list(b)?.0);
                Ok(Value::List(items, sep))
            }))
        }
        ("list.index-of" | "list.index", [a, needle]) => {
            Some(as_list(a).map(|(items, _)| {
                items.iter().position(|x| x == needle).map_or(Value::Null, |i| {
                    Value::Number(i as f64 + 1.0, String::new())
                })
            }))
        }
        ("list.separator", [a]) => {
            Some(as_list(a).map(|(_, sep)| {
                Value::Raw(match sep {
                    Sep::Comma => "comma",
                    Sep::Space => "space",
                    Sep::Slash => "slash",
                }
                .to_string())
            }))
        }
        _ => None,
    }
}

fn as_list(v: &Value) -> Result<(Vec<Value>, Sep), String> {
    match v {
        Value::List(items, sep) => Ok((items.clone(), *sep)),
        Value::Null => Ok((Vec::new(), Sep::Comma)),
        _ => Err("expected a list".to_string()),
    }
}

fn index_arg(idx: &Value, len: usize) -> Result<usize, String> {
    let n = num(idx)?.0;
    let i = if n < 0.0 { len as f64 + n } else { n - 1.0 };
    if i < 0.0 || i >= len as f64 {
        return Err(format!("index {n} out of range for list of length {len}"));
    }
    Ok(i as usize)
}

fn nth_of(list: &Value, idx: &Value) -> Result<Value, String> {
    let (items, _) = as_list(list)?;
    let i = index_arg(idx, items.len())?;
    Ok(items[i].clone())
}

/// Item count for a list, character count otherwise (bare `length` dispatch).
fn length_of(v: &Value) -> Value {
    match v {
        Value::List(items, _) => Value::Number(items.len() as f64, String::new()),
        _ => Value::Number(str_of(v).chars().count() as f64, String::new()),
    }
}
