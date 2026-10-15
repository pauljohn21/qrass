//! Expanded CSS serialization (task group 9): the output-node layer that turns
//! evaluated declarations, comments, and nested at-rule blocks into indented text.
//!
//! The eval fold streams owned chunk `String`s (the reactive "event → String"
//! protocol the render stage reduces); this module owns *how a block looks* —
//! two-space indent, block order = source order, `!important` already baked into
//! the value string by `SassValue::to_css`, and loud `/* … */` comments preserved
//! while silent `//` comments are dropped upstream by the lexer. Statement code
//! therefore stays free of formatting concerns.
//!
//! Value-level numeric/color formatting lives in [`crate::value`] (`to_css`,
//! `format_number`); the token-passthrough helper [`number_to_string`] (raw
//! literals kept inside selectors and at-rule heads) is consolidated here.

/// One entry in a rule body, kept in source order so comments interleave with
/// declarations exactly as they appeared in the stylesheet.
#[derive(Clone, Debug)]
pub(crate) enum Item {
    /// A `prop: value;` declaration (`value` already carries any `!important`).
    Decl { prop: String, value: String },
    /// A loud `/* … */` comment. Silent `//` comments never reach this layer.
    Comment(String),
}

/// Format one qualified rule (`selectors { items }`). Callers are responsible for
/// dropping empty rules; a rule whose only content is comments still renders.
pub(crate) fn render_rule(selectors: &[String], items: &[Item]) -> String {
    // dart-sass expanded puts each comma-separated selector on its own line, the
    // `{` trailing the last one; the shared body indent keeps continuation lines
    // aligned, and a wrapping `@media`/`@supports` shifts them uniformly.
    let sel = selectors
        .iter()
        .map(|s| normalize_selector(s))
        .collect::<Vec<_>>()
        .join(",\n");
    let body = items
        .iter()
        .map(|it| match it {
            Item::Decl { prop, value } => format!("  {prop}: {value};"),
            Item::Comment(text) => format!("  {}", text.trim()),
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{sel} {{\n{body}\n}}")
}

/// Normalize one selector to dart-sass output form. Attribute-selector values
/// that are valid CSS identifiers are emitted unquoted (`[a^='top']` → `[a^=top]`);
/// quoted values that need quoting and bare values pass through unchanged. Only
/// the contents of `[…]` blocks are rewritten so pseudo-class parentheses etc.
/// are untouched.
fn normalize_selector(sel: &str) -> String {
    let chars: Vec<char> = sel.chars().collect();
    let mut out = String::with_capacity(sel.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            let mut j = i + 1;
            let mut buf = String::new();
            while j < chars.len() && chars[j] != ']' {
                buf.push(chars[j]);
                j += 1;
            }
            out.push('[');
            out.push_str(&normalize_attr(&buf));
            if j < chars.len() {
                out.push(']');
            }
            i = j + 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Within one `[ … ]` body, drop the quotes around an `=` value when that value
/// is a valid CSS identifier (matching dart-sass selector serialization).
fn normalize_attr(body: &str) -> String {
    let Some(eq) = body.find('=') else {
        return body.to_string();
    };
    let (lhs, rhs) = body.split_at(eq); // `rhs` starts with the `=`
    let value = rhs[1..].trim();
    let v: Vec<char> = value.chars().collect();
    let quoted = v.len() >= 2
        && (v[0] == '\'' || v[0] == '"')
        && v[0] == *v.last().unwrap();
    if quoted {
        let content: String = v[1..v.len() - 1].iter().collect();
        if is_css_ident(&content) {
            return format!("{lhs}={content}");
        }
    }
    body.to_string()
}

/// Whether `s` is a bare CSS identifier safe to emit unquoted in an attribute
/// selector: non-empty, not starting with a digit, and composed only of
/// letters/digits/`-`/`_`/non-ASCII (a leading `-` must be followed by an
/// identifier char or another `-`).
fn is_css_ident(s: &str) -> bool {
    let c: Vec<char> = s.chars().collect();
    if c.is_empty() {
        return false;
    }
    let ident_cont = |ch: char| ch.is_alphanumeric() || ch == '-' || ch == '_' || !ch.is_ascii();
    if c[0] == '-' {
        if c.len() < 2 {
            return false;
        }
        let n = c[1];
        if !(n.is_alphabetic() || n == '_' || n == '-' || !n.is_ascii()) {
            return false;
        }
    } else if c[0].is_ascii_digit() || !(c[0].is_alphabetic() || c[0] == '_' || !c[0].is_ascii())
    {
        return false;
    }
    c[1..].iter().all(|&ch| ident_cont(ch))
}

/// Indent every line of each child block two spaces and wrap the group in a
/// single `header { … }` at-rule, so sibling rules under one `@media`/`@supports`
/// /`@keyframes` share a merged block.
pub(crate) fn render_at_block(header: &str, children: &[String]) -> String {
    let inner = children
        .iter()
        .flat_map(|b| b.lines())
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{header} {{\n{inner}\n}}")
}

/// A top-level loud comment, emitted verbatim (no indentation).
pub(crate) fn render_comment(text: &str) -> String {
    text.trim().to_string()
}

/// Format a number for token passthrough (raw literals preserved inside selectors,
/// at-rule heads, and interpolated text): no exponent, no trailing zeros.
pub(crate) fn number_to_string(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let mut s = format!("{v}");
    if s.contains('e') || s.contains('E') {
        s = format!("{v:.10}");
    }
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}
