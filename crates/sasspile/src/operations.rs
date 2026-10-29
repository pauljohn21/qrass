//! The lexeme-level readers that turn a token run into an **operation tree**: the top-level split at
//! operators, number lexemes, string concatenation of identifiers, and the two-pass precedence fold. They
//! are the half of `expr` that is about *source shape* rather than about values, which is why they live
//! here and why `expr` keeps only the reading order and `eval`.
//!
//! Four rules the fixtures pinned, restated where they are enforced:
//! * an operand that cannot be read makes the **whole run decline** rather than being skipped — dropping it
//!   silently would shift the alternation and turn `rgb(18 52 none)` into the number `52`;
//! * `+`/`-` are operators only with whitespace on both sides (`10px - 5px` subtracts, `10px -5px` is a
//!   two-item list), while `*` needs no such law and `/` is an operator only where a caller released it
//!   (`expr::release_slashes`, i.e. inside a call argument);
//! * brackets nest the run: the depth is a number carried by the fold, and an operator inside brackets
//!   never splits what encloses it — which is what lets a call or a calculation stand as one operand
//!   (`core_functions/math/div.hrx::unit/denominator`: `math.div(6, 3px) * 1px` → `2`);
//! * precedence is resolved by **two folds over the flat span list** (tight operators inside each loose
//!   piece, then the pieces), so no recursive descent over the run is needed anywhere.

use crate::env::Env;
use crate::expr::{self, BinOp, Expr};
use crate::lexer::Tok;

/// The operand spans of a split run: index ranges into the lexemes the caller already holds.
pub(crate) type Spans = Vec<(usize, usize)>;

/// A value that may not be arithmetic at all: a slash, star or percent anywhere in a lexeme is a CSS
/// separator (or a percentage unit) until the value algebra covers them.
///
/// **Inside brackets is not the same context.** A `/` that sits inside a `calc( … )` belongs to that
/// calculation, which is why `calc(1/2px + 1/4px) * 1px` must be readable as a multiplication of a
/// calculation by a number (`values/calculation/calc/operator/units::denominators` → `0.75`). The depth is
/// counted as data over the run, so `font: 12px/1.5`, `grid: 1/2` and a bare `a {b: 1/2}` — none of which
/// holds a bracket — keep being protected exactly as before.
pub(crate) fn protected(toks: &[Tok]) -> bool {
    let seeded = (false, 0i32);
    let (found, _) = toks.iter().fold(seeded, |(found, depth), tok| {
        let net = tok.net();
        let outside = depth == 0 && net == 0;
        let separator = match tok {
            Tok::Word(word) => word.contains(['/', '*', '%']),
            Tok::Interp(inner) => protected(inner),
            _ => false,
        };
        (found || (outside && separator), (depth + net).max(0))
    });
    found
}

/// Whether a run holds anything that only a live environment can answer — a reference, a module constant
/// or an interpolation. This is the shape test [`crate::expr`] consults before spending an evaluation.
pub(crate) fn has_dynamic(toks: &[Tok]) -> bool {
    toks.iter()
        .any(|tok| matches!(tok, Tok::Var(_) | Tok::MemberVar(..) | Tok::Interp(_)))
}

/// A run cut at its top-level operators: the operand spans, and the operator between each pair.
/// `None` means "not an operation we read" — no top-level operator at all, so a lone number or a CSS call
/// is left to the other readers.
pub(crate) fn split(toks: &[Tok]) -> Option<(Spans, Vec<BinOp>)> {
    let (cuts, _) =
        toks.iter()
            .enumerate()
            .fold((Vec::<(usize, BinOp)>::new(), 0i32), |acc, (index, tok)| {
                let (mut cuts, depth) = acc;
                let balance = tok.net();
                if depth == 0
                    && balance == 0
                    && let Some(op) = operator(toks, index)
                {
                    cuts.push((index, op));
                }
                (cuts, depth + balance)
            });
    let operators = cuts.iter().map(|(_, op)| *op).collect::<Vec<BinOp>>();
    let spans = cuts
        .iter()
        .map(|(index, _)| *index)
        .chain(std::iter::once(toks.len()))
        .scan(0usize, |from, end| {
            let span = (*from, end);
            *from = end + 1;
            Some(span)
        })
        .collect::<Spans>();
    (!operators.is_empty()).then_some((spans, operators))
}

/// The operator at this position of the run. `+`/`-` need whitespace on **both** sides or they are not
/// operators at all: that single law keeps `10px -5px` a two-item list and `a -b` an identifier argument.
fn operator(toks: &[Tok], index: usize) -> Option<BinOp> {
    let Tok::Op(sign) = &toks[index] else {
        return None;
    };
    let spaced =
        |side: Option<usize>| matches!(side.and_then(|i| toks.get(i)), Some(Tok::Space(_)));
    match sign {
        '+' if spaced(index.checked_sub(1)) && spaced(index.checked_add(1)) => Some(BinOp::Add),
        '-' if spaced(index.checked_sub(1)) && spaced(index.checked_add(1)) => Some(BinOp::Sub),
        '*' => Some(BinOp::Mul),
        '/' => Some(BinOp::Div),
        _ => None,
    }
}

/// How much of `tail` is an exponent — `e15`, `E-3`, `e+2`. Zero when there are no digits after the (maybe
/// signed) `e`, which is what keeps `1e` and `1e-` a number with the **unit** `e`: only the
/// exponent-with-digits form has a golden (`values/calculation/calc/no_operator::number/exponent`).
fn exponent_len(tail: &str) -> usize {
    let marker = match tail.chars().next() {
        Some(c @ ('e' | 'E')) => c.len_utf8(),
        _ => return 0,
    };
    let rest = &tail[marker..];
    let signed = usize::from(matches!(rest.chars().next(), Some('+') | Some('-')));
    let digits = rest[signed..]
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map_or(rest.len() - signed, |(i, _)| i);
    if digits > 0 {
        marker + signed + digits
    } else {
        0
    }
}

/// A whole lexeme as `value + unit`: digits (with an optional fraction and an optional exponent) followed by
/// an identifier-shaped unit. `52)` or `1px)` are not numbers — a unit is alphanumeric, so a
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
    // An **exponent** belongs to the number, not to the unit: `calc(1e2px)` → `100px`
    // (`values/calculation/calc/no_operator::number/exponent`) and `math.$pi * 1e15` is a product of two
    // numbers (`core_functions/math/variables.hrx::pi`). Reading `1e15` as "1 with the unit `e15`" is what
    // left those rows unfolded — the unit `e15` exists nowhere, it was just where the digit scan stopped.
    let digits = digits + exponent_len(&digits_area[digits..]);
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

/// Operands joined by `+`/`-`/`*`/`/`: the operands are read first (a number, a call, a calculation), then
/// the flat span list is folded twice — once inside each additive piece for `*` and `/`, once over the
/// pieces. A lone number counts (it is what a `$var: 5px` binding needs, and what
/// [`crate::exec::State::evaluated`] later filters out at the whole-value level).
pub(crate) fn arithmetic(toks: &[Tok], env: &Env) -> Option<Expr> {
    let (spans, operators) = split(toks)?;
    let operands = spans
        .iter()
        .filter_map(|(from, to)| expr::operand_expr(&toks[*from..*to], env))
        .collect::<Vec<Expr>>();
    // Every span must have produced an operand: a dropped one would shift the alternation and pair the
    // wrong operator with the wrong value, which is how `rgb(18 52 none)` once became the number `52`.
    (operands.len() == operators.len() + 1).then(|| tree(operands, &operators))?
}

/// The additive pieces of a run: each with the operator that joins it to the piece before, and the
/// multiplicative chain it contains.
type Pieces = Vec<(Option<BinOp>, Vec<(Option<BinOp>, Expr)>)>;

/// One operand with the operator that precedes it (`None` for the first of a list).
type Chain = Vec<(Option<BinOp>, Expr)>;

/// The two-pass tree over already-read operands: additive pieces first, products inside each piece.
/// Left-associative throughout, so `1 - 2 - 3` is `(1 - 2) - 3` and `2 * 3 + 4` is `(2 * 3) + 4` — the same
/// order the corpus states for `calc( … )` (`operator/precedence::simplified/*`).
fn tree(operands: Vec<Expr>, operators: &[BinOp]) -> Option<Expr> {
    let paired = std::iter::once(None)
        .chain(operators.iter().copied().map(Some))
        .zip(operands);
    let pieces = paired.fold(Pieces::new(), |mut acc, (op, operand)| {
        match op {
            Some(loose @ (BinOp::Add | BinOp::Sub)) => {
                acc.push((Some(loose), vec![(None, operand)]));
            }
            tight => {
                if acc.is_empty() {
                    acc.push((None, Vec::new()));
                }
                if let Some((_, factors)) = acc.last_mut() {
                    factors.push((tight, operand));
                }
            }
        }
        acc
    });
    products(pieces)
}

/// Fold each additive piece into a product, then the products into a sum. Both are one `fold` over the
/// piece's own list, joining with the operator that preceded each operand.
fn products(pieces: Pieces) -> Option<Expr> {
    let folded = pieces
        .into_iter()
        .map(|(join, factors)| chain(factors).map(|product| (join, product)))
        .collect::<Option<Vec<(Option<BinOp>, Expr)>>>()?;
    folded
        .into_iter()
        .fold(None::<Expr>, |sum, (join, product)| {
            Some(match (sum, join) {
                (None, _) => product,
                (Some(left), Some(op)) => Expr::Binary(Box::new(left), op, Box::new(product)),
                (Some(_), None) => return None,
            })
        })
}

/// The same fold for one multiplicative chain: `2 * 3 / 5` is `((2 * 3) / 5)`.
fn chain(factors: Chain) -> Option<Expr> {
    factors
        .into_iter()
        .fold(None::<Expr>, |acc, (op, operand)| {
            Some(match (acc, op) {
                (None, _) => operand,
                (Some(left), Some(op)) => Expr::Binary(Box::new(left), op, Box::new(operand)),
                (Some(_), None) => return None,
            })
        })
}
