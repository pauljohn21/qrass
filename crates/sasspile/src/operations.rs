//! The lexeme-level readers that turn a token run into an **operation tree**: number lexemes, the
//! spacing law for `+`/`-`, string concatenation of identifiers, and the precedence-aware fold. They are
//! the half of `expr` that is about *source shape* rather than about values, which is why they live here
//! and why `expr` keeps only the reading order and `eval`.
//!
//! Two rules the fixtures pinned, restated where they are enforced:
//! * a lexeme that is not a number must mark `Other` so the whole run bails — dropping it silently would
//!   shift the alternation and turn `rgb(18 52 none)` into the number `52`;
//! * `+`/`-` are operators only with whitespace on both sides (`10px - 5px` subtracts, `10px -5px` is a
//!   two-item list), while `*` needs no such law and `/` is an operator only where a caller released it
//!   (`expr::release_slashes`, i.e. inside a call argument).

use crate::expr::{BinOp, Expr};
use crate::lexer::Tok;
use crate::unit::Unit;

/// One classified lexeme of the arithmetic path.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Piece<'a> {
    Num(f64, &'a str),
    Op(BinOp),
    Other,
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
        // `*` needs no spacing law: unlike `-` it can never continue an identifier, and unlike `/` it
        // is never a list separator, so `2*3` and `2 * 3` are the same multiplication.
        Tok::Op('*') => Some(Piece::Op(BinOp::Mul)),
        // A `/` reaches this point only as its own lexeme, which happens only inside a call argument
        // (see [`release_slashes`]) — in a declaration value it is still the slash-separated list.
        Tok::Op('/') => Some(Piece::Op(BinOp::Div)),
        _ => Some(Piece::Other),
    }
}

/// A whole lexeme as `value + unit`: digits (with optional fraction) followed by an
/// identifier-shaped unit. `52)` or `1px)` are not numbers — a unit is alphanumeric, so a
/// CSS call like `rgb(18 none 66)` bails to verbatim text instead of losing its paren.
pub(crate) fn number_lexeme(word: &str) -> Option<(f64, &str)> {
    // A leading sign belongs to the number: the lexer defers a boundary `-` until it sees the next
    // character, so `-5.6` and `-7px` arrive as one lexeme and Sass reads them as negative numbers
    // (`math.round(-5.6)` → `-6`). This was already required for `@for`'s negative bounds, and reading it
    // as text instead produced a false `$number: -5.6 is not a number.`.
    let signed = word.starts_with(&['-', '+'][..]);
    let (sign, digits_area) = if signed {
        (&word[..1], &word[1..])
    } else {
        ("", word)
    };
    let digits = digits_area
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(digits_area.len());
    if digits == 0 {
        return None;
    }
    let (numeric, unit) = digits_area.split_at(digits);
    // `%` is a unit character for Sass; it stays out of every other reading of a run because `protected`
    // still refuses to evaluate a run that carries one outside a known call
    // (`math.percentage(1%)` must reach its body to report the unit, not be told it is not a number).
    if !unit.chars().all(|c| c.is_ascii_alphanumeric() || c == '%') {
        return None;
    }
    format!("{sign}{numeric}")
        .parse::<f64>()
        .ok()
        .map(|value| (value, unit))
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
pub(crate) fn concatenation(toks: &[Tok]) -> Option<Expr> {
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

/// Numbers joined by `+`/`-`/`*`, left-associative with multiplication binding tighter; a lone number
/// counts (it is what a `$var: 5px` binding needs).
pub(crate) fn arithmetic(toks: &[Tok]) -> Option<Expr> {
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
    build(&numbers, &operators)
}

/// The tree for an alternating list of numbers and operators. The **rightmost** additive operator is the
/// root (so `1 - 2 - 3` becomes `(1 - 2) - 3`, left-associative, and `*` binds tighter because it can
/// never be that root); with no additive operator left, the run folds multiplicatively left-to-right.
///
/// Recursion here is over a *value expression*, whose depth is the number of operands in one
/// declaration — not a document tree being walked, which is the shape the rules forbid.
fn build(numbers: &[(f64, &str)], operators: &[BinOp]) -> Option<Expr> {
    let literal = |(value, unit): (f64, &str)| Expr::Num(value, Unit::named(unit));
    let head = *numbers.first()?;
    let root = operators
        .iter()
        .rposition(|op| matches!(op, BinOp::Add | BinOp::Sub));
    Some(match root {
        // The operands to the left of the root include the one it joins, hence `..=at` on the numbers
        // and `..at` on the operators; to its right both start just after it.
        Some(at) => Expr::Binary(
            Box::new(build(&numbers[..=at], &operators[..at])?),
            operators[at],
            Box::new(build(&numbers[at + 1..], &operators[at + 1..])?),
        ),
        None => operators
            .iter()
            .zip(numbers.iter().skip(1))
            .fold(literal(head), |acc, (op, operand)| {
                Expr::Binary(Box::new(acc), *op, Box::new(literal(*operand)))
            }),
    })
}
