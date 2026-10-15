//! Mixin machinery tests (task 7.1): definitions, positional / named / default /
//! rest arguments, and `@content` — including its lexical scope (content sees the
//! include site's variables, not the mixin's) and nesting inside selectors.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn css(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n")
}

fn eval_err(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    match eval_all(0, "t.scss".into(), src.into(), program) {
        Ok(_) => panic!("expected eval error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn mixin_no_args_inlines_decls() {
    let out = css("@mixin button { color: red; } .a { @include button; }");
    assert_eq!(out, ".a {\n  color: red;\n}", "{out}");
}

#[test]
fn mixin_emits_rule_at_top_level() {
    let out = css("@mixin block { .b { color: red; } } @include block;");
    assert_eq!(out, ".b {\n  color: red;\n}", "{out}");
}

#[test]
fn mixin_default_argument() {
    let out = css("@mixin size($w, $h: $w) { width: $w; height: $h; } .b { @include size(5px); }");
    assert_eq!(out, ".b {\n  width: 5px;\n  height: 5px;\n}", "{out}");
}

#[test]
fn mixin_named_arguments_out_of_order() {
    let out =
        css("@mixin box($w, $h) { width: $w; height: $h; } .b { @include box($h: 3px, $w: 1px); }");
    assert_eq!(out, ".b {\n  width: 1px;\n  height: 3px;\n}", "{out}");
}

#[test]
fn mixin_content_folds_into_parent() {
    let out = css("@mixin media { @content; } .x { @include media { color: blue; } }");
    assert_eq!(out, ".x {\n  color: blue;\n}", "{out}");
}

#[test]
fn mixin_content_inside_selector() {
    let out = css("@mixin wrap { &-inner { @content; } } .btn { @include wrap { color: red; } }");
    assert_eq!(out, ".btn-inner {\n  color: red;\n}", "{out}");
}

#[test]
fn mixin_rest_arguments() {
    let out = css("@mixin j($a, $rest...) { p: $a; q: $rest; } .x { @include j(1, 2, 3); }");
    assert!(out.contains("p: 1;"), "{out}");
    assert!(out.contains("2") && out.contains("3"), "{out}");
}

#[test]
fn mixin_content_sees_include_site_vars() {
    // `@content` runs in the include site's lexical scope: `$ctx` is defined at
    // the call, the mixin's own `$local` must not leak into the content block.
    let out =
        css("@mixin m2 { $local: fromMixin; @content; } .r { $ctx: 5; @include m2 { y: $ctx; } }");
    assert_eq!(out, ".r {\n  y: 5;\n}", "{out}");
}

#[test]
fn undefined_mixin_errors() {
    let msg = eval_err(".a { @include nope; }");
    assert!(msg.to_lowercase().contains("undefined mixin"), "{msg}");
}

#[test]
fn missing_argument_errors() {
    let msg = eval_err("@mixin two($a, $b) { x: $a; } .a { @include two(1); }");
    assert!(msg.contains("Missing argument"), "{msg}");
}
