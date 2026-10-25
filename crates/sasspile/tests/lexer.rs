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
