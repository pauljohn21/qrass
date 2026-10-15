//! Module-syntax statement parsing (`@use` / `@forward`), split out of
//! [`parser_at`] to keep that file within the size budget. These builders
//! reconstruct the module URL, alias, `as prefix*` rename, `show`/`hide` member
//! lists and the `with (…)` configuration from a raw token run.

use crate::ast::{Stmt, Use, UseAlias};
use crate::ast_expr::{Arg, Expr};
use crate::error::SassError;
use crate::span::{BytePos, Span};
use crate::token::{Punct, Spanned, Token};

use super::Parser;
use super::parser_at::{find_ident, split_paren};
use super::parser_expr::args_of;

type Or<T> = Result<T, SassError>;

pub(crate) fn build_use(p: &Parser, rest: &[Spanned], span: Span) -> Or<Stmt> {
    Ok(Stmt::Use(Use {
        url: module_url(rest),
        alias: use_alias(rest),
        config: with_config(p, rest)?,
        span,
    }))
}

pub(crate) fn build_forward(p: &Parser, rest: &[Spanned], span: Span) -> Or<Stmt> {
    Ok(Stmt::Forward(crate::ast::Forward {
        url: module_url(rest),
        prefix: forward_prefix(rest),
        shown: forward_members(rest, "show"),
        hidden: forward_members(rest, "hide"),
        config: with_config(p, rest)?,
        span,
    }))
}

/// Reconstruct the `as <prefix>*` rename prefix. The lexer splits `c-*` into
/// `Ident("c")` `Punct(Minus)` `Punct(Star)`, so re-join the idents and hyphen /
/// dot puncts up to the `*` into the literal prefix (`c-`).
fn forward_prefix(rest: &[Spanned]) -> String {
    let Some(as_i) = find_ident(rest, "as") else {
        return String::new();
    };
    let mut out = String::new();
    for s in &rest[as_i + 1..] {
        match &s.token {
            Token::Ident(t) => out.push_str(t),
            Token::Punct(Punct::Minus) => out.push('-'),
            Token::Punct(Punct::Dot) => out.push('.'),
            Token::Punct(Punct::Star) => return out,
            _ => break,
        }
    }
    String::new()
}

/// Collect the comma-separated member names after a `show`/`hide` keyword,
/// stopping at `with` or the first non-name token. Variable members arrive as
/// `$name` (→ `name`); mixin/function members as bare idents.
fn forward_members(rest: &[Spanned], kw: &str) -> Vec<String> {
    let Some(start) = find_ident(rest, kw) else {
        return Vec::new();
    };
    rest[start + 1..]
        .iter()
        .take_while(|s| match &s.token {
            Token::Ident(name) => name != "with",
            Token::Variable(_) => true,
            Token::Punct(Punct::Comma) => true,
            _ => false,
        })
        .filter_map(|s| match &s.token {
            Token::Ident(name) if name != "with" => Some(name.clone()),
            Token::Variable(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn module_url(rest: &[Spanned]) -> String {
    rest.first()
        .map(|s| match &s.token {
            Token::Str { text, .. } => text.clone(),
            other => {
                let mut v = Vec::new();
                crate::parser::token_to_pieces(other, &mut v);
                v.iter().fold(String::new(), |mut a, p| {
                    if let crate::token::Piece::Text(t) = p {
                        a.push_str(t);
                    }
                    a
                })
            }
        })
        .unwrap_or_default()
}

fn use_alias(rest: &[Spanned]) -> UseAlias {
    match find_ident(rest, "as")
        .and_then(|i| rest.get(i + 1))
        .map(|s| &s.token)
    {
        Some(Token::Punct(Punct::Star)) => UseAlias::Wildcard,
        Some(Token::Ident(name)) => UseAlias::Named(name.clone()),
        _ => UseAlias::Default,
    }
}

fn with_config(p: &Parser, rest: &[Spanned]) -> Or<Vec<(String, Expr)>> {
    match find_ident(rest, "with").and_then(|i| split_paren(&rest[i + 1..]).1) {
        Some(paren) => args_of(p, paren)?
            .into_iter()
            .map(|a| match a {
                Arg::Named { name, value } => Ok((name, value)),
                _ => Err(p.err(
                    "@use/@forward `with` expects named configuration",
                    BytePos(0),
                )),
            })
            .collect(),
        None => Ok(Vec::new()),
    }
}
