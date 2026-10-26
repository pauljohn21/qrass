//! Expr — a declaration value (or a selector) as a structured expression, assembled from the
//! `Tok` run in one forward pass — never by re-scanning rendered text — and evaluated
//! bottom-up against the owned [`Env`].
//!
//! P1 keeps the grammar honest by **bailing instead of guessing**: a run becomes an `Expr`
//! only when it is unambiguous. Any lexeme carrying `/`, `*` or `%`, and any shape the
//! algebra does not cover yet, returns `None` and keeps its verbatim text — which is what
//! protects `font: 12px/1.5`, `grid: 1/2` and CSS-native `calc()`/`min()` from being read as
//! Sass. Sass itself supplies the spacing law for `+`/`-`: they are operators only with
//! whitespace on both sides, so `10px - 5px` subtracts while `10px -5px` is a two-item list.

use crate::SassError;
use crate::calls::Argument;
use crate::env::Env;
use crate::lexer::{Tok, canon, render, token_text};
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
}

/// An expression over declaration-value lexemes. Numbers carry their unit text verbatim.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64, String),
    Binary(Box<Expr>, BinOp, Box<Expr>),
    /// verbatim lexeme text around an interpolation (`".col-"`, `"0 "`)
    Text(String),
    /// `$name` — the lookup key never carries the `$` (see [`Env`])
    Var(String),
    /// a call into a builtin whose signature we have (`1.7a`); each argument keeps its `$label:` so
    /// binding can be by name rather than by position
    Call {
        name: String,
        args: Vec<Argument>,
    },
    /// the parts of an interpolated run, concatenated with **no extra space** at eval
    Concat(Vec<Expr>),
}

/// One classified lexeme of the arithmetic path.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Piece<'a> {
    Num(f64, &'a str),
    Op(BinOp),
    Other,
}

/// A value that may not be arithmetic at all: a slash, star or percent anywhere in a lexeme
/// is a CSS separator (or a percentage unit) until the value algebra covers them.
fn protected(toks: &[Tok]) -> bool {
    toks.iter().any(|tok| match tok {
        Tok::Word(w) => w.contains(['/', '*', '%']),
        Tok::Interp(inner) => protected(inner),
        _ => false,
    })
}

/// Sass's **special** calls, matched on the base name after a vendor prefix: their arguments are
/// plain CSS, where `+`/`-` are not arithmetic and an **unbound** `$x` is just text rather than an
/// error (`css/functions/special/prefixed/lowercase::calc/script_like` keeps `-a-calc($c)`; the same
/// family's `calc/interpolation` still evaluates `#{0}`, and `values/calculation/calc/no_operator::
/// variable/unquoted_string` resolves a *bound* `$a`). A function that is merely unknown is **not**
/// special: `css/functions/not_special.hrx` computes `-c-type(1 + 1)` → `-c-type(2)`.
///
/// Grows with the builtin registry (P1.7/P2); `min`/`max`/`clamp` are excluded on purpose — their
/// variables resolve too.
const SPECIAL_CALLS: [&str; 4] = ["url", "calc", "element", "expression"];

/// Strip a vendor prefix from a call name (`-a-calc` → `calc`). A `fn`, not a closure: a closure's
/// elided lifetimes cannot tie the returned `&str` to its argument.
fn base_call_name(name: &str) -> &str {
    name.rsplit('-').next().unwrap_or(name)
}

/// Whether a run calls a special (or namespaced, `progid:`-style) function. The name match ignores
/// case, because the corpus keeps `-C-EXPRESSION($d)` verbatim too
/// (`css/functions/special/prefixed/uppercase::expression/script_like`).
pub fn special_call(toks: &[Tok]) -> bool {
    toks.iter().any(|tok| match tok {
        Tok::Word(word) => match word.strip_suffix('(') {
            Some(name) => {
                SPECIAL_CALLS
                    .iter()
                    .any(|call| base_call_name(name).eq_ignore_ascii_case(call))
                    || name.contains(':')
            }
            None => false,
        },
        _ => false,
    }) || toks.windows(2).any(|pair| {
        matches!(pair[0], Tok::Punct(':')) && matches!(&pair[1], Tok::Word(w) if w.ends_with('('))
    })
}

fn has_dynamic(toks: &[Tok]) -> bool {
    toks.iter()
        .any(|tok| matches!(tok, Tok::Var(_) | Tok::Interp(_)))
}

/// Whether a run **passes named arguments** — `f(1, $w: 30%)`, `color.hwb($hue: 180, …)`. There the `$w`
/// is an argument *label*, not a variable reference, so looking it up invents an `Undefined variable`
/// error for a program Sass accepts (`core_functions/color/hwb/four_args::named`,
/// `core_functions/color/is_powerless/named`; measured as 1,611 in-scope cases).
///
/// The test is lexeme-level because a map literal writes the same shape with the opposite intent:
/// `($z: null)` **is** a reference to `$z` as a key. The lexer distinguishes them — a call's `(` is glued
/// to its callee (`Word("hwb(")`) while a group's `(` stands alone (`Word("(")`) — so an argument starts
/// only after a glued `(` or a `,`, and whitespace is not meaningful here.
///
/// A run holding one is not an expression P1 evaluates at all: it keeps its verbatim text, which also
/// means any *other* reference inside it stays unresolved for now (call evaluation arrives with P4).
pub fn named_argument(toks: &[Tok]) -> bool {
    let meaningful: Vec<&Tok> = toks
        .iter()
        .filter(|tok| !matches!(tok, Tok::Space(_)))
        .collect();
    let opens_argument = |tok: &&Tok| match tok {
        Tok::Word(word) => word.ends_with('(') && word.len() > 1,
        Tok::Punct(',') => true,
        _ => false,
    };
    let names_argument = |tok: &&Tok| matches!(tok, Tok::Var(_));
    let is_colon = |tok: &&Tok| matches!(tok, Tok::Punct(':'));
    meaningful.windows(3).any(|window| {
        opens_argument(&window[0]) && names_argument(&window[1]) && is_colon(&window[2])
    })
}

/// Build an `Expr` from a token run, or `None` when the run holds a shape P1 must not read.
pub fn from_tokens(toks: &[Tok]) -> Option<Expr> {
    if protected(toks) {
        return None;
    }
    // A call is tried first because its run is frequently dynamic (`math.div($number2: 3, …)`): the
    // `$label:` there is an argument name, and only the signature check inside [`crate::calls`] can tell
    // that apart from a reference. Nothing else about a run changes when it is a call.
    if let Some((name, args)) = crate::calls::known_call(toks) {
        return Some(Expr::Call { name, args });
    }
    if has_dynamic(toks) {
        interpolation(toks)
    } else {
        arithmetic(toks).or_else(|| concatenation(toks))
    }
}

/// One argument's expression. Unlike a declaration value, an argument **is** an expression even when it is
/// a lone number (`math.div(6, 3)` → `2`), so the "a bare number is not an expression yet" filter that
/// [`crate::exec::State::evaluated`] applies to whole declaration values does not apply here.
pub fn argument_expr(toks: &[Tok]) -> Option<Expr> {
    from_tokens(toks)
}

/// The interpolation shape only — what a **selector** may use, because a selector's lexemes
/// must never be read as numbers (`.2xl` is an identifier, not `0.2xl`).
pub fn interpolated(toks: &[Tok]) -> Option<Expr> {
    has_dynamic(toks).then(|| interpolation(toks)).flatten()
}

/// Classify one token, given its position in the run. Only whitespace is dropped; a lexeme
/// that is not a number must mark `Other` so the whole run bails — silently dropping it would
/// shift the alternation and turn `rgb(18 52 none)` into the number `52`.
/// `+`/`-` is an operator **only** when whitespace sits on both sides.
fn classify<'a>(tok: &'a Tok, index: usize, run: &[Tok]) -> Option<Piece<'a>> {
    let spaced = |side: Option<usize>| matches!(side.and_then(|i| run.get(i)), Some(Tok::Space(_)));
    match tok {
        Tok::Space(_) => None,
        Tok::Word(w) => {
            Some(number_lexeme(w).map_or(Piece::Other, |(value, unit)| Piece::Num(value, unit)))
        }
        Tok::Op('+') if spaced(index.checked_sub(1)) && spaced(index.checked_add(1)) => {
            Some(Piece::Op(BinOp::Add))
        }
        Tok::Op('-') if spaced(index.checked_sub(1)) && spaced(index.checked_add(1)) => {
            Some(Piece::Op(BinOp::Sub))
        }
        _ => Some(Piece::Other),
    }
}

/// A whole lexeme as `value + unit`: digits (with optional fraction) followed by an
/// identifier-shaped unit. `52)` or `1px)` are not numbers — a unit is alphanumeric, so a
/// CSS call like `rgb(18 none 66)` bails to verbatim text instead of losing its paren.
fn number_lexeme(word: &str) -> Option<(f64, &str)> {
    let digits = word
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(word.len());
    if digits == 0 {
        return None;
    }
    let (numeric, unit) = word.split_at(digits);
    if !unit.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    numeric.parse::<f64>().ok().map(|value| (value, unit))
}

/// Identifiers joined by `+`/`-` are Sass's **string concatenation**, and the two operators print
/// differently: `operators/plus.hrx::syntax/*` folds `c+d`, `c + d`, `c +d`, `c+ d` and `c/**/+/**/d`
/// all into `cd` (the `+` disappears), while `operators/minus.hrx::syntax/*` folds `(c)-(d)`, `c - d`,
/// `c -(d)` and `(c)- d` all into `c-d` — the `-` survives as text because Sass parses `-` on an
/// unquoted string as adding its negation.
///
/// Deliberately narrow, so nothing that works today moves: **every** operand must be an identifier
/// lexeme (a whole-paren wrapper is peeled first), at least one operator must appear, and any number in
/// the run sends it back to the arithmetic path — which is what keeps the spacing law and the two-item
/// list `10px -5px` intact, and leaves `1px+2px` un-evaluated as before. Whitespace *is* data for
/// numbers but not for this shape: the fixtures collapse all four spacings to one result.
fn concatenation(toks: &[Tok]) -> Option<Expr> {
    let lexemes: Vec<&Tok> = toks
        .iter()
        .filter(|tok| !matches!(tok, Tok::Space(_)))
        .collect();
    let operands = lexemes
        .iter()
        .step_by(2)
        .map(operand_identifier)
        .collect::<Option<Vec<&str>>>()?;
    let operators = lexemes
        .iter()
        .skip(1)
        .step_by(2)
        .map(operator_sign)
        .collect::<Option<Vec<char>>>()?;
    if operands.len() < 2 || operators.len() + 1 != operands.len() {
        return None;
    }
    Some(Expr::Text(
        operands[1..]
            .iter()
            .zip(&operators)
            .fold(operands[0].to_string(), |joined, (operand, op)| {
                joined + if *op == '+' { "" } else { "-" } + operand
            }),
    ))
}

/// One non-whitespace lexeme as an operand name, if it is an identifier (possibly paren-wrapped).
/// Named lifetimes because the `&str` comes out of a `Tok` reached through a double reference — the
/// same elision trap that already bit this crate twice.
fn operand_identifier<'tok>(tok: &'tok &Tok) -> Option<&'tok str> {
    match tok {
        Tok::Word(word) => identifier(unwrapped(word)),
        _ => None,
    }
}

/// One lexeme in an operator position: only a standalone `+`/`-` counts here.
fn operator_sign(tok: &&Tok) -> Option<char> {
    match tok {
        Tok::Op(op @ ('+' | '-')) => Some(*op),
        _ => None,
    }
}

/// A lexeme that is an identifier: ASCII letter or `_` first, then letters, digits, `_` and `-`
/// (the hyphen inside `foo-bar` never reaches us as an operator, since the lexer glues it).
fn identifier(text: &str) -> Option<&str> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    chars
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        .then_some(text)
}

/// Strip every whole-paren wrapper: `(c)` and `((c))` give `c`, while `(c)(d)` gives nothing because
/// its first `(` is closed before the end. One `successors` fold, no scanning loop.
fn unwrapped(text: &str) -> &str {
    std::iter::successors(Some(text), |open| whole_wrap(open))
        .last()
        .unwrap_or(text)
}

/// The inside of `text` when the whole lexeme is one balanced parenthesis group: dropping the outer
/// pair must leave a run whose depth never returns to zero early (`()(c)` is two groups, not one).
fn whole_wrap(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    let balanced = inner.chars().try_fold(0i32, |depth, c| match c {
        '(' => Some(depth + 1),
        ')' => (depth > 0).then_some(depth - 1),
        _ => Some(depth),
    })?;
    (balanced == 0).then_some(inner)
}

/// Numbers joined by spaced `+`/`-`, left-associative; a lone number counts (it is what a
/// `$var: 5px` binding needs).
fn arithmetic(toks: &[Tok]) -> Option<Expr> {
    let lexemes: Vec<Piece<'_>> = toks
        .iter()
        .enumerate()
        .filter_map(|(index, tok)| classify(tok, index, toks))
        .collect();
    let numbers: Vec<(f64, &str)> = lexemes
        .iter()
        .step_by(2)
        .map(|piece| match piece {
            Piece::Num(value, unit) => Some((*value, *unit)),
            Piece::Op(_) | Piece::Other => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let operators: Vec<BinOp> = lexemes
        .iter()
        .skip(1)
        .step_by(2)
        .map(|piece| match piece {
            Piece::Op(op) => Some(*op),
            Piece::Num(_, _) | Piece::Other => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if numbers.is_empty() || operators.len() + 1 != numbers.len() {
        return None;
    }
    let mut numbers = numbers.into_iter();
    let (head, head_unit) = numbers.next()?;
    Some(numbers.zip(operators).fold(
        Expr::Num(head, head_unit.to_string()),
        |acc, ((value, unit), op)| {
            Expr::Binary(
                Box::new(acc),
                op,
                Box::new(Expr::Num(value, unit.to_string())),
            )
        },
    ))
}

/// An interpolated run folded into the parts to concatenate: literal text, `$var` reads and
/// nested `#{ … }` bodies. Built by one `fold` so no imperative loop drives the shape.
fn interpolation(toks: &[Tok]) -> Option<Expr> {
    let parts = toks
        .iter()
        .fold((Vec::new(), String::new()), |(mut parts, mut lit), tok| {
            let flush = |parts: &mut Vec<Expr>, lit: &mut String| {
                if !lit.is_empty() {
                    parts.push(Expr::Text(std::mem::take(lit)));
                }
            };
            match tok {
                Tok::Var(name) => {
                    flush(&mut parts, &mut lit);
                    parts.push(Expr::Var(name.clone()));
                }
                Tok::Interp(inner) => {
                    flush(&mut parts, &mut lit);
                    parts.push(
                        from_tokens(inner).map_or(Expr::Text(canon(&render(inner))), |body| {
                            Expr::Concat(vec![body])
                        }),
                    );
                }
                other => lit.push_str(&token_text(other)),
            }
            (parts, lit)
        });
    let (mut parts, lit) = parts;
    if !lit.is_empty() {
        parts.push(Expr::Text(lit));
    }
    if parts.is_empty() {
        return None;
    }
    Some(Expr::Concat(parts))
}

impl BinOp {
    /// Apply one operator to two evaluated operands, leaving the unit law to `Value`.
    fn apply(self, lhs: &Value, rhs: &Value) -> Result<Value, SassError> {
        match self {
            BinOp::Add => lhs.plus(rhs),
            BinOp::Sub => lhs.minus(rhs),
        }
    }
}

impl Expr {
    /// Evaluate bottom-up against the owned environment. Errors are Sass's own (undefined
    /// variable, incompatible units); the caller decides how far they travel down the stream.
    pub fn eval(&self, env: &Env) -> Result<Value, SassError> {
        match self {
            Expr::Num(value, unit) => Ok(Value::Number(*value, unit.clone())),
            Expr::Text(text) => Ok(Value::Raw(text.clone())),
            Expr::Var(name) => env.get(name),
            Expr::Call { name, args } => args
                .iter()
                .map(|arg| {
                    let value = argument_expr(&arg.toks)
                        .ok_or_else(|| SassError::new("argument is not an expression yet"))?;
                    value.eval(env).map(|value| (arg.label.clone(), value))
                })
                .collect::<Result<Vec<(Option<String>, Value)>, SassError>>()
                .and_then(|given| crate::builtins::evaluate(name, given)),
            Expr::Binary(lhs, op, rhs) => op.apply(&lhs.eval(env)?, &rhs.eval(env)?),
            Expr::Concat(parts) => parts
                .iter()
                .map(|part| part.eval(env).map(|value| value.to_css()))
                .collect::<Result<Vec<String>, SassError>>()
                .map(|text| Value::Raw(canon(&text.concat()))),
        }
    }
}
