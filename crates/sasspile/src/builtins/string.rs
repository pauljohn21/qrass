//! Sass `string.*` builtins (and the bare legacy aliases). Bare `length` is intentionally
//! NOT handled here — it is ambiguous (list vs string) and dispatched by the list module.

use super::str_of;
use crate::value::Value;

/// `Some` when this domain owns `name`.
pub fn call(name: &str, args: &[Value]) -> Option<Result<Value, String>> {
    match (name, args) {
        ("to-upper-case" | "string.to-upper-case", [a]) => Some(quoted(&str_of(a).to_uppercase())),
        ("to-lower-case" | "string.to-lower-case", [a]) => Some(quoted(&str_of(a).to_lowercase())),
        ("quote" | "string.quote", [a]) => Some(quoted(&str_of(a))),
        ("unquote" | "string.unquote", [a]) => {
            Some(Ok(Value::Str { text: str_of(a), quoted: false }))
        }
        ("string.length", [a]) => Some(Ok(Value::Number(str_of(a).chars().count() as f64, String::new()))),
        _ => None,
    }
}

fn quoted(text: &str) -> Result<Value, String> {
    Ok(Value::Str { text: text.to_string(), quoted: true })
}
