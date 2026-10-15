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
use crate::token::{Piece, Punct, Quote, Spanned, Token};

use super::Parser;

/// Recursive-descent over an owned token iterator. It borrows the parser only to
/// read source text for error locations — it never mutates shared parse state.
struct E<'a> {
    it: Peekable<IntoIter<Spanned>>,
    last: Span,
    p: &'a Parser,
}

type Ex<T> = Result<T, SassError>;

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

    fn pk(&mut self) -> Option<&Token> {
        self.it.peek().map(|s| &s.token)
    }

    fn is(&mut self, x: Punct) -> bool {
        matches!(self.pk(), Some(Token::Punct(q)) if *q == x)
    }

    fn bump(&mut self) -> Option<Spanned> {
        let t = self.it.next();
        if let Some(s) = &t {
            self.last = s.span;
        }
        t
    }

    fn eat(&mut self, x: Punct) -> bool {
        if self.is(x) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, x: Punct, msg: &str) -> Ex<()> {
        if self.eat(x) {
            Ok(())
        } else {
            Err(self.p.err(msg, self.last.start))
        }
    }

    fn starts_value(&mut self) -> bool {
        match self.pk() {
            Some(
                Token::Number { .. }
                | Token::Str { .. }
                | Token::HexColor(_)
                | Token::Url(_)
                | Token::Interpolated { .. }
                | Token::Variable(_)
                | Token::Ident(_)
                | Token::Placeholder(_)
                | Token::Ampersand,
            ) => true,
            Some(Token::Punct(Punct::LParen | Punct::Minus | Punct::Plus)) => true,
            _ => false,
        }
    }

    fn span(&self, start: BytePos) -> Span {
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
        let mut items = vec![self.space_list()?];
        while self.eat(Punct::Comma) {
            if self.is(Punct::RParen) || self.it.peek().is_none() {
                break; // tolerate a trailing comma
            }
            items.push(self.space_list()?);
        }
        Ok(if items.len() == 1 {
            items.pop().unwrap()
        } else {
            Expr::List {
                items,
                sep: ListSep::Comma,
                span: self.span(start),
            }
        })
    }

    fn pk_start(&mut self) -> BytePos {
        self.it
            .peek()
            .map(|s| s.span.start)
            .unwrap_or(self.last.start)
    }

    /// A whitespace-separated operand run (`margin: 0 auto`).
    fn space_list(&mut self) -> Ex<Expr> {
        let start = self.pk_start();
        let mut items = vec![self.binary(0)?];
        while self.starts_value() && !self.is(Punct::Comma) {
            items.push(self.binary(0)?);
        }
        Ok(if items.len() == 1 {
            items.pop().unwrap()
        } else {
            Expr::List {
                items,
                sep: ListSep::Space,
                span: self.span(start),
            }
        })
    }

    // ─── Operator precedence ───────────────────────────────────────────────

    fn binary(&mut self, min: u8) -> Ex<Expr> {
        let start = self.pk_start();
        let mut left = self.unary()?;
        while let Some((op, prec)) = self.next_binop() {
            if prec < min {
                break;
            }
            self.bump();
            let right = self.binary(prec + 1)?;
            let span = self.span(start);
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
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
            Token::Variable(name) => Ok(Expr::Variable(name, self.span(start))),
            Token::Ident(name) => self.ident_value(name, start),
            Token::Punct(Punct::LParen) => self.paren_group(start),
            other => Err(self
                .p
                .err(format!("unexpected token in value: {other:?}"), start)),
        }
    }

    fn ident_value(&mut self, name: String, start: BytePos) -> Ex<Expr> {
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
                self.bump();
                return Ok(Expr::Member {
                    namespace: name,
                    name: vname,
                    span: self.span(start),
                });
            }
            let fname = match self.bump().map(|s| s.token) {
                Some(Token::Ident(f)) => f,
                _ => return Err(self.p.err("expected function name after '.'", start)),
            };
            return self.namespaced(name, start, fname);
        }
        // Bare call `fn(...)`.
        if self.is(Punct::LParen) {
            let args = self.args()?;
            return Ok(Expr::FunctionCall(FunctionCall {
                namespace: None,
                name,
                args,
                span: self.span(start),
            }));
        }
        Ok(match name.as_str() {
            "true" => Expr::Literal(Literal::Bool(true)),
            "false" => Expr::Literal(Literal::Bool(false)),
            "null" => Expr::Literal(Literal::Null),
            _ => Expr::Literal(Literal::Ident(name)),
        })
    }

    fn namespaced(&mut self, ns: String, start: BytePos, fname: String) -> Ex<Expr> {
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

    fn paren_group(&mut self, start: BytePos) -> Ex<Expr> {
        if self.eat(Punct::RParen) {
            return Ok(Expr::List {
                items: Vec::new(),
                sep: ListSep::Comma,
                span: self.span(start),
            });
        }
        let first = self.space_list()?;
        if self.eat(Punct::Colon) {
            let mut pairs = vec![(first, self.space_list()?)];
            while self.eat(Punct::Comma) && !self.is(Punct::RParen) {
                let key = self.space_list()?;
                self.expect(Punct::Colon, "expected ':' in map")?;
                pairs.push((key, self.space_list()?));
            }
            self.expect(Punct::RParen, "expected ')' to close map")?;
            return Ok(Expr::Map {
                pairs,
                span: self.span(start),
            });
        }
        if self.is(Punct::Comma) {
            let mut items = vec![first];
            while self.eat(Punct::Comma) && !self.is(Punct::RParen) {
                items.push(self.space_list()?);
            }
            self.expect(Punct::RParen, "expected ')'")?;
            return Ok(Expr::List {
                items,
                sep: ListSep::Comma,
                span: self.span(start),
            });
        }
        self.expect(Punct::RParen, "expected ')'")?;
        Ok(Expr::Paren(Box::new(first), self.span(start)))
    }

    // ─── Argument / parameter tables ───────────────────────────────────────

    fn args(&mut self) -> Ex<Vec<Arg>> {
        self.expect(Punct::LParen, "expected '(' for arguments")?;
        let mut args = Vec::new();
        if self.eat(Punct::RParen) {
            return Ok(args);
        }
        loop {
            if let Some(Token::Variable(n)) = self.pk().cloned() {
                if matches!(
                    self.it.clone().nth(1).map(|s| s.token),
                    Some(Token::Punct(Punct::Colon))
                ) {
                    self.bump();
                    self.bump();
                    let value = self.space_list()?;
                    args.push(Arg::Named { name: n, value });
                    if !self.eat_named_comma(&mut args)? {
                        break;
                    }
                    continue;
                }
                if self.eat_var_ellipsis(&mut args, n)? {
                    break;
                }
            }
            let value = self.space_list()?;
            args.push(Arg::Positional(value));
            if !(self.eat(Punct::Comma) && !self.is(Punct::RParen)) {
                break;
            }
        }
        self.expect(Punct::RParen, "expected ')' after arguments")?;
        Ok(args)
    }

    /// Helper: consume `,` and continue unless a `)` follows.
    fn eat_named_comma(&mut self, _acc: &mut Vec<Arg>) -> Ex<bool> {
        Ok(self.eat(Punct::Comma) && !self.is(Punct::RParen))
    }

    /// Helper: `$rest...` splat → push and signal break; `$plain,` → false (continue
    /// as positional handled by caller).
    fn eat_var_ellipsis(&mut self, args: &mut Vec<Arg>, n: String) -> Ex<bool> {
        if matches!(
            self.it.clone().nth(1).map(|s| s.token),
            Some(Token::Punct(Punct::Ellipsis))
        ) {
            let start = self.pk_start();
            self.bump();
            self.bump();
            args.push(Arg::Splat(Expr::Variable(n, self.span(start))));
            return Ok(!(self.eat(Punct::Comma) && !self.is(Punct::RParen)));
        }
        Ok(false)
    }

    fn params(&mut self) -> Ex<Vec<Param>> {
        self.expect(Punct::LParen, "expected '(' for parameters")?;
        let mut params = Vec::new();
        if self.eat(Punct::RParen) {
            return Ok(params);
        }
        loop {
            let start = self.pk_start();
            let name = match self.pk().cloned() {
                Some(Token::Variable(n)) => n,
                _ => return Err(self.p.err("expected parameter name", start)),
            };
            self.bump();
            let default = if self.eat(Punct::Colon) {
                Some(self.space_list()?)
            } else {
                None
            };
            let rest = self.eat(Punct::Ellipsis);
            params.push(Param {
                name,
                default,
                rest,
                span: self.span(start),
            });
            if !(self.eat(Punct::Comma) && !self.is(Punct::RParen)) {
                break;
            }
        }
        self.expect(Punct::RParen, "expected ')' after parameters")?;
        Ok(params)
    }
}

// Keep imports that document the interpolation shape referenced above.
const _: Option<(Piece, Quote)> = None;
