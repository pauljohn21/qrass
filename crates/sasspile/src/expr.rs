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

fn has_dynamic(toks: &[Tok]) -> bool {
    toks.iter()
        .any(|tok| matches!(tok, Tok::Var(_) | Tok::Interp(_)))
}

/// Build an `Expr` from a token run, or `None` when the run holds a shape P1 must not read.
pub fn from_tokens(toks: &[Tok]) -> Option<Expr> {
    if protected(toks) {
        return None;
    }
    if has_dynamic(toks) {
        interpolation(toks)
    } else {
        arithmetic(toks)
    }
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
            Expr::Binary(lhs, op, rhs) => op.apply(&lhs.eval(env)?, &rhs.eval(env)?),
            Expr::Concat(parts) => parts
                .iter()
                .map(|part| part.eval(env).map(|value| value.to_css()))
                .collect::<Result<Vec<String>, SassError>>()
                .map(|text| Value::Raw(canon(&text.concat()))),
        }
    }
}
