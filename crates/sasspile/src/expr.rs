//! Expr — the arithmetic slice of a declaration value, assembled from the structured `Tok`
//! run in one forward pass (never by re-scanning rendered text) and evaluated bottom-up.
//!
//! P1 starts deliberately narrow: a run becomes an `Expr` only when it is **unambiguous** —
//! number lexemes joined by space-separated `+`/`-`. Any lexeme carrying `/`, `*` or `%`,
//! any `$var`, interpolation, silent comment or CSS-native function bails out to
//! `Value::Raw` verbatim text, which is exactly what the wiring rule requires:
//! `font: 12px/1.5` and `grid: 1/2` must never be read as arithmetic, and `calc()`/`min()`
//! stay CSS-native until the `Calculation` type arrives.
//!
//! Sass itself is the source of the spacing rule, not a convention invented here: `+`/`-`
//! are operators only when whitespace surrounds them, so `10px - 5px` subtracts while
//! `10px -5px` is a two-item space list (which bails out, as it must).

use crate::SassError;
use crate::lexer::Tok;
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
}

/// One classified lexeme; `Other` is the bail-out marker for anything P1 must not evaluate.
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

/// Classify one token, given its position in the run: `+`/`-` is an operator **only** when
/// whitespace sits on both sides, otherwise the run is not a pure arithmetic expression.
fn classify<'a>(tok: &'a Tok, index: usize, run: &[Tok]) -> Option<Piece<'a>> {
    let spaced = |side: Option<usize>| matches!(side.and_then(|i| run.get(i)), Some(Tok::Space(_)));
    match tok {
        Tok::Space(_) => None,
        Tok::Word(w) => number_lexeme(w).map(|(value, unit)| Piece::Num(value, unit)),
        Tok::Op('+') if spaced(index.checked_sub(1)) && spaced(index.checked_add(1)) => {
            Some(Piece::Op(BinOp::Add))
        }
        Tok::Op('-') if spaced(index.checked_sub(1)) && spaced(index.checked_add(1)) => {
            Some(Piece::Op(BinOp::Sub))
        }
        _ => Some(Piece::Other),
    }
}

/// A whole lexeme as `value + unit`: digits (with optional fraction), then the unit text.
fn number_lexeme(word: &str) -> Option<(f64, &str)> {
    let digits = word
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(word.len());
    if digits == 0 {
        return None;
    }
    let (numeric, unit) = word.split_at(digits);
    numeric.parse::<f64>().ok().map(|value| (value, unit))
}

/// Build an `Expr` from a declaration-value token run, or `None` when the run is anything
/// other than number lexemes joined by spaced `+`/`-` (then the text survives verbatim).
pub fn from_tokens(toks: &[Tok]) -> Option<Expr> {
    if protected(toks) {
        return None;
    }
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
    if numbers.len() < 2 || operators.len() + 1 != numbers.len() {
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
    /// Evaluate bottom-up into a `Value`. Errors are the Sass ones (incompatible units);
    /// the caller decides how far they travel down the stream.
    pub fn eval(&self) -> Result<Value, SassError> {
        match self {
            Expr::Num(value, unit) => Ok(Value::Number(*value, unit.clone())),
            Expr::Binary(lhs, op, rhs) => op.apply(&lhs.eval()?, &rhs.eval()?),
        }
    }
}
