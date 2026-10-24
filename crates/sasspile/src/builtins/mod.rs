//! Sass builtin function dispatch, split per domain so each family lives in its own
//! module (and stays small) instead of one growing `match` in `expr.rs`. `call` tries each
//! domain; the first that recognises the name answers. An unknown name → `Err`, so the
//! pipeline keeps the value verbatim and CSS-native functions (`url`/`calc`/`min`/`max`…)
//! are never mis-evaluated as Sass builtins.

mod color;
mod list;
mod map;
mod math;
mod meta;
mod string;

use crate::value::Value;

/// Resolve a builtin call. `Err` (unknown / bad args) makes the caller fall back to the
/// verbatim source text.
pub fn call(name: &str, args: &[Value]) -> Result<Value, String> {
    // generic `if(cond, then, else)` (also reachable as a lazy `Expr::If` from the parser)
    if name == "if" {
        return match args {
            [c, t, e] => Ok(if c.is_falsey() { e.clone() } else { t.clone() }),
            _ => Err(format!("if: {name}() expects 3 arguments")),
        };
    }
    // domain order matters only where bare names overlap; none collide across these domains.
    // `find_map` over the domain table (no imperative loop): first domain that owns `name` answers.
    match [math::call, string::call, list::call, map::call, color::call, meta::call]
        .into_iter()
        .find_map(|domain| domain(name, args))
    {
        Some(res) => res,
        None => Err(format!("{name}: unknown builtin")),
    }
}

/// Fold a non-empty value list down with `pick`; empty → `Err`. Shared by min/max.
pub(crate) fn reduce_vals(vs: &[Value], pick: fn(Value, Value) -> Value) -> Result<Value, String> {
    let mut it = vs.iter().cloned();
    match it.next() {
        None => Err("requires at least one argument".to_string()),
        Some(first) => Ok(it.fold(first, pick)),
    }
}

/// The textual payload of a string-ish value (unquoted).
pub(crate) fn str_of(v: &Value) -> String {
    match v {
        Value::Str { text, .. } => text.clone(),
        Value::Raw(s) => s.clone(),
        other => other.to_css(),
    }
}

/// Apply a numeric transform, preserving the operand's unit.
pub(crate) fn map_num(v: &Value, f: fn(f64) -> f64) -> Result<Value, String> {
    crate::expr::num(v).map(|(n, u)| Value::Number(f(n), u))
}

/// Require a unitless number (`sqrt`/`log`/`pow` reject units in Sass).
pub(crate) fn unitless(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n, u) if u.is_empty() => Ok(*n),
        Value::Number(_, u) => Err(format!("expected a unitless number, got unit \"{u}\"")),
        _ => Err("expected a number".to_string()),
    }
}
