//! `@for` over a literal integer range (tasks 3.2, first slice). Each expectation is a section of
//! `sass-spec/directives/for/for.hrx`; the boundary cases are the shapes the reader refuses to guess at,
//! which keep today's behaviour instead of producing a plausible-but-wrong expansion.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `inclusive_forward/scss` + `inclusive_backward` + `exclusive_forward`: `through` keeps its endpoint,
/// `to` drops it, and a descending range walks down. The loop contributes no selector of its own and does
/// not split the rule it sits in — the iterations land in that rule's declaration run.
#[tokio::test]
async fn for_walks_a_literal_range_into_the_enclosing_rule() {
    assert_eq!(
        css("a {\n  @for $i from 1 through 5 {b: $i;}\n}").await,
        "a {\n  b: 1;\n  b: 2;\n  b: 3;\n  b: 4;\n  b: 5;\n}\n"
    );
    assert_eq!(
        css("a {\n  @for $i from 5 through 1 {b: $i;}\n}").await,
        "a {\n  b: 5;\n  b: 4;\n  b: 3;\n  b: 2;\n  b: 1;\n}\n"
    );
    assert_eq!(
        css("a {\n  @for $i from 1 to 5 {b: $i;}\n}").await,
        "a {\n  b: 1;\n  b: 2;\n  b: 3;\n  b: 4;\n}\n"
    );
    // `negative_to_positive` / `negative_to_negative`: a leading minus is part of the bound lexeme.
    assert_eq!(
        css("a { @for $i from -1 through 2 {b: $i;} }").await,
        "a {\n  b: -1;\n  b: 0;\n  b: 1;\n  b: 2;\n}\n"
    );
}

/// `empty/scss`: an empty range contributes nothing, and the surrounding declarations stay in one rule
/// (a loop is not a CSS child block, so it must not flush the pending run around it).
#[tokio::test]
async fn an_empty_range_and_its_neighbours() {
    assert_eq!(
        css("a { b: 0; @for $i from 1 to 1 {b: $i;} d: 9; }").await,
        "a {\n  b: 0;\n  d: 9;\n}\n"
    );
}

/// Two declarations in a body interleave **per iteration** — the order the replay binds the variable in.
#[tokio::test]
async fn body_declarations_keep_their_order_within_each_iteration() {
    assert_eq!(
        css("a { @for $i from 1 to 3 { x: $i; y: $i; } }").await,
        "a {\n  x: 1;\n  y: 1;\n  x: 2;\n  y: 2;\n}\n"
    );
}

/// The boundary the reader will not cross: a bound that is not an integer literal leaves the header
/// unrecognised, so no iteration is invented. `$limit` (a variable) is `to_scope`'s situation; `1.5` is
/// `error/from_float`'s, where the corpus wants `1.5 is not an int.` — until that diagnostic exists the
/// program must keep its pre-loop behaviour rather than guess a range.
#[tokio::test]
async fn a_non_literal_bound_is_not_read_as_a_range() {
    let err = compile("$limit: 3;\na { @for $i from 1 through $limit { b: $i; } }")
        .await
        .expect_err("a variable bound is not evaluated into a range");
    assert_eq!(err.message, "Undefined variable.");
    // A float bound reads the same way: `1.5` is not an integer, so no range is invented and the body is
    // left un-expanded — which surfaces as the unresolved `$i`. The corpus asks for
    // `Error: 1.5 is not an int.` (`for.hrx::error/from_float`); emitting that message is P7 work, and
    // until then this assertion documents that the loop does **not** guess a range from it.
    let float = compile("a { @for $i from 1.5 through 2 { b: $i; } }")
        .await
        .expect_err("a float bound is not read as an integer range");
    assert_eq!(float.message, "Undefined variable.");
}
