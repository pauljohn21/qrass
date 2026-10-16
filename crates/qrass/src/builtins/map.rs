//! `map.*` builtins (task 8.2): the highest-volume namespace in element-plus
//! (`map.get` 393, `map.merge` 171). Maps are insertion-ordered key/value vectors;
//! results never mutate their input (values move by value, per the ownership rule).

use super::{Or, Registry};
use crate::ast_expr::ListSep;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

pub(crate) fn register(r: &mut Registry) {
    r.nsp("map", "get", 1, None, &["map", "key"], get);
    r.nsp("map", "has-key", 1, None, &["map", "key"], has_key);
    r.nsp("map", "keys", 1, Some(1), &["map"], keys);
    r.nsp("map", "values", 1, Some(1), &["map"], values);
    r.nsp("map", "merge", 2, Some(2), &["map1", "map2"], merge);
    r.nsp("map", "deep-merge", 1, None, &["map1", "map2"], deep_merge_n);
    r.nsp("map", "set", 3, Some(3), &["map", "key", "value"], set_call);
    r.nsp("map", "remove", 1, None, &["map", "key"], remove);
    // Legacy global spellings.
    r.def("map-get", 1, None, get);
    r.def("map-merge", 2, Some(2), merge);
    r.def("map-has-key", 1, None, has_key);
    r.def("map-keys", 1, Some(1), keys);
    r.def("map-values", 1, Some(1), values);
}

/// The map at argument `i` (empty map tolerated), or a type error.
fn as_map(c: &super::Call, i: usize) -> Or<Vec<(SassValue, SassValue)>> {
    match c.positional.get(i) {
        Some(SassValue::Map { entries }) => Ok(entries.clone()),
        Some(SassValue::List { items, .. }) if items.is_empty() => Ok(Vec::new()),
        Some(other) => Err(SassError::new(
            ErrorKind::Builtins,
            format!("expected a map, given {}", other.type_name()),
        )),
        None => Err(SassError::new(ErrorKind::Builtins, "expected a map")),
    }
}

fn map_value(entries: Vec<(SassValue, SassValue)>) -> SassValue {
    SassValue::Map { entries }
}

fn comma(items: Vec<SassValue>) -> SassValue {
    SassValue::List {
        items,
        sep: ListSep::Comma,
        bracketed: false,
    }
}

/// `map.get($map, $keys…)`: descend through nested keys; `null` when absent or a
/// non-map is reached before the keys are exhausted.
fn get(c: &super::Call) -> Or<SassValue> {
    descend(c, SassValue::Null)
}

/// Walk the nested-key path, returning `on_missing` when a step fails.
fn descend(c: &super::Call, on_missing: SassValue) -> Or<SassValue> {
    let mut current = c.positional[0].clone();
    for key in &c.positional[1..] {
        let entries = match current {
            SassValue::Map { entries } => entries,
            SassValue::List { items, .. } if items.is_empty() => Vec::new(),
            _ => return Ok(on_missing),
        };
        match entries.into_iter().find(|(k, _)| k == key) {
            Some((_, value)) => current = value,
            None => return Ok(on_missing),
        }
    }
    Ok(current)
}

fn has_key(c: &super::Call) -> Or<SassValue> {
    // Walk the nested-key path; the key "exists" only if every step is found.
    let mut current = c.positional[0].clone();
    for key in &c.positional[1..] {
        let entries = match current {
            SassValue::Map { entries } => entries,
            SassValue::List { items, .. } if items.is_empty() => Vec::new(),
            _ => return Ok(SassValue::Bool(false)),
        };
        match entries.into_iter().find(|(k, _)| k == key) {
            Some((_, value)) => current = value,
            None => return Ok(SassValue::Bool(false)),
        }
    }
    Ok(SassValue::Bool(true))
}

fn keys(c: &super::Call) -> Or<SassValue> {
    let entries = as_map(c, 0)?;
    Ok(comma(entries.into_iter().map(|(k, _)| k).collect()))
}

fn values(c: &super::Call) -> Or<SassValue> {
    let entries = as_map(c, 0)?;
    Ok(comma(entries.into_iter().map(|(_, v)| v).collect()))
}

fn set(entries: Vec<(SassValue, SassValue)>, key: SassValue, value: SassValue) -> SassValue {
    let mut entries = entries;
    match entries.iter_mut().find(|(k, _)| *k == key) {
        Some(slot) => slot.1 = value,
        None => entries.push((key, value)),
    }
    map_value(entries)
}

fn set_call(c: &super::Call) -> Or<SassValue> {
    let entries = as_map(c, 0)?;
    Ok(set(
        entries,
        c.positional[1].clone(),
        c.positional[2].clone(),
    ))
}

/// Shallow merge: `right` overrides `left` on key collisions.
fn merge(c: &super::Call) -> Or<SassValue> {
    let mut left = as_map(c, 0)?;
    let right = as_map(c, 1)?;
    for (k, v) in right {
        match left.iter_mut().find(|(lk, _)| *lk == k) {
            Some(slot) => slot.1 = v,
            None => left.push((k, v)),
        }
    }
    Ok(map_value(left))
}

/// Recursive merge of any number of maps (`map.deep-merge($maps...)`).
fn deep_merge_n(c: &super::Call) -> Or<SassValue> {
    let mut acc = as_map(c, 0)?;
    for i in 1..c.positional.len() {
        let next = as_map(c, i)?;
        acc = deep_merge(acc, next);
    }
    Ok(map_value(acc))
}

fn deep_merge(
    mut left: Vec<(SassValue, SassValue)>,
    right: Vec<(SassValue, SassValue)>,
) -> Vec<(SassValue, SassValue)> {
    for (k, v) in right {
        match left.iter_mut().find(|(lk, _)| *lk == k) {
            Some(slot) => {
                slot.1 = match (slot.1.clone(), v) {
                    (SassValue::Map { entries: a }, SassValue::Map { entries: b }) => {
                        map_value(deep_merge(a, b))
                    }
                    (_, v) => v,
                };
            }
            None => left.push((k, v)),
        }
    }
    left
}

/// `map.set($map, $key, $value)` registered through the shared helper.
fn remove(c: &super::Call) -> Or<SassValue> {
    let entries = as_map(c, 0)?;
    let keys: Vec<SassValue> = c.positional[1..].to_vec();
    Ok(map_value(
        entries
            .into_iter()
            .filter(|(k, _)| !keys.contains(k))
            .collect(),
    ))
}
