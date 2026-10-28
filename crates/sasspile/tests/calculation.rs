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
/// `-1px` → `-1px`), `.2px` prints in Sass's form (`0.2px`), and **exponential notation is a number**:
/// `calc(1e2px)` → `100px` (`values/calculation/calc/no_operator::number/exponent`). This line used to pin
/// the verbatim text while `number_lexeme` read `1e2px` as "1 with unit `e2px`"; the comment said it must
/// move to `100px` once that was fixed, and the exponent tail is now part of the lexeme.
#[tokio::test]
async fn numbers_print_in_sass_form_after_collapsing() {
    assert_eq!(value("a {b: calc(+1px)}").await, "1px");
    assert_eq!(value("a {b: calc(-1px)}").await, "-1px");
    assert_eq!(value("a {b: calc(1e2px)}").await, "100px");
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

/// What 1.4d *grants*: a chain folds while every step is computable. Each expectation is quoted from a
/// fixture, not derived — `calc/operator/times::simplified` (`1px * 2` → `2px`), `…/times::no_whitespace`
/// (`1px*2` → `2px`), `…/minus::simplified` (`1px - 2px` → `-1px`), and the four
/// `…/precedence::simplified/*` rows that settle associativity and that `*` binds tighter than `+`.
#[tokio::test]
async fn chains_fold_while_every_step_is_computable() {
    for (input, want) in [
        ("a {b: calc(1px + 2px)}", "3px"),
        ("a {b: calc(1px - 2px)}", "-1px"),
        ("a {b: calc(1px * 2)}", "2px"),
        ("a {b: calc(1px*2)}", "2px"),
        (
            "a {b: calc(1px + 20px - 300px + 4000px - 50000px)}",
            "-46279px",
        ),
        ("a {b: calc(2 * 3 / 5 * 7 / 11)}", "0.7636363636"),
        ("a {b: calc(2 * 3 + 4 / 5 - 6)}", "0.8"),
        ("a {b: calc(2 * (3 + 4) / (5 - 6))}", "-14"),
        ("a {b: calc(1 / (2 * 3))}", "0.1666666667"),
        // An incompatible step stops only itself (`operator/minus::preserved/number`,
        // `operator/units::percent/and_known`).
        ("a {b: calc(1px + 1%)}", "calc(1px + 1%)"),
        ("a {b: calc(1px - 2%)}", "calc(1px - 2%)"),
        ("a {b: calc(1px + 2%)}", "calc(1px + 2%)"),
        // The left-associative reading the corpus states: `(1% + 1px) + 2px` does not fold
        // (`calc/simplify.hrx`'s own README), and neither does the right-leaning one.
        ("a {b: calc(1% + 1px + 2px)}", "calc(1% + 1px + 2px)"),
        // A compound unit is calculation-only, so the shell stays
        // (`values/numbers/units/multiple::multiple_denominators`).
        ("a {b: calc(1 / 1px / 1rad)}", "calc(1 / 1px / 1rad)"),
        // And a slash in a plain declaration value is still the separator CSS shorthands need.
        ("a {b: 1/2}", "1/2"),
    ] {
        assert_eq!(value(input).await, want, "for {input}");
    }
}

/// Parentheses are re-derived from precedence, never copied: `operator/var` and
/// `operator/precedence::preserved/*` together spell out when a group survives. The two sides of the law
/// are the same expression written with a group and with a nested `calc( … )` — both print identically,
/// which is why the nested form is not a special case here.
#[tokio::test]
async fn parentheses_follow_precedence_not_source() {
    for (input, want) in [
        // Dropped where the algebra allows.
        ("a {b: calc((1 + var(--c)))}", "calc(1 + var(--c))"),
        ("a {b: calc(1px + (2% - 3em))}", "calc(1px + 2% - 3em)"),
        (
            "a {b: calc(1px * (2 / var(--c)))}",
            "calc(1px * 2 / var(--c))",
        ),
        ("a {b: calc(1px + calc(2% - 3em))}", "calc(1px + 2% - 3em)"),
        (
            "a {b: calc(1px * calc(2 / var(--c)))}",
            "calc(1px * 2 / var(--c))",
        ),
        // Kept where dropping them would change the value.
        (
            "a {b: calc(1px / (2 * var(--c)))}",
            "calc(1px / (2 * var(--c)))",
        ),
        (
            "a {b: calc(1px / calc(2 * var(--c)))}",
            "calc(1px / (2 * var(--c)))",
        ),
        (
            "a {b: calc(1px * (2 + var(--c)))}",
            "calc(1px * (2 + var(--c)))",
        ),
        (
            "a {b: calc(1px - (2% + var(--c)))}",
            "calc(1px - (2% + var(--c)))",
        ),
        (
            "a {b: calc(1px - (2% - var(--c)))}",
            "calc(1px - (2% - var(--c)))",
        ),
        // A group around an unmodellable operand keeps its parentheses whatever precedence says.
        ("a {b: calc(1 + (var(--c)))}", "calc(1 + (var(--c)))"),
        ("a {b: calc(1 + calc(var(--c)))}", "calc(1 + (var(--c)))"),
    ] {
        assert_eq!(value(input).await, want, "for {input}");
    }
}

/// What stays **unreadable**, so nothing 1.4a printed correctly moves: a run that does not alternate into
/// operands and operators keeps the source text. `values/calculation/calc/space.hrx::var/before`
/// (`calc(var(--c) 1)` → `calc(var(--c) 1)`) is the fixture that states adjacent operands are a
/// space-separated list rather than a product.
#[tokio::test]
async fn text_this_cannot_model_stays_as_written() {
    assert_eq!(value("a {b: calc(foobar)}").await, "calc(foobar)");
    assert_eq!(value("a {b: calc(var(--c))}").await, "calc(var(--c))");
    assert_eq!(value("a {b: calc(c())}").await, "calc(c())");
    assert_eq!(value("a {b: calc(var(--c) 1)}").await, "calc(var(--c) 1)");
    assert_eq!(value("a {b: calc(1 var(--c))}").await, "calc(1 var(--c))");
    // A leading sign glued to its number is part of the literal (`no_operator::number/leading_plus`).
    assert_eq!(value("a {b: calc(+1px)}").await, "1px");
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
    // A URL's slashes are literal text: `release_slashes` only ever reads a call argument, and a
    // declaration value still goes through the protection rule.
    assert_eq!(value("a {b: url(http://x/y)}").await, "url(http://x/y)");
}

/// The inverse-trig names are calculation shells as well: the corpus's own `simplification` rows keep the
/// callee's spelling, fold what computes inside the argument, and leave the unreadable part of it alone
/// (`values/calculation/{acos,asin,atan,atan2}.hrx::simplification`). `atan2` shows both halves of the law
/// at once — the second argument folds to `-1.75px` while the first keeps `var(--c)` beside its folded
/// `2px`. What is *not* claimed here: the same files' `one`/`zero`/`overridden`/`case_insensitive` rows
/// want the function's **value** (`0deg`, `1`), which needs the builtin registration plus user-function
/// precedence, and stays a separate batch (tasks.md 1.7b).
#[tokio::test]
async fn inverse_trig_shells_fold_what_they_can() {
    for (input, want) in [
        (
            "a { b: acos(3px - 1px + var(--c)); }",
            "acos(2px + var(--c))",
        ),
        (
            "a { b: asin(3px - 1px + var(--c)); }",
            "asin(2px + var(--c))",
        ),
        (
            "a { b: atan(3px - 1px + var(--c)); }",
            "atan(2px + var(--c))",
        ),
        (
            "a { b: atan2(3px - 1px + var(--c), -7px / 4em * 1em); }",
            "atan2(2px + var(--c), -1.75px)",
        ),
    ] {
        assert_eq!(value(input).await, want, "for {input}");
    }
}

/// The same law, one literal pair per file, for the nine remaining number names whose `simplification`
/// row asks for it: `sqrt`/`exp`/`log`/`pow`/`sin`/`cos`/`tan`/`hypot`/`rem`
/// (`values/calculation/<name>.hrx::simplification`, read in this session). `hypot`'s second argument
/// `-7px + 4em` is **not** folded — `em` is context-relative, so `math.compatible(1px, 4em)` is false —
/// which is a different reason from `log`'s untouched `var(--e)`: that argument is not readable as a
/// value. `rem` is in the list only while `a_multi_argument_shell_claims_no_value` holds: with more than
/// one argument a shell claims no value, so `rem`'s own rows keep their text instead of fabricating one.
#[tokio::test]
async fn number_shells_fold_what_their_own_rows_ask() {
    for (input, want) in [
        (
            "a { b: sqrt(1px + 2px - var(--c)) }",
            "sqrt(3px - var(--c))",
        ),
        ("a { b: exp(1px + 2px - var(--c)) }", "exp(3px - var(--c))"),
        (
            "a {b: log(3px - 1px + var(--c), var(--e));}",
            "log(2px + var(--c), var(--e))",
        ),
        (
            "a {b: pow(3px - 1px + var(--c), 4px + 10px)}",
            "pow(2px + var(--c), 14px)",
        ),
        ("a { b: sin(3px - 1px + var(--c)); }", "sin(2px + var(--c))"),
        ("a { b: cos(3px - 1px + var(--c)); }", "cos(2px + var(--c))"),
        ("a { b: tan(3px - 1px + var(--c)); }", "tan(2px + var(--c))"),
        (
            "a { b: rem(3px - 1px + var(--c), -7px / 4em * 1em); }",
            "rem(2px + var(--c), -1.75px)",
        ),
        (
            "a { b: hypot(1px + 2px - var(--c), -7px + 4em) }",
            "hypot(3px - var(--c), -7px + 4em)",
        ),
    ] {
        assert_eq!(value(input).await, want, "for {input}");
    }
}

/// A shell with **more than one argument claims no value**: `min(10, 20)` is not `10`. `calculation::folded`
/// walks the parts with `find_map`, so a multi-argument shell was handed out as its first argument and
/// could then take part in arithmetic — `math.div(1, min(10, 20))` printed `0.1`, `max(10, 20) * 1`
/// printed `10`. I searched the corpus for a row that puts a multi-argument calculation into arithmetic
/// (a shell on the right of `math.div`, and `/ calc(`) and there is none, so nothing pins the fabricated
/// number; the shape this project already uses when it cannot evaluate is the text as written (1.4a's
/// do-not-guess law), which is what these two rows assert. The **one**-argument path is not claimed by a
/// literal of mine either: `calc/operator/units::multiplication` pins only how `calc(2px * 3px)` prints,
/// so the hrx record is the guard there.
#[tokio::test]
async fn a_multi_argument_shell_claims_no_value() {
    assert_eq!(
        value("a {b: math.div(1, min(10, 20))}").await,
        "math.div(1, min(10, 20))"
    );
    assert_eq!(value("a {b: max(10, 20) * 1}").await, "max(10, 20) * 1");
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
