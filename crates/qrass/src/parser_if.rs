//! Modern `if(…)` clause grammar (per the `expressions/if/*.hrx` goldens):
//! detection, clause chains, the condition levels, and the golden-anchored parse
//! diagnostics of `expressions/if/error/*.hrx`. The raw CSS term capture lives
//! in the sibling `parser_if_raw` module as a second `impl` block on this
//! cursor.
//!
//! Grammar the goldens pin down:
//! * a condition is `not`-prefixed (then it is a single unit, no chain), or a
//!   chain of units joined by one consistent connective (`and`, `or`, or plain
//!   adjacency);
//! * an operator mix without parentheses (`a and b or c`) is rejected;
//! * a `(`-group may lead a chain or follow an explicit `and`/`or`, but never
//!   continue an adjacency run;
//! * a term must be `ident(…)`, `(…)`, or an interpolation — a bare identifier
//!   is an error at the column right after it;
//! * `not(` / `and(` / `or(` need whitespace before the `(`.

use crate::ast_expr::{Expr, IfCall, IfClause, IfCond, Join};
use crate::error::SassError;
use crate::span::BytePos;
use crate::token::{Punct, Spanned, Token};

use super::parser_expr::E;

type Ex<T> = Result<T, SassError>;

// ─── Golden error sentences (verbatim) ───────────────────────────────────────

const EXPECT_IDENT: &str = "Expected identifier.";
const EXPECT_LPAREN: &str = "expected \"(\".";
const EXPECT_COLON: &str = "expected \":\".";
const EXPECT_RPAREN: &str = "expected \")\".";

/// `not` / `and` / `or` glued to a `(`.
fn ws_required(word: &str) -> String {
    format!("Whitespace is required between \"{word}\" and \"(\"")
}

/// The connective keywords, matched case-insensitively like the goldens do.
fn is_kw(word: &str) -> bool {
    matches!(word.to_ascii_lowercase().as_str(), "not" | "and" | "or")
}

impl E<'_> {
    // ─── Detection ──────────────────────────────────────────────────────────

    /// Peek-only scan from the `(` of an `if` call: a top-level `:` or `;`
    /// before the first top-level `,` means the modern clause syntax;
    /// otherwise it stays the legacy three-argument builtin call.
    pub(crate) fn looks_like_modern_if(&mut self) -> bool {
        let it = self.it.clone();
        let mut depth = 0i32;
        for s in it {
            match &s.token {
                Token::Punct(Punct::LParen | Punct::LBracket) => depth += 1,
                Token::Punct(Punct::RParen | Punct::RBracket) => {
                    depth -= 1;
                    if depth <= 0 {
                        return false; // closed before any clause colon
                    }
                }
                Token::Punct(Punct::Colon | Punct::Semicolon) if depth == 1 => return true,
                Token::Punct(Punct::Comma) if depth == 1 => return false,
                _ => {}
            }
        }
        false
    }

    // ─── Clause chain ───────────────────────────────────────────────────────

    /// Parse a whole `if(…)` starting at its `(`; returns the [`Expr::IfCall`]
    /// with the span of the full call.
    pub(crate) fn modern_if(&mut self, start: BytePos) -> Ex<Expr> {
        self.expect(Punct::LParen, "expected '(' before if() clauses")?;
        let mut clauses: Vec<IfClause> = Vec::new();
        loop {
            if self.is(Punct::RParen) {
                break;
            }
            if self.peek_else_clause() {
                self.bump(); // `else`
                self.bump(); // `:`
                clauses.push(IfClause {
                    cond: None,
                    value: self.clause_value()?,
                });
            } else {
                self.check_arbitrary_with_sass()?;
                let cond = self.if_cond()?;
                if !self.eat(Punct::Colon) {
                    return Err(self.caret_err(EXPECT_COLON));
                }
                clauses.push(IfClause {
                    cond: Some(cond),
                    value: self.clause_value()?,
                });
            }
            // At most one `;` may separate clauses; a second one reaches the
            // condition parse and fails there (`error/semicolon.hrx`).
            if !self.eat(Punct::Semicolon) {
                break;
            }
        }
        if !self.is(Punct::RParen) {
            return Err(self.caret_err(EXPECT_RPAREN));
        }
        self.bump();
        Ok(Expr::IfCall(Box::new(IfCall {
            clauses,
            span: self.span(start),
        })))
    }

    /// `else` as a whole clause keyword: identifier `else` followed by `:`.
    fn peek_else_clause(&mut self) -> bool {
        let colon_next = matches!(
            self.it.clone().nth(1).map(|s| s.token),
            Some(Token::Punct(Punct::Colon))
        );
        colon_next && matches!(self.pk(), Some(Token::Ident(w)) if w.eq_ignore_ascii_case("else"))
    }

    /// A clause result/else value: a Sass expression spanning the tokens up to
    /// the next top-level `;`, `:` or the closing `)`.
    fn clause_value(&mut self) -> Ex<Expr> {
        let mut run: Vec<Spanned> = Vec::new();
        let mut depth = 0i32;
        loop {
            let done = match self.pk() {
                None => true,
                Some(Token::Punct(Punct::Semicolon | Punct::Colon | Punct::RParen)) if depth == 0 => {
                    true
                }
                _ => false,
            };
            if done {
                break;
            }
            let s = self.bump().expect("pk checked");
            match &s.token {
                Token::Punct(Punct::LParen | Punct::LBracket) => depth += 1,
                Token::Punct(Punct::RParen | Punct::RBracket) => depth -= 1,
                _ => {}
            }
            run.push(s);
        }
        if run.is_empty() {
            return Err(self.caret_err("expected a value in if() clause"));
        }
        let p = self.p;
        super::parser_expr::value_of(p, run)
    }

    // ─── Condition levels ───────────────────────────────────────────────────

    /// A whole condition: an optional `not` prefix over one unit, or a chain of
    /// units under a single connective.
    fn if_cond(&mut self) -> Ex<IfCond> {
        if let Some((word, at)) = self.glued_kw() {
            return Err(self.p.err_marked(ws_required(&word), at, vec![self.p.caret(at)]));
        }
        if self.peek_word("not") {
            self.bump();
            let unit = self.if_unit()?;
            return Ok(IfCond::Not(Box::new(unit)));
        }
        // Captured before any token is consumed: the glued-operator report below
        // needs to know whether the *whole* condition holds a substitution.
        let has_sub = self.region_has_substitution();
        let mut parts: Vec<IfCond> = vec![self.if_unit()?];
        let mut joins: Vec<Join> = Vec::new();
        let mut kind: Option<Join> = None;
        loop {
            if let Some((word, at)) = self.glued_kw() {
                // `error/missing_whitepsace.hrx` distinguishes the two shapes of
                // a glued `or(` after a term: with an arbitrary substitution in
                // the condition (`var(--clause-or) css(1) or(css(2))`) the golden
                // names `or`, without one (`css(1) or(css(2))`) it names `and`.
                let reported = if word.eq_ignore_ascii_case("or") && !has_sub {
                    "and".to_string()
                } else {
                    word
                };
                return Err(self
                    .p
                    .err_marked(ws_required(&reported), at, vec![self.p.caret(at)]));
            }
            let op = if self.peek_word("and") {
                Some(Join::And)
            } else if self.peek_word("or") {
                Some(Join::Or)
            } else {
                None
            };
            if let Some(op) = op {
                // Mixing `and` and `or` needs parentheses (`error/and.hrx`,
                // `error/or.hrx`): the stray operator is reported where the
                // condition should have ended.
                if let Some(seen) = kind {
                    if seen != op {
                        return Err(self.caret_err(EXPECT_COLON));
                    }
                } else {
                    kind = Some(op);
                }
                self.bump();
                parts.push(self.if_unit()?);
                joins.push(op);
                continue;
            }
            // Adjacency continues an operand run but never follows a
            // `(`-group (`error/raw.hrx` paren family).
            if !matches!(parts.last(), Some(IfCond::Paren(_))) && self.starts_adjacent_term() {
                parts.push(self.if_term()?);
                joins.push(Join::Adjacent);
                continue;
            }
            break;
        }
        Ok(if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            IfCond::Chain { parts, joins }
        })
    }

    /// One chain operand: a `(`-group or a term.
    fn if_unit(&mut self) -> Ex<IfCond> {
        if self.is(Punct::LParen) {
            self.bump();
            let inner = self.if_cond()?;
            if !self.eat(Punct::RParen) {
                return Err(self.caret_err(EXPECT_RPAREN));
            }
            return Ok(IfCond::Paren(Box::new(inner)));
        }
        self.if_term()
    }

    /// One condition term: `sass( … )` or any other CSS-ish call captured raw.
    fn if_term(&mut self) -> Ex<IfCond> {
        if let Some(Token::Ident(name)) = self.pk().cloned() {
            if let Some((_, at)) = self.glued_kw() {
                return Err(self.p.err_marked(ws_required(&name), at, vec![self.p.caret(at)]));
            }
            if self.next_is(Punct::LParen) {
                if name.eq_ignore_ascii_case("sass") {
                    self.bump();
                    self.bump();
                    // `sass_body` consumes the matching `)` itself.
                    let expr = self.sass_body()?;
                    return Ok(IfCond::Sass(expr));
                }
                let pieces = self.raw_capture()?;
                return Ok(IfCond::Css(pieces));
            }
            // A bare identifier: the goldens demand `(` in the very next column.
            let at = self.last_term_end();
            return Err(self.p.err_marked(EXPECT_LPAREN, at, vec![self.p.caret(at)]));
        }
        if matches!(self.pk(), Some(Token::Interpolated { .. })) {
            let pieces = self.raw_capture()?;
            return Ok(IfCond::Css(pieces));
        }
        Err(self.caret_err(EXPECT_IDENT))
    }

    // ─── Cursor helpers ─────────────────────────────────────────────────────

    fn peek_word(&mut self, w: &str) -> bool {
        matches!(self.pk(), Some(Token::Ident(i)) if i.eq_ignore_ascii_case(w))
    }

    fn next_is(&mut self, x: Punct) -> bool {
        matches!(self.it.clone().nth(1).map(|s| s.token), Some(Token::Punct(q)) if q == x)
    }

    /// End offset of the identifier just peeked (1-based column right after it).
    fn last_term_end(&mut self) -> BytePos {
        self.it.peek().map(|s| s.span.end).unwrap_or(self.last.end)
    }

    /// The `(adjacency` continuation set: identifiers (keywords included, they
    /// fail as bare terms) and interpolations, never a `(`-group.
    fn starts_adjacent_term(&mut self) -> bool {
        matches!(self.pk(), Some(Token::Ident(_) | Token::Interpolated { .. }))
    }

    /// A `not`/`and`/`or` identifier with no gap before a `(`: the keyword and
    /// its `(` position, as the `error/invalid_function_name.hrx` and
    /// `error/missing_whitepsace.hrx` goldens report them.
    fn glued_kw(&mut self) -> Option<(String, BytePos)> {
        let mut it = self.it.clone();
        let first = it.next()?;
        let second = it.next()?;
        let Token::Ident(word) = &first.token else {
            return None;
        };
        if !is_kw(word) {
            return None;
        }
        let glued =
            matches!(second.token, Token::Punct(Punct::LParen)) && second.span.start == first.span.end;
        glued.then(|| (word.clone(), second.span.start))
    }

    /// A golden-style single-caret error at the current token's first column.
    fn caret_err(&mut self, msg: &str) -> SassError {
        let at = self.pk_start();
        let p = self.p;
        p.err_marked(msg, at, vec![p.caret(at)])
    }
}
