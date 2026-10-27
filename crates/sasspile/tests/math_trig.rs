//! The trigonometric `math` functions, pinned row by row on
//! `core_functions/math/{sin,cos,tan,asin,acos,atan}.hrx` and `core_functions/math/atan2/*`.
//! Every input and expectation is copied from the fixture's own `input.scss` / `output.css`.
//!
//! Two laws hold this family together, and each is stated by a pair of rows rather than inferred:
//! * a unitless argument to the direct three is **radians** (`math.cos(1)` and `math.cos(1rad)` are the same
//!   number), an angle unit converts (`1grad` is `0.9deg`), and any other unit is refused by name;
//! * the inverse three answer in **degrees**, and a carried `-0.0` keeps its sign (`math.sin(-0.0)` → `-0`,
//!   `math.atan2(-0.0, 0)` → `-0deg`) while an operation that *lands* on zero does not (`math.round(-0.2)` →
//!   `0`, `core_functions/math/round.hrx::up/to_zero`; `hsl(-1 * 0, 100%, 50%)` → `hsl(0, 100%, 50%)`,
//!   `core_functions/color/hsl/four_args/in_gamut.hrx::negative_zero/hue`) — so the distinction lives in the
//!   operation, and this file pins the trigonometric side of it.

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

/// `sin`/`cos`/`tan` take an angle and give back a plain number (`sin.hrx::{deg,grad,rad,turn}`,
/// `cos.hrx::{unitless,rad,deg,grad,turn}`, `tan.hrx::{deg,grad}`). `math.cos(1)` → `0.5403023059` equals
/// `math.cos(1rad)` — the unitless reading is radians, not degrees — and `math.sin(1turn)` → `0`.
#[tokio::test]
async fn an_angle_in_a_number_out() {
    for (input, want) in [
        ("a {b: math.sin(1deg)}", "0.0174524064"),
        ("a {b: math.sin(1grad)}", "0.0157073173"),
        ("a {b: math.sin(1rad)}", "0.8414709848"),
        ("a {b: math.sin(1turn)}", "0"),
        ("a {b: math.cos(1deg)}", "0.9998476952"),
        ("a {b: math.cos(1grad)}", "0.9998766325"),
        ("a {b: math.cos(1rad)}", "0.5403023059"),
        ("a {b: math.cos(1)}", "0.5403023059"),
        ("a {b: math.cos(1turn)}", "1"),
        ("a {b: math.tan(1deg)}", "0.0174550649"),
        ("a {b: math.tan(1grad)}", "0.0157092553"),
        ("a {b: math.sin(0)}", "0"),
        ("a {b: math.tan(0)}", "0"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The three inverse functions require a unitless ratio and answer in degrees: `math.atan(1)` → `45deg`,
/// `math.asin(-0.5)` → `-30deg`, `math.acos(0.5)` → `60deg`, `math.acos(1)` → `0deg`
/// (`{asin,acos,atan}.hrx`). A ratio outside the domain is `NaN` — printed `calc(NaN * 1deg)`, which is the
/// non-finite printing law and not this step's, so those rows (`asin.hrx::greater_than_one`,
/// `…::one_fuzzy`) stay red rather than being answered with a bare `NaNdeg`.
#[tokio::test]
async fn a_ratio_in_a_degree_out() {
    for (input, want) in [
        ("a {b: math.atan(1)}", "45deg"),
        ("a {b: math.atan(-1)}", "-45deg"),
        ("a {b: math.atan(0)}", "0deg"),
        ("a {b: math.asin(0.5)}", "30deg"),
        ("a {b: math.asin(-0.5)}", "-30deg"),
        ("a {b: math.asin(1)}", "90deg"),
        ("a {b: math.acos(0.5)}", "60deg"),
        ("a {b: math.acos(-0.5)}", "120deg"),
        ("a {b: math.acos(1)}", "0deg"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// A carried negative zero keeps its sign through a transcendental: `math.sin(-0.0)` → `-0`,
/// `math.tan(-0.0)` → `-0`, `math.atan(-0.0)` → `-0deg`, `math.asin(-0.0)` → `-0deg`
/// (each file's `negative_zero` row).
///
/// Not asserted here, because the law it needs is not this step's: `math.sin(-0.000000000001)` → `0` and
/// `math.tan(-0.000000000001)` → `0` (`{sin,tan}.hrx::negative_zero_fuzzy`) ask a number **very near an
/// integer** to print as that integer, which is the precision family (`values/numbers/precision/**`), while
/// `math.atan(-0.000000000001)` → `-0.0000000001deg` (`atan.hrx::negative_zero`) shows the same argument is
/// not a zero at all once converted to degrees. Both rows stay red until that law lands.
#[tokio::test]
async fn a_carried_negative_zero_keeps_its_sign() {
    for (input, want) in [
        ("a {b: math.sin(-0.0)}", "-0"),
        ("a {b: math.tan(-0.0)}", "-0"),
        ("a {b: math.atan(-0.0)}", "-0deg"),
        ("a {b: math.asin(-0.0)}", "-0deg"),
        ("a {b: math.atan(-0.000000000001)}", "-0.0000000001deg"),
        ("a {b: math.asin(0.000000000001)}", "0.0000000001deg"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `math.atan2($y, $x)`: `atan2(1, -1)` → `135deg` however the arguments are written
/// (`atan2/arguments.hrx::{named_args,…}`), and two units of one dimension convert first —
/// `atan2(1cm, -10mm)` is `atan2(1, -1)` in length terms (`…::compatible_units`). The sign laws come from
/// IEEE and are exactly what the fixtures state: `atan2(0, -1)` → `180deg` (`atan2/y_zero.hrx`),
/// `atan2(-1, -1)` → `-135deg` and `atan2(1, 1)` → `45deg` (`atan2/y_non_zero_finite.hrx`), and
/// `atan2(-0.0, -1)` → `-180deg` while `atan2(-0.0, 0)` → `-0deg` (`atan2/y_zero.hrx`).
///
/// Two of its rows are **not** asserted, both because a law outside this step is missing: `atan2(1cm,
/// -10mm)` → `135deg` needs the length factors to be exact rather than the corpus's ten-decimal readings
/// (ours lands on `135.0000000003`), and `atan2(1px, 1deg)` → `$x: 1deg and $y: 1px have incompatible units.`
/// needs `deg` in the dimension table at all (`crate::builtins::math::dimension`), which `comparable.hrx`
/// never demanded. Both stay red.
#[tokio::test]
async fn atan2_needs_one_dimension_and_keeps_ieee_signs() {
    for (input, want) in [
        ("a {b: math.atan2($y: 1, $x: -1)}", "135deg"),
        ("a {b: math.atan2(1, 1)}", "45deg"),
        ("a {b: math.atan2(-1, -1)}", "-135deg"),
        ("a {b: math.atan2(-0.0, -1)}", "-180deg"),
        ("a {b: math.atan2(-0.0, 0)}", "-0deg"),
        ("a {b: math.atan2(0, -1)}", "180deg"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The unit refusals, in the corpus's own words. The direct three name the accepted units
/// (`$number: Expected 1px to have an angle unit (deg, grad, rad, turn).`, `cos.hrx::error/unit`),
/// the inverse three want no unit at all (`$number: Expected 1px to have no units.`, `asin.hrx::error/units`),
/// `atan2` distinguishes two dimensions from *one side unitless* and always names `$x` first, and a
/// non-number is reported under its own parameter name (`$y: "0" is not a number.`).
#[tokio::test]
async fn each_family_refuses_a_wrong_unit_by_name() {
    for (input, want) in [
        (
            "a {b: math.cos(1px)}",
            "$number: Expected 1px to have an angle unit (deg, grad, rad, turn).",
        ),
        (
            "a {b: math.sin(1px)}",
            "$number: Expected 1px to have an angle unit (deg, grad, rad, turn).",
        ),
        (
            "a {b: math.asin(1px)}",
            "$number: Expected 1px to have no units.",
        ),
        ("a {b: math.cos(\"0\")}", "$number: \"0\" is not a number."),
        ("a {b: math.cos()}", "Missing argument $number."),
        (
            "a {b: math.cos(0, 0)}",
            "Only 1 argument allowed, but 2 were passed.",
        ),
        (
            "a {b: math.atan2(1, 1px)}",
            "$x: 1px and $y: 1 have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.atan2(1px, 1)}",
            "$x: 1 and $y: 1px have incompatible units (one has units and the other doesn't).",
        ),
        ("a {b: math.atan2(\"0\", 0)}", "$y: \"0\" is not a number."),
        ("a {b: math.atan2(0, \"0\")}", "$x: \"0\" is not a number."),
        ("a {b: math.atan2()}", "Missing argument $y."),
        ("a {b: math.atan2(0)}", "Missing argument $x."),
        (
            "a {b: math.atan2(0, 0, 0)}",
            "Only 2 arguments allowed, but 3 were passed.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}
