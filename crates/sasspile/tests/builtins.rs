//! The builtin call spine (tasks 1.7a), pinned on `core_functions/math/div.hrx` goldens: argument
//! splitting, `$label:` binding, the division unit law, and the two signature-generic arity messages.
//! The last test is the boundary — a call we have no signature for, or a run whose value this compiler
//! cannot model yet, must keep its verbatim text rather than be half-evaluated.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `unitless/{whole,fraction}` + `unit/{numerator,same}`: same unit cancels, a unitless divisor keeps the
/// numerator's unit, and a lone number argument is still an expression inside a call.
#[tokio::test]
async fn div_evaluates_number_arguments() {
    assert_eq!(css("a {b: math.div(6, 3)}").await, "a {\n  b: 2;\n}\n");
    assert_eq!(css("a {b: math.div(6, 5)}").await, "a {\n  b: 1.2;\n}\n");
    assert_eq!(css("a {b: math.div(6px, 3)}").await, "a {\n  b: 2px;\n}\n");
    assert_eq!(css("a {b: math.div(6px, 3px)}").await, "a {\n  b: 2;\n}\n");
}

/// `unit/compatible`: convertible units are converted before cancelling, and `1in = 96px` is the only
/// factor the corpus evidences (`math.div(6in, 3px)` → `192`).
#[tokio::test]
async fn div_converts_lengths_it_can_prove() {
    assert_eq!(
        css("a {b: math.div(6in, 3px)}").await,
        "a {\n  b: 192;\n}\n"
    );
}

/// `unitless/named`: a `$label:` argument binds by parameter name, wherever it is written — and the
/// label itself is never looked up as a variable (the 1.2b refusal, now resolved through a signature).
#[tokio::test]
async fn named_arguments_bind_by_label() {
    assert_eq!(
        css("a {b: math.div($number2: 3, $number1: 6)}").await,
        "a {\n  b: 2;\n}\n"
    );
    assert_eq!(
        css("a {b: math.div(6, $number2: 3)}").await,
        "a {\n  b: 2;\n}\n"
    );
}

/// `error/{too_few_args,too_many_args}`: the messages come from the signature, and the harness compares
/// the oracle's `Error:` header, so these must match word for word — including the English plural, which
/// the corpus shows declining on both counts independently.
#[tokio::test]
async fn arity_diagnostics_are_the_oracle_words() {
    for (input, want) in [
        ("a {b: math.div(6)}", "Missing argument $number2."),
        (
            "a {b: math.div(6, 3, 1)}",
            "Only 2 arguments allowed, but 3 were passed.",
        ),
        (
            "a {b: math.div(6, 3, 1, 2)}",
            "Only 2 arguments allowed, but 4 were passed.",
        ),
    ] {
        let err = compile(input)
            .await
            .expect_err("an arity mistake must terminate the stream as an error");
        assert_eq!(err.message, want, "for input {input}");
    }
}

/// `core_functions/math/{round,abs,ceil,floor}.hrx` — the rounding family the spine makes cheap: one
/// `Builtin` each, same parameter name, unitless results matched exactly. `round` breaks ties away from
/// zero (`-5.6` → `-6`) and a value just under `.5` inside Sass's precision still gives `1`.
#[tokio::test]
async fn rounding_family_matches_goldens() {
    assert_eq!(css("a {b: math.round(2.2)}").await, "a {\n  b: 2;\n}\n");
    assert_eq!(css("a {b: math.round(-5.6)}").await, "a {\n  b: -6;\n}\n");
    assert_eq!(css("a {b: math.round(0.2)}").await, "a {\n  b: 0;\n}\n");
    assert_eq!(
        css("a {b: math.round(1.4999999999949998)}").await,
        "a {\n  b: 1;\n}\n"
    );
    assert_eq!(css("a {b: math.abs(-17)}").await, "a {\n  b: 17;\n}\n");
    assert_eq!(
        css("a {b: math.abs(-123.456)}").await,
        "a {\n  b: 123.456;\n}\n"
    );
    assert_eq!(css("a {b: math.ceil(1.2)}").await, "a {\n  b: 2;\n}\n");
    assert_eq!(css("a {b: math.floor(-1.2)}").await, "a {\n  b: -2;\n}\n");
}

/// The type contract differs **per function**, and the corpus is what says so: `round` wants
/// `$number: c is not a number.` (value rendered in CSS form — a bare identifier unquoted, a string
/// quoted), while `div.hrx::non_numeric/numerator` expects a deprecation **warning** and a real output.
/// So `div` declines on a non-number instead of inventing an error the oracle does not state.
#[tokio::test]
async fn number_parameters_validate_their_own_way() {
    for (input, want) in [
        ("a {b: math.round(c)}", "$number: c is not a number."),
        ("a {b: math.ceil(\"0\")}", "$number: \"0\" is not a number."),
        ("a {b: math.abs()}", "Missing argument $number."),
    ] {
        let err = compile(input)
            .await
            .expect_err("a non-number argument to a number parameter is an error");
        assert_eq!(err.message, want, "for input {input}");
    }
    assert_eq!(
        css("a {b: math.div(c, 3)}").await,
        "a {\n  b: math.div(c, 3);\n}\n"
    );
}

/// The variadic pair. `min`/`max` compare under the unit law and return the **original** argument (never
/// a value rewritten into a common unit): `max(2px, 1)` → `2px` (a unitless argument is comparable),
/// `max(1px, 1in, 1cm)` → `1in` (lengths scaled by the factors the oracle itself states in
/// `libsass/units/conversion/size.hrx`). Their messages differ from a fixed-arity signature's: no
/// `$parameter:` prefix (`c is not a number.`) and `At least one argument must be passed.`.
#[tokio::test]
async fn variadic_extremes_compare_units_and_keep_the_original() {
    assert_eq!(css("a {b: math.max(1, 2)}").await, "a {\n  b: 2;\n}\n");
    assert_eq!(css("a {b: math.max(3, 1, 2)}").await, "a {\n  b: 3;\n}\n");
    assert_eq!(css("a {b: math.max(1)}").await, "a {\n  b: 1;\n}\n");
    assert_eq!(css("a {b: math.max(2px, 1)}").await, "a {\n  b: 2px;\n}\n");
    assert_eq!(
        css("a {b: math.max(1px, 1in, 1cm)}").await,
        "a {\n  b: 1in;\n}\n"
    );
    assert_eq!(
        css("a {b: math.min(1px, 1in, 1cm)}").await,
        "a {\n  b: 1px;\n}\n"
    );
    for (input, want) in [
        ("a {b: math.max()}", "At least one argument must be passed."),
        ("a {b: math.min(1, c)}", "c is not a number."),
        (
            "a {b: math.max(1px, 2s)}",
            "1px and 2s have incompatible units.",
        ),
    ] {
        let err = compile(input)
            .await
            .expect_err("a variadic arity/type/unit mistake is an error");
        assert_eq!(err.message, want, "for input {input}");
    }
}

/// Two more guards on the same code path, both about **not guessing**: a unit outside the shared conversion
/// table is not evidence of a clash (`max.hrx` only contrasts `px` with `s`), and a variadic call with a
/// `$label:` has no parameter to bind to, so it keeps its text.
///
/// The unlisted example used to be `2q`; `hypot.hrx::units` put `q` in the table (it converts against `cm`
/// and `in` there), so it now compares as the absolute length it is — and `rem`/`ch` stand in for what the
/// corpus has not evidenced, since no fixture converts either.
#[tokio::test]
async fn unknown_units_and_labels_on_a_variadic_decline() {
    assert_eq!(
        css("a {b: math.max(1px, 2rem)}").await,
        "a {\n  b: math.max(1px, 2rem);\n}\n"
    );
    assert_eq!(css("a {b: math.max(1px, 2q)}").await, "a {\n  b: 2q;\n}\n");
    assert_eq!(
        css("a {b: math.max($number1: 1, 2)}").await,
        "a {\n  b: math.max($number1: 1, 2);\n}\n"
    );
}

/// The boolean-returning family, which needed `Value::Bool` (and the corpus’s own spelling of a name:
/// the file is `unitless.hrx`, the function is `math.is-unitless`). `math.compatible` answers only for
/// units whose dimension is evidenced — `px` against `em` is `false` (`comparable.hrx::unit/to_different`),
/// `px` against `q` is `true` now that `hypot.hrx::units` converts `q`, and a unit no fixture converts
/// (`ch`) declines to verbatim rather than guessing.
#[tokio::test]
async fn booleans_and_unit_strings() {
    assert_eq!(
        css("a {b: math.is-unitless(1px)}").await,
        "a {\n  b: false;\n}\n"
    );
    assert_eq!(css("a {b: math.unit(1px)}").await, "a {\n  b: \"px\";\n}\n");
    assert_eq!(css("a {b: math.unit(1)}").await, "a {\n  b: \"\";\n}\n");
    assert_eq!(
        css("a {b: math.compatible(1px, 2in)}").await,
        "a {\n  b: true;\n}\n"
    );
    assert_eq!(
        css("a {b: math.compatible(1px, 2em)}").await,
        "a {\n  b: false;\n}\n"
    );
    assert_eq!(
        css("a {b: math.compatible(1px, 2q)}").await,
        "a {\n  b: true;\n}\n"
    );
    assert_eq!(
        css("a {b: math.compatible(1px, 2ch)}").await,
        "a {\n  b: math.compatible(1px, 2ch);\n}\n"
    );
}

/// `math.percentage` is where Sass's **ten-significant-digit** output becomes visible: `0.246 × 100` must
/// print `24.6`, not the f64 tail. The clamp in `format_number` applies only when the natural rendering is
/// longer than ten significant digits, so short values keep their text (and no currently-passing case
/// moved when it landed — measured `regress=0`). A unit on the argument is the corpus’s message, plural
/// “units” even for one, exactly as the fixture writes it.
#[tokio::test]
async fn percentage_scales_and_respects_precision() {
    for (input, want) in [
        ("a {b: math.percentage(42)}", "4200%"),
        ("a {b: math.percentage(0.246)}", "24.6%"),
        ("a {b: math.percentage(123.456)}", "12345.6%"),
        ("a {b: math.percentage(-0.4)}", "-40%"),
        ("a {b: math.percentage(0)}", "0%"),
        ("a {b: math.percentage($number: 1)}", "100%"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
    let err = compile("a {b: math.percentage(1%)}")
        .await
        .expect_err("percentage requires a unitless argument");
    assert_eq!(err.message, "$number: Expected 1% to have no units.");
}

/// A builtin's result is a value, so it also has to be *spelled* the way Sass spells it:
/// `values/calculation/calc/constant::infinity/math/unsimplified/from_variable` binds
/// `$var: math.div(1, 0)` and prints `calc(infinity * (1% + 1px))` — Rust's `inf` would not read as CSS.
/// `NaN` already agrees, which is why the sibling `nan` fixture passes without a special case.
#[tokio::test]
async fn infinite_results_use_sass_spelling() {
    assert_eq!(
        css("$var: math.div(1, 0);\na {b: calc($var * (1% + 1px))}").await,
        "a {\n  b: calc(infinity * (1% + 1px));\n}\n"
    );
}

/// The boundary, in both directions. A callee with no signature (`color.contrast`, `map.merge`) falls back to
/// its source text — no invented number, no invented error — and so does a result whose unit is **compound**,
/// because a compound unit is calculation-only (`Unit::is_plain`; the corpus never prints one alone, and the
/// only file that states the rejection is a commented-out `libsass-closed-issues` line).
///
/// `color.hwb` was this test's example until `hwb` gained a signature; using a now-implemented name to prove
/// a fall-back would only record the old state, so the example moved to `color.contrast`, which is still
/// unregistered. What `hwb` produces instead is pinned by `tests/hwb.rs`, per fixture row.
///
/// What *does* evaluate now is a run that merely contains a call: `math.div(…) * 1px` is an operation, and
/// the fixtures use exactly that shape — `core_functions/math/div.hrx::unit/denominator`
/// (`math.div(6, 3px) * 1px` → `2`), `…::unit/incompatible` (`math.div(6in, 3s) * 1s` → `2in`) and
/// `…::unit/unknown` (`math.div(6c, 3d) * 1d` → `2c`).
#[tokio::test]
async fn what_the_spine_does_not_know_stays_verbatim() {
    assert_eq!(
        css("a {b: color.contrast(red, blue)}").await,
        "a {\n  b: color.contrast(red, blue);\n}\n"
    );
    assert_eq!(
        css("a {b: math.div(6in, 3s)}").await,
        "a {\n  b: math.div(6in, 3s);\n}\n"
    );
    for (input, want) in [
        ("a {b: math.div(6, 3) * 1px}", "2px"),
        ("a {b: math.div(6, 3px) * 1px}", "2"),
        ("a {b: math.div(6in, 3s) * 1s}", "2in"),
        ("a {b: math.div(6c, 3d) * 1d}", "2c"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}
