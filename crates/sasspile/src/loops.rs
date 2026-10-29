//! Recognising and enumerating a `@for` header — the part of execution that decides *what a loop is*,
//! kept apart from [`crate::exec`]'s job of moving one event into the flowing state.
//!
//! The body is carried on the open block as data (the same nesting-as-value the `scopes` stack already
//! uses) and re-run per value, which is why the replay lives with the state and only this reading lives
//! here. Naming it plainly: **"collect a body, then replay it" is on the banned list**
//! (`.qoder/skills/sasspile-start/reference/gc-counterexamples.md`), and this is its standing instance.
//! The fan-out itself is real — one closing event now emits one rule per iteration — but the vehicle is not
//! yet design.md §3's `flat_map` over a body sub-stream, because that needs `Structurize` to hand the body
//! up as one owned value. Recorded as open in `tasks.md` 9.7, not written here as finished.
//!
//! The interval *is* data: [`Walk`] carries it as three numbers and generates it on demand.

use crate::SassError;
use crate::brackets;
use crate::env::Env;
use crate::lexer::{Event, Tok};
use crate::value::Value;

/// A `@for` loop captured on the way in: the variable it binds, the interval it walks, and the body
/// events held for one pass per value.
#[derive(Debug, Clone)]
pub(crate) struct ForLoop {
    pub(crate) var: String,
    pub(crate) walk: Walk,
    pub(crate) body: Vec<Event>,
    /// How many blocks captured in `body` are still open. Events arrive one at a time and a body block's
    /// `Close` looks exactly like the loop's own `Close` unless something counts them, so this is the depth
    /// as data — the same shape `brackets::Run{level}` and `calc` carry — not a cursor driving a traversal.
    pub(crate) open: i32,
}

/// The interval a `@for` walks, as **three numbers** — its first value, how many there are, and the step.
///
/// Kept as data rather than materialised: collecting the span into a `Vec<i64>` the moment the header was
/// read asked for one eight-byte slot per iteration before any declaration was evaluated, so
/// `@for $i from 1 through 1000000000000` failed to allocate and took the process with it. Moving the bound
/// for `to` is checked for the same reason — `@for $i from 1 to -9223372036854775808` wrapped to a huge
/// positive bound instead of declining.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Walk {
    first: i64,
    count: i64,
    step: i64,
}

impl Walk {
    /// The interval `from <first>` *direction* `<last>` names: `through` keeps its endpoint and `to` drops
    /// one step of it, a start above the end walks downwards (`5 through 1` → 5,4,3,2,1), and an empty range
    /// carries no values (`1 to 1` contributes nothing). `None` when the arithmetic does not fit an `i64`,
    /// which declines the header rather than inventing a bound.
    fn bounded(first: i64, last: i64, inclusive: bool) -> Option<Self> {
        let step = if first <= last { 1 } else { -1 };
        let end = last.checked_sub(i64::from(!inclusive) * step)?;
        let count = end.checked_sub(first)?.checked_mul(step)?.checked_add(1)?;
        (count >= 0).then_some(Self { first, count, step })
    }

    /// The values in document order, generated as they are asked for. `k` never exceeds `count - 1`, which
    /// is the distance between the two bounds, so every product lands inside them.
    pub(crate) fn values(self) -> impl Iterator<Item = i64> {
        (0..self.count).map(move |k| self.first + k * self.step)
    }
}

/// Recognise **`@for $i from <bound> through|to <bound>`** and nothing else, decided on lexemes: `@for`,
/// a variable, `from`, a bound, a direction word, a bound. Any other shape (`@each`, a nested
/// parenthesised header, a bound that is not an integer) returns `None` and keeps today's behaviour, so the
/// fan-out only happens where the header was read without guessing.
///
/// A bound is read through the environment because the corpus writes one as a reference: `through $limit`
/// (`directives/for/for.hrx::to_scope`). The walk is decided **here, once** — the same case states that
/// re-binding `$limit` inside the body does not move the end of the loop.
///
/// Goldens: `directives/for/for.hrx::{inclusive_forward/scss, exclusive_forward, inclusive_backward,
/// negative_to_positive, empty/scss, to_scope}`.
pub(crate) fn for_loop(toks: &[Tok], env: &Env) -> Result<Option<ForLoop>, SassError> {
    let header: Vec<&Tok> = toks
        .iter()
        .filter(|tok| !brackets::is_layout(tok))
        .collect();
    let [
        Tok::Word(keyword),
        Tok::Var(var),
        Tok::Word(from_kw),
        rest @ ..,
    ] = header.as_slice()
    else {
        return Ok(None);
    };
    if !keyword.eq_ignore_ascii_case("@for") || !from_kw.eq_ignore_ascii_case("from") {
        return Ok(None);
    }
    // The direction word is searched for rather than assumed to sit at a fixed offset, so a bound that is
    // more than one lexeme is read as the run it is instead of shifting every later slot.
    let turned = rest.iter().enumerate().find_map(|(offset, tok)| match tok {
        Tok::Word(word) => known_direction(word).map(|inclusive| (offset, inclusive)),
        _ => None,
    });
    let Some((offset, inclusive)) = turned else {
        return Ok(None);
    };
    let (Some(first), Some(last)) = (
        integer(&rest[..offset], env)?,
        integer(&rest[offset + 1..], env)?,
    ) else {
        return Ok(None);
    };
    Ok(Walk::bounded(first, last, inclusive).map(|walk| ForLoop {
        var: var.clone(),
        walk,
        body: Vec::new(),
        open: 0,
    }))
}

/// The direction word and whether it keeps its endpoint. One table for both spellings, so a third word
/// declines instead of being guessed at.
fn known_direction(word: &str) -> Option<bool> {
    [("through", true), ("to", false)]
        .into_iter()
        .find(|(name, _)| word.eq_ignore_ascii_case(name))
        .map(|(_, inclusive)| inclusive)
}

/// One `@for` bound as an integer, or an `Err` when reading it fails the way the corpus lets a program fail.
///
/// A **literal** bound is parsed exactly as the header always read one (`from -1` keeps its minus inside the
/// lexeme, per `for.hrx::negative_to_positive`). A **reference** bound is resolved in the environment at the
/// header's position — `through $limit` (`for.hrx::to_scope`) — and resolved *once*, because that same case
/// states that re-binding `$limit` inside the body does not move the end of the walk. An unbound name is
/// **not folded into a decline**: `Env::get` raises the `UndefinedVariable` that the crate root counts among
/// the fatal kinds, and eating it with `.ok()` is the `.ok()`-swallows-a-real-`Err` shape this project has
/// now measured twice.
///
/// A bound that is not an integer declines, and the corpus's two statements about those are kept apart here:
/// `for.hrx::error/{from_float,to_float,from_type,to_type,incompatible_units,unit_coersion_to_float}` pin
/// the **messages** (`1.5 is not an int.`), which is P7's error work, while `for.hrx::unit/same` pins that a
/// `1px through 5px` header **expands and keeps its units** — a reading this slice does not have, so it
/// declines rather than dropping the `px` and printing a number nobody wrote.
fn integer(bound: &[&Tok], env: &Env) -> Result<Option<i64>, SassError> {
    let whole = match bound {
        [Tok::Word(word)] => word.parse::<i64>().ok(),
        [Tok::Var(name)] => match env.get(name)? {
            Value::Number(value, unit) if unit.is_unitless() && value.fract() == 0.0 => {
                Some(value as i64)
            }
            // A binding of a lone number is carried as its **source text** — refusing to turn one into an
            // expression is `expr::from_tokens`'s own law (it is what keeps `w: 10px` written exactly), and
            // `for.hrx::to_scope` writes `$limit: 4` at root — so the header reads that text back as the
            // integer it says. Anything that does not parse as one declines: `4px`, `1.5`, `"4"` and `4e0`
            // stay unread rather than silently becoming a number. Printing a value and scanning it back is a
            // stopgap; the shape that removes it is an integer slot carried by the binding itself, recorded
            // as open in `tasks.md` 9.7.
            bound => bound.to_css().parse::<i64>().ok(),
        },
        _ => None,
    };
    Ok(whole)
}
