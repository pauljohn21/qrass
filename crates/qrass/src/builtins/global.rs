//! Global (un-namespaced) builtins that are not tied to one module: the `if()`
//! ternary and the number-unit predicates `unit` / `unitless` / `comparable`, plus
//! `feature-exists`. These are pure — `if()` receives all three already-evaluated
//! arguments (Sass evaluates eagerly; there is no lazy arg here), which is the
//! documented behavior for the function form.

use super::{Or, Registry};
use crate::error::{ErrorKind, SassError};
use crate::value::{SassValue, convert_number};

pub(crate) fn register(r: &mut Registry) {
    r.def("if", 3, Some(3), if_fn);
    r.def("unit", 1, Some(1), unit);
    r.def("unitless", 1, Some(1), unitless);
    r.def("comparable", 2, Some(2), comparable);
    r.def("feature-exists", 1, Some(1), feature_exists);
}

fn if_fn(c: &super::Call) -> Or<SassValue> {
    Ok(if c.positional[0].is_truthy() {
        c.positional[1].clone()
    } else {
        c.positional[2].clone()
    })
}

fn as_number(v: &SassValue) -> Option<(f64, Option<String>)> {
    match v {
        SassValue::Number { value, unit } => Some((*value, unit.clone())),
        _ => None,
    }
}

fn unit(c: &super::Call) -> Or<SassValue> {
    match as_number(&c.positional[0]) {
        Some((_, u)) => Ok(SassValue::Str {
            text: u.unwrap_or_default(),
            quoted: true,
        }),
        None => Err(SassError::new(
            ErrorKind::Builtins,
            "unit() expects a number",
        )),
    }
}

fn unitless(c: &super::Call) -> Or<SassValue> {
    match as_number(&c.positional[0]) {
        Some((_, u)) => Ok(SassValue::Bool(u.is_none())),
        None => Err(SassError::new(
            ErrorKind::Builtins,
            "unitless() expects a number",
        )),
    }
}

fn comparable(c: &super::Call) -> Or<SassValue> {
    let a = as_number(&c.positional[0]);
    let b = as_number(&c.positional[1]);
    let ok = match (a, b) {
        (Some((_, ua)), Some((_, ub))) => ua == ub || is_convertible(ua, ub),
        _ => false,
    };
    Ok(SassValue::Bool(ok))
}

/// Two units are comparable when `convert_number` maps between them (px↔cm↔… etc).
fn is_convertible(from: Option<String>, to: Option<String>) -> bool {
    convert_number(1.0, from.as_deref(), to.as_deref()).is_some()
}

/// At-rule feature detection is not modeled; report every feature as unsupported,
/// which is the safe answer for `@supports`-guarding code paths.
fn feature_exists(c: &super::Call) -> Or<SassValue> {
    let _ = c;
    Ok(SassValue::Bool(false))
}
