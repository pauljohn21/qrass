//! `@forward` strict grammar and the `@use`/`@forward` placement rules
//! (task 11.3, round 67): the `directives/forward/error/syntax.hrx`,
//! `directives/use/error/syntax/{after,within}.hrx` and the lone-`$`
//! `css/supports/error` goldens, pinned through the public lexer+parser pair.

use qrass::error::SassError;
use qrass::lexer::lex;
use qrass::parser::parse;

fn parse_err(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    match parse(0, "t.scss".into(), src.to_string(), tokens) {
        Ok(_) => panic!("expected parse error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

fn parses(src: &str) -> Result<Vec<qrass::ast::Stmt>, SassError> {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    parse(0, "t.scss".into(), src.to_string(), tokens)
}

#[test]
fn forward_grammar_errors() {
    assert!(parse_err("@forward;").contains("Expected string."), "url");
    assert!(parse_err("@forward foo;").contains("Expected string."), "unquoted url");
    assert!(
        parse_err("@forward \"a\" as;").contains("Expected identifier."),
        "dangling as"
    );
    assert!(
        parse_err("@forward \"a\" as *;").contains("Expected identifier."),
        "star not a prefix start"
    );
    assert!(
        parse_err("@forward \"a\" as 1-*;").contains("Expected identifier."),
        "number not a prefix start"
    );
    assert!(
        parse_err("@forward \"a\" as foo;").contains("expected \"*\"."),
        "prefix must reach the star"
    );
    assert!(parse_err("@forward \"a\" show;").contains("Expected variable, mixin, or function name"));
    assert!(parse_err("@forward \"a\" show 1;").contains("Expected variable, mixin, or function name"));
    assert!(parse_err("@forward \"a\" show $;").contains("Expected variable, mixin, or function name"));
    assert!(parse_err("@forward \"a\" show b,;").contains("Expected variable, mixin, or function name"));
    assert!(parse_err("@forward \"a\" hide;").contains("Expected variable, mixin, or function name"));
    assert!(parse_err("@forward \"a\" show b hide c;").contains("expected \";\"."));
    assert!(parse_err("@forward \"a\" hide b show c;").contains("expected \";\"."));
}

#[test]
fn forward_grammar_accepts_valid_spellings() {
    parses("@forward \"a\";").expect("plain");
    parses("@forward \"a\" as c-*;").expect("prefix");
    parses("@forward \"a\" show b;").expect("show");
    parses("@forward \"a\" hide b, c;").expect("hide list");
    parses("@forward \"a\" show b with ($v: 1);").expect("show + config");
    // A module phase may start after root-level variable declarations.
    parses("$v: 1; @forward \"a\";").expect("variable before module rules");
}

#[test]
fn module_rules_must_precede_other_rules() {
    assert!(
        parse_err("@a; @forward \"b\";").contains("@forward rules must be written before"),
        "forward after unknown at-rule"
    );
    assert!(
        parse_err("@import \"a\"; @forward \"b\";").contains("@forward rules must be written before"),
        "forward after import"
    );
    assert!(
        parse_err("a {}; @use \"b\";").contains("@use rules must be written before"),
        "use after style rule"
    );
    assert!(
        parse_err("@if true {}; @forward \"b\";").contains("@forward rules must be written before"),
        "forward after @if"
    );
    // the other direction is the module phase working normally
    parses("@use \"a\"; @forward \"b\";").expect("forward after use");
    parses("@forward \"a\"; @use \"b\";").expect("use after forward");
}

#[test]
fn module_rules_reject_nesting() {
    for src in [
        "@mixin a { @forward \"b\"; }",
        "@function a() { @forward \"b\"; }",
        "a { @use \"b\"; }",
        "@media print { @forward \"b\"; }",
    ] {
        assert!(
            parse_err(src).contains("This at-rule is not allowed here."),
            "{src}"
        );
    }
}

#[test]
fn lone_dollar_gets_context_wording() {
    // supports condition: dart keeps demanding the colon
    assert!(parse_err("@supports (a !:$) {b}").contains("expected \":\"."));
    // with-clause head: an identifier is owed
    assert!(
        parse_err("@forward \"o\" with ($ a: b);").contains("Expected identifier."),
        "with empty name"
    );
    // root declaration head `$: 1`
    assert!(parse_err("$: 1;").contains("Expected identifier."));
}

// ─── member-syntax at-rule heads (round 71, use/error/syntax/member.hrx) ─────

#[test]
fn member_syntax_parse_time_errors() {
    assert!(parse_err("a {a: .member()}").contains("Expected digit."));
    assert!(parse_err("a {a: namespace.()}").contains("Expected identifier."));
    assert!(
        parse_err("a {a: namespace._member()}")
            .contains("Private members can't be accessed from outside their modules.")
    );
    assert!(parse_err("a {a: $.member}").contains("Expected identifier."));
    assert!(parse_err("a {a: namespace.$}").contains("Expected identifier."));
    assert!(
        parse_err("a {a: namespace.$_member}")
            .contains("Private members can't be accessed from outside their modules.")
    );
    assert!(
        parse_err("namespace.$member: value !global;")
            .contains("!global isn't allowed for variables in other modules.")
    );
    assert!(parse_err("a {@include .member}").contains("Expected identifier."));
    assert!(parse_err("a {@include namespace.}").contains("Expected identifier."));
    assert!(
        parse_err("a {@include namespace._member}")
            .contains("Private members can't be accessed from outside their modules.")
    );
    assert!(
        parse_err("@function namespace.member() {@return null}").contains("expected \"(\".")
    );
    assert!(parse_err("@mixin namespace.member() {}").contains("expected \"{\"."));
}

#[test]
fn legal_member_spellings_still_parse() {
    parses("a {b: .5em;}").expect("decimal number");
    // namespaced includes and member calls stay legal shapes (they may fail
    // later at load/eval, but not as parse-time syntax errors)
    let _ = parses("@use \"other\"; a {b: other.$x;}");
}
