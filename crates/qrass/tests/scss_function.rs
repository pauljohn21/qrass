//! User-defined function tests (task 7.2): `@function`/`@return`, argument
//! binding, recursion, default-`null` when no `@return`, and lexical capture (a
//! function reads its definition-time global, never the call-site's locals).

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
fn function_returns_value() {
    let out = css("@function twice($n) { @return $n * 2; } .a { width: twice(5px); }");
    assert_eq!(out, ".a {\n  width: 10px;\n}", "{out}");
}

#[test]
fn function_adds_arguments() {
    let out = css("@function add($a, $b) { @return $a + $b; } .a { p: add(2, 3); }");
    assert!(out.contains("p: 5;"), "{out}");
}

#[test]
fn function_without_return_is_null() {
    // A missing `@return` yields null, so the declaration is dropped entirely.
    let out = css("@function nada() { $x: 1; } .a { width: nada(); }");
    assert!(!out.contains("width"), "{out}");
}

#[test]
fn function_recursion() {
    let out = css(
        "@function fact($n) { @if $n <= 1 { @return 1; } @return $n * fact($n - 1); } .a { p: fact(5); }",
    );
    assert!(out.contains("p: 120;"), "{out}");
}

#[test]
fn function_lexical_capture_ignores_call_site_local() {
    // `use_g` reads the global `$g` (10); the `$g: 99` at the include site inside
    // `.a` must NOT be visible to the function body.
    let out = css("$g: 10; @function use_g() { @return $g; } .a { $g: 99; p: use_g(); }");
    assert!(out.contains("p: 10;"), "{out}");
}

#[test]
fn function_return_inside_each_short_circuits() {
    let out = css(
        "@function first($list) { @each $x in $list { @return $x; } @return null; } .a { p: first(red blue); }",
    );
    assert!(out.contains("p: red;"), "{out}");
}

#[test]
fn unknown_bare_function_passes_through_namespaced_errors() {
    // A bare unknown call is emitted verbatim as plain CSS (the goldens leave calls it
    // doesn't recognize to the browser). A *namespaced* unknown still errors.
    let out = css(".a { p: nope(1); }");
    assert!(out.contains("p: nope(1);"), "{out}");
    let msg = eval_err(".a { p: nope.deeper(1); }");
    assert!(msg.to_lowercase().contains("undefined function"), "{msg}");
}
