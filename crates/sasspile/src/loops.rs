//! Recognising and enumerating a `@for` header — the part of execution that decides *what a loop is*,
//! kept apart from [`crate::exec`]'s job of moving one event into the flowing state.
//!
//! The captured body stays inside the open block as data (the same nesting-as-value the `scopes` stack
//! already uses), which is why replay itself lives with the state and only this reading lives here.

use crate::lexer::{Event, Tok};

/// A `@for` loop captured on the way in: the variable it binds, the values it walks, and the body
/// declarations held for replay.
#[derive(Debug, Clone)]
pub(crate) struct ForLoop {
    pub(crate) var: String,
    pub(crate) values: Vec<i64>,
    pub(crate) body: Vec<Event>,
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
    let inclusive = if direction.eq_ignore_ascii_case("through") {
        true
    } else if direction.eq_ignore_ascii_case("to") {
        false
    } else {
        return None;
    };
    let (first, last) = (from.parse::<i64>().ok()?, to.parse::<i64>().ok()?);
    Some(ForLoop {
        var: var.clone(),
        values: sequence(first, last, inclusive),
        body: Vec::new(),
    })
}

/// The `@for` value sequence: `through` includes its endpoint, `to` excludes it, and a start above the
/// end walks downwards (`5 through 1` → 5,4,3,2,1); an empty range stays empty instead of wrapping
/// (`1 to 1` contributes nothing).
fn sequence(first: i64, last: i64, inclusive: bool) -> Vec<i64> {
    if first <= last {
        let bound = if inclusive { last } else { last - 1 };
        (first..=bound).collect()
    } else {
        let bound = if inclusive { last } else { last + 1 };
        (bound..=first).rev().collect()
    }
}
