//! `string.*` builtins (task 8.3): quoting, case conversion and (1-based)
//! substring / index operations. Sass string functions treat numbers and colors
//! by their CSS text, so non-string arguments are coerced via `to_css`.

use super::{Or, Registry};
use crate::ast_expr::ListSep;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

pub(crate) fn register(r: &mut Registry) {
    r.nsp("string", "unquote", 1, Some(1), &["string"], unquote);
    r.nsp("string", "quote", 1, Some(1), &["string"], quote);
    r.nsp("string", "length", 1, Some(1), &["string"], length);
    r.nsp("string", "slice", 2, Some(3), &["string", "start-at", "end-at"], slice);
    r.nsp(
        "string",
        "split",
        2,
        Some(3),
        &["string", "separator", "limit"],
        split,
    );
    r.nsp("string", "index", 2, Some(2), &["string", "substring"], index);
    // `split` is a newer module-only function with no legacy global spelling; only
    // the `string.` namespace is claimed (see the round-25 CSS-filter lesson).
    r.nsp("string", "insert", 3, Some(3), &["string", "insert", "index"], insert);
    r.nsp("string", "to-upper-case", 1, Some(1), &["string"], upper);
    r.nsp("string", "to-lower-case", 1, Some(1), &["string"], lower);
    r.def("unquote", 1, Some(1), unquote);
    r.def("quote", 1, Some(1), quote);
    r.def("str-length", 1, Some(1), length);
    r.def("str-slice", 2, Some(3), slice);
    r.def("str-index", 2, Some(2), index);
    r.def("to-upper-case", 1, Some(1), upper);
    r.def("to-lower-case", 1, Some(1), lower);
}

/// The string content of an argument (CSS text for non-strings).
fn text(c: &super::Call, i: usize) -> String {
    match c.positional.get(i) {
        Some(SassValue::Str { text, .. }) => text.clone(),
        Some(v) => v.to_css(),
        None => String::new(),
    }
}

/// `string.split($string, $separator, $limit: null)`: a **bracketed comma list**
/// of substrings that inherit the source string's quote style. An empty
/// `$separator` splits on Unicode codepoints; a `$limit` caps the number of
/// splits (so the remainder stays whole). An empty `$string` yields the empty
/// list, and a separator that never matches yields a one-element list holding
/// the whole string. Arguments are strictly typed to the goldens' dart-shaped
/// errors (`$string: … is not a string.`, `$limit: … is not an int.`, etc.).
fn split(c: &super::Call) -> Or<SassValue> {
    let (hay, quoted) = match c.positional.first() {
        Some(SassValue::Str { text, quoted }) => (text.clone(), *quoted),
        Some(other) => return Err(not_a("string", other, "a string")),
        None => return Err(not_a("string", &SassValue::Null, "a string")),
    };
    let sep = match c.positional.get(1) {
        Some(SassValue::Str { text, .. }) => text.clone(),
        Some(other) => return Err(not_a("separator", other, "a string")),
        None => return Err(not_a("separator", &SassValue::Null, "a string")),
    };
    // `$limit` is optional; a present value must be a unitless integer >= 1.
    let limit: Option<usize> = match c.positional.get(2) {
        None | Some(SassValue::Null) => None,
        Some(v @ SassValue::Number { value, unit: None }) => {
            if value.fract() != 0.0 {
                return Err(not_a("limit", v, "an int"));
            }
            if *value < 1.0 {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!("$limit: Must be 1 or greater, was {}.", trim_int(*value)),
                ));
            }
            Some(*value as usize)
        }
        Some(other) => return Err(not_a("limit", other, "a number")),
    };

    let parts: Vec<String> = if hay.is_empty() {
        Vec::new()
    } else if sep.is_empty() {
        hay.chars().map(|ch| ch.to_string()).collect()
    } else {
        match limit {
            // `$limit` counts separator matches consumed, so `k` splits → `k + 1`
            // pieces: `splitn(k + 1)` keeps the remainder whole.
            Some(k) => hay.splitn(k + 1, sep.as_str()).map(str::to_string).collect(),
            None => hay.split(sep.as_str()).map(str::to_string).collect(),
        }
    };
    Ok(SassValue::List {
        items: parts
            .into_iter()
            .map(|t| SassValue::Str { text: t, quoted })
            .collect(),
        sep: ListSep::Comma,
        bracketed: true,
    })
}

/// A dart-shaped `${name}: {value} is not {ty}.` argument-type error.
fn not_a(name: &str, value: &SassValue, ty: &str) -> SassError {
    SassError::new(
        ErrorKind::Builtins,
        format!("${name}: {} is not {ty}.", value.to_css()),
    )
}

/// Render a whole float without a trailing `.0` (Sass prints `0`, not `0.0`).
fn trim_int(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn unquoted(s: String) -> SassValue {
    SassValue::Str {
        text: s,
        quoted: false,
    }
}

fn unquote(c: &super::Call) -> Or<SassValue> {
    Ok(unquoted(text(c, 0)))
}

fn quote(c: &super::Call) -> Or<SassValue> {
    Ok(SassValue::Str {
        text: text(c, 0),
        quoted: true,
    })
}

fn length(c: &super::Call) -> Or<SassValue> {
    Ok(SassValue::Number {
        value: text(c, 0).chars().count() as f64,
        unit: None,
    })
}

/// `str-slice($string, $start-at, $end-at: -1)`: 1-based inclusive, negatives
/// count from the end.
fn slice(c: &super::Call) -> Or<SassValue> {
    let chars: Vec<char> = text(c, 0).chars().collect();
    let n = chars.len() as i64;
    let idx = |v: Option<usize>, default: i64| -> i64 {
        match v
            .and_then(|i| c.positional.get(i))
            .and_then(|a| a.as_number())
        {
            Some((x, _)) => {
                let x = x as i64;
                if x == 0 {
                    1
                } else if x > 0 {
                    x
                } else {
                    n + x + 1
                }
            }
            None => default,
        }
    };
    let start = idx(Some(1), 1);
    let end = idx(Some(2), -1);
    let lo = start.max(1).min(n + 1) as usize - 1;
    let hi = if end < 0 { n } else { end.min(n) } as usize;
    let out: String = chars[lo..hi.max(lo)].iter().collect();
    Ok(unquoted(out))
}

fn index(c: &super::Call) -> Or<SassValue> {
    let hay = text(c, 0);
    let needle = text(c, 1);
    Ok(match hay.find(&needle) {
        Some(byte) => SassValue::Number {
            value: hay[..byte].chars().count() as f64 + 1.0,
            unit: None,
        },
        None => SassValue::Null,
    })
}

/// `str-insert($string, $insert, $index)`: 1-based Dart-style insertion point.
/// Sass indexes by Unicode codepoint (a `Vec<char>` matches exactly: an emoji is
/// one char even when UTF-16 needs two units, while a base+combining pair is two
/// chars). Out-of-range indices clamp to the nearest edge; index 0 inserts at the
/// front. The result mirrors `$string`'s quoting.
fn insert(c: &super::Call) -> Or<SassValue> {
    let chars: Vec<char> = text(c, 0).chars().collect();
    let n = chars.len() as i64;
    let insertion = text(c, 1);
    let index = c
        .positional
        .get(2)
        .and_then(|a| a.as_number())
        .map(|(v, _)| v as i64)
        .unwrap_or(0);
    let pos: usize = if index > 0 {
        (index - 1).min(n)
    } else if index < 0 {
        (n + 1 + index).max(0)
    } else {
        0
    } as usize;
    let mut out: String = chars[..pos].iter().collect();
    out.push_str(&insertion);
    out.extend(chars[pos..].iter());
    let quoted = matches!(
        c.positional.first(),
        Some(SassValue::Str { quoted: true, .. })
    );
    Ok(SassValue::Str { text: out, quoted })
}

fn upper(c: &super::Call) -> Or<SassValue> {
    Ok(unquoted(text(c, 0).to_uppercase()))
}

fn lower(c: &super::Call) -> Or<SassValue> {
    Ok(unquoted(text(c, 0).to_lowercase()))
}
