//! Grouped expression literals for the owned-run cursor — a sibling `impl`
//! block on [`E`] split out of `parser_expr` for the 500-line budget
//! (round 73): the parenthesized group / map `(…)` and the CSS-`[]` bracketed
//! list.

use crate::ast_expr::{Expr, ListSep};
use crate::span::BytePos;
use crate::token::Punct;

use super::parser_expr::E;

type Ex<T> = Result<T, crate::error::SassError>;

impl<'a> E<'a> {
    pub(crate) fn paren_group(&mut self, start: BytePos) -> Ex<Expr> {
        if self.eat(Punct::RParen) {
            // `()` is dart's dual-valued empty literal: `meta.type-of` calls
            // it a map, list/channel operations see an empty list. The
            // dualism is resolved at the VALUE layer (`SassValue::is_empty_paren`
            // sites below), so the parser keeps the plain empty-list shape
            // (`lab(())` demands "Color component list may not be empty.",
            // `color/lab/error` `list/too_few_channels`).
            return Ok(Expr::List {
                items: Vec::new(),
                sep: ListSep::Comma,
                bracketed: false,
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
                bracketed: false,
                span: self.span(start),
            });
        }
        self.expect(Punct::RParen, "expected ')'")?;
        Ok(Expr::Paren(Box::new(first), self.span(start)))
    }

    /// A CSS-`[]` bracketed list literal (`[a b c]`, `[1, 2]`, `[]`). Unlike a
    /// parenthesized group, a single element stays a one-item list and no map is
    /// possible; the `bracketed` flag round-trips through serialization and drives
    /// `list.is-bracketed`.
    pub(crate) fn bracket_group(&mut self, start: BytePos) -> Ex<Expr> {
        if self.eat(Punct::RBracket) {
            return Ok(Expr::List {
                items: Vec::new(),
                sep: ListSep::Comma,
                bracketed: true,
                span: self.span(start),
            });
        }
        let first = self.space_list()?;
        if self.is(Punct::Comma) {
            let mut items = vec![first];
            while self.eat(Punct::Comma) && !self.is(Punct::RBracket) {
                items.push(self.space_list()?);
            }
            self.expect(Punct::RBracket, "expected ']' to close bracketed list")?;
            return Ok(Expr::List {
                items,
                sep: ListSep::Comma,
                bracketed: true,
                span: self.span(start),
            });
        }
        self.expect(Punct::RBracket, "expected ']' to close bracketed list")?;
        // A space run like `[1 2]` is a bracketed list of the run's items, NOT a
        // one-element list wrapping a nested space list. `space_list()` already
        // grouped the run into a flat (unbracketed) space list, so adopt its items
        // under the bracket flag directly — otherwise `list.length([1 2])` reads 1.
        let (items, sep) = match first {
            Expr::List {
                items,
                sep,
                bracketed: false,
                ..
            } => (items, sep),
            other => (vec![other], ListSep::Space),
        };
        Ok(Expr::List {
            items,
            sep,
            bracketed: true,
            span: self.span(start),
        })
    }
}
