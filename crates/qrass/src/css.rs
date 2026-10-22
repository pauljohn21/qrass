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
    /// A verbatim line (e.g. a nested unknown at-rule like `@b c;`) kept as-is.
    Raw(String),
}

/// Format one qualified rule (`selectors { items }`). Callers are responsible for
/// dropping empty rules; a rule whose only content is comments still renders.
pub(crate) fn render_rule(selectors: &[String], items: &[Item]) -> String {
    // dart's bogus-combinators tolerance (`css/selector/combinator/*` ~27
    // sections): a selector leading with a combinator (`+ a`, `:is(> a)`) or
    // carrying adjacent combinators (`a > + b`) is INVALID — dart drops the
    // selector (empty output when none survive) and warns. The shared
    // lightningcss canonicalization lets an empty rule fall through harmlessly.
    let selectors: Vec<String> = selectors
        .iter()
        .filter(|s| {
            let bogus = split_top_commas(s).iter().all(|seg| is_bogus_combinator(seg));
            if bogus {
                tracing::warn!("DEPRECATION WARNING [bogus-combinators]: The selector \"{s}\" is invalid.");
            }
            !bogus
        })
        .cloned()
        .collect();
    if selectors.is_empty() {
        return String::new();
    }
    // the goldens' expanded output puts each comma-separated selector on its own line, the
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
            Item::Raw(line) => format!("  {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{sel} {{\n{body}\n}}")
}

/// Normalize one selector to the goldens output form. Attribute-selector values
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
/// is a valid CSS identifier (matching the goldens selector serialization).
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

/// Split a selector list on top-level commas (paren/bracket depth aware).
pub(crate) fn split_top_commas(sel: &str) -> Vec<&str> {
    let bytes: Vec<char> = sel.chars().collect();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in bytes.iter().enumerate() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&sel[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&sel[start..]);
    parts
}

/// One selector segment is a bogus combinator form when it (a) starts with
/// `+`, `~` or `>`, (b) carries two combinators adjacent (whitespace between
/// them allowed), or (c) opens a parenthesis straight onto a combinator
/// (`:is(+ a)`, `:has(> a)`).
/// Does this selector segment carry a *dropped* bogus combinator form?
/// Golden matrix (`css/selector/combinator/*`):
/// * top-level leading (`+ a`, `> a` …) — KEPT with a warning;
/// * adjacent combinators at any depth (`a > + b`) — dropped;
/// * trailing combinator (`a +`) — dropped;
/// * combinator straight after a parenthesis (`:is(+ a)`, `:has( > a)` …) —
///   dropped everywhere EXCEPT inside `:has(…)` (relative selectors are the
///   spec-sanctioned exception, `has.hrx` keeps `:has(+ a)` verbatim).
pub(crate) fn is_bogus_combinator(seg: &str) -> bool {
    let is_comb = |c: char| matches!(c, '+' | '~' | '>');
    let chars: Vec<char> = seg.trim().chars().collect();
    let mut prev_comb = false;
    let mut depth = 0usize;
    let mut expect_paren_head: Vec<bool> = vec![false]; // per open paren: combinator-next position armed
    let mut has_exempt: Vec<bool> = vec![false];
    let mut seen: String = String::new();
    let mut non_ws_seen = false; // first real token of the whole segment (leading is kept)
    for c in &chars {
        match c {
            '(' | '[' => {
                depth += 1;
                expect_paren_head.push(true);
                // the pseudo directly before the paren decides the exemption
                has_exempt.push(seen.ends_with(":has"));
                prev_comb = false;
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                expect_paren_head.pop();
                has_exempt.pop();
                prev_comb = false;
                non_ws_seen = true;
            }
            ' ' | '\t' => {}
            &c if is_comb(c) => {
                if depth > 0
                    && *expect_paren_head.last().unwrap_or(&false)
                    && !*has_exempt.last().unwrap_or(&false)
                {
                    return true; // `:is(+ a)` / `( +a`; `:has(+ a)` is exempt
                }
                if prev_comb {
                    return true; // adjacent `> +`
                }
                prev_comb = true;
                non_ws_seen = true;
                *expect_paren_head.last_mut().unwrap_or(&mut false) = false;
            }
            '&' => {
                prev_comb = false;
                non_ws_seen = true;
                *expect_paren_head.last_mut().unwrap_or(&mut false) = false;
            }
            ':' => {
                // scan the pseudo name for `has` to exempt its parentheses
                prev_comb = false;
                non_ws_seen = true;
                *expect_paren_head.last_mut().unwrap_or(&mut false) = false;
            }
            c => {
                if !c.is_whitespace() {
                    prev_comb = false;
                    non_ws_seen = true;
                    *expect_paren_head.last_mut().unwrap_or(&mut false) = false;
                }
                let _ = non_ws_seen;
            }
        }
        seen.push(*c);
    }
    // trailing combinator: the segment ends on `+`/`~`/`>`
    prev_comb && chars.last().is_some_and(|c| is_comb(*c))
}

/// Re-serialize an `@supports`/`@media` query head like dart does: a
/// *declaration-shaped* group (top-level `:` before any paren, no nesting)
/// drops loud comments, trims its edges and never leaves a space before the
/// `:`; every other form (`a(/**/ b)`, `selector(…)`, `(a b)`) is kept
/// VERBATIM (`css/supports/comment` `function/*`, `syntax/*`).
pub(crate) fn tidy_condition(q: &str) -> String {
    let chars: Vec<char> = q.chars().collect();
    let mut out: Vec<char> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '(' => {
                out.push('(');
                i += 1;
                let start = i;
                let mut depth = 1usize;
                while i < chars.len() && depth > 0 {
                    match chars[i] {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        i += 1;
                    }
                }
                let inner: String = chars[start..i].iter().collect();
                let inner = if is_declaration_group(&inner) {
                    let inner = strip_loud_comments(&inner);
                    let mut t = inner.trim_matches(' ').trim_matches('\t').to_string();
                    if let Some(at) = t.find(':') {
                        let head = t[..at].trim_end();
                        t = format!("{}{}", head, &t[at..]);
                    }
                    t
                } else {
                    inner
                };
                out.extend(inner.chars());
                if i < chars.len() {
                    out.push(')');
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out.into_iter().collect()
}

/// Top-level `:` before any `(` and no parentheses at all inside.
fn is_declaration_group(inner: &str) -> bool {
    for c in inner.chars() {
        match c {
            '(' => return false,
            ':' => return true,
            _ => {}
        }
    }
    false
}

fn strip_loud_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find("/*") {
        out.push_str(&rest[..at]);
        let close = rest[at..].find("*/").map(|c| at + c + 2).unwrap_or(rest.len());
        rest = &rest[close.min(rest.len())..];
    }
    out.push_str(rest);
    out
}
