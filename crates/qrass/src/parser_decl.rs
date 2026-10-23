//! Statement-head value syntax split out of `parser_at` for the 500-line budget
//! (round 71): the `$var: value` declaration, the plain property declaration
//! (with the member.hrx `!global`-into-another-module check), and the
//! `.`-member at-rule head checks (`@function`/`@mixin`/`@include`).

use crate::ast::{Declaration, Stmt, VariableDecl};
use crate::error::SassError;
use crate::span::Span;
use crate::token::{Punct, Spanned, Token};

use super::parser_at::{expr, split_flags};
use super::Parser;

type Or<T> = Result<T, SassError>;

/// The `.`-member syntax errors pinned by `directives/use/error/syntax/member.hrx`
/// for at-rule heads: a leading `.member` is an identifier expectation, a
/// `_`-prefixed member is private to its module (a *parse-time* error), a
/// trailing/empty member (`@include namespace.`) again wants the identifier,
/// and a `.`-joined name on `@function`/`@mixin` means the goldens still expects the
/// parameter list (`expected "(".`) or the block (`expected "{".`) — only
/// `@include ns.member` is the legal namespaced spelling.
pub(crate) fn check_atrule_member_syntax(
    p: &Parser,
    name_run: &[Spanned],
    is_mixin: bool,
    is_include: bool,
) -> Or<()> {
    let dotish = |s: &Spanned| match &s.token {
        Token::Punct(Punct::Dot) => Some("".to_string()),
        Token::Ident(w) => w.strip_prefix('.').map(str::to_owned),
        _ => None,
    };
    for (i, s) in name_run.iter().enumerate() {
        let Some(member) = dotish(s) else { continue };
        if i == 0 {
            // a member with no namespace at all: `@include .member` — the goldens
            // cannot open an identifier on the dot.
            return Err(p.err("Expected identifier.", s.span.start));
        }
        let member = if member.is_empty() {
            // bare `.`: the member name is the next token, if any
            match name_run.get(i + 1) {
                Some(n) => match &n.token {
                    Token::Ident(w) => w.clone(),
                    Token::Variable(w) => w.clone(),
                    _ => return Err(p.err("Expected identifier.", n.span.start)),
                },
                None => return Err(p.err("Expected identifier.", s.span.start)),
            }
        } else {
            member
        };
        if member.is_empty() || member.starts_with('_') {
            return Err(p.err(
                "Private members can't be accessed from outside their modules.",
                s.span.start,
            ));
        }
        if !is_include {
            return Err(p.err(
                if is_mixin { "expected \"{\"." } else { "expected \"(\"." },
                s.span.start,
            ));
        }
    }
    Ok(())
}

pub(crate) fn variable_decl(p: &Parser, run: &[Spanned], span: Span) -> Or<Stmt> {
    let name = match &run[0].token {
        Token::Variable(v) if v.is_empty() => {
            // a lone `$` is not a declaration head: the goldens want the name first
            return Err(p.err("Expected identifier.", run[0].span.start));
        }
        Token::Variable(v) => v.clone(),
        _ => return Err(p.err("expected variable name", span.start)),
    };
    let colon = Parser::first_colon(run)
        .ok_or_else(|| p.err("expected ':' after variable name", span.start))?;
    let (val_run, flags) = split_flags(&run[colon + 1..]);
    Ok(Stmt::Variable(VariableDecl {
        name,
        value: expr(p, val_run)?,
        default: flags.iter().any(|f| f == "default"),
        global: flags.iter().any(|f| f == "global"),
        span,
    }))
}

pub(crate) fn declaration(p: &Parser, run: &[Spanned], span: Span) -> Or<Stmt> {
    let colon =
        Parser::first_colon(run).ok_or_else(|| p.err("expected ':' in declaration", span.start))?;
    let (val_run, flags) = split_flags(&run[colon + 1..]);
    // member.hrx `variable/global` golden: `namespace.$member: value !global;`
    // is a member assignment into another module — the `!global` flag can never
    // apply there.
    let head = &run[..colon];
    let member_assign = head
        .iter()
        .any(|s| matches!(&s.token, Token::Variable(_)))
        && head.iter().any(|s| match &s.token {
            Token::Punct(Punct::Dot) => true,
            Token::Ident(w) => w.starts_with('.'),
            _ => false,
        });
    if member_assign && flags.iter().any(|f| f == "global") {
        return Err(p.err(
            "!global isn't allowed for variables in other modules.",
            span.start,
        ));
    }
    // A lone `$` where the value belongs: the goldens' declaration parse never reaches
    // an expression there and keeps demanding the colon (`@supports (a !:$)`,
    // `syntax/anything/colon` golden `expected ":".`).
    if val_run.len() == 1 && matches!(&val_run[0].token, Token::Variable(v) if v.is_empty()) {
        return Err(p.err("expected \":\".", run[colon].span.start));
    }
    Ok(Stmt::Declaration(Declaration {
        prop: p.pieces(&run[..colon]),
        value: expr(p, val_run)?,
        important: flags.iter().any(|f| f == "important"),
        span,
    }))
}
