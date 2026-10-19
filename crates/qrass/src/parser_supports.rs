//! `@supports` condition grammar validation, split from `parser_at` for the
//! 500-line budget (round 71). Follows the crate-wide conservative-check
//! doctrine: recognize only the invalid spellings the
//! `css/supports/error.hrx` goldens pin, and let every other shape keep
//! today's verbatim passthrough untouched.
//!
//! Golden-pinned rules (input → message):
//! * `@supports a`, `@supports a: b`, `@supports a#{b}`, `@supports #{a}b`,
//!   `@supports (not a: b)` → `Expected @supports condition.`;
//! * `@supports not a` → `Expected @supports condition.`;
//! * two groups without an operator, or an operator after a leading
//!   `not (…)` group → `expected "{".`;
//! * `and`/`or` followed by anything that is not a `(…)` group or
//!   `not (…)` → `Expected @supports condition.`.

use crate::error::SassError;
use crate::token::{Punct, Spanned, Token};

use super::Parser;

type Or<T> = Result<T, SassError>;

pub(crate) fn validate_supports(p: &Parser, rest: &[Spanned]) -> Or<()> {
    let rest: Vec<&Spanned> = rest
        .iter()
        .filter(|s| !matches!(s.token, Token::Comment(_)))
        .collect();
    let is_paren = |s: Option<&&Spanned>| s.is_some_and(|s| matches!(s.token, Token::Punct(Punct::LParen)));
    let ident_is = |s: &&Spanned, w: &str| {
        matches!(&s.token, Token::Ident(x) if x.eq_ignore_ascii_case(w))
    };
    let Some(first) = rest.first() else {
        return Ok(());
    };
    let mut i;
    // The head: either a `(…)` group, or a `not` followed by a group.
    let leading_not;
    // A call-shaped head `name(…)` — the name possibly interpolated
    // (`a#"b"c(d)`, `#"a"(b)`) — is a legal functional condition (`after_not`
    // golden `not a()` included); so is a lone interpolation that *renders* to
    // a group before an operator (`#"(a: b)" and …`).
    let call_paren = |from: usize| -> Option<usize> {
        let pi = from + 1 + rest[from + 1..].iter().position(|s| matches!(s.token, Token::Punct(Punct::LParen)))?;
        // the name run may only be identifiers/interpolations, and the `(`
        // must touch the name (`and (…)` has a gap — it is an operator, not
        // a call head)
        (rest[from + 1..pi]
            .iter()
            .all(|s| matches!(s.token, Token::Ident(_) | Token::Interpolated { .. }))
            && rest
                .get(pi)
                .is_some_and(|l| rest.get(pi - 1).is_some_and(|n| n.span.end == l.span.start)))
        .then_some(pi)
    };
    if is_paren(Some(first)) {
        leading_not = false;
        i = skip_group(p, &rest, 0)?;
    } else if ident_is(first, "not") {
        leading_not = true;
        let second = rest.get(1);
        if is_paren(second) {
            i = skip_group(p, &rest, 1)?;
        } else if let Some(pi) = call_paren(0) {
            // `@supports not a()` — a functional condition after not.
            i = skip_group(p, &rest, pi)?;
        } else {
            // `@supports not a` — the condition must be parenthesized.
            let at = second.map_or(first.span.end, |s| s.span.start);
            return Err(p.err("Expected @supports condition.", at));
        }
    } else if matches!(first.token, Token::Ident(_) | Token::Interpolated { .. })
        && let Some(pi) = call_paren(0)
    {
        // functional condition (`selector(…)`, `font-tech(…)`, interpolated
        // names): a call-shaped head passes through like a group
        // (the css/supports selector goldens).
        leading_not = false;
        i = skip_group(p, &rest, pi)?;
    } else if matches!(first.token, Token::Interpolated { .. })
        && rest
            .get(1)
            .is_none_or(|s| ident_is(s, "and") || ident_is(s, "or"))
    {
        // a lone interpolation *renders* the group (`#"(a: b)"` alone or
        // before an operator) — nothing static to check
        // (`lone_interpolation/top_level` goldens).
        return Ok(());
    } else {
        // bare identifier, raw `a: b` declaration, interpolation — no group at all
        return Err(p.err("Expected @supports condition.", first.span.start));
    }
    // Operator tail: `and`/`or` each demand another group (optionally `not`).
    loop {
        let Some(s) = rest.get(i) else { return Ok(()) };
        if leading_not {
            // dart closes the block right after a leading `not (…)` group:
            // an operator chain on it is an `expected "{".`
            return Err(p.err("expected \"{\".", s.span.start));
        }
        if ident_is(s, "and") || ident_is(s, "or") {
            let Some(n) = rest.get(i + 1) else {
                return Err(p.err("Expected @supports condition.", s.span.end));
            };
            if is_paren(Some(n)) {
                i = skip_group(p, &rest, i + 1)?;
                continue;
            }
            if matches!(n.token, Token::Interpolated { .. }) {
                // `… and #"(a: b)"` — the interpolated operand renders its
                // own group (`lone_interpolation/after_operator` golden).
                return Ok(());
            }
            if ident_is(n, "not") {
                let m = rest.get(i + 2);
                if !is_paren(m) {
                    let at = m.map_or(n.span.end, |s| s.span.start);
                    return Err(p.err("Expected @supports condition.", at));
                }
                i = skip_group(p, &rest, i + 2)?;
                continue;
            }
            return Err(p.err("Expected @supports condition.", n.span.start));
        }
        // a second group with no operator between (`(a: b) (c: d)`)
        return Err(p.err("expected \"{\".", s.span.start));
    }
}

/// Walk past one balanced `( … )` group starting at `open`; also rejects the
/// `not`-led interior the goldens pin (`(not a: b)` wants a condition).
/// Unbalanced groups stay unchecked — other goldens own paren repair.
fn skip_group(p: &Parser, rest: &[&Spanned], open: usize) -> Or<usize> {
    let mut depth = 0usize;
    let mut inner_seen = false;
    let mut j = open;
    while let Some(s) = rest.get(j) {
        match &s.token {
            Token::Punct(Punct::LParen) => {
                depth += 1;
                if depth == 1 {
                    inner_seen = false;
                }
            }
            Token::Punct(Punct::RParen) => {
                depth -= 1;
                if depth == 0 {
                    return Ok(j + 1);
                }
            }
            Token::Ident(w)
                if depth == 1 && !inner_seen && w.eq_ignore_ascii_case("not") =>
            {
                // `(not …` : legal only as `not (…)` — the golden
                // `(not a: b)` fails with a condition error.
                inner_seen = true;
                if !rest.get(j + 1).is_some_and(|n| matches!(n.token, Token::Punct(Punct::LParen)))
                {
                    let at = rest
                        .get(j + 1)
                        .map_or(s.span.end, |n| n.span.start);
                    return Err(p.err("Expected @supports condition.", at));
                }
            }
            _ => {
                if depth == 1 {
                    inner_seen = true;
                }
            }
        }
        j += 1;
    }
    Ok(j)
}
