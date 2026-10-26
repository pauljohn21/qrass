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
/// `directives/use/error/member/missing` and `core_functions/meta/call.hrx::error/if_args` both state
/// the oracle header `Error: Undefined variable.`
///
/// The header carries no variable name: in the corpus the name appears only in the source span drawn
/// under the header (`css/comment.hrx::error/loud/interpolation/failure`), which P7 7.2 emits.
#[tokio::test]
async fn unbound_variable_travels_the_err_channel() {
    let err = compile("a { b: $undefined }")
        .await
        .expect_err("an unbound reference must fail the compile");
    assert_eq!(err.kind, sasspile::ErrorKind::UndefinedVariable);
    assert_eq!(err.message, "Undefined variable.");
}

/// The third guard flipping `FATAL` needed: a declaration inside a **plain-CSS at-rule body** is CSS
/// text, so an unbound `$x` must not abort — while the same body still resolves `#{ … }`, and an
/// ordinary style rule still fails on the same name. Every shape is a fixture of `css/function.hrx`.
#[tokio::test]
async fn plain_css_at_rule_body_keeps_an_unbound_variable_as_text() {
    // `lowercase/`, `uppercase/result/sass_script`, `result/uppercase/sass_script`
    assert_eq!(
        compile("@function --a() {\n  result: $b;\n}")
            .await
            .expect("plain-CSS body is text"),
        "@function --a() {\n  result: $b;\n}\n"
    );
    assert_eq!(
        compile("@FUNCTION --a() {\n  result: $b;\n}")
            .await
            .expect("the at-rule name is case-insensitive"),
        "@FUNCTION --a() {\n  result: $b;\n}\n"
    );
    // `lowercase/result/interpolation`: interpolation in that body is still SassScript.
    assert_eq!(
        compile("@function --a() {\n  result: #{1 + 1};\n}")
            .await
            .expect("#{ } evaluates inside a plain-CSS body"),
        "@function --a() {\n  result: 2;\n}\n"
    );
    // The boundary: the identical reference in a style rule is the corpus's fatal case.
    assert_eq!(
        compile("a {\n  result: $b;\n}")
            .await
            .expect_err("a style rule is not plain CSS")
            .message,
        "Undefined variable."
    );
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
    // The name match ignores case: the corpus states the same shape upper-cased
    // (`css/functions/special/prefixed/uppercase::expression/script_like`, `-C-EXPRESSION($d)`), which
    // must not fail either. Sass also lower-cases the printed name, which we do not do yet — the
    // assertion is that no error is raised, not the exact text.
    compile("a {b: -C-EXPRESSION($d)}")
        .await
        .expect("a prefixed special call is plain CSS whatever its case");
}
