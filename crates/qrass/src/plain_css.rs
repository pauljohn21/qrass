//! Plain-CSS content validation (the sass-spec "plain CSS" goldens).
//!
//! A `.css` source loaded through `@use`/`@forward` (or compiled as the entry)
//! is parsed in *plain CSS* mode, where SCSS-only constructs are rejected. The
//! compiler passes a resolved `.css` through verbatim, so these constructs are
//! never otherwise inspected. [`validate`] walks an already-parsed program and
//! raises the golden-mandated message for the first SCSS-only node it finds.
//!
//! It is deliberately **conservative**: it only rejects nodes that can never
//! occur in valid CSS (`@mixin`, `$var`, `#{…}`, SCSS arithmetic/logic
//! operators), never a CSS construct the value model shares with SCSS (`/`
//! division, `a(b)` calls, comma/space/slash lists, `!important`). The caller
//! also skips validation entirely when the source fails to parse, so a CSS
//! feature the SCSS parser cannot read keeps its previous passthrough behavior
//! instead of turning into a spurious error.

use crate::ast::{Block, InterpText, Stmt};
use crate::ast_expr::{Arg, BinOp, Expr};
use crate::error::{ErrorKind, SassError};
use crate::token::Piece;

const AT_RULE: &str = "This at-rule isn't allowed in plain CSS.";
const VARIABLE: &str = "Sass variables aren't allowed in plain CSS.";
const INTERP: &str = "Interpolation isn't allowed in plain CSS.";
const OPERATOR: &str = "Operators aren't allowed in plain CSS.";
const NAMESPACE: &str = "Module namespaces aren't allowed in plain CSS.";
const PLACEHOLDER: &str = "Placeholder selectors aren't allowed in plain CSS.";
const COMBINATOR: &str = "Top-level leading combinators aren't allowed in plain CSS.";
const PARENT_SUFFIX: &str = "Parent selectors can't have suffixes in plain CSS.";
// A map literal `(k: v)` is a `:` inside parentheses, which the CSS grammar in
// the goldens reads as an unterminated paren group and reports as `expected ")"`.
const EXPECT_PAREN: &str = "expected \")\".";

/// Whether a parse failure came from the conservative `@media`/`@supports`
/// condition grammar (`parser_supports`) rather than the SCSS parser being
/// unable to read exotic CSS. Those validators only fire on golden-pinned
/// invalid logic spellings, so plain-CSS files carrying them must surface the
/// error (`css/plain/error/media.hrx`) instead of the silent passthrough.
/// `expected "{".` / `expected "(".` are shared with the general parser, so
/// they only count when the located excerpt shows an `@media`/`@supports` head.
pub(crate) fn is_css_grammar_error(err: &SassError) -> bool {
    let msg = err.to_string();
    msg.contains("expected media condition in parentheses.")
        || msg.contains("Expected @supports condition.")
        || ((msg.contains("@media") || msg.contains("@supports"))
            && (msg.contains("expected \"{\".") || msg.contains("expected \"(\".")))
}

/// Reject SCSS-only constructs in a parsed plain-CSS program.
pub(crate) fn validate(program: &[Stmt]) -> Result<(), SassError> {
    for stmt in program {
        check_stmt(stmt, false)?;
    }
    Ok(())
}

/// Concatenated literal text of an interpolated fragment, ignoring `#{…}`
/// pieces (those are rejected separately by [`interp`]).
fn plain_text(text: &InterpText) -> String {
    text.iter()
        .filter_map(|p| match p {
            Piece::Text(s) => Some(s.as_str()),
            Piece::Expr(_) => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Reject SCSS-only selector forms the CSS grammar forbids but a valid
/// CSS selector can never produce: `%` placeholders, a leading combinator on a
/// top-level rule, and a parent selector carrying an ident suffix (`&b`).
/// Keyframes steps (`50%`, `from`, `to`) match none of these, so they stay safe.
fn check_selectors(text: &InterpText, nested: bool) -> Result<(), SassError> {
    let sel = plain_text(text);
    for part in sel.split(',') {
        let t = part.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with('%') {
            return Err(scss_err(PLACEHOLDER));
        }
        if !nested && t.starts_with(['>', '+', '~']) {
            return Err(scss_err(COMBINATOR));
        }
    }
    let bytes = sel.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'&'
            && let Some(&next) = bytes.get(i + 1)
            && (next.is_ascii_alphanumeric() || next == b'_' || next == b'-')
        {
            return Err(scss_err(PARENT_SUFFIX));
        }
    }
    Ok(())
}

/// True when an interpolated text carries a real `#{…}` expression piece.
fn has_interp(text: &InterpText) -> bool {
    text.iter().any(|p| matches!(p, Piece::Expr(_)))
}

/// SCSS-only arithmetic/logic operators. `/` (`Div`) and `=` (`Assign`) are
/// ordinary CSS (font shorthand, IE filters); `And`/`Or` are CSS keywords that
/// the plain parser keeps as literal identifiers (`and: true and false`);
/// none of them may be flagged.
fn is_scss_operator(op: BinOp) -> bool {
    !matches!(op, BinOp::Div | BinOp::Assign | BinOp::And | BinOp::Or)
}

/// Functions whose arguments are plain-CSS math scopes: the goldens parse the
/// operators inside them as literal CSS, so the validator must not descend.
fn is_css_math_function(name: &str) -> bool {
    matches!(name, "calc" | "min" | "max" | "clamp")
}

fn check_stmt(stmt: &Stmt, nested: bool) -> Result<(), SassError> {
    match stmt {
        // Control-flow directives with no plain-CSS spelling at all.
        Stmt::Include(_)
        | Stmt::Extend(_)
        | Stmt::Each(_)
        | Stmt::For(_)
        | Stmt::While(_)
        | Stmt::If(_)
        | Stmt::Return(_)
        | Stmt::Content(_)
        | Stmt::Diagnostic(_)
        | Stmt::AtRoot(_) => Err(scss_err(AT_RULE)),

        // `@function`/`@mixin` are SCSS-only UNLESS the name is a CSS custom
        // ident (`--a`): a custom at-rule is passed through verbatim by the
        // goldens and its body is not SassScript-validated.
        Stmt::Function(f) => {
            if f.name.starts_with("--") {
                Ok(())
            } else {
                Err(scss_err(AT_RULE))
            }
        }
        Stmt::Mixin(m) => {
            let custom = matches!(m.name.as_slice(), [Piece::Text(t)] if t.starts_with("--"));
            if custom {
                Ok(())
            } else {
                Err(scss_err(AT_RULE))
            }
        }

        // `$name: value;` at the top of a block.
        Stmt::Variable(_) => Err(scss_err(VARIABLE)),

        // CSS at-rules that may legally nest declarations / rules: descend into
        // the header text and body but keep the directive itself allowed.
        Stmt::Media(m) => {
            interp(&m.queries)?;
            check_block(&m.body, true)
        }
        Stmt::Supports(s) => {
            interp(&s.condition)?;
            check_block(&s.body, true)
        }
        Stmt::Keyframes(k) => {
            interp(&k.name)?;
            check_block(&k.body, true)
        }
        // Any other at-rule is one the CSS grammar does not special-case
        // (`@charset`, `@font-face`, `@page`, custom `@function --a()`/`@mixin`,
        // and their uppercased spellings). The goldens treat the whole construct as
        // opaque CSS and never inspects the body for SassScript, so we check the
        // header for interpolation but pass the body through verbatim — descending
        // would falsely reject literal payloads like `result: $b`.
        Stmt::AtRule(a) => interp(&a.value),

        Stmt::Ruleset(r) => {
            interp(&r.selectors)?;
            check_selectors(&r.selectors, nested)?;
            check_block(&r.body, true)
        }
        Stmt::Declaration(d) => {
            interp(&d.prop)?;
            check_expr(&d.value)
        }

        // `@use` / `@forward` / `@import` / comments are left as-is: module
        // directives rarely appear inside a `.css` payload and comments are
        // valid, so flagging them would only risk false positives.
        Stmt::Use(_) | Stmt::Forward(_) | Stmt::Comment(_) => Ok(()),
    }
}

fn check_block(body: &Block, nested: bool) -> Result<(), SassError> {
    for stmt in body {
        check_stmt(stmt, nested)?;
    }
    Ok(())
}

fn check_expr(expr: &Expr) -> Result<(), SassError> {
    match expr {
        Expr::Variable(..) => Err(scss_err(VARIABLE)),
        Expr::Member { .. } => Err(scss_err(NAMESPACE)),
        Expr::Interpolated { pieces, .. } => {
            if pieces.iter().any(|p| matches!(p, Piece::Expr(_))) {
                Err(scss_err(INTERP))
            } else {
                Ok(())
            }
        }
        Expr::Binary { op, left, right, .. } => {
            if is_scss_operator(*op) {
                return Err(scss_err(OPERATOR));
            }
            check_expr(left)?;
            check_expr(right)
        }
        Expr::Unary { operand, .. } => check_expr(operand),
        Expr::Paren(inner, _) => check_expr(inner),
        Expr::List { items, .. } => {
            for item in items {
                check_expr(item)?;
            }
            Ok(())
        }
        // A map literal is SCSS-only: `(k: v)` has no plain-CSS spelling.
        Expr::Map { .. } => Err(scss_err(EXPECT_PAREN)),
        Expr::FunctionCall(call) => {
            // `ns.fn(…)` uses a module namespace, which plain CSS forbids.
            if call.namespace.is_some() {
                return Err(scss_err(NAMESPACE));
            }
            // CSS math functions (`calc`, `min`, `max`, `clamp`) take plain-CSS
            // arithmetic; their operands are literal, not SassScript.
            if is_css_math_function(&call.name) {
                return Ok(());
            }
            for arg in &call.args {
                check_arg(arg)?;
            }
            Ok(())
        }
        // Scalars (numbers, strings, colors, urls, bools, null, idents) are CSS.
        Expr::Literal(_) => Ok(()),
        // Modern `if()`: a plain-CSS file may use the raw CSS clause syntax, but
        // any Sass arm (`sass($x)` tests, `#{}` interpolation, `$var` values) is
        // still disallowed, so recurse over the whole clause tree.
        Expr::IfCall(call) => {
            for clause in &call.clauses {
                check_if_cond(clause.cond.as_ref())?;
                check_expr(&clause.value)?;
            }
            Ok(())
        }
    }
}

fn check_if_cond(cond: Option<&crate::ast_expr::IfCond>) -> Result<(), SassError> {
    use crate::ast_expr::IfCond;
    match cond {
        None => Ok(()),
        Some(IfCond::Css(pieces)) => {
            if pieces.iter().any(|p| matches!(p, Piece::Expr(_))) {
                Err(scss_err(INTERP))
            } else {
                Ok(())
            }
        }
        Some(IfCond::Sass(e)) => check_expr(e),
        Some(IfCond::Not(inner)) => check_if_cond(Some(inner)),
        Some(IfCond::Paren(inner)) => check_if_cond(Some(inner)),
        Some(IfCond::Chain { parts, .. }) => {
            for part in parts {
                check_if_cond(Some(part))?;
            }
            Ok(())
        }
    }
}

fn check_arg(arg: &Arg) -> Result<(), SassError> {
    match arg {
        Arg::Positional(e) | Arg::Splat(e) => check_expr(e),
        // `f($name: v)` is SassScript named-argument syntax (the parser folds a
        // `$name:` argument into `Arg::Named`); plain CSS has no such form, so
        // the goldens report the `$` as a disallowed Sass variable.
        Arg::Named { .. } => Err(scss_err(VARIABLE)),
    }
}

fn interp(text: &InterpText) -> Result<(), SassError> {
    if has_interp(text) {
        Err(scss_err(INTERP))
    } else {
        Ok(())
    }
}

fn scss_err(msg: &str) -> SassError {
    SassError::new(ErrorKind::Unsupported, msg.to_string())
}
