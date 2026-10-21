//! Directive / statement construction for [`Parser`]: folds an owned pending
//! token run into an [`Owner`] (a block head) or a simple [`Stmt`] (a `;`-ended
//! statement). Pure functions over the owned run — split points are found with
//! iterator combinators (`position`, `take_while`), never a shared cursor.

use crate::ast::{
    AtRule, Content, DiagLevel, Diagnostic, Each, Extend, For, FunctionDecl, Include,
    Media, Return, Ruleset, Stmt, Supports, While,
};
use crate::ast_expr::Expr;
use crate::error::SassError;
use crate::span::{BytePos, Span};
use crate::token::{Punct, Spanned, Token};

use super::parser_decl::{check_atrule_member_syntax, declaration, variable_decl};
use super::parser_expr::{args_of, params_of, value_of};
use super::{Owner, Parser};

type Or<T> = Result<T, SassError>;

// ─── Block heads ───────────────────────────────────────────────────────────────

pub(crate) fn build_owner(p: &Parser, run: Vec<Spanned>) -> Or<Owner> {
    let span = p.span_of(&run);
    let head = run
        .first()
        .map(|s| s.token.clone())
        .ok_or_else(|| p.err("expected a selector or directive before '{'", span.start))?;
    match head {
        Token::AtKeyword(name) => {
            let rest = &run[1..];
            match name.as_str() {
                "media" => {
                    super::parser_supports::validate_media(p, rest)?;
                    Ok(Owner::Media {
                        queries: p.pieces(rest),
                        span,
                    })
                }
                "supports" => {
                    // A lone `$` never parses inside a supports condition — dart
                    // keeps demanding the declaration colon there
                    // (`css/supports/error` `syntax/anything/colon`, `expected ":".`).
                    if let Some(bad) = rest
                        .iter()
                        .find(|s| matches!(&s.token, Token::Variable(v) if v.is_empty()))
                    {
                        return Err(p.err("expected \":\".", bad.span.start));
                    }
                    super::parser_supports::validate_supports(p, rest)?;
                    Ok(Owner::Supports {
                        condition: p.pieces(rest),
                        span,
                    })
                }
                "keyframes" => Ok(Owner::Keyframes {
                    name: p.pieces(rest),
                    span,
                }),
                "at-root" => {
                    let header = at_root_header(rest);
                    let selector = if header.is_none() {
                        p.pieces(rest)
                    } else {
                        Vec::new()
                    };
                    Ok(Owner::AtRoot {
                        header,
                        selector,
                        span,
                    })
                }
                "if" => Ok(Owner::If {
                    cond: Some(expr(p, rest)?),
                    span,
                }),
                "else" => Ok(Owner::If {
                    cond: else_cond(p, rest)?,
                    span,
                }),
                "each" => each_owner(p, rest, span),
                "for" => for_owner(p, rest, span),
                "while" => Ok(Owner::While {
                    cond: expr(p, rest)?,
                    span,
                }),
                "mixin" => Ok(mixin_owner(p, rest, span, true)?),
                "function" => Ok(mixin_owner(p, rest, span, false)?),
                "include" => include_owner(p, rest, span),
                _ => Ok(Owner::AtRule {
                    name,
                    value: p.pieces(rest),
                    span,
                }),
            }
        }
        _ => Ok(Owner::Ruleset {
            selectors: p.pieces(&run),
            span,
        }),
    }
}

fn else_cond(p: &Parser, rest: &[Spanned]) -> Or<Option<Expr>> {
    // `@else if <cond>` → Some(cond); bare `@else` → None.
    let is_if = matches!(rest.first().map(|s| &s.token), Some(Token::Ident(w)) | Some(Token::AtKeyword(w)) if w == "if");
    if is_if {
        Ok(Some(expr(p, &rest[1..])?))
    } else {
        Ok(None)
    }
}

fn each_owner(p: &Parser, rest: &[Spanned], span: Span) -> Or<Owner> {
    let in_idx = find_ident(rest, "in").ok_or_else(|| p.err("@each missing 'in'", span.start))?;
    let vars = rest[..in_idx]
        .iter()
        .filter_map(|s| match &s.token {
            Token::Variable(v) => Some(v.clone()),
            _ => None,
        })
        .collect();
    Ok(Owner::Each {
        vars,
        list: expr(p, &rest[in_idx + 1..])?,
        span,
    })
}

fn for_owner(p: &Parser, rest: &[Spanned], span: Span) -> Or<Owner> {
    let var = match rest.first().map(|s| &s.token) {
        Some(Token::Variable(v)) => v.clone(),
        _ => return Err(p.err("@for expects a loop variable", span.start)),
    };
    let from_idx =
        find_ident(rest, "from").ok_or_else(|| p.err("@for missing 'from'", span.start))?;
    let (dir_idx, through) = find_ident(rest, "through")
        .map(|i| (i, true))
        .or_else(|| find_ident(rest, "to").map(|i| (i, false)))
        .ok_or_else(|| p.err("@for missing 'to'/'through'", span.start))?;
    Ok(Owner::For {
        var,
        from: expr(p, &rest[from_idx + 1..dir_idx])?,
        to: expr(p, &rest[dir_idx + 1..])?,
        inclusive: through,
        span,
    })
}

/// Shared by `@mixin` and `@function`: `name(params)?`.
fn mixin_owner(p: &Parser, rest: &[Spanned], span: Span, is_mixin: bool) -> Or<Owner> {
    let (name_run, paren) = split_paren(rest);
    check_atrule_member_syntax(p, name_run, is_mixin, false)?;
    let params = paren
        .map(|r| params_of(p, r))
        .transpose()?
        .unwrap_or_default();
    Ok(if is_mixin {
        Owner::Mixin {
            name: p.pieces(name_run),
            params,
            span,
        }
    } else {
        Owner::Function {
            name: join_ident(name_run),
            params,
            span,
        }
    })
}

fn include_owner(p: &Parser, rest: &[Spanned], span: Span) -> Or<Owner> {
    let (name_run, paren) = split_paren(rest);
    check_atrule_member_syntax(p, name_run, false, true)?;
    let args = paren
        .map(|r| args_of(p, r))
        .transpose()?
        .unwrap_or_default();
    Ok(Owner::Include {
        name: p.pieces(name_run),
        args,
        content_params: Vec::new(),
        span,
    })
}


fn at_root_header(rest: &[Spanned]) -> Option<(String, Vec<String>)> {
    // `@at-root (without: media)` / `(with: …)`.
    let (name_run, paren) = split_paren(rest);
    if !name_run.is_empty() {
        return None;
    }
    let inner = paren.as_ref()?;
    let inner = &inner[1..inner.len() - 1];
    let key = match inner.first().map(|s| &s.token) {
        Some(Token::Ident(w)) if w == "with" || w == "without" => w.clone(),
        _ => return None,
    };
    let list = inner[1..]
        .iter()
        .filter_map(|s| match &s.token {
            Token::Ident(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    Some((key, list))
}

// ─── Simple (`;`-terminated) statements ────────────────────────────────────────

pub(crate) fn build_simple(p: &Parser, run: Vec<Spanned>, _no_semi: bool) -> Or<Stmt> {
    let span = p.span_of(&run);
    match run.first().map(|s| s.token.clone()) {
        Some(Token::AtKeyword(name)) => simple_at(p, name, &run[1..], span),
        Some(Token::Variable(_)) => variable_decl(p, &run, span),
        _ => declaration(p, &run, span),
    }
}

fn simple_at(p: &Parser, name: String, rest: &[Spanned], span: Span) -> Or<Stmt> {
    match name.as_str() {
        "extend" => {
            let (sel, flags) = split_flags(rest);
            Ok(Stmt::Extend(Extend {
                selector: p.pieces(sel),
                optional: flags.iter().any(|f| f == "optional"),
                span,
            }))
        }
        "return" => Ok(Stmt::Return(Return {
            expr: if rest.is_empty() {
                None
            } else {
                Some(expr(p, rest)?)
            },
            span,
        })),
        "content" => {
            let args = split_paren(rest)
                .1
                .map(|r| args_of(p, r))
                .transpose()?
                .unwrap_or_default();
            Ok(Stmt::Content(Content { args, span }))
        }
        "include" => {
            let (name_run, paren) = split_paren(rest);
            // the member-syntax errors are parse-time for both include forms
            check_atrule_member_syntax(p, name_run, false, true)?;
            let args = paren
                .map(|r| args_of(p, r))
                .transpose()?
                .unwrap_or_default();
            Ok(Stmt::Include(Include {
                name: p.pieces(name_run),
                args,
                content_params: Vec::new(),
                content: None,
                span,
            }))
        }
        "use" => super::parser_module::build_use(p, rest, span),
        "forward" => super::parser_forward::build_forward(p, rest, span),
        "warn" | "error" | "debug" => {
            let level = match name.as_str() {
                "warn" => DiagLevel::Warn,
                "error" => DiagLevel::Error,
                _ => DiagLevel::Debug,
            };
            Ok(Stmt::Diagnostic(Diagnostic {
                level,
                message: expr(p, rest)?,
                span,
            }))
        }
        "if" | "else" | "each" | "for" | "while" | "mixin" | "function" | "media" | "supports"
        | "keyframes" | "at-root" => Err(p.err(
            format!("@{name} requires a block body '{{ … }}'"),
            span.start,
        )),
        _ => Ok(Stmt::AtRule(AtRule {
            name,
            value: p.pieces(rest),
            body: None,
            span,
        })),
    }
}


// ─── Sealing a block body into a node ──────────────────────────────────────────

pub(crate) fn seal_block(owner: Owner, body: Vec<Stmt>) -> Stmt {
    match owner {
        Owner::Ruleset { selectors, span } => Stmt::Ruleset(Ruleset {
            selectors,
            body,
            span,
        }),
        Owner::Media { queries, span } => Stmt::Media(Media {
            queries,
            body,
            span,
        }),
        Owner::Supports { condition, span } => Stmt::Supports(Supports {
            condition,
            body,
            span,
        }),
        Owner::Keyframes { name, span } => {
            Stmt::Keyframes(crate::ast::Keyframes { name, body, span })
        }
        Owner::AtRoot {
            header,
            selector,
            span,
        } => {
            // The header form `@at-root <selector> { … }` is equivalent to lifting
            // a rule with that selector to the top level: wrap the body as a
            // ruleset so the block-form traversal (`run_at_root`) re-parents it.
            let body = if header.is_none() && !selector.is_empty() {
                vec![Stmt::Ruleset(Ruleset {
                    selectors: selector,
                    body,
                    span,
                })]
            } else {
                body
            };
            Stmt::AtRoot(crate::ast::AtRoot { header, body, span })
        }
        Owner::Mixin { name, params, span } => Stmt::Mixin(crate::ast::MixinDecl {
            name,
            params,
            body,
            span,
        }),
        Owner::Function { name, params, span } => Stmt::Function(FunctionDecl {
            name,
            params,
            body,
            span,
        }),
        Owner::Include {
            name,
            args,
            content_params,
            span,
        } => Stmt::Include(Include {
            name,
            args,
            content_params,
            content: Some(body),
            span,
        }),
        Owner::Each { vars, list, span } => Stmt::Each(Each {
            vars,
            list,
            body,
            span,
        }),
        Owner::For {
            var,
            from,
            to,
            inclusive,
            span,
        } => Stmt::For(For {
            var,
            from,
            to,
            inclusive,
            body,
            span,
        }),
        Owner::While { cond, span } => Stmt::While(While { cond, body, span }),
        Owner::If { .. } => unreachable!("@if bodies are sealed via if_clauses, not seal_block"),
        Owner::AtRule { name, value, span } => Stmt::AtRule(AtRule {
            name,
            value,
            body: Some(body),
            span,
        }),
    }
}

// ─── Run-splitting combinators (pure, depth-aware) ───────────────────────────

pub(crate) fn expr(p: &Parser, run: &[Spanned]) -> Or<Expr> {
    if run.is_empty() {
        // Every golden spells this capitalized (`Error: Expected expression.`,
        // 33 sections across the suite — load_css/error `syntax` included).
        return Err(p.err("Expected expression.", BytePos(0)));
    }
    value_of(p, run.to_vec())
}

/// Index of the first `name` identifier at paren/bracket depth 0.
pub(crate) fn find_ident(run: &[Spanned], name: &str) -> Option<usize> {
    let mut depth = 0i32;
    run.iter().position(|s| match &s.token {
        Token::Punct(Punct::LParen | Punct::LBracket) => {
            depth += 1;
            false
        }
        Token::Punct(Punct::RParen | Punct::RBracket) => {
            depth -= 1;
            false
        }
        Token::Ident(w) if depth == 0 => w == name,
        _ => false,
    })
}

/// Split at the first depth-0 `(` … matching `)` run: `(prefix, Some(paren_run))`.
pub(crate) fn split_paren(run: &[Spanned]) -> (&[Spanned], Option<Vec<Spanned>>) {
    let open = match run
        .iter()
        .position(|s| matches!(s.token, Token::Punct(Punct::LParen)))
    {
        Some(i) => i,
        None => return (run, None),
    };
    let mut depth = 0i32;
    for i in open..run.len() {
        match &run[i].token {
            Token::Punct(Punct::LParen) => depth += 1,
            Token::Punct(Punct::RParen) => {
                depth -= 1;
                if depth == 0 {
                    return (&run[..open], Some(run[open..=i].to_vec()));
                }
            }
            _ => {}
        }
    }
    (run, None)
}

/// Peel trailing `!flag` pairs (Bang + Ident) off a value run.
pub(crate) fn split_flags(run: &[Spanned]) -> (&[Spanned], Vec<String>) {
    let mut flags = Vec::new();
    let mut end = run.len();
    while end >= 2 {
        let bang = matches!(run[end - 2].token, Token::Punct(Punct::Bang));
        let name = match &run[end - 1].token {
            Token::Ident(w) => Some(w.clone()),
            _ => None,
        };
        match (bang, name) {
            (true, Some(w)) => {
                flags.push(w);
                end -= 2;
            }
            _ => break,
        }
    }
    (&run[..end], flags)
}

fn join_ident(run: &[Spanned]) -> String {
    run.iter()
        .filter_map(|s| match &s.token {
            Token::Ident(t) => Some(t.clone()),
            _ => None,
        })
        .next()
        .unwrap_or_default()
}
