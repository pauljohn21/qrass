//! Argument / parameter table folding for the expression cursor [`E`]. Split out
//! of `parser_expr.rs` to keep that module within the source-size budget; it is a
//! second `impl` block on the same owned-token cursor, whose shared primitives
//! (`pk` / `bump` / `eat` / `expect` / `space_list` / `span` / …) live there as
//! `pub(crate)`.

use crate::ast::Param;
use crate::ast_expr::Arg;
use crate::error::SassError;
use crate::token::{Punct, Token};

use super::parser_expr::E;

type Ex<T> = Result<T, SassError>;

impl<'a> E<'a> {
    pub(crate) fn args(&mut self) -> Ex<Vec<Arg>> {
        self.expect(Punct::LParen, "expected '(' for arguments")?;
        let args = self.arg_slots()?;
        self.expect(Punct::RParen, "expected ')' after arguments")?;
        Ok(args)
    }

    /// `arg (, arg)*` up to the closing paren: the separator is consumed by the
    /// continuation test, so a trailing comma before `)` ends the run without
    /// eating the `)` (the old `loop`/`break` shape, expressed recursively).
    fn arg_slots(&mut self) -> Ex<Vec<Arg>> {
        match self.is(Punct::RParen) {
            true => Ok(Vec::new()),
            false =>
                self.parse_arg().and_then(|arg| {
                    Ok(match self.eat(Punct::Comma) && !self.is(Punct::RParen) {
                        true => std::iter::once(arg).chain(self.arg_slots()?).collect(),
                        false => vec![arg],
                    })
                }),
        }
    }

    /// One argument slot: named `$k: v` (the parser folds the `$`-prefixed key),
    /// a splat when a trailing `...` follows the expression (`$args...`, `1...`,
    /// `foo()...`), else positional.
    fn parse_arg(&mut self) -> Ex<Arg> {
        let named = match (self.pk().cloned(), self.it.clone().nth(1).map(|s| s.token)) {
            (Some(Token::Variable(n)), Some(Token::Punct(Punct::Colon))) => Some(n),
            _ => None,
        };
        match named {
            Some(name) => {
                self.bump();
                self.bump();
                Ok(Arg::Named {
                    name,
                    value: self.space_list()?,
                })
            }
            None => {
                let value = self.space_list()?;
                Ok(match self.eat(Punct::Ellipsis) {
                    true => Arg::Splat(value),
                    false => Arg::Positional(value),
                })
            }
        }
    }

    pub(crate) fn params(&mut self) -> Ex<Vec<Param>> {
        self.expect(Punct::LParen, "expected '(' for parameters")?;
        let params = self.param_slots()?;
        self.expect(Punct::RParen, "expected ')' after parameters")?;
        Ok(params)
    }

    /// `param (, param)*` with the same separator discipline as `arg_slots`.
    fn param_slots(&mut self) -> Ex<Vec<Param>> {
        match self.is(Punct::RParen) {
            true => Ok(Vec::new()),
            false => self.parse_param().and_then(|param| {
                Ok(match self.eat(Punct::Comma) && !self.is(Punct::RParen) {
                    true => std::iter::once(param).chain(self.param_slots()?).collect(),
                    false => vec![param],
                })
            }),
        }
    }

    /// One `$name[: default][...]` parameter.
    fn parse_param(&mut self) -> Ex<Param> {
        let start = self.pk_start();
        let name = match self.pk().cloned() {
            Some(Token::Variable(n)) => n,
            _ => return Err(self.p.err("expected parameter name", start)),
        };
        self.bump();
        let default = self
            .eat(Punct::Colon)
            .then(|| self.space_list())
            .transpose()?;
        let rest = self.eat(Punct::Ellipsis);
        Ok(Param {
            name,
            default,
            rest,
            span: self.span(start),
        })
    }
}
