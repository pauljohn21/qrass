//! The SCSS value model — owned data that the pipeline moves and evaluates.
//!
//! A declaration/selector value is an `Expr` (see `expr.rs`) evaluated to a `Value`.
//! `Value::Raw` preserves a verbatim CSS fragment so plain CSS values stay
//! byte-identical (nothing is re-serialised unless it actually contained Sass
//! syntax); the typed variants exist for values Sass constructs produce.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sep {
    Space,
    Comma,
    Slash,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Verbatim CSS text (the default for anything Sass does not evaluate).
    Raw(String),
    Number(f64, String),
    Str { text: String, quoted: bool },
    Color(String),
    Bool(bool),
    Null,
    List(Vec<Value>, Sep),
}

impl Value {
    /// Serialise to CSS per the golden `toCss` form (not the inspect form).
    pub fn to_css(&self) -> String {
        match self {
            Value::Raw(s) => s.clone(),
            Value::Number(n, unit) => format_num(*n) + unit,
            Value::Str { text, quoted } => {
                if *quoted {
                    format!("\"{text}\"")
                } else {
                    text.clone()
                }
            }
            Value::Color(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            Value::List(items, sep) => {
                let join = match sep {
                    Sep::Space => " ",
                    Sep::Comma => ", ",
                    Sep::Slash => " / ",
                };
                items
                    .iter()
                    .map(Value::to_css)
                    .collect::<Vec<_>>()
                    .join(join)
            }
        }
    }

    /// Sass truthiness: only `false` and `null` are falsy.
    pub fn is_falsey(&self) -> bool {
        matches!(self, Value::Bool(false) | Value::Null)
    }
}

/// Numeric rendering: integers without a trailing `.0`, otherwise up to 10 sig
/// digits trimmed (rounded value is a golden concern to refine incrementally).
fn format_num(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n}");
        s
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_css())
    }
}
