//! The owned value model that flows down the stream. A value is either `Raw` verbatim
//! CSS (so plain values stay byte-identical) or a typed variant Sass constructs; CSS is
//! produced by `to_css` only when a value is finally demanded (lazy).

use std::fmt;

/// Separator for a `Value::List`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sep {
    Space,
    Comma,
    Slash,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// verbatim CSS text (default for anything Sass does not evaluate)
    Raw(String),
    Number(f64, String),
    Str { text: String, quoted: bool },
    Color(String),
    Bool(bool),
    Null,
    List(Vec<Value>, Sep),
    /// an ordered Sass map: `(key: value, …)`; insertion order is preserved
    Map(Vec<(Value, Value)>),
}

impl Value {
    /// Serialise to CSS.
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
                items.iter().map(Value::to_css).collect::<Vec<_>>().join(join)
            }
            Value::Map(pairs) => {
                let body = pairs
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.to_css(), v.to_css()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({body})")
            }
        }
    }

    /// Sass truthiness: only `false` / `null` (and empty) are falsy.
    pub fn is_falsey(&self) -> bool {
        match self {
            Value::Bool(false) | Value::Null => true,
            Value::Raw(s) => {
                let t = s.trim();
                t.is_empty() || t == "false" || t == "null"
            }
            _ => false,
        }
    }

    /// Parse a numeric literal with an optional unit (`"10px"` → `Number(10,"px")`,
    /// `"-3"` → `Number(-3,"")`, `"50%"` → `Number(50,"%")`); `None` otherwise.
    pub fn parse_number(s: &str) -> Option<Value> {
        let t = s.trim();
        let end = t
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
            .unwrap_or(t.len());
        let (num, unit) = t.split_at(end);
        num.parse::<f64>().ok().map(|n| Value::Number(n, unit.to_string()))
    }
}

/// Integers without a trailing `.0`; otherwise the shortest round-trip form.
fn format_num(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_css())
    }
}
