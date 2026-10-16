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
        let mut args = Vec::new();
        if self.eat(Punct::RParen) {
            return Ok(args);
        }
        loop {
            // Named argument `$k: v` (the parser folds the `$`-prefixed key).
            if let Some(Token::Variable(n)) = self.pk().cloned()
                && matches!(
                    self.it.clone().nth(1).map(|s| s.token),
                    Some(Token::Punct(Punct::Colon))
                )
            {
                self.bump();
                self.bump();
                let value = self.space_list()?;
                args.push(Arg::Named { name: n, value });
                if !(self.eat(Punct::Comma) && !self.is(Punct::RParen)) {
                    break;
                }
                continue;
            }
            // One expression per slot; a trailing `...` promotes it to a splat
            // (`$args...`, `1...`, `foo()...` all splat their expression).
            let value = self.space_list()?;
            if self.eat(Punct::Ellipsis) {
                args.push(Arg::Splat(value));
            } else {
                args.push(Arg::Positional(value));
            }
            if !(self.eat(Punct::Comma) && !self.is(Punct::RParen)) {
                break;
            }
        }
        self.expect(Punct::RParen, "expected ')' after arguments")?;
        Ok(args)
    }

    pub(crate) fn params(&mut self) -> Ex<Vec<Param>> {
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
