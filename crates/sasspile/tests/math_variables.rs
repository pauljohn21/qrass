//! The `math` module's own constants — `math.$pi`, `math.$max-number` — pinned on
//! `core_functions/math/variables.hrx`. The inputs and expectations are copied from that file.
//!
//! What this slice reads: a `module.$name` reference is **one lexeme** (`crate::lexer::Tok::MemberVar`),
//! because a word ending in `.` beside a `$name` cannot be re-joined downstream — `math . $pi`, `5.$pi` and
//! `math.$pi` would all look alike to `expr`, and re-scanning the rendered text there is the mistake this
//! project has measured four times.
//!
//! Still open, in the corpus's own words:
//! * the multiplication rows (`math.$pi * 1e15` → `3141592653589793`, `variables.hrx::pi`) wait on
//!   **exponent literals** — we leave `… * 1e15` unfolded, so the constant prints alone;
//! * `math.$pi: 0;` → `Error: Cannot modify built-in variable.` (`…::error/assignment/*`): an assignment
//!   whose target is namespaced is not read as an assignment at all yet.

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
