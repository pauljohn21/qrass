//! Attribute-selector helpers (Round 23 Phase 1): turn a verbatim `[...]` group
//! into the structured [`Attribute`] the AST carries, and decide when one
//! attribute selector subsumes another — the core of `is-superselector` on
//! `[c=d]`-style selectors.
//!
//! Kept separate from the AST and the parser because both need it and the logic
//! is self-contained: operator classification, top-level comma splitting,
//! namespace splitting, and unquoting.

use crate::selector_ast::{AttrOp, Attribute};

/// Attribute operators, longest-first so `^=`/`$=`/`*=`/`~=`/`|=` win over a bare
/// `=`. Width is derived from the string, not hard-coded, to keep the table and
/// matcher in sync.
const OPERATORS: &[(AttrOp, &str)] = &[
    (AttrOp::Includes, "~="),
    (AttrOp::DashMatch, "|="),
    (AttrOp::Prefix, "^="),
    (AttrOp::Suffix, "$="),
    (AttrOp::Substring, "*="),
    (AttrOp::Equals, "="),
];

/// Classify an operator token (e.g. `"~="`) into its [`AttrOp`].
pub fn classify(op: &str) -> Option<AttrOp> {
    OPERATORS.iter().find(|(_, s)| *s == op).map(|(k, _)| *k)
}

/// Find the operator inside the body of a `[name op value]` group, and where it
/// sits. Returns `(op, byte index just past the operator)`. A trailing `=` of a
/// doubled `==` must not be swallowed as the operator for the first `=`; we scan
/// left to right and take the first complete operator, which is why a lone `=`
/// only matches when not followed by another operator char that would extend it.
pub fn find_operator(chars: &[char], from: usize) -> Option<(AttrOp, usize)> {
    let mut i = from;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        for (op, token) in OPERATORS {
            if rest.starts_with(token) {
                // Guard `a==b`: the `|=`/`~=`/... two-char forms are unambiguous,
                // but a bare `=` immediately followed by another `=` should be read
                // as `=` with an empty-ish value, not skipped. We accept the first
                // (longest) match, so this is well-defined.
                return Some((*op, i + token.chars().count()));
            }
        }
        i += 1;
    }
    None
}

/// Parse one `[...]` group's inner text (without the brackets) into an
/// [`Attribute`], preserving the verbatim `raw` form for canonical re-rendering.
pub fn parse_attribute(inner: &str) -> Attribute {
    let chars: Vec<char> = inner.chars().collect();
    // Leading optional namespace on the attribute name is not consumed here —
    // `[ns|attr]` keeps the whole `ns|attr` as the name, matching how the raw
    // text is retained.
    let mut name_end = 0;
    while name_end < chars.len() && !matches!(chars[name_end], '=' | '~' | '|' | '^' | '$' | '*') {
        name_end += 1;
    }
    let name: String = chars[..name_end].iter().collect::<String>().trim().to_string();

    let op_at = name_end;
    let (op, value_start) = match find_operator(&chars, op_at) {
        Some(pair) => pair,
        None => {
            return Attribute {
                raw: format!("[{}]", inner.trim()),
                name,
                op: None,
                value: None,
                modifier: None,
            };
        }
    };

    let tail: String = chars[value_start..]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();

    // The value may be quoted, or a bare identifier; an optional trailing
    // `i`/`s` modifier is separated from the value by whitespace.
    let (value, modifier) = split_value_modifier(&tail);

    Attribute {
        raw: format!("[{}]", inner.trim()),
        name,
        op: Some(op),
        value,
        modifier,
    }
}

/// Split `[...=value i]` into its (unquoted) value and optional flag char.
fn split_value_modifier(tail: &str) -> (Option<String>, Option<char>) {
    let chars: Vec<char> = tail.chars().collect();
    if chars.is_empty() {
        return (None, None);
    }
    let mut end = 0;
    if chars[0] == '"' || chars[0] == '\'' {
        let quote = chars[0];
        end = 1;
        while end < chars.len() && chars[end] != quote {
            end += 1;
        }
        // include closing quote if present
        if end < chars.len() {
            end += 1;
        }
    } else {
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }
    }
    let value_raw: String = chars[..end].iter().collect();
    let value = Some(unquote(&value_raw));

    let rest: String = chars[end..].iter().collect();
    let modifier = rest
        .trim()
        .chars()
        .next()
        .map(|c| c.to_ascii_lowercase());
    (value, modifier)
}

/// Strip surrounding single or double quotes from a token.
pub fn unquote(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() >= 2 && (chars[0] == chars[chars.len() - 1])
        && (chars[0] == '"' || chars[0] == '\'')
    {
        return chars[1..chars.len() - 1].iter().collect();
    }
    s.to_string()
}

/// Split a selector list on top-level commas (parens/brackets protected).
/// Shared with the parser's `:is(...)`/`:has(...)` argument splitting.
pub fn split_top_commas(raw: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in raw.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth <= 0 => {
                parts.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(ch),
        }
    }
    let last = cur.trim().to_string();
    if !last.is_empty() || parts.is_empty() {
        parts.push(last);
    }
    parts
}

/// Split a qualified name `ns|local` into `(namespace, local)`. A leading `|`
/// means the empty namespace; `*|` a wildcard; no bar means bare.
pub fn split_ns(text: &str) -> (Option<String>, String) {
    match text.find('|') {
        Some(idx) => {
            let ns = &text[..idx];
            let local = text[idx + 1..].to_string();
            let ns = if ns.is_empty() || ns == "*" {
                None
            } else {
                Some(ns.to_string())
            };
            (ns, local)
        }
        None => (None, text.to_string()),
    }
}

/// Whether attribute selector `super_` subsumes `sub`: `[c]` is a superselector
/// of `[c=d]`; a valued operator is only a superselector of another when the
/// name, operator, value and case-modifier all match (a `^=` does not subsume an
/// `=` even for the same prefix, per the is_superselector/attribute goldens).
pub fn covers(super_: &Attribute, sub: &Attribute) -> bool {
    if super_.name != sub.name {
        return false;
    }
    // A presence test `[c]` covers any `[c...]` on the same attribute.
    if super_.op.is_none() {
        return true;
    }
    // A valued operator covers only an identical one.
    super_.op == sub.op
        && super_.value == sub.value
        && super_.modifier == sub.modifier
}
