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
use crate::span::BytePos;
use crate::token::{Punct, Spanned, Token};

use super::Parser;

type Or<T> = Result<T, SassError>;

/// A `(` token.
fn is_paren(s: Option<&&Spanned>) -> bool {
    s.is_some_and(|s| matches!(s.token, Token::Punct(Punct::LParen)))
}

/// An `Ident` token equal to `w`, read case-insensitively like the grammar.
fn ident_is(s: &&Spanned, w: &str) -> bool {
    matches!(&s.token, Token::Ident(x) if x.eq_ignore_ascii_case(w))
}

/// The `(` of a call-shaped head `name(…)` at `from` — the name possibly
/// interpolated (`a#"b"c(d)`, `#"a"(b)`) — is a legal functional condition
/// (`after_not` golden `not a()` included). The name run may only be
/// identifiers/interpolations, and the `(` must touch the name (`and (…)` has a
/// gap — it is an operator, not a call head).
fn call_paren(rest: &[&Spanned], from: usize) -> Option<usize> {
    let pi = from
        + 1
        + rest[from + 1..]
            .iter()
            .position(|s| matches!(s.token, Token::Punct(Punct::LParen)))?;
    (rest[from + 1..pi]
        .iter()
        .all(|s| matches!(s.token, Token::Ident(_) | Token::Interpolated { .. }))
        && rest
            .get(pi)
            .is_some_and(|l| rest.get(pi - 1).is_some_and(|n| n.span.end == l.span.start)))
        .then_some(pi)
}

/// The shape of an `@supports` condition head.
enum Head {
    /// a parenthesized (or functional) group opening at `open`, `not`-led or not
    Group { open: usize, leading_not: bool },
    /// a lone interpolation *renders* the group (`#"(a: b)"` alone or before an
    /// operator) — nothing static to check (`lone_interpolation/top_level`)
    LoneInterp,
    /// bare identifier, raw `a: b` declaration, … — no group at all; the golden's
    /// `Expected @supports condition.` is raised at this offset
    Invalid(BytePos),
}

pub(crate) fn validate_supports(p: &Parser, rest: &[Spanned]) -> Or<()> {
    let rest: Vec<&Spanned> = rest
        .iter()
        .filter(|s| !matches!(s.token, Token::Comment(_)))
        .collect();
    let Some(first) = rest.first() else {
        return Ok(());
    };
    let second = rest.get(1);
    let callable = matches!(first.token, Token::Ident(_) | Token::Interpolated { .. });
    // a lone interpolation renders its own group when nothing but an operator
    // follows it
    let lone_interp = matches!(first.token, Token::Interpolated { .. })
        && second.is_none_or(|s| ident_is(s, "and") || ident_is(s, "or"));
    let head = match (is_paren(Some(first)), ident_is(first, "not")) {
        (true, _) => Head::Group {
            open: 0,
            leading_not: false,
        },
        (false, true) => match (is_paren(second), call_paren(&rest, 0)) {
            (true, _) => Head::Group {
                open: 1,
                leading_not: true,
            },
            // `@supports not a()` — a functional condition after not.
            (false, Some(pi)) => Head::Group {
                open: pi,
                leading_not: true,
            },
            // `@supports not a` — the condition must be parenthesized.
            (false, None) => Head::Invalid(second.map_or(first.span.end, |s| s.span.start)),
        },
        (false, false) => match (callable.then(|| call_paren(&rest, 0)).flatten(), lone_interp) {
            // functional condition (`selector(…)`, `font-tech(…)`, interpolated
            // names): a call-shaped head passes through like a group
            // (the css/supports selector goldens).
            (Some(pi), _) => Head::Group {
                open: pi,
                leading_not: false,
            },
            (None, true) => Head::LoneInterp,
            (None, false) => Head::Invalid(first.span.start),
        },
    };
    match head {
        Head::LoneInterp => Ok(()),
        Head::Invalid(at) => Err(p.err("Expected @supports condition.", at)),
        Head::Group {
            open,
            leading_not,
        } => {
            let i = skip_group(p, &rest, open)?;
            supports_tail(p, &rest, i, leading_not)
        }
    }
}

/// Operator tail: `and`/`or` each demand another group (optionally `not`).
fn supports_tail(p: &Parser, rest: &[&Spanned], i: usize, leading_not: bool) -> Or<()> {
    let Some(s) = rest.get(i) else {
        return Ok(());
    };
    match (leading_not, ident_is(s, "and") || ident_is(s, "or")) {
        // the goldens close the block right after a leading `not (…)` group:
        // an operator chain on it is an `expected "{".`
        (true, _) => Err(p.err("expected \"{\".", s.span.start)),
        // a second group with no operator between (`(a: b) (c: d)`)
        (false, false) => Err(p.err("expected \"{\".", s.span.start)),
        (false, true) => match rest.get(i + 1) {
            None => Err(p.err("Expected @supports condition.", s.span.end)),
            Some(n) => {
                match (
                    is_paren(Some(n)),
                    matches!(n.token, Token::Interpolated { .. }),
                    ident_is(n, "not"),
                ) {
                    (true, _, _) => {
                        supports_tail(p, rest, skip_group(p, rest, i + 1)?, leading_not)
                    }
                    // `… and #"(a: b)"` — the interpolated operand renders its
                    // own group (`lone_interpolation/after_operator` golden).
                    (false, true, _) => Ok(()),
                    (false, false, true) => {
                        let m = rest.get(i + 2);
                        match is_paren(m) {
                            true => {
                                supports_tail(p, rest, skip_group(p, rest, i + 2)?, leading_not)
                            }
                            false => Err(p.err(
                                "Expected @supports condition.",
                                m.map_or(n.span.end, |s| s.span.start),
                            )),
                        }
                    }
                    (false, false, false) => {
                        Err(p.err("Expected @supports condition.", n.span.start))
                    }
                }
            }
        },
    }
}

/// Walk past one balanced `( … )` group starting at `open`; also rejects the
/// `not`-led interior the goldens pin (`(not a: b)` wants a condition).
/// Unbalanced groups stay unchecked — other goldens own paren repair.
fn skip_group(p: &Parser, rest: &[&Spanned], open: usize) -> Or<usize> {
    // cursor fold `(next, depth, inner-seen, closed)`: the `closed` latch freezes
    // the walk right after the matching `)`, exactly like the old early return
    let (end, ..) = (open..rest.len())
        .try_fold((open, 0usize, false, false), |st, _| {
            let (j, depth, inner_seen, closed) = st;
            match (closed, rest.get(j)) {
                (true, _) | (false, None) => Ok(st),
                (false, Some(s)) => match &s.token {
                    Token::Punct(Punct::LParen) => {
                        Ok((j + 1, depth + 1, inner_seen && depth != 0, closed))
                    }
                    Token::Punct(Punct::RParen) => match depth {
                        0 | 1 => Ok((j + 1, 0, inner_seen, true)),
                        _ => Ok((j + 1, depth - 1, inner_seen, closed)),
                    },
                    // `(not …` : legal only as `not (…)` — the golden
                    // `(not a: b)` fails with a condition error.
                    Token::Ident(w)
                        if depth == 1 && !inner_seen && w.eq_ignore_ascii_case("not") =>
                    {
                        match rest
                            .get(j + 1)
                            .is_some_and(|n| matches!(n.token, Token::Punct(Punct::LParen)))
                        {
                            true => Ok((j + 1, depth, true, closed)),
                            false => Err(p.err(
                                "Expected @supports condition.",
                                rest.get(j + 1).map_or(s.span.end, |n| n.span.start),
                            )),
                        }
                    }
                    _ => Ok((j + 1, depth, inner_seen || depth == 1, closed)),
                },
            }
        })?;
    Ok(end)
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
    run_queries(p, &build_items(p, &sig)?, 0)
}

/// Collapse the token stream into depth-0 grammar pieces: a balanced group
/// becomes one `Group`, everything else a single piece.
fn build_items<'a>(p: &Parser, sig: &[&'a Spanned]) -> Or<Vec<Item<'a>>> {
    (0..sig.len())
        .try_fold((0usize, Vec::new()), |(j, mut items), idx| {
            match idx == j {
                false => Ok((j, items)),
                true => {
                    let s = sig[j];
                    match is_paren(Some(&s)) {
                        true => {
                            let k = matching_close(sig, j);
                            // Media *ranges* allow at most two top-level comparators
                            // (`1 <= width <= 2`); a third reads to the goldens' grammar
                            // as garbage inside the paren group (`css/media/range/error`
                            // `invalid_binary_operator`: `expected ")".`).
                            match comparator_count(&sig[j..=k.min(sig.len() - 1)]) >= 3 {
                                true => Err(p.err("expected \")\".", s.span.start)),
                                false => {
                                    items.push(Item::Group);
                                    Ok((k + 1, items))
                                }
                            }
                        }
                        false => {
                            items.push(classify_item(s));
                            Ok((j + 1, items))
                        }
                    }
                }
            }
        })
        .map(|(_, items)| items)
}

/// Index of the `)` closing the group that opens at `from` (the stream length
/// when the group never closes).
fn matching_close(sig: &[&Spanned], from: usize) -> usize {
    sig[from..]
        .iter()
        .scan(0i32, |depth, s| {
            *depth += match &s.token {
                Token::Punct(Punct::LParen) => 1,
                Token::Punct(Punct::RParen) => -1,
                _ => 0,
            };
            Some(*depth)
        })
        .position(|d| d == 0)
        .map_or(sig.len(), |offset| from + offset)
}

/// Comparators sitting at depth 1 inside a paren group.
fn comparator_count(group: &[&Spanned]) -> usize {
    group
        .iter()
        .fold((0i32, 0usize), |(d, cmp), s| {
            match &s.token {
                Token::Punct(Punct::LParen) => (d + 1, cmp),
                Token::Punct(Punct::RParen) => (d - 1, cmp),
                Token::Punct(Punct::Lt | Punct::Le | Punct::Gt | Punct::Ge) if d == 1 => {
                    (d, cmp + 1)
                }
                _ => (d, cmp),
            }
        })
        .1
}

/// One depth-0 non-paren token as a media grammar piece.
fn classify_item(s: &Spanned) -> Item<'_> {
    match &s.token {
        Token::Punct(Punct::Comma) => Item::Comma,
        Token::Ident(w) => {
            match ["and", "or", "not"]
                .iter()
                .position(|kind| w.eq_ignore_ascii_case(kind))
            {
                Some(kind) => Item::Op(kind as u8, s),
                None => Item::Operand(s),
            }
        }
        _ => Item::Operand(s),
    }
}

/// Walk the comma-separated queries one at a time until the item stream ends.
fn run_queries(p: &Parser, items: &[Item<'_>], idx: usize) -> Or<()> {
    let Some(head) = items.get(idx) else {
        return Ok(());
    };
    match head {
        // an empty query between separators carries nothing to check
        Item::Comma => run_queries(p, items, idx + 1),
        _ => {
            let q = Query {
                idx,
                saw_or: false,
                first_and_or: None,
                mixed_at: None,
                head_is_group: matches!(head, Item::Group),
                head_span: head.span_of(),
            }
            .take_head(p, items)?
            .chain(p, items)?;
            match (q.mixed_at, q.saw_or && !q.head_is_group) {
                (Some(at), _) => Err(p.err("expected \"{\".", at)),
                (None, true) => Err(p.err("expected \"{\".", q.head_span)),
                (None, false) => run_queries(p, items, q.idx),
            }
        }
    }
}

/// Rolling state of one `@media` query: `[not]? operand ((and|or) [not]? operand)*`
struct Query {
    idx: usize,
    saw_or: bool,
    first_and_or: Option<u8>,
    mixed_at: Option<BytePos>,
    head_is_group: bool,
    head_span: BytePos,
}

impl Query {
    /// Consume the query head: an optional leading `not` and its operand.
    fn take_head(mut self, p: &Parser, items: &[Item<'_>]) -> Or<Self> {
        match &items[self.idx] {
            // unreachable: the caller steps over separators
            Item::Comma => Ok(self),
            Item::Op(2, opspan) => {
                self.head_span = opspan.span.start;
                self.idx += 1;
                let (group, present) = match items.get(self.idx) {
                    Some(Item::Group) => (true, true),
                    Some(Item::Operand(_)) => (false, true),
                    _ => (false, false),
                };
                match present {
                    false => Err(p.err(
                        "expected media condition in parentheses.",
                        opspan.span.start,
                    )),
                    true => {
                        self.head_is_group = group;
                        self.idx += 1;
                        Ok(self)
                    }
                }
            }
            Item::Group => {
                self.head_is_group = true;
                self.idx += 1;
                Ok(self)
            }
            Item::Operand(_) | Item::Op(_, _) => {
                self.head_is_group = false;
                self.idx += 1;
                Ok(self)
            }
        }
    }

    /// Fold the operator chain: each `and`/`or` (with an optional `not`) must be
    /// followed by an operand, and mixing `and` with `or` is pinned by the
    /// goldens.
    fn chain(self, p: &Parser, items: &[Item<'_>]) -> Or<Self> {
        let Some(Item::Op(kind, opspan)) = items.get(self.idx) else {
            // two operands without an operator: not claimed here (the
            // media type goldens own that shape)
            return Ok(self);
        };
        let (kind, at) = (*kind, opspan.span.start);
        let q = Query {
            saw_or: self.saw_or || kind == 1,
            first_and_or: match self.first_and_or {
                None if kind != 2 => Some(kind),
                other => other,
            },
            mixed_at: match (self.first_and_or, self.mixed_at) {
                (Some(prev), None) if kind != 2 && prev != kind => Some(at),
                (_, other) => other,
            },
            idx: self.idx + 1,
            ..self
        };
        let (q, present) = q.take_operand(items);
        match present {
            true => q.chain(p, items),
            false => Err(p.err("expected media condition in parentheses.", at)),
        }
    }

    /// Consume the operand an operator demands: `operand` or `not operand`.
    fn take_operand(mut self, items: &[Item<'_>]) -> (Self, bool) {
        match items.get(self.idx) {
            Some(Item::Group) | Some(Item::Operand(_)) => {
                self.idx += 1;
                (self, true)
            }
            Some(Item::Op(2, _)) => {
                self.idx += 1;
                match items.get(self.idx) {
                    Some(Item::Group) | Some(Item::Operand(_)) => {
                        self.idx += 1;
                        (self, true)
                    }
                    _ => (self, false),
                }
            }
            _ => (self, false),
        }
    }
}

/// One depth-0 piece of a `@media` query head.
enum Item<'a> {
    Group,
    Op(u8, &'a Spanned),
    Operand(&'a Spanned),
    Comma,
}

impl<'a> Item<'a> {
    fn span_of(&self) -> BytePos {
        match self {
            Item::Group | Item::Comma => BytePos(0),
            Item::Op(_, s) | Item::Operand(s) => s.span.start,
        }
    }
}
