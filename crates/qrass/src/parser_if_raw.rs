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
        let mut out: Vec<Spanned> = Vec::new();
        let mut depth = 0i32;
        for s in self.it.clone() {
            match &s.token {
                Token::Punct(Punct::LParen | Punct::LBracket) => depth += 1,
                Token::Punct(Punct::RParen | Punct::RBracket) => {
                    depth -= 1;
                    if depth < 0 {
                        break; // the `)` closing the if() call
                    }
                }
                Token::Punct(Punct::Colon | Punct::Semicolon) if depth == 0 => break,
                _ => {}
            }
            out.push(s);
        }
        out
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
        let mut run: Vec<Spanned> = Vec::new();
        let mut depth = 1i32;
        while let Some(s) = self.bump() {
            match &s.token {
                Token::Punct(Punct::LParen | Punct::LBracket) => depth += 1,
                Token::Punct(Punct::RParen | Punct::RBracket) => {
                    depth -= 1;
                    if depth == 0 {
                        break; // matching ')' consumed
                    }
                }
                _ => {}
            }
            run.push(s);
        }
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
        let mut toks = vec![first];
        // An interpolation directly followed by `(` opens a call: `#{css}(…)`.
        let call_open = if matches!(toks[0].token, Token::Punct(Punct::LParen)) {
            false
        } else {
            self.is(Punct::LParen)
        };
        let mut end = toks[0].span.end;
        if call_open {
            let mut depth = 0i32;
            while let Some(s) = self.bump() {
                match &s.token {
                    Token::Punct(Punct::LParen | Punct::LBracket) => depth += 1,
                    Token::Punct(Punct::RParen | Punct::RBracket) => {
                        depth -= 1;
                        if depth == 0 {
                            end = s.span.end;
                            toks.push(s);
                            break;
                        }
                    }
                    _ => {}
                }
                end = s.span.end;
                toks.push(s);
            }
        }
        Ok(self.pieces_over(&toks, start, end))
    }

    /// Source slices of `toks` as pieces, splitting interpolated tokens out so
    /// their `#{…}` runs evaluate at render time while everything else stays
    /// verbatim text.
    fn pieces_over(&self, toks: &[Spanned], start: BytePos, end: BytePos) -> Vec<Piece> {
        let mut out: Vec<Piece> = Vec::new();
        let mut cur = start;
        let push_text = |from: BytePos, to: BytePos, out: &mut Vec<Piece>| {
            if let Some(len) = to.0.checked_sub(from.0)
                && len > 0
            {
                let text = self.p.slice(from, to);
                match out.last_mut() {
                    Some(Piece::Text(prev)) => prev.push_str(&text),
                    _ => out.push(Piece::Text(text)),
                }
            }
        };
        for t in toks {
            if let Token::Interpolated { quote: None, pieces } = &t.token {
                push_text(cur, t.span.start, &mut out);
                for pc in pieces {
                    match (pc, out.last_mut()) {
                        (Piece::Text(text), Some(Piece::Text(prev))) => prev.push_str(text),
                        _ => out.push(pc.clone()),
                    }
                }
                cur = t.span.end;
            }
        }
        push_text(cur, end, &mut out);
        out
    }
}

/// First `name(…)` call in `toks`, as the span from the identifier to its
/// closing `)`. `any_depth` keeps calls nested in a `(`-group visible;
/// otherwise only calls at the region's own level count.
fn first_call(toks: &[Spanned], names: &[&str], any_depth: bool) -> Option<(BytePos, BytePos)> {
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < toks.len() {
        let s = &toks[i];
        match &s.token {
            Token::Punct(Punct::LParen | Punct::LBracket) => depth += 1,
            Token::Punct(Punct::RParen | Punct::RBracket) => depth -= 1,
            Token::Ident(word) => {
                let opens = matches!(toks.get(i + 1).map(|n| &n.token), Some(Token::Punct(Punct::LParen)));
                let named = names.iter().any(|n| word.eq_ignore_ascii_case(n));
                if opens && named && (any_depth || depth == 0) {
                    return Some((s.span.start, call_end(toks, i + 1)));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// End offset of the call body opened at `open` (a `(` token).
fn call_end(toks: &[Spanned], open: usize) -> BytePos {
    let mut depth = 0i32;
    let mut i = open;
    let mut last = BytePos(0);
    while let Some(s) = toks.get(i) {
        last = s.span.end;
        match &s.token {
            Token::Punct(Punct::LParen | Punct::LBracket) => depth += 1,
            Token::Punct(Punct::RParen | Punct::RBracket) => {
                depth -= 1;
                if depth == 0 {
                    return s.span.end;
                }
            }
            _ => {}
        }
        i += 1;
    }
    last
}
