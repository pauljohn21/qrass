//! `math.clamp` and `math.hypot`, pinned on `core_functions/math/{clamp,hypot}.hrx`. Every input and
//! expectation is copied from the fixture's own `input.scss` / `output.css` (`Error:` first line below).
//!
//! The two share one thing beyond the unit law: their refusals name **both** parameters, in the order the
//! comparison reaches them — `min`/`max` (`math.max.hrx`) name neither, so this family's wording could not be
//! inferred from the functions already registered.

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

/// `math.clamp` folds inward from the maximum and then out from the minimum, and **the winner keeps its own
/// unit**: `clamp(180deg, 0.75turn, 360deg)` → `0.75turn`, `clamp(180deg, 1turn, 360deg)` → `360deg`,
/// `clamp(180deg, 0.5turn, 360deg)` → `180deg` (`clamp.hrx::preserves_units/{number,max,min}` — the two ties
/// belong to the bounds, which is what makes the fold `<` then `<=`). The rows where the bounds disagree show
/// the order too: `clamp(1, 2, 0)` → `1` and `clamp(1, 2, 1)` → `1`
/// (`…::{min_greater_than_max,min_equals_max}`).
#[tokio::test]
async fn clamp_folds_to_the_bounds_and_keeps_the_writing() {
    for (input, want) in [
        ("a {b: math.clamp(1, 0, 2)}", "1"),
        ("a {b: math.clamp(0, 1, 2)}", "1"),
        ("a {b: math.clamp(0, 2, 1)}", "1"),
        ("a {b: math.clamp(1, 2, 1)}", "1"),
        ("a {b: math.clamp(1, 2, 0)}", "1"),
        ("a {b: math.clamp(180deg, 0.5turn, 360deg)}", "180deg"),
        ("a {b: math.clamp(180deg, 0.75turn, 360deg)}", "0.75turn"),
        ("a {b: math.clamp(180deg, 1turn, 360deg)}", "360deg"),
        ("a {b: math.clamp($min: 0, $number: 1, $max: 2)}", "1"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `math.hypot` squares in a common scale and returns the root in the **first** argument's unit:
/// `hypot(3, 4, 5, 6, 7)` → `11.6189500386`, and the same digits from centimetres, millimetres, **quarters**,
/// inches and pixels (`hypot.hrx::units`) — the row that moved the length factors out of `math.rs` into the
/// one table in `crate::unit`, because `q` lives only there. An infinite argument carries the whole root away
/// (`…::infinite/*` → `calc(infinity)`).
#[tokio::test]
async fn hypot_scales_every_argument_and_answers_in_the_first_unit() {
    for (input, want) in [
        ("a {b: math.hypot(3, 4, 5, 6, 7)}", "11.6189500386"),
        (
            "a {b: math.hypot(3cm, 4mm * 10, 5q * 40, math.div(6in, 2.54), 7px * math.div(96, 2.54))}",
            "11.6189500386cm",
        ),
        ("a {b: math.hypot(math.div(1, 0), 1, 1)}", "calc(infinity)"),
        ("a {b: math.hypot(1, math.div(1, 0), 1)}", "calc(infinity)"),
        ("a {b: math.hypot(1, 1, math.div(1, 0))}", "calc(infinity)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The refusals, with both parameter names — `clamp` names the pair it reached
/// (`$number: 1px and $min: 1deg have incompatible units.` for its first comparison,
/// `$max: 1px and $min: 1deg …` when only the bounds disagree,
/// `clamp.hrx::error/incompatible_units/*`), and `hypot` counts its arguments from one
/// (`$numbers[3]: 2 and $numbers[1]: 0px have incompatible units (one has units and the other doesn't).`,
/// `hypot.hrx::error/some_unitless/*`). `hypot`'s type message names nothing, like `min`/`max`'s.
#[tokio::test]
async fn a_refusal_names_the_pair_it_compared() {
    for (input, want) in [
        (
            "a {b: math.clamp(\"0\", 1, 2)}",
            "$min: \"0\" is not a number.",
        ),
        (
            "a {b: math.clamp(1, \"0\", 2)}",
            "$number: \"0\" is not a number.",
        ),
        (
            "a {b: math.clamp(1, 2, \"0\")}",
            "$max: \"0\" is not a number.",
        ),
        (
            "a {b: math.clamp(1deg, 1px, 1turn)}",
            "$number: 1px and $min: 1deg have incompatible units.",
        ),
        (
            "a {b: math.clamp(1deg, 1turn, 1px)}",
            "$max: 1px and $min: 1deg have incompatible units.",
        ),
        (
            "a {b: math.clamp(1turn, 1deg, 1px)}",
            "$max: 1px and $min: 1turn have incompatible units.",
        ),
        (
            "a {b: math.clamp(0, 1px, 2px)}",
            "$number: 1px and $min: 0 have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.clamp(0px, 1, 2)}",
            "$number: 1 and $min: 0px have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.clamp(0px, 1px, 2)}",
            "$max: 2 and $min: 0px have incompatible units (one has units and the other doesn't).",
        ),
        ("a {b: math.clamp()}", "Missing argument $min."),
        ("a {b: math.clamp(0)}", "Missing argument $number."),
        ("a {b: math.clamp(0, 0)}", "Missing argument $max."),
        (
            "a {b: math.clamp(0, 0, 0, 0)}",
            "Only 3 arguments allowed, but 4 were passed.",
        ),
        (
            "a {b: math.hypot(1deg, 1px, 1turn)}",
            "$numbers[2]: 1px and $numbers[1]: 1deg have incompatible units.",
        ),
        (
            "a {b: math.hypot(1turn, 1deg, 1px)}",
            "$numbers[3]: 1px and $numbers[1]: 1turn have incompatible units.",
        ),
        (
            "a {b: math.hypot(0px, 1, 2px)}",
            "$numbers[2]: 1 and $numbers[1]: 0px have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.hypot(0, 1px, 2)}",
            "$numbers[2]: 1px and $numbers[1]: 0 have incompatible units (one has units and the other doesn't).",
        ),
        (
            "a {b: math.hypot(\"0\", 1px, 1px)}",
            "\"0\" is not a number.",
        ),
        (
            "a {b: math.hypot()}",
            "At least one argument must be passed.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}
