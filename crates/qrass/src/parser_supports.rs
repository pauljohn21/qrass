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
            // the goldens close the block right after a leading `not (…)` group:
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

/// Conservative `@media` logic grammar (`css/media/logic/error.hrx` goldens):
/// * mixing `and` with `or` in one comma-separated query, or an `or` whose
///   left operand is not a `(…)` group → `expected "{".`;
/// * `and` / `or` / `not` with no operand after it → `expected media
///   condition in parentheses.`
///
/// Every other shape — pure and-chains, pure or-chains, `not (…)`,
/// interpolated right operands, bare type heads with `and` — keeps today's
/// passthrough (the passing `logic/and`, `logic/or`, `logic/not`,
/// `logic/and_not` goldens).
pub(crate) fn validate_media(p: &Parser, rest: &[Spanned]) -> Or<()> {
    // Depth-0 item stream: each balanced `(…)` collapses into one Group item.
    let sig: Vec<&Spanned> = rest
        .iter()
        .filter(|s| !matches!(s.token, Token::Comment(_)))
        .collect();
    let mut items: Vec<Item> = Vec::new();
    let mut j = 0usize;
    while j < sig.len() {
        let s = sig[j];
        if matches!(s.token, Token::Punct(Punct::LParen)) {
            let mut depth = 0usize;
            let mut k = j;
            while k < sig.len() {
                match &sig[k].token {
                    Token::Punct(Punct::LParen) => depth += 1,
                    Token::Punct(Punct::RParen) => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            // Media *ranges* allow at most two top-level comparators
            // (`1 <= width <= 2`); a third reads to the goldens' grammar as garbage
            // inside the paren group (`css/media/range/error`
            // `invalid_binary_operator`: `expected ")".`).
            let mut cmp = 0usize;
            let mut d = 0usize;
            for g in &sig[j..=k.min(sig.len() - 1)] {
                match &g.token {
                    Token::Punct(Punct::LParen) => d += 1,
                    Token::Punct(Punct::RParen) => d -= 1,
                    Token::Punct(Punct::Lt | Punct::Le | Punct::Gt | Punct::Ge)
                        if d == 1 =>
                    {
                        cmp += 1;
                    }
                    _ => {}
                }
            }
            if cmp >= 3 {
                return Err(p.err("expected \")\".", s.span.start));
            }
            items.push(Item::Group);
            j = k + 1;
            continue;
        }
        match &s.token {
            Token::Punct(Punct::Comma) => items.push(Item::Comma),
            Token::Ident(w)
                if w.eq_ignore_ascii_case("and")
                    || w.eq_ignore_ascii_case("or")
                    || w.eq_ignore_ascii_case("not") =>
            {
                let kind = if w.eq_ignore_ascii_case("and") {
                    0
                } else if w.eq_ignore_ascii_case("or") {
                    1
                } else {
                    2
                };
                items.push(Item::Op(kind, s));
            }
            _ => items.push(Item::Operand(s)),
        }
        j += 1;
    }
    // Per-query state machine: [not]? operand ( (and|or) [not]? operand )*
    let mut idx = 0usize;
    while idx < items.len() {
        let mut saw_or = false;
        let mut first_and_or: Option<u8> = None;
        let mut mixed_at = None;
        let mut head_is_group = false;
        let mut head_span = items[idx].span_of();
        // head
        match &items[idx] {
            Item::Comma => {
                idx += 1;
                continue;
            }
            Item::Op(2, opspan) => {
                head_span = opspan.span.start;
                idx += 1;
                match items.get(idx) {
                    Some(Item::Group) => {
                        head_is_group = true;
                        idx += 1;
                    }
                    Some(Item::Operand(_)) => {
                        idx += 1;
                    }
                    _ => {
                        return Err(p.err(
                            "expected media condition in parentheses.",
                            opspan.span.start,
                        ));
                    }
                }
            }
            Item::Group => {
                head_is_group = true;
                idx += 1;
            }
            Item::Operand(_) | Item::Op(_, _) => {
                head_is_group = false;
                idx += 1;
            }
        }
        // chain
        #[allow(clippy::while_let_loop)] // two distinct break shapes: end-of-items and non-op
        loop {
            let Some(op) = items.get(idx) else { break };
            let Item::Op(kind, opspan) = op else {
                // two operands without an operator: not claimed here (the
                // media type goldens own that shape)
                break;
            };
            if *kind != 2 {
                saw_or |= *kind == 1;
                if first_and_or.is_none() {
                    first_and_or = Some(*kind);
                } else if first_and_or != Some(*kind) && mixed_at.is_none() {
                    mixed_at = Some(opspan.span.start);
                }
            }
            idx += 1;
            let operand_present = match items.get(idx) {
                Some(Item::Group) | Some(Item::Operand(_)) => {
                    idx += 1;
                    true
                }
                Some(Item::Op(2, _)) => {
                    idx += 1;
                    matches!(items.get(idx), Some(Item::Group) | Some(Item::Operand(_)))
                        .then(|| idx += 1)
                        .is_some()
                }
                _ => false,
            };
            if !operand_present {
                return Err(p.err(
                    "expected media condition in parentheses.",
                    opspan.span.start,
                ));
            }
        }
        if let Some(at) = mixed_at {
            return Err(p.err("expected \"{\".", at));
        }
        if saw_or && !head_is_group {
            return Err(p.err("expected \"{\".", head_span));
        }
    }
    Ok(())
}

/// One depth-0 piece of a `@media` query head.
enum Item<'a> {
    Group,
    Op(u8, &'a Spanned),
    Operand(&'a Spanned),
    Comma,
}

impl<'a> Item<'a> {
    fn span_of(&self) -> crate::span::BytePos {
        match self {
            Item::Group | Item::Comma => crate::span::BytePos(0),
            Item::Op(_, s) | Item::Operand(s) => s.span.start,
        }
    }
}
