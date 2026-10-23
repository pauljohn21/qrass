//! Expression fold for [`Parser`]: consumes an **owned token run** by move
//! (a `Peekable<vec::IntoIter>`, never a shared cursor) and returns an owned
//! [`Expr`] tree. Task group 4 (operator precedence, calls, maps, lists, args,
//! parameters, interpolation).

use std::iter::Peekable;
use std::vec::IntoIter;

use crate::ast::Param;
use crate::ast_expr::{Arg, BinOp, Expr, FunctionCall, ListSep, Literal, UnaryOp};
use crate::error::SassError;
use crate::span::{BytePos, Span};
use crate::token::{Punct, Spanned, Token};

use super::Parser;

/// Recursive-descent over an owned token iterator. It borrows the parser only to
/// read source text for error locations — it never mutates shared parse state.
/// The argument / parameter tables (`args` / `params`) fold lives in the sibling
/// `parser_args` module as a second `impl` block on this cursor.
pub(crate) struct E<'a> {
    pub(crate) it: Peekable<IntoIter<Spanned>>,
    pub(crate) last: Span,
    pub(crate) p: &'a Parser,
}

type Ex<T> = Result<T, SassError>;

/// The lazy operator tail of a precedence-climbing parse: each `next` consumes
/// one operator that binds at least `min` tightly, parses its right operand and
/// yields the `(op, rhs, span)` triple the fold wraps into a `Binary`.
struct BinTail<'e, 'p> {
    e: &'e mut E<'p>,
    min: u8,
    start: BytePos,
}

impl Iterator for BinTail<'_, '_> {
    type Item = Ex<(BinOp, Expr, Span)>;

    fn next(&mut self) -> Option<Self::Item> {
        let (op, prec) = self.e.next_binop()?;
        match prec >= self.min {
            false => None,
            true => {
                self.e.bump();
                let right = self.e.binary(prec + 1);
                Some(right.map(|right| (op, right, self.e.span(self.start))))
            }
        }
    }
}

// ─── Public entry points (each takes an owned run) ─────────────────────────────

pub(crate) fn value_of(p: &Parser, run: Vec<Spanned>) -> Ex<Expr> {
    E::new(p, run).value()
}

pub(crate) fn args_of(p: &Parser, run: Vec<Spanned>) -> Ex<Vec<Arg>> {
    E::new(p, run).args()
}

pub(crate) fn params_of(p: &Parser, run: Vec<Spanned>) -> Ex<Vec<Param>> {
    E::new(p, run).params()
}

impl<'a> E<'a> {
    fn new(p: &'a Parser, run: Vec<Spanned>) -> Self {
        let last = run.first().map(|s| s.span).unwrap_or(Span {
            file: p.file,
            start: BytePos(0),
            end: BytePos(0),
        });
        Self {
            it: run.into_iter().peekable(),
            last,
            p,
        }
    }

    // ─── Cursor primitives (peek/bump over owned tokens) ───────────────────

    pub(crate) fn pk(&mut self) -> Option<&Token> {
        self.it.peek().map(|s| &s.token)
    }

    pub(crate) fn is(&mut self, x: Punct) -> bool {
        matches!(self.pk(), Some(Token::Punct(q)) if *q == x)
    }

    pub(crate) fn bump(&mut self) -> Option<Spanned> {
        let t = self.it.next();
        if let Some(s) = &t {
            self.last = s.span;
        }
        t
    }

    pub(crate) fn eat(&mut self, x: Punct) -> bool {
        if self.is(x) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn expect(&mut self, x: Punct, msg: &str) -> Ex<()> {
        if self.eat(x) {
            Ok(())
        } else {
            Err(self.p.err(msg, self.last.start))
        }
    }

    fn starts_value(&mut self) -> bool {
        matches!(
            self.pk(),
            Some(
                Token::Number { .. }
                | Token::Str { .. }
                | Token::HexColor(_)
                | Token::Url(_)
                | Token::Interpolated { .. }
                | Token::Variable(_)
                | Token::Ident(_)
                | Token::Placeholder(_)
                | Token::Ampersand
                    | Token::Punct(
                        Punct::LParen
                            | Punct::LBracket
                            | Punct::Minus
                            | Punct::Plus
                            // A lone `%` is a value token (plain-CSS `attr(c, %)`
                            // type, `b: %`); `%` is not an infix operator here (a
                            // `50%` percentage lexes as a single Number-with-unit).
                            | Punct::Percent,
                    )
            )
        )
    }

    pub(crate) fn span(&self, start: BytePos) -> Span {
        Span {
            file: self.p.file,
            start,
            end: self.last.end,
        }
    }

    // ─── Value / list levels ───────────────────────────────────────────────

    /// A whole run folded to one expression: a comma list or a single value.
    fn value(&mut self) -> Ex<Expr> {
        if self.it.peek().is_none() {
            return Err(self.p.err("expected a value", self.last.start));
        }
        let start = self.pk_start();
        let first = self.space_list()?;
        // a trailing comma before `)`/EOF is tolerated: consumed, no item parsed
        let more = std::iter::from_fn(|| {
            (self.eat(Punct::Comma) && !(self.is(Punct::RParen) || self.it.peek().is_none()))
                .then(|| self.space_list())
        })
        .collect::<Ex<Vec<_>>>()?;
        let items: Vec<Expr> = std::iter::once(first).chain(more).collect();
        match items.len() {
            1 => Ok(items.into_iter().next().expect("one item")),
            _ => Ok(Expr::List {
                items,
                sep: ListSep::Comma,
                bracketed: false,
                span: self.span(start),
            }),
        }
    }

    pub(crate) fn pk_start(&mut self) -> BytePos {
        self.it
            .peek()
            .map(|s| s.span.start)
            .unwrap_or(self.last.start)
    }

    /// A whitespace-separated operand run (`margin: 0 auto`).
    pub(crate) fn space_list(&mut self) -> Ex<Expr> {
        let start = self.pk_start();
        let first = self.binary(0)?;
        let more = std::iter::from_fn(|| {
            (self.starts_value() && !self.is(Punct::Comma)).then(|| self.binary(0))
        })
        .collect::<Ex<Vec<_>>>()?;
        let items: Vec<Expr> = std::iter::once(first).chain(more).collect();
        match items.len() {
            1 => Ok(items.into_iter().next().expect("one item")),
            _ => Ok(Expr::List {
                items,
                sep: ListSep::Space,
                bracketed: false,
                span: self.span(start),
            }),
        }
    }

    // ─── Operator precedence ───────────────────────────────────────────────

    fn binary(&mut self, min: u8) -> Ex<Expr> {
        let start = self.pk_start();
        let left = self.unary()?;
        // fold the operator tail: each accepted `op rhs` wraps the accumulated left
        BinTail {
            e: self,
            min,
            start,
        }
        .try_fold(left, |left, item| {
            let (op, right, span) = item?;
            Ok(Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            })
        })
    }

    fn next_binop(&mut self) -> Option<(BinOp, u8)> {
        let op = match self.pk() {
            Some(Token::Punct(p)) => match p {
                Punct::Plus => Some(BinOp::Add),
                Punct::Minus => Some(BinOp::Sub),
                Punct::Star => Some(BinOp::Mul),
                Punct::Slash => Some(BinOp::Div),
                Punct::EqualsEquals => Some(BinOp::Eq),
                Punct::Eq => Some(BinOp::Assign),
                Punct::Ne => Some(BinOp::Ne),
                Punct::Lt => Some(BinOp::Lt),
                Punct::Le => Some(BinOp::Le),
                Punct::Gt => Some(BinOp::Gt),
                Punct::Ge => Some(BinOp::Ge),
                _ => None,
            },
            Some(Token::Ident(w)) if w == "and" => Some(BinOp::And),
            Some(Token::Ident(w)) if w == "or" => Some(BinOp::Or),
            _ => None,
        }?;
        let prec = match op {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Eq | BinOp::Ne => 3,
            BinOp::Assign => 3,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
            BinOp::Add | BinOp::Sub => 5,
            BinOp::Mul | BinOp::Div | BinOp::Rem | BinOp::IntDiv => 6,
        };
        Some((op, prec))
    }

    fn unary(&mut self) -> Ex<Expr> {
        let start = self.pk_start();
        let op = match self.pk() {
            Some(Token::Punct(Punct::Minus)) => Some(UnaryOp::Neg),
            Some(Token::Punct(Punct::Plus)) => Some(UnaryOp::Plus),
            Some(Token::Ident(w)) if w == "not" => Some(UnaryOp::Not),
            _ => None,
        };
        match op {
            Some(o) => {
                self.bump();
                let operand = self.unary()?;
                Ok(Expr::Unary {
                    op: o,
                    operand: Box::new(operand),
                    span: self.span(start),
                })
            }
            None => self.primary(),
        }
    }

    // ─── Primary ───────────────────────────────────────────────────────────

    fn primary(&mut self) -> Ex<Expr> {
        let start = self.pk_start();
        let tok = self
            .bump()
            .ok_or_else(|| self.p.err("unexpected end of expression", self.last.start))?;
        match tok.token {
            Token::Number { value, unit } => Ok(Expr::Literal(Literal::Number { value, unit })),
            Token::HexColor(raw) => Ok(Expr::Literal(Literal::Color(raw))),
            Token::Url(raw) => Ok(Expr::Literal(Literal::Url(raw))),
            Token::Str { text, quote } => Ok(Expr::Literal(Literal::Str { text, quote })),
            Token::Interpolated { quote, pieces } => Ok(Expr::Interpolated {
                quote,
                pieces,
                span: self.span(start),
            }),
            Token::Ampersand => Ok(Expr::Literal(Literal::Ident("&".into()))),
            Token::Placeholder(p) => Ok(Expr::Literal(Literal::Ident(format!("%{p}")))),
            Token::Punct(Punct::Percent) => Ok(Expr::Literal(Literal::Ident("%".into()))),
            // `member.hrx` goldens: a leading `.` reads as the start of a number
            // (`.5` glues in the lexer, so a surviving bare dot is a syntax
            // error demanding the digit), and a lone `$` opens with an
            // identifier.
            Token::Punct(Punct::Dot) => Err(self.p.err("Expected digit.", start)),
            Token::Variable(name) if name.is_empty() => {
                Err(self.p.err("Expected identifier.", start))
            }
            Token::Variable(name) => Ok(Expr::Variable(name, self.span(start))),
            Token::Ident(name) => self.ident_value(name, start),
            Token::Punct(Punct::LParen) => self.paren_group(start),
            Token::Punct(Punct::LBracket) => self.bracket_group(start),
            other => Err(self
                .p
                .err(format!("unexpected token in value: {other:?}"), start)),
        }
    }

    fn ident_value(&mut self, name: String, start: BytePos) -> Ex<Expr> {
        // A leading `.` glued onto the next word (the selector glue) still
        // opens a *number* for the goldens' value parser: `a: .member()` errors with
        // `Expected digit.` (member.hrx `function/no_namespace`).
        if let Some(rest) = name.strip_prefix('.')
            && !rest.starts_with('.')
        {
            return Err(self.p.err("Expected digit.", start));
        }
        // Namespaced call `ns.fn(...)`. The lexer glues a leading `.` into the next
        // word (`.a` selectors), so `map.get` arrives as adjacent `Ident("map")` +
        // `Ident(".get")` with no whitespace gap — span adjacency disambiguates it
        // from a space-separated list. A literal `Punct::Dot` is also accepted.
        if let Some(Token::Ident(next)) = self.pk().cloned() {
            let adjacent = self
                .it
                .peek()
                .is_some_and(|s| s.span.start == self.last.end);
            if next.starts_with('.') && adjacent {
                self.bump();
                return self.namespaced(name, start, next[1..].to_string());
            }
        }
        if self.is(Punct::Dot) {
            self.bump();
            // `ns.$var` — namespaced variable member.
            if let Some(Token::Variable(vname)) = self.pk().cloned() {
                let vstart = self.it.peek().map(|s| s.span.start).unwrap_or(self.last.end);
                self.bump();
                // `namespace.$` demands the member identifier; a `_`-prefixed
                // member is private to its module (member.hrx goldens).
                if vname.is_empty() {
                    return Err(self.p.err("Expected identifier.", vstart));
                }
                if vname.starts_with('_') {
                    return Err(self.p.err(
                        "Private members can't be accessed from outside their modules.",
                        vstart,
                    ));
                }
                return Ok(Expr::Member {
                    namespace: name,
                    name: vname,
                    span: self.span(start),
                });
            }
            let fstart = self
                .it
                .peek()
                .map(|s| s.span.start)
                .unwrap_or(self.last.end);
            let fname = match self.bump().map(|s| s.token) {
                Some(Token::Ident(f)) => f,
                // `namespace.()`: the goldens keep asking for the identifier.
                _ => return Err(self.p.err("Expected identifier.", fstart)),
            };
            return self.namespaced(name, start, fname);
        }
        // Bare call `fn(...)`.
        if self.is(Punct::LParen) {
            // Modern `if(…)` clause syntax (per `expressions/if/*.hrx`); the legacy 3-arg builtin shape
            // (a top-level comma first) still falls through to `args()` below.
            if name == "if" && self.looks_like_modern_if() {
                return self.modern_if(start);
            }
            let args = self.args()?;
            return Ok(Expr::FunctionCall(FunctionCall {
                namespace: None,
                name,
                args,
                span: self.span(start),
            }));
        }
        // CSS color keywords become color values (per the goldens); `transparent`
        // is not in the table and stays an unquoted identifier.
        let lower = name.to_ascii_lowercase();
        if let Some((cname, hex)) = crate::parser::named_colors::named_color(&lower) {
            return Ok(Expr::Literal(Literal::NamedColor { hex, name: cname }));
        }
        Ok(match name.as_str() {
            "true" => Expr::Literal(Literal::Bool(true)),
            "false" => Expr::Literal(Literal::Bool(false)),
            "null" => Expr::Literal(Literal::Null),
            _ => Expr::Literal(Literal::Ident(name)),
        })
    }

    fn namespaced(&mut self, ns: String, start: BytePos, fname: String) -> Ex<Expr> {
        // a `_`-prefixed member is private to its module — a *parse-time*
        // syntax error per the member.hrx goldens' comment section.
        if fname.starts_with('_') {
            return Err(self.p.err(
                "Private members can't be accessed from outside their modules.",
                start,
            ));
        }
        let args = if self.is(Punct::LParen) {
            self.args()?
        } else {
            Vec::new()
        };
        Ok(Expr::FunctionCall(FunctionCall {
            namespace: Some(ns),
            name: fname,
            args,
            span: self.span(start),
        }))
    }


    // Argument / parameter table folding (`args` / `params`) lives in the sibling
    // `parser_args` module as a second `impl` block on this cursor.
}
