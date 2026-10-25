//! Error-policy contract: which failures travel the real `Err` channel and which are plain CSS.
//!
//! P0′.3a plumbed the channel (`ErrorKind`, `Observer::error` from the `Exec` kernel, a terminal that
//! no longer relabels failures). Flipping `UndefinedVariable` to fatal needed the two corpus guards
//! documented on `FATAL`: a block head resolves only `#{ … }`, and an unbound `$x` inside a special
//! call is literal text. These tests pin that boundary — an unbound reference must fail, the literal
//! shapes must keep compiling. Every assertion cites the fixture that states it: an earlier `progid:`
//! case was dropped because no fixture supported it (`libsass-closed-issues/issue_931.hrx` only shows
//! a *bound* variable resolving inside `#{ … }`).

use sasspile::compile;

/// Corpus-mandated passthrough: an escaped dollar in a selector is text, not a variable reference
/// (`css/selector/escaping::dollar_char`).
#[tokio::test]
async fn escaped_dollar_in_selector_is_not_a_variable() {
    let css = compile(r"a\\$b { c: d }").await.expect("escape compiles");
    assert!(css.contains(r"\$"), "escape must stay, got {css}");
}

/// An unbound variable in an ordinary value must become a real error on the stream's `Err` channel —
/// `directives/use/error/member/missing` expects `Error: Undefined variable.`
///
/// Ignored until plain-CSS at-rule bodies pass through (0′.3b-4): flipping `FATAL` today breaks
/// `css/plain/function.hrx::*\/sass_script`, which must leave `result: $b` untouched inside
/// `@function --a() { … }`. Kept as a test rather than deleted so the target stays executable.
#[tokio::test]
#[ignore = "pending 0'.3b-4: plain-CSS at-rule passthrough"]
async fn unbound_variable_travels_the_err_channel() {
    let err = compile("a { b: $undefined }")
        .await
        .expect_err("an unbound reference must fail the compile");
    assert_eq!(err.kind, sasspile::ErrorKind::UndefinedVariable);
    assert_eq!(err.message, "Undefined variable: \"$undefined\".");
}

/// The other half of the rule: the same unbound name inside a **special** call is plain-CSS text and
/// must not fail the compile (`css/functions/special/prefixed/lowercase::calc/script_like`).
#[tokio::test]
async fn unbound_variable_inside_a_special_call_stays_text() {
    assert_eq!(
        compile("a {b: -a-calc($c)}")
            .await
            .expect("special args are CSS"),
        "a {\n  b: -a-calc($c);\n}\n"
    );
    // …while a bound one still resolves (values/calculation/calc/no_operator::variable/unquoted_string).
    assert_eq!(
        compile("$a: foobar;\nb {c: calc($a)}")
            .await
            .expect("bound var resolves"),
        "b {\n  c: calc(foobar);\n}\n"
    );
}
