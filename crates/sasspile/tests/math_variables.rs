//! The `math` module's own constants — `math.$pi`, `math.$max-number` — pinned on
//! `core_functions/math/variables.hrx`. The inputs and expectations are copied from that file.
//!
//! What this slice reads: a `module.$name` reference is **one lexeme** (`crate::lexer::Tok::MemberVar`),
//! because a word ending in `.` beside a `$name` cannot be re-joined downstream — `math . $pi`, `5.$pi` and
//! `math.$pi` would all look alike to `expr`, and re-scanning the rendered text there is the mistake this
//! project has measured four times.
//!
//! Still open: `math.$pi: 0;` → `Error: Cannot modify built-in variable.`
//! (`…::error/assignment/*`) — an assignment whose target is namespaced is not read as an assignment yet.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// The constants that print without any arithmetic: `math.$max-safe-integer` → `9007199254740991` and
/// `math.$min-safe-integer` → `-9007199254740991` (`variables.hrx::{max_safe_integer,min_safe_integer}`).
/// The file's third such row, `math.$max-number`, is left to the hrx run rather than copied here: its answer
/// is the 309 digits of a maximum double, and retyping a digit string is exactly how this session has twice
/// invented a fixture — the harness compares against the file itself.
#[tokio::test]
async fn the_safe_integer_constants_print_whole() {
    for (input, want) in [
        ("a {b: math.$max-safe-integer}", "9007199254740991"),
        ("a {b: math.$min-safe-integer}", "-9007199254740991"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// A constant multiplied by an **exponent literal** is arithmetic, not interpolation:
/// `math.$pi * 1e15` → `3141592653589793`, `math.$e * 1e15` → `2718281828459045`,
/// `math.$epsilon * 1e31` → `2220446049250313`,
/// `math.$min-number * 1e300 * 1e39` → `4940656458412465` (`variables.hrx::{pi,e,epsilon,min_number}`).
/// Two laws meet in these rows: `1e15` is a number rather than "1 with the unit `e15`"
/// (`operations::number_lexeme`), and a run holding a module constant reaches
/// [`crate::operations::arithmetic`] before the interpolation branch — which is also what lets
/// `math.tan(0.5rad * math.$pi)` → `16331239353195370` fold its operand
/// (`core_functions/math/tan.hrx::{asymptote,negative_asymptote}/radian`).
/// `calc(1e2px)` → `100px` is the same lexeme rule seen from `calc`
/// (`values/calculation/calc/no_operator::number/exponent`).
#[tokio::test]
async fn a_constant_times_an_exponent_literal_folds() {
    for (input, want) in [
        ("a {b: math.$pi * 1e15}", "3141592653589793"),
        ("a {b: math.$e * 1e15}", "2718281828459045"),
        ("a {b: math.$epsilon * 1e31}", "2220446049250313"),
        ("a {b: math.$min-number * 1e300 * 1e39}", "4940656458412465"),
        ("a {b: calc(1e2px)}", "100px"),
        ("a {b: math.tan(0.5rad * math.$pi)}", "16331239353195370"),
        ("a {b: math.tan(-0.5rad * math.$pi)}", "-16331239353195370"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// A namespaced name with no built-in member — from a file-loaded module (`upstream.$a`, P5) or simply
/// invented — is refused quietly: the run keeps its text, because `Undefined variable.` is not what the
/// corpus states for it and answering `0` would be a value nobody asked for.
#[tokio::test]
async fn a_name_with_no_built_in_member_keeps_its_text() {
    for call in ["math.$nope", "upstream.$a", "utils.$empty-map"] {
        let input = format!("a {{b: {call}}}");
        assert_eq!(
            css(&input).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "an unresolvable module member must survive verbatim"
        );
    }
}
