//! `math.pow`, `math.sqrt`, `math.log` and the **non-finite printing form**, pinned on
//! `core_functions/math/pow/*.hrx`, `core_functions/math/{sqrt,log}.hrx` and the `NaN` rows of
//! `{sin,cos,tan,asin,acos,atan,atan2}.hrx`. Every input and expectation is copied from the fixture's own
//! `input.scss` / `output.css` (`Error:` first line for the refusals).
//!
//! The printing law is the one that unlocked 179 rows: a number that is not finite has no plain-CSS spelling,
//! so Sass writes it inside `calc()`, and a unit rides **inside** the wrapper multiplied by one —
//! `calc(NaN)`, `calc(infinity)`, `calc(-infinity)`, `calc(NaN * 1deg)`, `calc(infinity * 1px)`. A finite
//! number keeps the ordinary form, so `math.pow(2, 3)` → `8` and not `calc(8)`.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

async fn failure(input: &str) -> String {
    match compile(input).await {
        Ok(css) => panic!("{input} was expected to fail, but it compiled to:{css}"),
        Err(error) => error.message,
    }
}

/// `math.pow($base: 2, $exponent: 3)` → `8` (`pow/arguments.hrx::named_args`), and the zero/signed cases are
/// IEEE: `pow(0, -2)` → `calc(infinity)` (`pow/base_positive_zero.hrx::with_exponent/negative_even_integer`),
/// `pow(-0.0, -3)` → `calc(-infinity)` while `pow(-0.0, -3.000000000001)` → `calc(infinity)` — the odd
/// **integer** inverts the sign of zero, a non-integer exponent cannot (`pow/base_negative_zero.hrx::with_exponent/{negative_odd_integer,negative_odd_integer_fuzzy}`).
#[tokio::test]
async fn pow_follows_ieee_through_zero_and_the_infinities() {
    for (input, want) in [
        ("a {b: math.pow($base: 2, $exponent: 3)}", "8"),
        ("a {b: math.pow(0, -2)}", "calc(infinity)"),
        ("a {b: math.pow(-0.0, -3)}", "calc(-infinity)"),
        ("a {b: math.pow(-0.0, -3.000000000001)}", "calc(infinity)"),
        ("a {b: math.pow(0, -0.5)}", "calc(infinity)"),
        ("a {b: math.pow(1, math.div(-1, 0))}", "1"),
        (
            "a {b: math.pow(1.000000000001, math.div(1, 0))}",
            "calc(infinity)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `math.sqrt`: `-0.0` keeps its sign (`sqrt.hrx::negative_zero` → `-0`) while a negative **number** is
/// `calc(NaN)` (`…::negative`, and `…::negative_zero_fuzzy` for `-0.000000000001`), and the fuzzy zero is
/// small but real: `math.sqrt(0.000000000001)` → `0.000001` (`…::zero_fuzzy`).
#[tokio::test]
async fn sqrt_keeps_a_signed_zero_and_refuses_a_negative() {
    for (input, want) in [
        ("a {b: math.sqrt(-0.0)}", "-0"),
        ("a {b: math.sqrt(0)}", "0"),
        ("a {b: math.sqrt(-1)}", "calc(NaN)"),
        ("a {b: math.sqrt(-0.000000000001)}", "calc(NaN)"),
        ("a {b: math.sqrt(0.000000000001)}", "0.000001"),
        ("a {b: math.sqrt(2)}", "1.4142135624"),
        ("a {b: math.sqrt($number: 2)}", "1.4142135624"),
        ("a {b: math.sqrt(math.div(1, 0))}", "calc(infinity)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `math.log` is natural unless a base arrives, and a written `null` **is** no base:
/// `math.log(2, null)` → `0.6931471806` equals `math.log(2)` (`log.hrx::base/null`), `math.log(2, 10)` →
/// `0.3010299957` (`…::base/positive`), and the extremes print through `calc`: `math.log(0)` →
/// `calc(-infinity)` (`…::base/zero`), `math.log(-1)` → `calc(NaN)` (`…::base/negative`).
#[tokio::test]
async fn log_takes_an_optional_base_and_prints_its_extremes_in_calc() {
    for (input, want) in [
        ("a {b: math.log(2)}", "0.6931471806"),
        ("a {b: math.log($number: 2)}", "0.6931471806"),
        ("a {b: math.log(2, null)}", "0.6931471806"),
        ("a {b: math.log(2, 10)}", "0.3010299957"),
        ("a {b: math.log($number: 2, $base: 10)}", "0.3010299957"),
        ("a {b: math.log(0)}", "calc(-infinity)"),
        ("a {b: math.log(-1)}", "calc(NaN)"),
        ("a {b: math.log(0.000000000001)}", "-27.6310211159"),
        ("a {b: math.log(math.div(1, 0))}", "calc(infinity)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The refusals name the failing parameter, in the same words the rest of the domain uses
/// (`pow/arguments.hrx::error/{base_type,exponent_type,base_has_units,exponent_has_units,zero_args,one_arg,too_many_args}`,
/// `sqrt.hrx::error/*`, `log.hrx::error/*` — including `$base:` for `math.log`'s optional parameter).
#[tokio::test]
async fn every_argument_is_refused_under_its_own_name() {
    for (input, want) in [
        ("a {b: math.pow(\"0\", 0)}", "$base: \"0\" is not a number."),
        (
            "a {b: math.pow(0, \"0\")}",
            "$exponent: \"0\" is not a number.",
        ),
        (
            "a {b: math.pow(1px, 0)}",
            "$base: Expected 1px to have no units.",
        ),
        (
            "a {b: math.pow(0, 1px)}",
            "$exponent: Expected 1px to have no units.",
        ),
        ("a {b: math.pow()}", "Missing argument $base."),
        ("a {b: math.pow(0)}", "Missing argument $exponent."),
        (
            "a {b: math.pow(0, 0, 0)}",
            "Only 2 arguments allowed, but 3 were passed.",
        ),
        (
            "a {b: math.sqrt(1px)}",
            "$number: Expected 1px to have no units.",
        ),
        ("a {b: math.sqrt(\"0\")}", "$number: \"0\" is not a number."),
        ("a {b: math.sqrt()}", "Missing argument $number."),
        (
            "a {b: math.sqrt(0, 0)}",
            "Only 1 argument allowed, but 2 were passed.",
        ),
        (
            "a {b: math.log(1px)}",
            "$number: Expected 1px to have no units.",
        ),
        (
            "a {b: math.log(1, 1px)}",
            "$base: Expected 1px to have no units.",
        ),
        ("a {b: math.log()}", "Missing argument $number."),
        (
            "a {b: math.log(0, 0, 0)}",
            "Only 2 arguments allowed, but 3 were passed.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// The printing law itself, from the functions that reach it through an argument rather than a domain
/// refusal: `math.div(1, 0)` is an infinity, and `sin`/`cos`/`tan` of one is `calc(NaN)`
/// (`{sin,cos,tan}.hrx::{infinity,negative_infinity}`), while `math.atan2(-1, 0)` keeps a real degree answer
/// (`atan2/y_infinite.hrx`).
#[tokio::test]
async fn a_non_finite_number_prints_inside_calc() {
    for (input, want) in [
        ("a {b: math.sin(math.div(1, 0))}", "calc(NaN)"),
        ("a {b: math.sin(math.div(-1, 0))}", "calc(NaN)"),
        ("a {b: math.cos(math.div(1, 0))}", "calc(NaN)"),
        ("a {b: math.tan(math.div(-1, 0))}", "calc(NaN)"),
        ("a {b: math.atan(math.div(1, 0))}", "90deg"),
        ("a {b: math.atan2(-1, 0)}", "-90deg"),
        ("a {b: math.round(1.4999999999949998)}", "1"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}
