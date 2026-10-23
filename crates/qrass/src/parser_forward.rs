//! `@forward` statement building and its strict grammar (split from
//! `parser_module` for the 500-line budget, round 72). The builder
//! reconstructs URL / prefix / show-hide lists / config from the raw token run;
//! `validate_forward` claims exactly the malformed spellings pinned by
//! `directives/forward/error/syntax.hrx` before the run is ever rebuilt.

use crate::ast::Stmt;
use crate::error::SassError;
use crate::span::Span;
use crate::token::{Punct, Spanned, Token};

use super::parser_at::find_ident;
use super::parser_module::{module_url, reject_builtin_config, strip_comments, with_config};
use super::Parser;

type Or<T> = Result<T, SassError>;

pub(crate) fn build_forward(p: &Parser, rest: &[Spanned], span: Span) -> Or<Stmt> {
    let rest = strip_comments(rest);
    validate_forward(p, &rest, span)?;
    let url = module_url(&rest);
    let config = with_config(p, &rest, true)?;
    reject_builtin_config(p, &url, &config, span, &rest)?;
    Ok(Stmt::Forward(crate::ast::Forward {
        url,
        prefix: forward_prefix(&rest),
        shown: forward_members(&rest, "show"),
        hidden: forward_members(&rest, "hide"),
        config,
        span,
    }))
}


/// the goldens parse `@forward` against a strict grammar and reports malformed
/// spellings at the statement (before the URL is ever loaded). The
/// `directives/forward/error/syntax.hrx` goldens pin each message:
/// * the URL must be a quoted string — `Expected string.`;
/// * `as <prefix>*` — the prefix starts with an identifier (`Expected
///   identifier.`) and must run to a `*` (`expected "*".`);
/// * `show`/`hide` take a non-empty list of variable/mixin/function names
///   (`Expected variable, mixin, or function name`), and a second config keyword
///   where the statement should end is the goldens' `expected ";".`.
///
/// Only clearly-invalid spellings are claimed; anything else keeps today's
/// reconstruct-and-load behavior.
fn validate_forward(p: &Parser, rest: &[Spanned], span: Span) -> Or<()> {
    let at = |i: usize| {
        rest.get(i)
            .map(|s| s.span.start)
            .unwrap_or(span.start)
    };
    let end = |t: &Token| matches!(t, Token::Punct(Punct::Semicolon) | Token::Punct(Punct::LBrace));
    // 1. The URL word.
    match rest.first() {
        None => return Err(p.err("Expected string.", span.start)),
        Some(s) if matches!(s.token, Token::Str { .. }) => {}
        Some(s) => return Err(p.err("Expected string.", s.span.start)),
    }
    // 2. `as <ident>(-|.)* *`
    if let Some(i) = find_ident(rest, "as") {
        let after = &rest[i + 1..];
        match after.first() {
            None => return Err(p.err("Expected identifier.", at(i))),
            Some(s) if end(&s.token) => return Err(p.err("Expected identifier.", s.span.start)),
            Some(s) if matches!(s.token, Token::Ident(_)) => {}
            Some(s) => return Err(p.err("Expected identifier.", s.span.start)),
        }
        let mut j = 1usize;
        loop {
            match after.get(j) {
                None => return Err(p.err("expected \"*\".", at(i))),
                Some(s) => match &s.token {
                    Token::Punct(Punct::Star) => break,
                    Token::Ident(_) | Token::Punct(Punct::Minus) | Token::Punct(Punct::Dot) => {
                        j += 1
                    }
                    _ => return Err(p.err("expected \"*\".", s.span.start)),
                },
            }
        }
    }
    // 3. `show` / `hide` member lists.
    for kw in ["show", "hide"] {
        let Some(i) = find_ident(rest, kw) else { continue };
        let mut j = i + 1;
        let mut expect_name = true;
        while let Some(s) = rest.get(j) {
            match &s.token {
                Token::Ident(name) if name == "with" => break, // `with (…)` handled by with_config
                Token::Variable(name) if expect_name && name.is_empty() => {
                    // a lone `$` (no name after it) is not a member name
                    return Err(p.err("Expected variable, mixin, or function name", s.span.start));
                }
                _ if expect_name && matches!(&s.token, Token::Variable(_) | Token::Ident(_)) => {
                    if let Token::Ident(name) = &s.token {
                        match name.as_str() {
                            // A second config keyword (or `as`) where a member
                            // name belongs: the goldens wanted the statement to end.
                            "show" | "hide" | "as" => {
                                return Err(p.err("expected \";\".", s.span.start));
                            }
                            _ => {}
                        }
                    }
                    expect_name = false;
                    j += 1;
                }
                _ if expect_name => {
                    return Err(p.err("Expected variable, mixin, or function name", s.span.start));
                }
                Token::Punct(Punct::Comma) => {
                    expect_name = true;
                    j += 1;
                }
                Token::Punct(Punct::Semicolon) | Token::Punct(Punct::LBrace) => break,
                Token::Punct(Punct::LParen) => break, // `with (…` already broken above
                _ => return Err(p.err("expected \";\".", s.span.start)),
            }
        }
        if expect_name {
            // ran out (or ended right after a comma): a member name was still owed
            return Err(p.err("Expected variable, mixin, or function name", at(j)));
        }
    }
    Ok(())
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
            // Keep the `$` spelling: `show`/`hide` entries are type-scoped —
            // `$name` targets variables, a bare name targets mixins/functions
            // (`forward/member/visibility` `hide/wrong_type/*`).
            Token::Variable(name) => Some(format!("${name}")),
            _ => None,
        })
        .collect()
}
