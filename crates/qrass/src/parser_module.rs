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
    // dart validates the `@use` head at parse time, before any load:
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
                let at = rest
                    .get(as_i + 1)
                    .map(|s| s.span.start)
                    .unwrap_or(span.end);
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
/// non-module rule. dart raises both violations at parse time
/// (`directives/use/error/syntax/within.hrx` + `after.hrx`, and the placement
/// sections of `directives/forward/error/syntax.hrx`). Comments stay neutral.
pub(crate) fn check_module_placement(program: &[Stmt]) -> Or<()> {
    walk_nested(program, false)?;
    let mut phase_closed = false;
    for s in program {
        match s {
            Stmt::Use(_) if phase_closed => {
                return Err(SassError::new(
                    crate::error::ErrorKind::Parse,
                    "@use rules must be written before any other rules.",
                ));
            }
            Stmt::Forward(_) if phase_closed => {
                return Err(SassError::new(
                    crate::error::ErrorKind::Parse,
                    "@forward rules must be written before any other rules.",
                ));
            }
            Stmt::Use(_) | Stmt::Forward(_) | Stmt::Comment(_) => {}
            // A root-level variable declaration is not a *rule* for dart: it
            // legally precedes `@use`/`@forward` and may even read a `with`'
            // configured value (`use|forward/with/from_variable` goldens).
            Stmt::Variable(_) => {}
            _ => phase_closed = true,
        }
    }
    Ok(())
}

/// Depth walk: a module directive inside any block body is out of place.
fn walk_nested(stmts: &[Stmt], nested: bool) -> Or<()> {
    for s in stmts {
        match s {
            Stmt::Use(_) | Stmt::Forward(_) if nested => {
                return Err(SassError::new(
                    crate::error::ErrorKind::Parse,
                    "This at-rule is not allowed here.",
                ));
            }
            Stmt::Ruleset(r) => walk_nested(&r.body, true)?,
            Stmt::Each(e) => walk_nested(&e.body, true)?,
            Stmt::For(f) => walk_nested(&f.body, true)?,
            Stmt::If(i) => {
                for clause in &i.clauses {
                    walk_nested(&clause.body, true)?;
                }
            }
            Stmt::While(w) => walk_nested(&w.body, true)?,
            Stmt::Mixin(m) => walk_nested(&m.body, true)?,
            Stmt::Function(f) => walk_nested(&f.body, true)?,
            Stmt::Include(i) => {
                if let Some(content) = &i.content {
                    walk_nested(content, true)?;
                }
            }
            Stmt::AtRoot(a) => walk_nested(&a.body, true)?,
            Stmt::Media(m) => walk_nested(&m.body, true)?,
            Stmt::Supports(su) => walk_nested(&su.body, true)?,
            Stmt::Keyframes(k) => walk_nested(&k.body, true)?,
            _ => {}
        }
    }
    Ok(())
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
        if name.is_empty() {
            // `$` with no name: dart reads the variable declaration as an
            // identifier expectation (`with/empty_variable_name` golden).
            return Err(p.err_marked(
                "Expected identifier.",
                head.span.start,
                vec![p.caret(head.span.start)],
            ));
        }
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
