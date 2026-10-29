//! Recognising and enumerating a `@for` header — the part of execution that decides *what a loop is*,
//! kept apart from [`crate::exec`]'s job of moving one event into the flowing state.
//!
//! The captured body stays inside the open block as data (the same nesting-as-value the `scopes` stack
//! already uses), which is why replay itself lives with the state and only this reading lives here. The
//! interval is *not* held as data: [`Walk`] carries it as three numbers and generates it on demand.

use crate::lexer::{Event, Tok};

/// A `@for` loop captured on the way in: the variable it binds, the interval it walks, and the body
/// declarations held for replay.
#[derive(Debug, Clone)]
pub(crate) struct ForLoop {
    pub(crate) var: String,
    pub(crate) walk: Walk,
    pub(crate) body: Vec<Event>,
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

/// Recognise **`@for $i from <int> through|to <int>`** and nothing else, decided on lexemes: exactly
/// `@for`, a variable, `from`, an integer, a direction, an integer. Any other shape (an expression as a
/// bound, `@each`, a nested parenthesised header) returns `None` and keeps today's behaviour, so the
/// replay only happens where the header was read without guessing.
///
/// Goldens: `directives/for/for.hrx::{inclusive_forward/scss, exclusive_forward, inclusive_backward,
/// negative_to_positive, empty/scss}`.
pub(crate) fn for_loop(toks: &[Tok]) -> Option<ForLoop> {
    let header: Vec<&Tok> = toks
        .iter()
        .filter(|tok| !matches!(tok, Tok::Space(_) | Tok::Silent(_)))
        .collect();
    let [
        Tok::Word(keyword),
        Tok::Var(var),
        Tok::Word(from_kw),
        Tok::Word(from),
        Tok::Word(direction),
        Tok::Word(to),
    ] = header.as_slice()
    else {
        return None;
    };
    if !keyword.eq_ignore_ascii_case("@for") || !from_kw.eq_ignore_ascii_case("from") {
        return None;
    }
    // The two direction words as data, chosen by a search rather than an `if`/`else if` ladder: any other
    // spelling leaves `None` and the header declines.
    let inclusive = [("through", true), ("to", false)]
        .into_iter()
        .find(|(word, _)| direction.eq_ignore_ascii_case(word))
        .map(|(_, flag)| flag)?;
    let (first, last) = (from.parse::<i64>().ok()?, to.parse::<i64>().ok()?);
    Some(ForLoop {
        var: var.clone(),
        walk: Walk::bounded(first, last, inclusive)?,
        body: Vec::new(),
    })
}
