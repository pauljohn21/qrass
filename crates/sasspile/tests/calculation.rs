//! Calculation, the first slice of tasks 1.4 (`crate::calculation`). Every expectation below is a
//! section of `sass-spec/values/calculation/calc/no_operator.hrx` (with `calc/parens.hrx` for the
//! parenthesised shapes), quoted verbatim from the fixture, not inferred.
//!
//! The second half of this file is the important half: it pins the shapes the slice **refuses**,
//! because 1.4a forbids guessing them — same-unit simplification belongs to 1.4d, evaluating
//! `min`/`max`/`clamp` and user functions belongs to a later batch, and five cases of this very
//! cluster already passed before the rule existed (`ledger passed bool-units-746
//! "values/calculation/calc/no_operator"`), so anything that rewrote them would be a regression.

use sasspile::compile;

async fn value(input: &str) -> String {
    let css = compile(input).await.unwrap_or_else(|e| {
        panic!("calculation compiles: {input} failed with {}", e.message);
    });
    // Every fixture here is one block with one declaration; return just its value, whatever the
    // property is (`b`, `c`, `font` — the last one is a shorthand the protection rule guards).
    css.lines()
        .nth(1)
        .unwrap_or_default()
        .trim()
        .trim_end_matches(';')
        .split_once(':')
        .unwrap_or_else(|| panic!("unexpected output {css:?}"))
        .1
        .trim()
        .to_string()
}

/// `::case_insensitive`, `::extra_whitespace`, `syntax/extra_whitespace/number`, `::number/integer` —
/// a single value leaves the calc, whatever the name's case or the padding around it.
#[tokio::test]
async fn a_single_value_leaves_its_calc() {
    assert_eq!(value("a {b: CaLc(1px)}").await, "1px");
    assert_eq!(value("a {b: calc( 1px )}").await, "1px");
    assert_eq!(value("a {b: calc( 1 )}").await, "1");
    assert_eq!(value("a {b: calc(1px)}").await, "1px");
}

/// `syntax/extra_whitespace/parens` and `calc/parens::number` — redundant parentheses are peeled, then
/// the value leaves: `calc( ( 1 ) )` → `1`, `calc((1px))` → `1px`.
#[tokio::test]
async fn redundant_parentheses_peel_to_the_value() {
    assert_eq!(value("a {b: calc( ( 1 ) )}").await, "1");
    assert_eq!(value("a {b: calc((1px))}").await, "1px");
}

/// The number forms of `::number/*`: a leading sign is part of the literal (`+1px` → `1px`,
/// `-1px` → `-1px`), and `.2px` prints in Sass's form (`0.2px`).
///
/// `1e2px` is pinned at its **verbatim** form on purpose: exponential notation is a gap in
/// `expr::number_lexeme` (it reads `1e2px` as the number `1` with unit `e2px`), which belongs to the
/// number-literal work of 1.3, not to this slice. When that is fixed, this line must move to `100px`.
#[tokio::test]
async fn numbers_print_in_sass_form_after_collapsing() {
    assert_eq!(value("a {b: calc(+1px)}").await, "1px");
    assert_eq!(value("a {b: calc(-1px)}").await, "-1px");
    assert_eq!(value("a {b: calc(1e2px)}").await, "1e2px");
    assert_eq!(value("a {b: calc(.2px)}").await, "0.2px");
    assert_eq!(value("a {b: calc(1.2px)}").await, "1.2px");
}

/// `::calculation/calc/simplified` + `::calculation/{clamp,min,max}/preserved` — a nested calculation
/// drops exactly **one** shell, and the inner name is what survives.
#[tokio::test]
async fn a_nested_calculation_drops_one_shell() {
    assert_eq!(value("a {b: calc(calc(1px))}").await, "1px");
    assert_eq!(value("a {b: calc(calc(1px + 1%))}").await, "calc(1px + 1%)");
    assert_eq!(
        value("a {b: calc(clamp(1%, 2px, 3%))}").await,
        "clamp(1%, 2px, 3%)"
    );
    assert_eq!(value("a {b: calc(min(1%, 2px))}").await, "min(1%, 2px)");
    assert_eq!(value("a {b: calc(max(1%, 2px))}").await, "max(1%, 2px)");
    // `calc/parens::calculation` — peeling parentheses and dropping a shell compose.
    assert_eq!(
        value("a {b: calc((calc(1px + 1%)))}").await,
        "calc(1px + 1%)"
    );
}

/// `::variable/calculation` — a bound calculation carries its own body out of the outer shell, so the
/// wrapper does not double up.
#[tokio::test]
async fn a_bound_variable_reads_through_the_environment() {
    assert_eq!(
        value("$a: calc(1px + 1%);\nb {c: calc($a)}").await,
        "calc(1px + 1%)"
    );
    assert_eq!(value("$a: foobar;\nb {c: calc($a)}").await, "calc(foobar)");
}

/// `$a: 1px; b {c: calc($a)}` (`::variable/number/simple_unit`) stays `calc(1px)` — and that is not this
/// slice's rule: a **lone** number in a binding is still `Value::Raw` because `exec::evaluated` filters
/// `Expr::Num` ("a bare number is not an expression yet", P0′), so nothing collapsible reaches the shell.
/// Lifting that filter is a separate decision with its own A/B, and this line is where its effect shows.
#[tokio::test]
async fn a_bound_plain_number_is_not_collapsible_yet() {
    assert_eq!(value("$a: 1px;\nb {c: calc($a)}").await, "calc(1px)");
}

/// The refusal half. `calc/simplify.hrx`'s own README states that only "number + number where both are
/// compatible" simplifies — and 1.4a puts that with `*`/`/` and the rational units of 1.4d, so this
/// slice keeps the expression as written rather than inventing the fold early.
#[tokio::test]
async fn simplification_is_not_guessed() {
    assert_eq!(value("a {b: calc(1px + 2px)}").await, "calc(1px + 2px)");
    assert_eq!(value("a {b: calc(1px + 1%)}").await, "calc(1px + 1%)");
    // A slash inside a calculation stays the calculation's own syntax, not a division, until 1.4d.
    assert_eq!(value("a {b: calc(1px / 2)}").await, "calc(1px / 2)");
    assert_eq!(value("a {b: calc(3px * 2)}").await, "calc(3px * 2)");
}

/// The five cases this cluster already passed before any calculation rule existed — the corpus wants
/// their outer shell, so a rule that "simplified" them away would be a silent regression. Each
/// expectation is the fixture's own `output.css`.
#[tokio::test]
async fn what_already_passed_still_passes() {
    // ::function/css — an unknown callee is not evidence of a value.
    assert_eq!(value("a {b: calc(c())}").await, "calc(c())");
    // ::var/bare and calc/parens::var/direct — a CSS `var()` keeps its shell, padding normalised.
    assert_eq!(value("a {b: calc(var(--c))}").await, "calc(var(--c))");
    assert_eq!(value("a {b: calc((var(--c)))}").await, "calc((var(--c)))");
    // ::variable/unquoted_string — a bound identifier is content, not a value that leaves a shell.
    assert_eq!(value("$a: foobar;\nb {c: calc($a)}").await, "calc(foobar)");
    // ::interpolation/number — interpolation inside a calc is an unquoted string (fixture's own comment),
    // so it may be evaluated but never leaves its shell.
    assert_eq!(value("a {b: calc(#{1px + 2px})}").await, "calc(3px)");
    // ::interpolation/parens — and the parentheses around it survive.
    assert_eq!(value("a {b: calc((#{1px + 2px}))}").await, "calc((3px))");
    // calc/parens::identifier — a lone identifier inside redundant parens keeps both.
    assert_eq!(value("a {b: calc((d))}").await, "calc((d))");
}

/// Outside a calculation nothing here changes: `min`/`max`/`clamp` with no registered signature keep
/// their text (1.4a refuses to evaluate them), a vendor-prefixed `-a-calc(` is **not** a calculation
/// (`css/functions/special/prefixed/lowercase::calc/script_like` keeps `-a-calc($c)`), and the CSS
/// shorthands the protection rule exists for are untouched.
#[tokio::test]
async fn outside_a_calculation_nothing_is_newly_read() {
    assert_eq!(value("a {b: min(1px, 2px)}").await, "min(1px, 2px)");
    assert_eq!(value("a {b: -a-calc(1px)}").await, "-a-calc(1px)");
    assert_eq!(
        value("a {font: 12px/1.5 sans-serif}").await,
        "12px/1.5 sans-serif"
    );
    assert_eq!(value("a {b: 1/2}").await, "1/2");
}

/// An unbound `$x` inside a calc must not become a fatal error — the fixture family
/// `::variable/not_parsed_as_interpolation/*` and `css/functions/special_variable` both keep the text,
/// which is the second guard `expr::special_call` records.
#[tokio::test]
async fn an_unbound_variable_inside_a_calc_is_not_fatal() {
    assert_eq!(value("a {b: calc($undefined)}").await, "calc($undefined)");
    assert_eq!(
        value("a {b: calc($undefined + 1px)}").await,
        "calc($undefined + 1px)"
    );
}
