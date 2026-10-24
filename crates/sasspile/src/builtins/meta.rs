//! Sass `meta.*` reflection — this step covers `meta.type-of` (pure `Value` introspection).
//! The rest of the domain (`keywords`, `get-function`/`call`, `*-{function,mixin,variable}-exists`,
//! `module-*`, `accepts-*`) depends on the callable registry / arglist that arrive with the
//! pipeline (tasks 5, 8), so they are intentionally deferred.

use crate::value::Value;

/// `Some` when this domain owns `name`.
pub fn call(name: &str, args: &[Value]) -> Option<Result<Value, String>> {
    match (name, args) {
        ("meta.type-of" | "type-of", [v]) => Some(Ok(Value::Str {
            text: type_of(v).to_string(),
            quoted: false,
        })),
        _ => None,
    }
}

/// Sass `type-of` result for a value. `Raw` (verbatim text: unquoted strings, identifiers)
/// reports as `"string"`; typed variants map directly.
fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Number(..) => "number",
        Value::Str { .. } => "string",
        Value::Color(_) => "color",
        Value::Bool(_) => "bool",
        Value::Null => "null",
        Value::List(..) => "list",
        Value::Map(_) => "map",
        Value::Raw(_) => "string",
    }
}
