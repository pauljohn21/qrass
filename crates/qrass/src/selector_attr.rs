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
    // leftmost position that starts with any operator token, taking the table
    // order within that position (the first complete match wins)
    (from..chars.len())
        .flat_map(|i| {
            let rest: String = chars[i..].iter().collect();
            OPERATORS
                .iter()
                .find(|(_, token)| rest.starts_with(*token))
                .map(|(op, token)| (*op, i + token.chars().count()))
        })
        .next()
}

/// Parse one `[...]` group's inner text (without the brackets) into an
/// [`Attribute`], preserving the verbatim `raw` form for canonical re-rendering.
pub fn parse_attribute(inner: &str) -> Attribute {
    let chars: Vec<char> = inner.chars().collect();
    // Leading optional namespace on the attribute name is not consumed here —
    // `[ns|attr]` keeps the whole `ns|attr` as the name, matching how the raw
    // text is retained.
    let name_end = (0..chars.len())
        .take_while(|&k| !matches!(chars[k], '=' | '~' | '|' | '^' | '$' | '*'))
        .count();
    let name: String = chars[..name_end].iter().collect::<String>().trim().to_string();
    let raw = format!("[{}]", inner.trim());
    // no operator means a bare `[name]` group
    match find_operator(&chars, name_end) {
        None => Attribute {
            raw,
            name,
            op: None,
            value: None,
            modifier: None,
        },
        Some((op, value_start)) => {
            let tail: String = chars[value_start..]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            // The value may be quoted, or a bare identifier; an optional trailing
            // `i`/`s` modifier is separated from the value by whitespace.
            let (value, modifier) = split_value_modifier(&tail);
            Attribute {
                raw,
                name,
                op: Some(op),
                value,
                modifier,
            }
        }
    }
}

/// Split `[...=value i]` into its (unquoted) value and optional flag char.
fn split_value_modifier(tail: &str) -> (Option<String>, Option<char>) {
    let chars: Vec<char> = tail.chars().collect();
    match chars.first().copied() {
        None => (None, None),
        Some(first) => {
            // a quoted value runs through its closing quote (an unterminated one
            // to the end); a bare value stops at the first whitespace
            let value_end = match first {
                q @ ('"' | '\'') => {
                    let run = (1..chars.len())
                        .take_while(|&k| chars[k] != q)
                        .count();
                    match chars.get(run + 1) {
                        Some(c) if *c == q => run + 2,
                        _ => chars.len(),
                    }
                }
                _ => (0..chars.len())
                    .take_while(|&k| !chars[k].is_whitespace())
                    .count(),
            };
            let value_raw: String = chars[..value_end].iter().collect();
            let modifier = chars[value_end..]
                .iter()
                .collect::<String>()
                .trim()
                .chars()
                .next()
                .map(|c| c.to_ascii_lowercase());
            (Some(unquote(&value_raw)), modifier)
        }
    }
}

/// Strip surrounding single or double quotes from a token.
pub fn unquote(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let quoted = chars.len() >= 2
        && chars[0] == chars[chars.len() - 1]
        && matches!(chars[0], '"' | '\'');
    match quoted {
        true => chars[1..chars.len() - 1].iter().collect(),
        false => s.to_string(),
    }
}

/// Split a selector list on top-level commas (parens/brackets protected).
/// Shared with the parser's `:is(...)`/`:has(...)` argument splitting.
pub fn split_top_commas(raw: &str) -> Vec<String> {
    // fold the char stream over (finished parts, open run, nesting depth)
    let (parts, cur, _) = raw.chars().fold(
        (Vec::new(), String::new(), 0i32),
        |(mut parts, mut cur, depth), ch| match ch {
            '(' | '[' | ')' | ']' => {
                cur.push(ch);
                let step = matches!(ch, '(' | '[') as i32 * 2 - 1;
                (parts, cur, depth + step)
            }
            ',' if depth <= 0 => {
                parts.push(cur.trim().to_string());
                (parts, String::new(), depth)
            }
            _ => {
                cur.push(ch);
                (parts, cur, depth)
            }
        },
    );
    let last = cur.trim().to_string();
    match parts.is_empty() || !last.is_empty() {
        true => parts.into_iter().chain([last]).collect(),
        false => parts,
    }
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
    match (super_.name == sub.name, super_.op.is_none()) {
        // the attribute name has to agree at all
        (false, _) => false,
        // A presence test `[c]` covers any `[c...]` on the same attribute.
        (true, true) => true,
        // A valued operator covers only an identical one (name, operator, value
        // and case-modifier all equal; a `^=` does not subsume an `=`).
        (true, false) => {
            super_.op == sub.op
                && super_.value == sub.value
                && super_.modifier == sub.modifier
        }
    }
}
