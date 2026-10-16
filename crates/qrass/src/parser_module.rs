//! Module-syntax statement parsing (`@use` / `@forward`), split out of
//! [`parser_at`] to keep that file within the size budget. These builders
//! reconstruct the module URL, alias, `as prefix*` rename, `show`/`hide` member
//! lists and the `with (…)` configuration from a raw token run.

use crate::ast::{ConfigEntry, Stmt, Use, UseAlias};
use crate::error::SassError;
use crate::span::{BytePos, Span};
use crate::token::{Punct, Spanned, Token};

use super::Parser;
use super::parser_at::{find_ident, split_flags};
use super::parser_expr::value_of;

type Or<T> = Result<T, SassError>;

/// Drop loud-comment tokens from a `@use`/`@forward` argument run: within a
/// module directive's head, `/* */` carries no meaning and the goldens treat it
/// as whitespace, so `@use /**/ "m"` must still resolve the URL `"m"`.
fn strip_comments(rest: &[Spanned]) -> Vec<Spanned> {
    rest.iter()
        .filter(|s| !matches!(s.token, Token::Comment(_)))
        .cloned()
        .collect()
}

pub(crate) fn build_use(p: &Parser, rest: &[Spanned], span: Span) -> Or<Stmt> {
    let rest = strip_comments(rest);
    let url = module_url(&rest);
    let config = with_config(p, &rest, false)?;
    reject_builtin_config(p, &url, &config, span, &rest)?;
    Ok(Stmt::Use(Use {
        url,
        alias: use_alias(&rest),
        config,
        span,
    }))
}

/// A built-in module has no configurable surface, and the golden rejects the
/// attempt where the statement was written — for both `@use` and `@forward`
/// (`directives/use/error/with/core_module.hrx`,
/// `directives/forward/error/with.hrx` `core_module`) — underlining the whole
/// statement text (its start through the closing `)`, terminator excluded).
fn reject_builtin_config(
    p: &Parser,
    url: &str,
    config: &[crate::ast::ConfigEntry],
    span: Span,
    rest: &[Spanned],
) -> Or<()> {
    if !url.starts_with("sass:") || config.is_empty() {
        return Ok(());
    }
    let (start, end) = stmt_span(span, rest);
    Err(p.err_marked(
        "Built-in modules can't be configured.",
        start,
        vec![p.marker('^', start, end, None)],
    ))
}

pub(crate) fn build_forward(p: &Parser, rest: &[Spanned], span: Span) -> Or<Stmt> {
    let rest = strip_comments(rest);
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

/// The `with (…)` group of a module directive: the entries inside the
/// parentheses, whatever follows the closing paren, and the closing paren's own
/// position (the place a missing value is reported at).
struct WithGroup<'a> {
    inner: &'a [Spanned],
    tail: &'a [Spanned],
    close: BytePos,
}

/// Locate the group after the `with` keyword. `with` not followed by `(` is the
/// `with/no_arguments` golden, and a group that never closes keeps the last
/// token's position so the caller still reports where it ran out.
fn with_group<'a>(p: &Parser, rest: &'a [Spanned], with_i: usize) -> Or<WithGroup<'a>> {
    let mut idx = with_i + 1;
    while rest.get(idx).is_some_and(|s| matches!(s.token, Token::Comment(_))) {
        idx += 1;
    }
    if !rest.get(idx).is_some_and(|s| matches!(s.token, Token::Punct(Punct::LParen))) {
        let at = rest
            .get(idx)
            .map(|s| s.span.start)
            .unwrap_or(rest[with_i].span.end);
        return Err(p.err_marked("expected \"(\".", at, vec![p.caret(at)]));
    }
    let mut depth = 0i32;
    for i in idx..rest.len() {
        match &rest[i].token {
            Token::Punct(Punct::LParen) => depth += 1,
            Token::Punct(Punct::RParen) => {
                depth -= 1;
                if depth == 0 {
                    return Ok(WithGroup {
                        inner: &rest[idx + 1..i],
                        tail: &rest[i + 1..],
                        close: rest[i].span.start,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(WithGroup {
        inner: &rest[idx + 1..],
        tail: &[],
        close: rest.last().map(|s| s.span.end).unwrap_or(BytePos(0)),
    })
}

/// Parse a `with (…)` clause. `forward` distinguishes the two spellings the
/// goldens accept: only a `@forward` clause may carry a `!default` flag
/// (`error/syntax/with` `default`), and both reject anything but `;` after the
/// closing paren (`with/before_as`, `before_show`, `before_hide`).
fn with_config(p: &Parser, rest: &[Spanned], forward: bool) -> Or<Vec<ConfigEntry>> {
    let Some(with_i) = find_ident(rest, "with") else {
        return Ok(Vec::new());
    };
    let group = with_group(p, rest, with_i)?;
    if let Some(extra) = group
        .tail
        .iter()
        .find(|s| !matches!(s.token, Token::Punct(Punct::Semicolon) | Token::Comment(_)))
    {
        return Err(p.err_marked(
            "expected \";\".",
            extra.span.start,
            vec![p.caret(extra.span.start)],
        ));
    }
    if group.inner.is_empty() {
        return Err(p.err_marked(
            "expected \"$\".",
            group.close,
            vec![p.caret(group.close)],
        ));
    }
    let mut out: Vec<ConfigEntry> = Vec::new();
    for (chunk, comma) in split_entries(group.inner) {
        let Some(head) = chunk.first() else {
            // Two commas in a row: the group has to end here
            // (`with/extra_comma`, which wants a trailing comma instead).
            return Err(p.err_marked("expected \")\".", comma, vec![p.caret(comma)]));
        };
        let Token::Variable(name) = &head.token else {
            return Err(p.err_marked(
                "expected \"$\".",
                head.span.start,
                vec![p.caret(head.span.start)],
            ));
        };
        let Some(colon) = chunk.get(1).filter(|s| matches!(s.token, Token::Punct(Punct::Colon)))
        else {
            let at = chunk.get(1).map(|s| s.span.start).unwrap_or(group.close);
            return Err(p.err_marked("expected \":\".", at, vec![p.caret(at)]));
        };
        let (value_run, flags) = split_flags(&chunk[2..]);
        if !forward && !flags.is_empty() {
            let bang = chunk
                .iter()
                .find(|s| matches!(s.token, Token::Punct(Punct::Bang)))
                .map(|s| s.span.start)
                .unwrap_or(colon.span.end);
            return Err(p.err_marked("expected \")\".", bang, vec![p.caret(bang)]));
        }
        // The golden underlines `$name: value` and stops at the value's last
        // token: a `!default` flag and the separating comma are outside it.
        let value_start = value_run
            .first()
            .map(|s| s.span.start)
            .unwrap_or(colon.span.end);
        let value_end = value_run
            .last()
            .map(|s| s.span.end)
            .unwrap_or(colon.span.end);
        let file = head.span.file;
        let entry = Span { file, start: head.span.start, end: value_end };
        let value_span = Span { file, start: value_start, end: value_end };
        if out.iter().any(|e| config_key(&e.name) == config_key(name)) {
            return Err(p.err_marked(
                "The same variable may only be configured once.",
                entry.start,
                vec![p.marker('^', entry.start, entry.end, None)],
            ));
        }
        out.push(ConfigEntry {
            name: name.clone(),
            value: value_of(p, value_run.to_vec())?,
            default: flags.iter().any(|f| f == "default"),
            entry,
            value_span,
        });
    }
    Ok(out)
}

/// Split a `with (…)` body on its depth-0 commas, pairing each entry with the
/// comma that ended it (the last entry has none). An entry with no tokens in it
/// is the `with/extra_comma` golden, so its comma position has to travel along.
fn split_entries(run: &[Spanned]) -> Vec<(&[Spanned], BytePos)> {
    let mut out: Vec<(&[Spanned], BytePos)> = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, s) in run.iter().enumerate() {
        match &s.token {
            Token::Punct(Punct::LParen | Punct::LBracket) => depth += 1,
            Token::Punct(Punct::RParen | Punct::RBracket) => depth -= 1,
            Token::Punct(Punct::Comma) if depth == 0 => {
                out.push((&run[start..i], s.span.start));
                start = i + 1;
            }
            _ => {}
        }
    }
    if run.len() > start {
        out.push((&run[start..], BytePos(0)));
    }
    out
}

/// A configuration name as the goldens compare it: no leading `$`, and `_` the
/// spelling of `-` (`with/dash_insensitive.hrx` configures `$a_b` onto `$a-b`).
pub(crate) fn config_key(name: &str) -> String {
    name.trim_start_matches('$').replace('_', "-")
}

/// The underlineable text of a module directive: from the `@use`/`@forward`
/// keyword to the last token before the terminating `;` — the golden's marker
/// run never covers the terminator.
fn stmt_span(span: Span, rest: &[Spanned]) -> (BytePos, BytePos) {
    let start = span.start;
    let last = rest
        .iter()
        .rfind(|s| !matches!(s.token, Token::Punct(Punct::Semicolon)))
        .map(|s| s.span.end)
        .unwrap_or(start);
    (start, last)
}
