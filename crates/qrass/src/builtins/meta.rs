//! `meta.*` builtins (task 8.3) — the *pure* subset. `type-of` and `inspect`
//! serialize a value with no environment access; `keywords`/`keyword-get` operate
//! on the rest-argument map a function was handed. The environment-dependent meta
//! forms (`variable-exists`, `mixin-exists`, `function-exists`, `global-variable-
//! exists`, `content-exists`, `call`) are intercepted in `EvalState::eval_call`,
//! where the scope and registries are reachable — per D6 a registry builtin must
//! stay env-free.

use super::{Or, Registry};
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

pub(crate) fn register(r: &mut Registry) {
    r.nsp("meta", "type-of", 1, Some(1), &["value"], type_of);
    r.nsp("meta", "inspect", 1, Some(1), &["value"], inspect);
    r.ns("meta", "keywords", 1, Some(1), keywords);
    r.ns("meta", "keyword-get", 2, Some(2), keyword_get);
    // Legacy global spellings.
    r.def("type-of", 1, Some(1), type_of);
    r.def("inspect", 1, Some(1), inspect);
    r.def("keywords", 1, Some(1), keywords);
    r.def("keyword-get", 2, Some(2), keyword_get);
}

fn type_of(c: &super::Call) -> Or<SassValue> {
    Ok(unquoted(c.positional[0].type_name().to_string()))
}

/// `inspect` yields the value's source text, unquoted (a string arg prints its
/// CSS form, including its own quotes, exactly like the goldens). Source form
/// parenthesizes nested lists so their structure round-trips (`to_source`), unlike
/// the flat CSS serialization. An unbracketed empty list carries no separator
/// text, so dart's source form is the empty paren `()` — `to_source` would
/// otherwise collapse a top-level one to "".
fn inspect(c: &super::Call) -> Or<SassValue> {
    let text = match &c.positional[0] {
        SassValue::List {
            items,
            bracketed: false,
            ..
        } if items.is_empty() =>
        {
            "()".to_string()
        }
        other => other.to_source(),
    };
    Ok(unquoted(text))
}

/// `keywords($args)`: the named part of a rest-argument ArgList, as a map. A value
/// that is not an ArgList (`keywords == None`) is a golden error we surface as-is.
fn keywords(c: &super::Call) -> Or<SassValue> {
    match &c.positional[0] {
        SassValue::List {
            keywords: Some(kw),
            ..
        } => {
            let entries = kw
                .iter()
                .map(|(k, v)| (unquoted(k.clone()), v.clone()))
                .collect();
            Ok(SassValue::Map { entries })
        }
        other => Err(SassError::new(
            ErrorKind::Eval,
            format!("$args: {} is not an argument list.", arg_repr(other)),
        )),
    }
}

/// Render an argument value the way the goldens' `… is not an argument list.`
/// message spells it: an unbracketed list is parenthesized, other values use CSS.
fn arg_repr(v: &SassValue) -> String {
    match v {
        SassValue::List {
            bracketed: false, ..
        } => format!("({})", v.to_css()),
        other => other.to_css(),
    }
}

fn keyword_get(c: &super::Call) -> Or<SassValue> {
    let name = match &c.positional[1] {
        SassValue::Str { text, .. } => text.clone(),
        other => other.to_css(),
    };
    let key = name.trim_start_matches('$').replace('_', "-");
    match &c.positional[0] {
        // An ArgList: look the name up in its keyword part.
        SassValue::List {
            keywords: Some(kw), ..
        } => Ok(kw
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(SassValue::Null)),
        // Legacy map spelling still supported.
        SassValue::Map { entries } => Ok(entries
            .iter()
            .find(|(k, _)| matches!(k, SassValue::Str { text, .. } if text == &key))
            .map(|(_, v)| v.clone())
            .unwrap_or(SassValue::Null)),
        _ => Ok(SassValue::Null),
    }
}

fn unquoted(s: String) -> SassValue {
    SassValue::Str {
        text: s,
        quoted: false,
    }
}
