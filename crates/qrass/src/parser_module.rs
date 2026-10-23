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
pub(crate) fn strip_comments(rest: &[Spanned]) -> Vec<Spanned> {
    rest.iter()
        .filter(|s| !matches!(s.token, Token::Comment(_)))
        .cloned()
        .collect()
}

pub(crate) fn build_use(p: &Parser, rest: &[Spanned], span: Span) -> Or<Stmt> {
    let rest = strip_comments(rest);
    // the goldens validate the `@use` head at parse time, before any load:
    // `directives/use/error/syntax/*` — a non-string URL (absent or bare,
    // `Expected string.`), an `as` without an identifier/`*`
    // (`Expected identifier.`), and a derived default namespace that is not
    // a valid Sass identifier (`The default namespace "x" is not a valid
    // Sass identifier.`). Interpolated URLs keep today's load path.
    match rest.first().map(|s| &s.token) {
        None | Some(Token::Punct(Punct::Semicolon)) => {
            return Err(p.err("Expected string.", span.end));
        }
        Some(Token::Str { .. }) | Some(Token::Interpolated { .. }) => {}
        Some(_) => {
            return Err(p.err("Expected string.", rest[0].span.start));
        }
    }
    if let Some(as_i) = find_ident(&rest, "as") {
        match rest.get(as_i + 1).map(|s| &s.token) {
            Some(Token::Punct(Punct::Star)) | Some(Token::Ident(_)) => {}
            _ => {
                let at = rest.get(as_i + 1).map(|s| s.span.start).unwrap_or(span.end);
                return Err(p.err("Expected identifier.", at));
            }
        }
    }
    let url = module_url(&rest);
    let alias = use_alias(&rest);
    if matches!(alias, UseAlias::Default) {
        let ns = crate::module::default_namespace(&url);
        if !is_valid_namespace(&ns) {
            return Err(p.err(
                format!("The default namespace \"{ns}\" is not a valid Sass identifier."),
                span.start,
            ));
        }
    }
    let config = with_config(p, &rest, false)?;
    reject_builtin_config(p, &url, &config, span, &rest)?;
    Ok(Stmt::Use(Use {
        url,
        alias,
        config,
        span,
    }))
}

/// A derived default namespace must be a plain Sass identifier: leading
/// letter/underscore, then alphanumerics, `-` or `_` (`use/error/syntax/url`
/// `empty`/`non_identifier` reject `""` and `"123"`).
fn is_valid_namespace(ns: &str) -> bool {
    let mut chars = ns.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// A built-in module has no configurable surface, and the golden rejects the
/// attempt where the statement was written — for both `@use` and `@forward`
/// (`directives/use/error/with/core_module.hrx`,
/// `directives/forward/error/with.hrx` `core_module`) — underlining the whole
/// statement text (its start through the closing `)`, terminator excluded).
pub(crate) fn reject_builtin_config(
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

/// `@use`/`@forward` must sit at the file's top level *and* precede every
/// non-module rule. the goldens raise both violations at parse time
/// (`directives/use/error/syntax/within.hrx` + `after.hrx`, and the placement
/// sections of `directives/forward/error/syntax.hrx`). Comments stay neutral.
pub(crate) fn check_module_placement(program: &[Stmt]) -> Or<()> {
    walk_nested(program, false)?;
    // the directive phase closes at the first rule that is neither a directive,
    // a comment, nor a root-level variable declaration (the latter legally
    // precedes `@use`/`@forward` and may even read a `with`' configured value —
    // `use|forward/with/from_variable` goldens)
    let closed = program.iter().position(|s| {
        !matches!(
            s,
            Stmt::Use(_) | Stmt::Forward(_) | Stmt::Comment(_) | Stmt::Variable(_)
        )
    });
    let late_use = closed.is_some_and(|i| program[i..].iter().any(|s| matches!(s, Stmt::Use(_))));
    let late_forward =
        closed.is_some_and(|i| program[i..].iter().any(|s| matches!(s, Stmt::Forward(_))));
    match (late_use, late_forward) {
        (true, _) => Err(SassError::new(
            crate::error::ErrorKind::Parse,
            "@use rules must be written before any other rules.",
        )),
        (_, true) => Err(SassError::new(
            crate::error::ErrorKind::Parse,
            "@forward rules must be written before any other rules.",
        )),
        _ => Ok(()),
    }
}

/// Depth walk: a module directive inside any block body is out of place.
fn walk_nested(stmts: &[Stmt], nested: bool) -> Or<()> {
    stmts.iter().try_fold((), |(), s| {
        match (nested, matches!(s, Stmt::Use(_) | Stmt::Forward(_))) {
            (true, true) => Err(SassError::new(
                crate::error::ErrorKind::Parse,
                "This at-rule is not allowed here.",
            )),
            _ => bodies_of(s)
                .into_iter()
                .try_fold((), |(), body| walk_nested(body, true)),
        }
    })
}

/// Every block body a statement carries, in source order (`@if` contributes one
/// per clause; an `@include` content block only when it has one).
fn bodies_of(s: &Stmt) -> Vec<&[Stmt]> {
    match s {
        Stmt::Ruleset(r) => vec![&r.body],
        Stmt::Each(e) => vec![&e.body],
        Stmt::For(f) => vec![&f.body],
        Stmt::If(i) => i.clauses.iter().map(|c| c.body.as_slice()).collect(),
        Stmt::While(w) => vec![&w.body],
        Stmt::Mixin(m) => vec![&m.body],
        Stmt::Function(f) => vec![&f.body],
        Stmt::Include(i) => i.content.iter().map(|c| c.as_slice()).collect(),
        Stmt::AtRoot(a) => vec![&a.body],
        Stmt::Media(m) => vec![&m.body],
        Stmt::Supports(su) => vec![&su.body],
        Stmt::Keyframes(k) => vec![&k.body],
        _ => Vec::new(),
    }
}

pub(crate) fn module_url(rest: &[Spanned]) -> String {
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
    let after = with_i + 1;
    let idx = after
        + (after..rest.len())
            .take_while(|&k| matches!(rest[k].token, Token::Comment(_)))
            .count();
    if !rest
        .get(idx)
        .is_some_and(|s| matches!(s.token, Token::Punct(Punct::LParen)))
    {
        let at = rest
            .get(idx)
            .map(|s| s.span.start)
            .unwrap_or(rest[with_i].span.end);
        return Err(p.err_marked("expected \"(\".", at, vec![p.caret(at)]));
    }
    // the group closes where the running paren depth returns to zero; an
    // unclosed one runs to the end and reports the last token's position
    let mut depth = 0i32;
    let closed_at = rest[idx..]
        .iter()
        .map(|s| {
            depth += match &s.token {
                Token::Punct(Punct::LParen) => 1,
                Token::Punct(Punct::RParen) => -1,
                _ => 0,
            };
            depth
        })
        .position(|d| d == 0)
        .map(|off| idx + off);
    match closed_at {
        Some(i) => Ok(WithGroup {
            inner: &rest[idx + 1..i],
            tail: &rest[i + 1..],
            close: rest[i].span.start,
        }),
        None => Ok(WithGroup {
            inner: &rest[idx + 1..],
            tail: &[],
            close: rest.last().map(|s| s.span.end).unwrap_or(BytePos(0)),
        }),
    }
}

/// Parse a `with (…)` clause. `forward` distinguishes the two spellings the
/// goldens accept: only a `@forward` clause may carry a `!default` flag
/// (`error/syntax/with` `default`), and both reject anything but `;` after the
/// closing paren (`with/before_as`, `before_show`, `before_hide`).
pub(crate) fn with_config(p: &Parser, rest: &[Spanned], forward: bool) -> Or<Vec<ConfigEntry>> {
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
        return Err(p.err_marked("expected \"$\".", group.close, vec![p.caret(group.close)]));
    }
    // one entry per comma-separated chunk; `try_fold` keeps the accumulated
    // entries for the duplicate-name golden and short-circuits on the first
    // malformed chunk (the per-chunk `return Err` arms are unchanged)
    split_entries(group.inner).into_iter().try_fold(
        Vec::new(),
        |mut out, (chunk, comma)| -> Or<Vec<ConfigEntry>> {
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
            if name.is_empty() {
                // `$` with no name: the goldens read the variable declaration as an
                // identifier expectation (`with/empty_variable_name` golden).
                return Err(p.err_marked(
                    "Expected identifier.",
                    head.span.start,
                    vec![p.caret(head.span.start)],
                ));
            }
            let Some(colon) = chunk
                .get(1)
                .filter(|s| matches!(s.token, Token::Punct(Punct::Colon)))
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
            let entry = Span {
                file,
                start: head.span.start,
                end: value_end,
            };
            let value_span = Span {
                file,
                start: value_start,
                end: value_end,
            };
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
            Ok(out)
        },
    )
}

/// Split a `with (…)` body on its depth-0 commas, pairing each entry with the
/// comma that ended it (the last entry has none). An entry with no tokens in it
/// is the `with/extra_comma` golden, so its comma position has to travel along.
fn split_entries(run: &[Spanned]) -> Vec<(&[Spanned], BytePos)> {
    // tag each token with the depth in front of it and cut at the depth-0 commas:
    // every entry is the slice between two consecutive cuts, paired with the
    // comma that ended it (the final entry has none, so `BytePos(0)`)
    let mut depth = 0i32;
    let cuts: Vec<usize> = run
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let level = depth;
            depth += match &s.token {
                Token::Punct(Punct::LParen | Punct::LBracket) => 1,
                Token::Punct(Punct::RParen | Punct::RBracket) => -1,
                _ => 0,
            };
            (i, level, matches!(s.token, Token::Punct(Punct::Comma)))
        })
        .filter(|(_, level, is_comma)| *is_comma && *level == 0)
        .map(|(i, _, _)| i)
        .collect();
    let starts: Vec<usize> = std::iter::once(0usize)
        .chain(cuts.iter().map(|&i| i + 1))
        .collect();
    let tail_start = starts.last().copied().unwrap_or(0);
    cuts.iter()
        .enumerate()
        .map(|(k, &end)| (&run[starts[k]..end], run[end].span.start))
        .chain((tail_start < run.len()).then(|| (&run[tail_start..], BytePos(0))))
        .collect()
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
