//! `map.*` builtins (task 8.2): the highest-volume namespace in element-plus
//! (`map.get` 393, `map.merge` 171). Maps are insertion-ordered key/value vectors;
//! results never mutate their input (values move by value, per the ownership rule).

use super::{Or, Registry};
use crate::ast_expr::ListSep;
use crate::drain::drain_one;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;
use rxrust::prelude::*;

pub(crate) fn register(r: &mut Registry) {
    r.nsp("map", "get", 2, None, &["map", "key"], get);
    r.nsp("map", "has-key", 2, None, &["map", "key"], has_key);
    r.nsp("map", "keys", 1, Some(1), &["map"], keys);
    r.nsp("map", "values", 1, Some(1), &["map"], values);
    r.nsp("map", "merge", 1, None, &["map1", "map2"], merge);
    r.nsp("map", "deep-merge", 2, Some(2), &["map1", "map2"], deep_merge_n);
    r.nsp("map", "set", 1, None, &["map", "key", "value"], set_call);
    r.nsp("map", "remove", 1, None, &["map", "key"], remove);
    r.nsp("map", "deep-remove", 2, None, &["map", "key"], deep_remove);
    // Legacy global spellings.
    r.def("map-get", 1, None, get);
    r.def("map-merge", 2, Some(2), merge);
    r.def("map-has-key", 1, None, has_key);
    r.def("map-keys", 1, Some(1), keys);
    r.def("map-values", 1, Some(1), values);
}

/// Wrap entries back into a map value.
fn map_value(entries: Vec<(SassValue, SassValue)>) -> SassValue {
    SassValue::Map { entries }
}

fn comma(items: Vec<SassValue>) -> SassValue {
    SassValue::List {
        items,
        sep: ListSep::Comma,
        bracketed: false,
        keywords: None,
    }
}

/// `map.get($map, $keys…)`: descend through nested keys; `null` when absent or a
/// non-map is reached before the keys are exhausted.
fn get(c: &super::Call) -> Or<SassValue> {
    // `get($map, $key, $keys…)`: `$map` must itself be a map (a non-map is a
    // type error naming `$map`), even though descent is lenient for a *miss*.
    coerce_map(&c.positional[0], "map")?;
    descend(c, SassValue::Null)
}

/// Walk the nested-key path, returning `on_missing` when a step fails.
/// The path is an owned stream fold; a failed step latches the `None` marker
/// (every later key sees `None` and stays missing — the `?`/early-return shape).
fn descend(c: &super::Call, on_missing: SassValue) -> Or<SassValue> {
    let walked = drain_one(
        Local::from_iter(c.positional[1..].to_vec())
            .reduce_initial(Some(c.positional[0].clone()), |acc, key| {
                acc.and_then(|current| step_into(&current, &key))
            })
            .map(Ok)
            .box_it(),
    );
    Ok(walked.unwrap_or(on_missing))
}

/// One key step: map entries by key lookup; the empty list `()` walks as the
/// empty map; anything else misses.
fn step_into(current: &SassValue, key: &SassValue) -> Option<SassValue> {
    match current {
        SassValue::Map { entries } => entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone()),
        SassValue::List { items, .. } if items.is_empty() => None,
        _ => None,
    }
}

fn has_key(c: &super::Call) -> Or<SassValue> {
    // `has-key($map, $key, $keys…)`: `$map` must be a map (type error names it).
    coerce_map(&c.positional[0], "map")?;
    // Walk the nested-key path; the key "exists" only if every step is found.
    let walked = drain_one(
        Local::from_iter(c.positional[1..].to_vec())
            .reduce_initial(Some(c.positional[0].clone()), |acc, key| {
                acc.and_then(|current| step_into(&current, &key))
            })
            .map(Ok)
            .box_it(),
    );
    Ok(SassValue::Bool(walked.is_some()))
}

fn keys(c: &super::Call) -> Or<SassValue> {
    let entries = coerce_map(&c.positional[0], "map")?;
    Ok(comma(entries.into_iter().map(|(k, _)| k).collect()))
}

fn values(c: &super::Call) -> Or<SassValue> {
    let entries = coerce_map(&c.positional[0], "map")?;
    Ok(comma(entries.into_iter().map(|(_, v)| v).collect()))
}

/// `map.set($map, $keys…, $value)`: descend `$map` along `$keys…` (fabricating
/// intermediate maps for any missing or non-map step) and set `$value` (the final
/// argument) at the leaf. With a single key it is the plain `set($map, $key,
/// $value)`. Fewer than three arguments raise the goldens' `$args` key/value
/// errors; a non-map `$map` names the offending argument.
fn set_call(c: &super::Call) -> Or<SassValue> {
    let args = &c.positional;
    if args.len() < 3 {
        return Err(SassError::new(
            ErrorKind::Builtins,
            if args.len() == 1 {
                "Expected $args to contain a key."
            } else {
                "Expected $args to contain a value."
            },
        ));
    }
    let map = coerce_map(&args[0], "map")?;
    let value = args[args.len() - 1].clone();
    Ok(map_value(set_at(map, &args[1..args.len() - 1], value)))
}

/// Descend `entries` along `keys`, setting `value` at the leaf key.
fn set_at(
    mut entries: Vec<(SassValue, SassValue)>,
    keys: &[SassValue],
    value: SassValue,
) -> Vec<(SassValue, SassValue)> {
    let key = &keys[0];
    let idx = entries.iter().position(|(k, _)| k == key);
    if keys.len() == 1 {
        match idx {
            Some(i) => entries[i].1 = value,
            None => entries.push((key.clone(), value)),
        }
        return entries;
    }
    let child = match idx {
        Some(i) => match &entries[i].1 {
            SassValue::Map { entries } => entries.clone(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    let new_child = map_value(set_at(child, &keys[1..], value));
    match idx {
        Some(i) => entries[i].1 = new_child,
        None => entries.push((key.clone(), new_child)),
    }
    entries
}

/// Shallow merge: `right` overrides `left` on key collisions.
fn shallow_merge(
    left: Vec<(SassValue, SassValue)>,
    right: Vec<(SassValue, SassValue)>,
) -> Vec<(SassValue, SassValue)> {
    drain_one(
        Local::from_iter(right)
            .reduce_initial(left, |mut acc, (k, v)| {
                match acc.iter_mut().find(|(lk, _)| *lk == k) {
                    Some(slot) => slot.1 = v,
                    None => acc.push((k, v)),
                }
                acc
            })
            .map(Ok)
            .box_it(),
    )
}

/// `map.merge($map1, $keys…, $map2)`: merge `$map2` (the final argument) into the
/// map reached by descending `$map1` along `$keys…`, fabricating intermediate
/// maps for any missing or non-map step. With no keys it is the plain two-map
/// shallow merge; `$map1`/`$map2` type errors and the single-arg `$args` error
/// mirror the goldens.
fn merge(c: &super::Call) -> Or<SassValue> {
    let args = &c.positional;
    if args.len() < 2 {
        return Err(SassError::new(
            ErrorKind::Builtins,
            "Expected $args to contain a key.",
        ));
    }
    let map1 = coerce_map(&args[0], "map1")?;
    let map2 = coerce_map(args.last().unwrap(), "map2")?;
    Ok(map_value(merge_at(
        map1,
        &args[1..args.len() - 1],
        map2,
    )))
}

/// A map's entries (empty list counts as empty), or a `$name: <value> is not a
/// map.` error naming the offending argument.
fn coerce_map(v: &SassValue, name: &str) -> Or<Vec<(SassValue, SassValue)>> {
    match v {
        SassValue::Map { entries } => Ok(entries.clone()),
        SassValue::List { items, .. } if items.is_empty() => Ok(Vec::new()),
        other => Err(SassError::new(
            ErrorKind::Builtins,
            format!("${name}: {} is not a map.", other.to_css()),
        )),
    }
}

/// Descend `entries` along `keys`, shallow-merging `map2` at the leaf.
fn merge_at(
    mut entries: Vec<(SassValue, SassValue)>,
    keys: &[SassValue],
    map2: Vec<(SassValue, SassValue)>,
) -> Vec<(SassValue, SassValue)> {
    let Some(key) = keys.first() else {
        return shallow_merge(entries, map2);
    };
    let idx = entries.iter().position(|(k, _)| k == key);
    let child = match idx {
        Some(i) => match &entries[i].1 {
            SassValue::Map { entries } => entries.clone(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    let merged = map_value(merge_at(child, &keys[1..], map2));
    match idx {
        Some(i) => entries[i].1 = merged,
        None => entries.push((key.clone(), merged)),
    }
    entries
}

/// `map.deep-merge($map1, $map2)`: recursively merge two maps. The goldens fix
/// the arity at exactly two arguments (1 → missing `$map2`, 3 → "Only 2
/// arguments allowed").
fn deep_merge_n(c: &super::Call) -> Or<SassValue> {
    let acc = coerce_map(&c.positional[0], "map1")?;
    let next = coerce_map(&c.positional[1], "map2")?;
    Ok(map_value(deep_merge(acc, next)))
}

/// A map's entries, or the empty list `()` read as an empty map, or `None` for a
/// genuine non-map. Reading `()` as an empty map is what makes
/// `deep-merge((c: (d: e)), (c: ()))` keep the left `(d: e)`.
fn as_map_or_empty(v: &SassValue) -> Option<Vec<(SassValue, SassValue)>> {
    match v {
        SassValue::Map { entries } => Some(entries.clone()),
        SassValue::List { items, .. } if items.is_empty() => Some(Vec::new()),
        _ => None,
    }
}

fn deep_merge(
    left: Vec<(SassValue, SassValue)>,
    right: Vec<(SassValue, SassValue)>,
) -> Vec<(SassValue, SassValue)> {
    drain_one(
        Local::from_iter(right)
            .reduce_initial(left, |mut acc, (k, v)| {
                match acc.iter_mut().find(|(lk, _)| *lk == k) {
                    Some(slot) => {
                        slot.1 = match (as_map_or_empty(&slot.1), as_map_or_empty(&v)) {
                            (Some(a), Some(b)) => map_value(deep_merge(a, b)),
                            _ => v.clone(),
                        };
                    }
                    None => acc.push((k, v)),
                }
                acc
            })
            .map(Ok)
            .box_it(),
    )
}

/// `map.remove($map, $keys…)`.
fn remove(c: &super::Call) -> Or<SassValue> {
    let entries = coerce_map(&c.positional[0], "map")?;
    let keys: Vec<SassValue> = c.positional[1..].to_vec();
    Ok(map_value(
        entries
            .into_iter()
            .filter(|(k, _)| !keys.contains(k))
            .collect(),
    ))
}

/// `map.deep-remove($map, $keys…)`: remove the value at a nested key *path*.
/// Any step that misses (`key` absent, or an intermediate value that is not a
/// map) leaves the whole `$map` unchanged — matching the goldens' `not_found/*`
/// cases. A found nested key replaces its child in place, preserving order
/// (so `deep-remove((c: (d: e)), c, d)` yields `(c: ())`, not `()`).
fn deep_remove(c: &super::Call) -> Or<SassValue> {
    let entries = coerce_map(&c.positional[0], "map")?;
    let keys: Vec<SassValue> = c.positional[1..].to_vec();
    Ok(map_value(deep_remove_entries(entries, &keys)))
}

fn deep_remove_entries(
    mut entries: Vec<(SassValue, SassValue)>,
    keys: &[SassValue],
) -> Vec<(SassValue, SassValue)> {
    let Some(key) = keys.first() else {
        return entries;
    };
    // Key not present at this level → nothing to remove; return untouched.
    let Some(idx) = entries.iter().position(|(k, _)| k == key) else {
        return entries;
    };
    // Last key in the path: drop the entry.
    if keys.len() == 1 {
        entries.remove(idx);
        return entries;
    }
    // Descend only when the child is itself a map; a non-map stops the walk with
    // the map left as-is.
    let child = match &entries[idx].1 {
        SassValue::Map { entries } => entries.clone(),
        SassValue::List { items, .. } if items.is_empty() => Vec::new(),
        _ => return entries,
    };
    entries[idx].1 = map_value(deep_remove_entries(child, &keys[1..]));
    entries
}
