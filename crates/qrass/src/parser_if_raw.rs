//! Raw-capture side of the modern `if(…)` condition grammar: the CSS term
//! slices, the `sass(…)` body, and the arbitrary-substitution check. Split out
//! of `parser_if.rs` (which holds the grammar) as a second `impl` block on the
//! same cursor, following the `parser_args` layout.
//!
//! Raw regions are sliced verbatim from the source and split around `#{…}`
//! tokens, so nothing inside a CSS term needs Sass lexing semantics — only the
//! interpolation is re-evaluated at render time.

use crate::ast_expr::Expr;
use crate::error::SassError;
use crate::span::BytePos;
use crate::token::{Punct, Piece, Spanned, Token};
use std::ops::ControlFlow;

use super::parser_expr::E;

type Ex<T> = Result<T, SassError>;

/// Golden sentence of `expressions/if/error/raw.hrx`, verbatim.
pub(crate) const SUB_SASS_CONFLICT: &str =
    "if() conditions with arbitrary substitutions may not contain sass() expressions.";

impl E<'_> {
    // ─── Arbitrary-substitution / sass() conflict ───────────────────────────

    /// The goldens forbid a condition whose unparenthesized text mixes an
    /// arbitrary substitution (`var(…)`/`attr(…)`, unknown until the browser
    /// evaluates them) with a `sass()` expression. `css(…)` and `#{…}` are
    /// Sass-known text and stay legal; a `(`-group shields what it holds.
    /// Checked before the grammar, matching `error/raw.hrx`.
    pub(crate) fn check_arbitrary_with_sass(&mut self) -> Ex<()> {
        let region = self.cond_region();
        let Some((sub_start, sub_end)) = first_call(&region, &["var", "attr"], false) else {
            return Ok(());
        };
        let Some((sass_start, sass_end)) = first_call(&region, &["sass"], true) else {
            return Ok(());
        };
        let p = self.p;
        let marks = vec![
            p.marker('^', sub_start, sub_end, Some("arbitrary substitution".to_string())),
            p.marker('=', sass_start, sass_end, Some("sass() expression".to_string())),
        ];
        Err(p.err_marked(SUB_SASS_CONFLICT, sub_start, marks))
    }

    /// The tokens of the condition about to be parsed: up to the first
    /// top-level `:`, `;` or the `)` closing the call.
    fn cond_region(&mut self) -> Vec<Spanned> {
        // fold the cloned iterator with a `done` latch: the first top-level
        // `:`/`;` or the unmatched `)` closing the call stops the collection
        self.it
            .clone()
            .fold((Vec::new(), 0i32, false), |(mut out, depth, done), s| {
                let (depth, stop) = match (done, &s.token) {
                    (true, _) => (depth, true),
                    (false, Token::Punct(Punct::LParen | Punct::LBracket)) => (depth + 1, false),
                    (false, Token::Punct(Punct::RParen | Punct::RBracket)) if depth == 0 => {
                        (depth, true)
                    }
                    (false, Token::Punct(Punct::RParen | Punct::RBracket)) => (depth - 1, false),
                    (false, Token::Punct(Punct::Colon | Punct::Semicolon)) if depth == 0 => {
                        (depth, true)
                    }
                    (false, _) => (depth, false),
                };
                match stop {
                    true => (out, depth, true),
                    false => {
                        out.push(s);
                        (out, depth, false)
                    }
                }
            })
            .0
    }

    /// Whether the pending condition holds a `var(…)`/`attr(…)` term at its own
    /// level (an arbitrary substitution).
    pub(crate) fn region_has_substitution(&mut self) -> bool {
        let region = self.cond_region();
        first_call(&region, &["var", "attr"], false).is_some()
    }

    // ─── Raw capture ────────────────────────────────────────────────────────

    /// `sass( … )` body: the inner tokens re-parsed as a Sass expression.
    pub(crate) fn sass_body(&mut self) -> Ex<Expr> {
        // pull tokens while the running depth stays positive: the `)` that
        // returns it to zero is consumed but not part of the body
        let mut depth = 1i32;
        let run: Vec<Spanned> = std::iter::from_fn(|| self.bump())
            .map(|s| {
                depth += match &s.token {
                    Token::Punct(Punct::LParen | Punct::LBracket) => 1,
                    Token::Punct(Punct::RParen | Punct::RBracket) => -1,
                    _ => 0,
                };
                (s, depth)
            })
            .take_while(|(_, depth)| *depth > 0)
            .map(|(s, _)| s)
            .collect();
        let p = self.p;
        super::parser_expr::value_of(p, run)
    }

    /// Capture a CSS term verbatim: an optional call `(...)` after the current
    /// token (or an interpolation-led `#{…}(…)`), sliced from the source and
    /// split around interpolated tokens. The closing `)` is consumed.
    pub(crate) fn raw_capture(&mut self) -> Ex<Vec<Piece>> {
        let first = self
            .bump()
            .ok_or_else(|| self.p.err("expected a condition term", self.last.start))?;
        let start = first.span.start;
        // An interpolation directly followed by `(` opens a call: `#{css}(…)`.
        let call_open = !matches!(first.token, Token::Punct(Punct::LParen))
            && self.is(Punct::LParen);
        let (toks, end) = match call_open {
            false => (vec![first.clone()], first.span.end),
            true => walk_call_body(std::iter::from_fn(|| self.bump()), first.clone()),
        };
        Ok(self.pieces_over(&toks, start, end))
    }

    /// Source slices of `toks` as pieces, splitting interpolated tokens out so
    /// their `#{…}` runs evaluate at render time while everything else stays
    /// verbatim text.
    fn pieces_over(&self, toks: &[Spanned], start: BytePos, end: BytePos) -> Vec<Piece> {
        let (out, cur) = toks.iter().fold(
            (Vec::new(), start),
            |(out, cur), t| match &t.token {
                Token::Interpolated { quote: None, pieces } => {
                    let out = self.push_text(out, cur, t.span.start);
                    let out = pieces.iter().fold(out, |mut out, pc| {
                        match (pc, out.last()) {
                            (Piece::Text(text), Some(Piece::Text(_))) => {
                                if let Some(Piece::Text(prev)) = out.last_mut() {
                                    prev.push_str(text);
                                }
                                out
                            }
                            _ => {
                                out.push(pc.clone());
                                out
                            }
                        }
                    });
                    (out, t.span.end)
                }
                _ => (out, cur),
            },
        );
        self.push_text(out, cur, end)
    }

    /// Append the verbatim source slice `[from, to)`, merging into a preceding
    /// text piece so adjacent runs stay a single piece.
    fn push_text(&self, mut out: Vec<Piece>, from: BytePos, to: BytePos) -> Vec<Piece> {
        match to.0.checked_sub(from.0).is_some_and(|len| len > 0) {
            false => out,
            true => {
                let text = self.p.slice(from, to);
                match out.last_mut() {
                    Some(Piece::Text(prev)) => prev.push_str(&text),
                    _ => out.push(Piece::Text(text)),
                }
                out
            }
        }
    }
}

/// First `name(…)` call in `toks`, as the span from the identifier to its
/// closing `)`. `any_depth` keeps calls nested in a `(`-group visible;
/// otherwise only calls at the region's own level count.
fn first_call(toks: &[Spanned], names: &[&str], any_depth: bool) -> Option<(BytePos, BytePos)> {
    // pair each token with the depth in front of it (an identifier never moves
    // the depth, so its own level is what the old cursor carried)
    let mut depth = 0i32;
    toks.iter()
        .map(|s| {
            let level = depth;
            depth += match &s.token {
                Token::Punct(Punct::LParen | Punct::LBracket) => 1,
                Token::Punct(Punct::RParen | Punct::RBracket) => -1,
                _ => 0,
            };
            (s, level)
        })
        .enumerate()
        .find(|(i, (s, level))| {
            matches!(&s.token, Token::Ident(w) if names.iter().any(|n| w.eq_ignore_ascii_case(n)))
                && matches!(
                    toks.get(*i + 1).map(|n| &n.token),
                    Some(Token::Punct(Punct::LParen))
                )
                && (any_depth || *level == 0)
        })
        .map(|(i, (s, _))| (s.span.start, call_end(toks, i + 1)))
}

/// End offset of the call body opened at `open` (a `(` token).
fn call_end(toks: &[Spanned], open: usize) -> BytePos {
    let mut depth = 0i32;
    toks[open..]
        .iter()
        .map(|s| {
            depth += match &s.token {
                Token::Punct(Punct::LParen | Punct::LBracket) => 1,
                Token::Punct(Punct::RParen | Punct::RBracket) => -1,
                _ => 0,
            };
            (s.span.end, depth)
        })
        .find(|(_, depth)| *depth == 0)
        .map_or_else(
            || toks[open..].last().map_or(BytePos(0), |s| s.span.end),
            |(end, _)| end,
        )
}

/// Consume a call body from the cursor: the tokens (closing `)` included) plus
/// the end offset. `try_fold` stops pulling, so the cursor sits right after the
/// closer exactly like the old `break`.
fn walk_call_body<I: Iterator<Item = Spanned>>(
    cursor: I,
    head: Spanned,
) -> (Vec<Spanned>, BytePos) {
    let reached = cursor
        .scan(0i32, |depth, s| {
            *depth += match &s.token {
                Token::Punct(Punct::LParen | Punct::LBracket) => 1,
                Token::Punct(Punct::RParen | Punct::RBracket) => -1,
                _ => 0,
            };
            Some((s, *depth))
        })
        .try_fold(
            (vec![head.clone()], head.span.end),
            |(mut toks, _), (s, depth)| {
                let end = s.span.end;
                toks.push(s);
                match depth {
                    0 => ControlFlow::Break((toks, end)),
                    _ => ControlFlow::Continue((toks, end)),
                }
            },
        );
    match reached {
        ControlFlow::Break(v) | ControlFlow::Continue(v) => v,
    }
}
