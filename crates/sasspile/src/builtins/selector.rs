//! Sass `selector.*` — this step covers the tractable, structural parts: `selector.nest`
//! and `selector.parse`. A selector is modelled transiently as `Vec<Complex>` where a
//! `Complex` is a whitespace-separated list of compound strings; parsing splits top-level
//! commas then whitespace (no `subst`/char-scanner on the final value — it is a real
//! selector split). The deeper algebra (`unify`/`extend`/`replace`/`is-superselector`)
//! is tied to `@extend` (task 6) and deliberately deferred.

use crate::value::{Sep, Value};

type Complex = Vec<String>;

/// `Some` when this domain owns `name`.
pub fn call(name: &str, args: &[Value]) -> Option<Result<Value, String>> {
    match (name, args) {
        ("selector.parse", [s]) => Some(arg_sel(s).map(to_value)),
        ("selector.nest", [first, rest @ ..]) => Some(nest(first, rest)),
        _ => None,
    }
}

/// Nested list-of-lists ("parsed selector format").
fn to_value(sel: Vec<Complex>) -> Value {
    Value::List(
        sel.into_iter()
            .map(|c| Value::List(c.into_iter().map(|s| Value::Str { text: s, quoted: false }).collect(), Sep::Space))
            .collect(),
        Sep::Comma,
    )
}

/// Read an argument as a selector: from a string, or from an already-parsed nested list.
fn arg_sel(v: &Value) -> Result<Vec<Complex>, String> {
    match v {
        Value::Str { text, .. } | Value::Raw(text) => parse_sel(text),
        Value::List(items, _) => items.iter().map(compound_of).collect(),
        _ => Err("expected a selector".to_string()),
    }
}

fn compound_of(v: &Value) -> Result<Complex, String> {
    match v {
        Value::List(items, _) => items
            .iter()
            .map(|c| match c {
                Value::Str { text, .. } | Value::Raw(text) => Ok(text.clone()),
                _ => Err("expected a compound selector".to_string()),
            })
            .collect(),
        Value::Str { text, .. } | Value::Raw(text) => parse_sel(text).and_then(|cs| cs.into_iter().next().map_or_else(|| Err("empty selector".to_string()), Ok)),
        _ => Err("expected a compound selector".to_string()),
    }
}

fn parse_sel(text: &str) -> Result<Vec<Complex>, String> {
    if text.contains('&') {
        return Err("Parent selectors aren't allowed here.".to_string());
    }
    Ok(split_top_commas(text).into_iter().map(|s| s.split_whitespace().map(str::to_string).collect()).filter(|c: &Complex| !c.is_empty()).collect())
}

/// Split on top-level commas (no nested parens/brackets in the subset we accept here).
fn split_top_commas(text: &str) -> Vec<String> {
    text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn nest(first: &Value, rest: &[Value]) -> Result<Value, String> {
    let base = arg_sel(first)?;
    let sel = rest.iter().try_fold(base, |acc, child| {
        let c = arg_sel(child)?;
        Ok::<Vec<Complex>, String>(cross(&acc, &c))
    })?;
    Ok(Value::Raw(to_string(&sel)))
}

/// Cartesian concatenation of two selector lists (each parent complex ++ each child complex).
fn cross(a: &[Complex], b: &[Complex]) -> Vec<Complex> {
    a.iter()
        .flat_map(|pa| b.iter().map(move |pb| {
            let mut v = pa.clone();
            v.extend(pb.iter().cloned());
            v
        }))
        .collect()
}

fn to_string(sel: &[Complex]) -> String {
    sel.iter().map(|c| c.join(" ")).collect::<Vec<_>>().join(", ")
}
