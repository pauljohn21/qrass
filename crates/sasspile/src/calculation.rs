//! The **shell law** of a calculation: what leaves a `calc( … )`, what drops one wrapper, and what stays
//! inside it. Locating the shell and normalising its lexemes is [`crate::brackets`]'s job; this module is
//! only the law, so it reads as decisions and not as bracket bookkeeping.
//!
//! Each rule is quoted from a fixture, never inferred:
//! * a single collapsible value leaves its shells — `calc(1px)` → `1px` (`no_operator::number/integer`),
//!   `CaLc(1px)` → `1px` (`…::case_insensitive`), `calc( 1px )` → `1px` (`…::extra_whitespace`),
//!   `calc((1px))` → `1px` (`calc/parens::number`);
//! * a nested calculation drops **one** shell, and that can expose the next collapse —
//!   `calc(calc(1px))` → `1px` (`…::calculation/calc/simplified`), `calc(clamp(1%, 2px, 3%))` →
//!   `clamp(1%, 2px, 3%)` (`…::calculation/clamp/preserved`), `calc(min(1%, 2px))` → `min(1%, 2px)`;
//! * anything else keeps its outer shell — `calc(var(--c))`, `calc(c())`, `calc(foobar)`,
//!   `calc((d))` (`calc/parens::{var/direct,double_preserved,identifier}`); several of these passed before
//!   the first calculation rule existed, so they are this law's safety net;
//! * the content itself is reduced by [`crate::calc`] — flattening, not a tree: same-unit folds
//!   (`operator/plus::simplified`), `*` and `/` (`operator/times::simplified`), rational units
//!   (`values/numbers/units/multiple`), and a bound calculation rejoining arithmetic
//!   (`…::division/cancels/numerator/once`);
//! * `min`/`max`/`clamp` are **not** evaluated as shells here: bare-name `min(1%, 2px)` keeping its text is
//!   the 1.4c law (`no_operator::function/css`), and `calculation/min::simplified` wants the *registered*
//!   builtin instead (`::calculation/min/preserved` against `::min/simplified`).
//!
//! What 1.4a deliberately left out is now in: simplification and rational units landed with 1.4d.

use crate::brackets::{Shell, find, is_calc, meaningful, open_at, resolved, without_leading_sign};
use crate::calc::{self, Content};
use crate::calls;
use crate::env::Env;
use crate::lexer::{Tok, render};
use crate::unit::Unit;
use crate::value::Value;

/// What one argument came down to. The shapes stay separate because the corpus treats them differently:
/// only a number/boolean leaves a shell, and an inner calculation's **body** is the one text that
/// replaces its wrapper instead of sitting inside it.
#[derive(Clone)]
enum Outcome {
    Collapsed(Value),
    /// an inner calculation's body without its wrapper: the enclosing shell's name takes it over
    Nested(Content),
    /// a calculation that already carries its own name — content to the enclosing shell, whose wrapper
    /// drops in favour of it
    Named(String),
    /// readable content that is not a calculation
    Kept(String),
    /// a chain `crate::calc` folded as far as it could: its content keeps the **value** it reached, so a
    /// bound calculation can be divided or multiplied again (`multiplication::cancels/*`)
    Folded(Content),
    /// nothing this slice reads; the caller keeps the run's own text
    Opaque,
}

/// Build the calculation value a run stands for, or `None` when it is not a calculation this slice
/// reads (unknown callee, unclosed bracket, an operation outside the call).
///
/// The event is the diagnostic this module exists to answer: the **lexeme shape as it arrived** and after
/// [`open_at`], plus which decision was taken. Every wrong version of this rule failed silently — the
/// callee test matched nothing, so nothing moved — and only these two facts made that visible.
pub fn value(toks: &[Tok], env: &Env) -> Option<Value> {
    let shell = find(toks)?;
    let built = to_value(&shell, &arguments(&shell, env), env);
    // Named outside the macro on purpose: `tracing` resolves `Value` to its own field trait inside a
    // macro body, so a `Value::Calculation` path there does not compile.
    let decision = match &built {
        None => "declined",
        Some(Value::Calculation(..)) => "preserved",
        Some(Value::Raw(..)) => "shell dropped",
        Some(_) => "collapsed",
    };
    let resolved = built.as_ref().map_or(String::new(), Value::to_css);
    tracing::debug!(
        stage = "exec",
        module = "calculation",
        name = shell.name.as_str(),
        arrived = %render(toks),
        cut = ?open_at(toks),
        decision,
        resolved = %resolved,
        "calculation read"
    );
    built
}

/// The shell law. A `calc` around **another named calculation** drops its wrapper and hands the inner
/// one over — even when the inner cannot be evaluated, which is how `calc(calc(1px))` reaches `1px` in
/// two steps while `calc(clamp(1%, 2px, 3%))` stops at `clamp(1%, 2px, 3%)`. A `calc` around plain
/// content keeps its wrapper (`calc(var(--c))`). `min`/`max`/`clamp` keep their own name and are never
/// evaluated here, because 1.4a refuses to guess that law.
fn to_value(shell: &Shell, parts: &[Outcome], env: &Env) -> Option<Value> {
    if !is_calc(&shell.lower) {
        // A `min`/`max`/`clamp` shell keeps its own name and never evaluates: that law is 1.4a's "do not
        // guess" case. One unreadable argument makes the whole run decline, which leaves the text exactly
        // as the source wrote it — no name or spacing is rewritten outside a `calc` (1.4c).
        let readable = parts.iter().all(|part| !matches!(part, Outcome::Opaque));
        return readable.then(|| {
            Value::Calculation(
                shell.name.clone(),
                Content {
                    folded: one_argument_value(parts),
                    printed: rendered(parts),
                    precedence: 3,
                },
            )
        });
    }
    match parts {
        [Outcome::Collapsed(value)] => Some(value.clone()),
        // One folded chain inside `calc( … )` keeps the shell *and* its value: `calc(2px * 3px)` prints
        // `calc(6px * 1px)` but is still the number `6px*px` for whoever divides it
        // (`calc/operator/units::multiplication`).
        [Outcome::Folded(content)] => Some(Value::Calculation(shell.name.clone(), content.clone())),
        [Outcome::Nested(inner)] => Some(Value::Calculation(
            shell.name.clone(),
            Content {
                folded: inner.folded.clone(),
                printed: inner.printed.clone(),
                precedence: inner.precedence,
            },
        )),
        // An inner calculation printed with its own name: that name survives, this wrapper goes.
        [Outcome::Named(text)] => Some(Value::Raw(text.clone())),
        [Outcome::Kept(text)] => Some(Value::Calculation(
            shell.name.clone(),
            Content {
                folded: None,
                printed: text.clone(),
                precedence: 3,
            },
        )),
        [Outcome::Opaque] => decline_to_text(shell, env),
        many => Some(Value::Calculation(
            shell.name.clone(),
            Content {
                folded: one_argument_value(many),
                printed: rendered(many),
                precedence: 3,
            },
        )),
    }
}

/// The value an argument list reached, when one part carried it: a compound product or quotient stays
/// printable as text and usable as a number at the same time.
fn folded(parts: &[Outcome]) -> Option<(f64, Unit)> {
    parts.iter().find_map(|part| match part {
        Outcome::Folded(content) | Outcome::Nested(content) => content.folded.clone(),
        Outcome::Collapsed(value) => match value.number() {
            Some(Value::Number(number, unit)) => Some((number, unit)),
            _ => None,
        },
        _ => None,
    })
}

/// [`folded`] behind the arity guard: a calculation may claim a value only when its argument list is
/// **one part**. `find_map` stopping at the first value is right for one argument and a fabrication for
/// more — `min(10, 20)` is not `10`, yet `math.div(1, min(10, 20))` printed `0.1`, and
/// `max(10, 20) * 1` printed `10`. Nothing in the corpus puts a multi-argument calculation into
/// arithmetic (`values/calculation/min.hrx::preserved/*` keep the call written and no row divides by
/// such a shell), so the shape left standing is the project's own answer for "cannot be evaluated":
/// the text as written (1.4a's do-not-guess law), reached because `Value::number()` then yields `None`
/// and the arithmetic declines.
fn one_argument_value(parts: &[Outcome]) -> Option<(f64, Unit)> {
    match parts {
        [single] => folded(std::slice::from_ref(single)),
        _ => None,
    }
}

/// The argument list this slice cannot read as a value: its normalised text survives, and if that text is
/// itself a calculation call, one wrapper drops in favour of the inner one
/// (`::calculation/*/preserved`, `calc/parens::calculation`). A bare `min( … )` with nothing readable in
/// it declines instead, which keeps today's verbatim behaviour for the shapes 1.4a leaves alone.
fn decline_to_text(shell: &Shell, env: &Env) -> Option<Value> {
    match find(&shell.inner) {
        Some(inner) => to_value(&inner, &arguments(&inner, env), env).map(|value| match value {
            Value::Calculation(name, body) => Value::Raw(format!("{name}({})", body.printed)),
            other => other,
        }),
        None if is_calc(&shell.lower) => Some(Value::Calculation(
            shell.name.clone(),
            Content {
                folded: None,
                printed: resolved(&shell.inner, env),
                precedence: 3,
            },
        )),
        None => None,
    }
}

/// The text a preserved calculation carries: its references **resolved**, because a `calc` argument is
/// still parsed as SassScript (`::variable/number/simple_unit`, `0′.3b-2`'s lesson that special-call
/// arguments resolve variables while `+`/`-` stay non-arithmetic). A run with nothing dynamic in it, and
/// one whose reference is unbound, both fall back to the normalised lexeme text — an unbound `$x` inside
/// `calc( … )` keeps its own text and never becomes a fatal error
/// (`::variable/not_parsed_as_interpolation/*`).
fn arguments(shell: &Shell, env: &Env) -> Vec<Outcome> {
    calls::top_level_groups(&shell.inner)
        .into_iter()
        .map(|group| classify(group, env))
        .collect()
}

/// One argument, read structurally first and as a value second.
fn classify(toks: Vec<Tok>, env: &Env) -> Outcome {
    let content = open_at(&toks);
    let run = meaningful(&content);
    if run.is_empty() {
        return Outcome::Opaque;
    }
    // Redundant parentheses are **not** peeled here: whether a group survives is a property of what it
    // holds, and `calc::read` decides it from the data (`parens::operation` drops `calc((1px + 1%))` to
    // `calc(1px + 1%)` while `parens::var/direct` keeps `calc((var(--c)))` as written).
    if let Some(shell) = find(&content) {
        return match to_value(&shell, &arguments(&shell, env), env) {
            Some(value @ (Value::Number(..) | Value::Bool(..))) => Outcome::Collapsed(value),
            // Only a `calc` hands its body over (that is the dropped shell); a `clamp` keeps its own
            // name, which is what `::calculation/clamp/preserved` prints as `clamp(1%, 2px, 3%)`.
            Some(Value::Calculation(name, inner)) if is_calc(&name) => {
                Outcome::Nested(inner.clone())
            }
            Some(Value::Calculation(name, inner)) => {
                Outcome::Named(format!("{name}({})", inner.printed))
            }
            Some(value) => Outcome::Kept(value.to_css()),
            None => Outcome::Opaque,
        };
    }
    // A lone `$var` is a **lookup**: a bound calculation carries its own form out of the outer shell
    // (`::variable/calculation`: `$a: calc(1px + 1%)` then `calc($a)` → `calc(1px + 1%)`), a number
    // collapses straight out (`syntax/extra_whitespace/number`), and any other bound value is content
    // that stays inside it — printed as the value it resolved to, which is how
    // `::variable/unquoted_string` turns `calc($a)` into `calc(foobar)`. An *unbound* reference keeps its
    // own text and must not become a fatal error (`::variable/not_parsed_as_interpolation/*`).
    if let [Tok::Var(name)] = run.as_slice() {
        return match env.get(name) {
            // The bound calculation's *body* is what replaces the outer wrapper, so `calc($a)` over
            // `$a: calc(1px + 1%)` prints `calc(1px + 1%)` — one shell, not two.
            Ok(Value::Calculation(_, inner)) => Outcome::Nested(inner.clone()),
            Ok(value) if collapsible(&value) => Outcome::Collapsed(value),
            // A bound number with a compound unit is calculation-only too: decline rather than print a
            // bare `1(px*rad)^-1` outside any `calc( … )`.
            Ok(Value::Number(_, unit)) if !unit.is_plain() => Outcome::Opaque,
            Ok(value) => Outcome::Kept(value.to_css()),
            Err(_) => Outcome::Opaque,
        };
    }
    // A leading sign that is glued to its number is part of the **literal**, not an operation:
    // `::number/leading_plus` wants `calc(+1px)` → `1px` while the lexer delivered `Op('+')` and
    // `Word("1px")` as two lexemes; every other operator is read by `crate::calc`.
    let signed = without_leading_sign(&content);
    // The group is then read as an **expression** (`crate::calc`): it folds while every step is
    // computable, and prints with Sass's own parentheses. A run it cannot alternate into operands and
    // operators — `foobar`, `c()`, `var(--c) 1` — stays on the text path, so nothing 1.4a already printed
    // correctly moves.
    let reading = calc::read(&signed, env);
    match reading {
        // Nothing was modelled (`foobar`, `c()`, `var(--c)`, a space-separated list, an interpolation body
        // that failed to evaluate): keep the source text, exactly as before this module.
        calc::Reading::Declined => Outcome::Opaque,
        // A plain folded value leaves the shells (`calc(1px * 2)` → `2px`).
        calc::Reading::Value(value) if collapsible(&value) => Outcome::Collapsed(value),
        // Everything else prints inside the shell — a chain that stopped at an incompatible step, a
        // compound unit (`1 / 1px / 1rad`), or a resolved interpolation (`calc(#{1px + 2px})` → `3px`).
        calc::Reading::Value(value) => Outcome::Kept(value.to_css()),
        // A chain that did not collapse out of the shell still keeps the number it reached.
        calc::Reading::Expression(content) => Outcome::Folded(content),
    }
}

/// Whether a value may leave its shells: a **boolean**, or a number whose unit is expressible outside a
/// calculation (see [`Unit::is_plain`]). A `Raw` never qualifies, and that one boundary is what keeps the
/// already-green cases of this cluster green (`calc(foobar)`, `calc(c())`, `calc(var(--c))`,
/// `calc(#{1px + 2px})`, `calc((#{1px + 2px}))`).
fn collapsible(value: &Value) -> bool {
    matches!(value, Value::Bool(..) | Value::Number(..)) && value.is_plain()
}

/// The readable parts of an argument list, joined the way the corpus separates them (`, `).
fn rendered(parts: &[Outcome]) -> String {
    parts
        .iter()
        .map(|part| match part {
            Outcome::Collapsed(value) => value.to_css(),
            Outcome::Nested(inner) | Outcome::Folded(inner) => inner.printed.clone(),
            Outcome::Named(inner) | Outcome::Kept(inner) => inner.clone(),
            Outcome::Opaque => String::new(),
        })
        .collect::<Vec<String>>()
        .join(", ")
}
