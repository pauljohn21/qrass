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

/// The boundary. A callee with no signature (`color.hwb`, `map.merge`) and a value the unit algebra cannot
/// produce yet (`math.div(6in, 3s)` is a compound unit `in/s`, which the fixture prints as `2in` after
/// multiplying by `1s`) must fall back to their source text: no invented number, no invented error.
#[tokio::test]
async fn what_the_spine_does_not_know_stays_verbatim() {
    assert_eq!(
        css("a {b: color.hwb(180, 50%, 50%)}").await,
        "a {\n  b: color.hwb(180, 50%, 50%);\n}\n"
    );
    assert_eq!(
        css("a {b: math.div(6in, 3s)}").await,
        "a {\n  b: math.div(6in, 3s);\n}\n"
    );
    // A run that merely *contains* a call is not one: the whole value must be the call.
    assert_eq!(
        css("a {b: math.div(6, 3) * 1px}").await,
        "a {\n  b: math.div(6, 3) * 1px;\n}\n"
    );
}
