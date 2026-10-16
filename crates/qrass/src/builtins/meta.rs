//! `meta.*` builtins (task 8.3) — the *pure* subset. `type-of` and `inspect`
//! serialize a value with no environment access; `keywords`/`keyword-get` operate
//! on the rest-argument map a function was handed. The environment-dependent meta
//! forms (`variable-exists`, `mixin-exists`, `function-exists`, `global-variable-
//! exists`, `content-exists`, `call`) are intercepted in `EvalState::eval_call`,
//! where the scope and registries are reachable — per D6 a registry builtin must
//! stay env-free.

use super::{Or, Registry};
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
/// CSS form, including its own quotes, exactly like the goldens).
fn inspect(c: &super::Call) -> Or<SassValue> {
    Ok(unquoted(c.positional[0].to_css()))
}

/// `keywords($args)`: the rest-argument map, returned unchanged.
fn keywords(c: &super::Call) -> Or<SassValue> {
    Ok(c.positional[0].clone())
}

fn keyword_get(c: &super::Call) -> Or<SassValue> {
    let name = match &c.positional[1] {
        SassValue::Str { text, .. } => text.clone(),
        other => other.to_css(),
    };
    let key = name.trim_start_matches('$');
    match &c.positional[0] {
        SassValue::Map { entries } => Ok(entries
            .iter()
            .find(|(k, _)| matches!(k, SassValue::Str { text, .. } if text == key))
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
