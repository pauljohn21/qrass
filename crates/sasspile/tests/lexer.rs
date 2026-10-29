//! Lexer goldens for the comment rules P1 establishes: a `// …` silent comment is **data**
//! (`Tok::Silent`) that each context keeps or drops, and a URL authority must never read as
//! a comment. Everything is observed through the sole `compile` exit.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .expect("silent-comment lexing compiles")
}

#[tokio::test]
async fn silent_comment_is_dropped_from_an_ordinary_value() {
    assert_eq!(css("a { b: c // note\n}").await, "a {\n  b: c;\n}\n");
    assert_eq!(css("a { b: c; // note\n}").await, "a {\n  b: c;\n}\n");
}

#[tokio::test]
async fn comment_line_before_a_rule_is_dropped_from_the_selector() {
    assert_eq!(css("// header\na { b: c; }").await, "a {\n  b: c;\n}\n");
}

#[tokio::test]
async fn custom_property_keeps_the_comment_text_verbatim() {
    // Sass parses a custom-property value as raw CSS text, so `//` is not a comment there
    // and the `;` still terminates the declaration (`sass-spec` oracle:
    // `css/custom_properties/trailing_comment`).
    assert_eq!(
        css("a { --b: c // comment;\n}").await,
        "a {\n  --b: c // comment;\n}\n"
    );
}

#[tokio::test]
async fn url_authority_is_never_a_comment() {
    assert_eq!(
        css("a { background: url(http://example.com/x.png); }").await,
        "a {\n  background: url(http://example.com/x.png);\n}\n"
    );
    assert_eq!(
        css("a { background: url(//cdn.example.com/x.png); }").await,
        "a {\n  background: url(//cdn.example.com/x.png);\n}\n"
    );
}

#[tokio::test]
async fn lone_slash_stays_division_text() {
    assert_eq!(
        css("a { b: 1px / 2px; }").await,
        "a {\n  b: 1px / 2px;\n}\n"
    );
}

/// A `$` inside a quoted string is text, never the start of a variable reference: Sass resolves no
/// variable inside a string literal, and the corpus passes strings like `":c(@#$)"` as ordinary
/// arguments (`core_functions/selector/extend/simple/pseudo/arg::class/equal`). Before the lexer
/// tracked quotes, this shape raised an `Undefined variable` error for a program Sass accepts
/// (measured as 118 in-scope cases).
#[tokio::test]
async fn dollar_inside_a_quoted_string_is_text() {
    let out = compile(r#"a {b: selector.extend(":c(@#$)", ":c(@#$)", "e")}"#)
        .await
        .expect("a quoted `$` must not be looked up");
    assert!(
        out.contains("selector.extend("),
        "the run keeps its text until calls evaluate (P4), got {out}"
    );
    // The other quote form behaves the same, and an escape must not close the string early.
    compile(r"a {b: 'cost: $5'}")
        .await
        .expect("single-quoted $");
    compile(r#"a {b: "escaped \" still one string: $x"}"#)
        .await
        .expect("an escaped quote does not end the literal");
}

/// A closing brace can arrive **without ever having been opened**, because a custom property's value is raw
/// CSS text: `css/custom_properties/error.hrx::brackets/curly` writes `--prop: };` and answers
/// `Error: unmatched "}".`. That error is the pending P7 work, so nothing here pins the message — what is
/// pinned is the thing the brace-counting phase must never do: subtract past zero. The structural phase now
/// carries a depth (it is the phase that can say where a `@for`'s body ends), and with that counter written
/// as `depth -= 1` this exact input panics the run under debug overflow checks — which is why it saturates.
#[tokio::test]
async fn a_closing_brace_that_opened_nothing_does_not_take_the_process_down() {
    let out = compile(".curly {\n  --prop: };\n}")
        .await
        .expect("an unmatched `}` must not panic the delimiter fold");
    // The declaration keeps its raw text and the unmatched brace goes with it. That is this compiler's
    // output, measured for this case as `expected error "unmatched \"}\".", got ".curly {\n  --prop: ;\n}"`;
    // the archive's own answer is the error, which is the P7 work, so nothing here pins it.
    assert!(
        out.contains("--prop"),
        "the custom property kept no text: {out}"
    );
}

/// A program **truncated inside a loop body** must still produce the blocks it reached. Before this phase cut
/// `@for` blocks out, every `{` emitted an `Event::Open` at once, so the enclosing block's pending run could
/// settle when a later block opened; an unclosed cut left sitting in the accumulator would instead lose that
/// rule silently — and a missing rule leaves no trace in a pass count. No in-scope row exercises a truncated
/// loop either: `ledger failures <run> "directives/for/for::" --count` reads **11** in both run
/// `for-fan-out-final` and run `for-structure-cut` — every `@for` row that is red is red for its own reason (a
/// unit-bearing or fractional bound, an error message, `in_declaration`), none of them an unbalanced body.
/// (The broader needle `directives/for` reads 47 because 36 of those rows are `directives/forward`, which has
/// nothing to do with a loop — the count one writes must be the count the needle actually selects.) So the
/// guard has to be written here, by the implementer, rather than inherited from a fixture. The unclosed run is
/// handed on through `structure::as_block`, the same construction a declined header takes: no range is read,
/// no body is run.
#[tokio::test]
async fn a_loop_the_input_never_closes_still_hands_on_the_blocks_it_reached() {
    let out = compile("a { b: 1; @for $i from 1 through 2 { c: $i")
        .await
        .expect("a truncated program must not panic");
    assert!(
        out.contains("b: 1"),
        "the rule the enclosing block had reached was lost: {out}"
    );
}
