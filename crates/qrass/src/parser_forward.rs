//! `@forward` statement building and its strict grammar (split from
//! `parser_module` for the 500-line budget, round 72). The builder
//! reconstructs URL / prefix / show-hide lists / config from the raw token run;
//! `validate_forward` claims exactly the malformed spellings pinned by
//! `directives/forward/error/syntax.hrx` before the run is ever rebuilt.

use crate::ast::Stmt;
use crate::error::SassError;
use crate::span::{BytePos, Span};
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
    // 1. The URL word.
    match rest.first() {
        None => return Err(p.err("Expected string.", span.start)),
        Some(s) if matches!(s.token, Token::Str { .. }) => {}
        Some(s) => return Err(p.err("Expected string.", s.span.start)),
    }
    // 2. `as <ident>(-|.)* *`
    find_ident(rest, "as").map_or(Ok(()), |i| {
        // the prefix run stops at the first token that cannot belong to it
        let after = &rest[i + 1..];
        let gap = after
            .iter()
            .skip(1)
            .position(|s| {
                !matches!(
                    &s.token,
                    Token::Ident(_) | Token::Punct(Punct::Minus) | Token::Punct(Punct::Dot)
                )
            })
            .map(|off| off + 1);
        match after.first() {
            None => Err(p.err("Expected identifier.", at_or(rest, span, i))),
            Some(s) if !matches!(s.token, Token::Ident(_)) => {
                Err(p.err("Expected identifier.", s.span.start))
            }
            Some(_) => match gap {
                Some(g) if matches!(after[g].token, Token::Punct(Punct::Star)) => Ok(()),
                Some(g) => Err(p.err("expected \"*\".", after[g].span.start)),
                None => Err(p.err("expected \"*\".", at_or(rest, span, i))),
            },
        }
    })?;
    // 3. `show` / `hide` member lists (the first failing list is reported).
    ["show", "hide"]
        .iter()
        .filter_map(|kw| find_ident(rest, kw))
        .try_fold((), |(), i| check_member_list(p, rest, span, i))
}

/// The offset the goldens blame for position `i`: the token's start, or the
/// statement start once the run is exhausted.
fn at_or(rest: &[Spanned], span: Span, i: usize) -> BytePos {
    rest.get(i).map_or(span.start, |s| s.span.start)
}

/// Validate one `show`/`hide` member list: `name (, name)*` up to `with`, `;`,
/// `{` or `(`.
fn check_member_list(p: &Parser, rest: &[Spanned], span: Span, i: usize) -> Or<()> {
    match member_walk(p, rest, i + 1, true) {
        Walk::Fail(e) => Err(e),
        // ran out (or ended right after a comma): a member name was still owed
        Walk::Stopped(j, true) => Err(p.err(
            "Expected variable, mixin, or function name",
            at_or(rest, span, j),
        )),
        Walk::Stopped(_, false) => Ok(()),
    }
}

/// One step of the member-list walk: `Stopped` ends it (the old `break`),
/// `Fail` carries the golden's error (the old early `return Err`).
enum Walk {
    Stopped(usize, bool),
    Fail(SassError),
}

fn member_walk(p: &Parser, rest: &[Spanned], j: usize, expect_name: bool) -> Walk {
    let Some(s) = rest.get(j) else {
        return Walk::Stopped(j, expect_name);
    };
    match &s.token {
        // `with (…)` is owned by `with_config`
        Token::Ident(name) if name == "with" => Walk::Stopped(j, expect_name),
        // a lone `$` (no name after it) is not a member name
        Token::Variable(name) if expect_name && name.is_empty() => {
            Walk::Fail(p.err("Expected variable, mixin, or function name", s.span.start))
        }
        _ if expect_name && matches!(&s.token, Token::Variable(_) | Token::Ident(_)) => {
            // A second config keyword (or `as`) where a member name belongs: the
            // goldens wanted the statement to end.
            let dup_kw = matches!(
                &s.token,
                Token::Ident(name) if matches!(name.as_str(), "show" | "hide" | "as")
            );
            match dup_kw {
                true => Walk::Fail(p.err("expected \";\".", s.span.start)),
                false => member_walk(p, rest, j + 1, false),
            }
        }
        _ if expect_name => {
            Walk::Fail(p.err("Expected variable, mixin, or function name", s.span.start))
        }
        Token::Punct(Punct::Comma) => member_walk(p, rest, j + 1, true),
        Token::Punct(Punct::Semicolon | Punct::LBrace | Punct::LParen) => {
            Walk::Stopped(j, expect_name)
        }
        _ => Walk::Fail(p.err("expected \";\".", s.span.start)),
    }
}

/// Reconstruct the `as <prefix>*` rename prefix. The lexer splits `c-*` into
/// `Ident("c")` `Punct(Minus)` `Punct(Star)`, so re-join the idents and hyphen /
/// dot puncts up to the `*` into the literal prefix (`c-`).
fn forward_prefix(rest: &[Spanned]) -> String {
    let Some(as_i) = find_ident(rest, "as") else {
        return String::new();
    };
    let tail = &rest[as_i + 1..];
    // the run of prefix characters, accepted only when a `*` terminates it
    let kept: Vec<&Spanned> = tail
        .iter()
        .take_while(|s| {
            matches!(
                &s.token,
                Token::Ident(_) | Token::Punct(Punct::Minus) | Token::Punct(Punct::Dot)
            )
        })
        .collect();
    match kept.len() < tail.len() && matches!(tail[kept.len()].token, Token::Punct(Punct::Star)) {
        true => kept.iter().fold(String::new(), |mut out, s| {
            match &s.token {
                Token::Ident(t) => out.push_str(t),
                Token::Punct(Punct::Minus) => out.push('-'),
                _ => out.push('.'),
            }
            out
        }),
        false => String::new(),
    }
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
