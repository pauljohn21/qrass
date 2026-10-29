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

/// A **reference** bound is read. `for.hrx::to_scope` writes `@for $i from 1 through $limit` and states in
/// its own comment that re-binding `$limit` inside the body does not move the end of the walk, so the range
/// is decided once at the header. The expansion this produces inside a rule is already pinned by value for
/// the literal spelling (`…::inclusive_forward/scss`), so what is asserted here is the **equality of the two
/// spellings** rather than a literal this compiler invented for a shape the fixture writes differently.
#[tokio::test]
async fn a_reference_bound_reads_the_same_range_as_the_literal_it_holds() {
    let literal = css("a { @for $i from 1 through 3 { b: $i; } }").await;
    let reference = css("$limit: 3;\na { @for $i from 1 through $limit { b: $i; } }").await;
    assert_eq!(literal, reference);
}

/// A body that holds a block settles **one rule per iteration**: `directives/for/for.hrx::to_scope`.
/// Both the input and the expected CSS below are copied verbatim from that archive section, including the
/// `through $limit` bound and the body's own `$limit: 2` — the case states in its comment that the walk is
/// decided once at the header, yet four iterations is also the only in-scope evidence that a reference bound
/// is read at all.
///
/// Blank lines are dropped on both sides before comparing. That is the conformance harness's own basis
/// (`tests/hrx.rs`'s `norm`, which does not judge a rule's text on blank lines); the empty line the fixture
/// prints *between* rules is a formatting law this compiler does not emit yet and is not asserted here.
#[tokio::test]
async fn a_body_block_settles_one_rule_per_iteration() {
    let unrolled = |text: &str| {
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let got = css(
        "$limit: 4;\n@for $i from 1 through $limit {\n  $limit: 2;\n  a {\n    limit: $limit;\n    i: $i;\n  }\n}",
    )
    .await;
    let want = "a {\n  limit: 2;\n  i: 1;\n}\n\na {\n  limit: 2;\n  i: 2;\n}\n\na {\n  limit: 2;\n  i: 3;\n}\n\na {\n  limit: 2;\n  i: 4;\n}\n";
    assert_eq!(unrolled(&got), unrolled(want));
}

/// The boundary the reader still will not cross: a bound that is **not an integer** leaves the header
/// unrecognised, so no range is invented. `1.5` is `error/from_float`'s case, where the corpus wants
/// `Error: 1.5 is not an int.` — emitting that message is P7 work, and until then the program keeps its
/// pre-loop behaviour rather than guessing a range or guessing a number out of a fractional bound.
#[tokio::test]
async fn a_fractional_bound_is_not_read_as_a_range() {
    let float = compile("a { @for $i from 1.5 through 2 { b: $i; } }")
        .await
        .expect_err("a float bound is not read as an integer range");
    assert_eq!(float.message, "Undefined variable.");
}
