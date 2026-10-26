//! A callable declaration is not executed where it is written (tasks 4.0, the P4 prerequisite that
//! 1.2d identified): `@mixin`/`@function` bodies emit nothing and are not evaluated until invoked.
//! Before this, the body's own parameter was looked up in the environment that only a call supplies, so
//! the whole program aborted with an `Undefined variable` that Sass never reports (29 measured cases).

use sasspile::compile;

/// `core_functions/meta/get_mixin/same_module.hrx::dash_insensitive/dash_to_underscore` — the exact
/// fixture input. Sass has no problem with this program; the assertion is that `$v` is not resolved here
/// (and so the compile does not fail). It still lacks `meta.get-mixin`/`@include`, so the *output* is not
/// the fixture's golden CSS yet — that is P4.2/P5.1, not this change.
#[tokio::test]
async fn a_mixin_body_is_not_evaluated_at_its_declaration() {
    let css = compile("@use \"sass:meta\";\n@mixin add_two($v) {b: $v + 2}\n\na {@include meta.apply(meta.get-mixin(add-two), 10)}")
        .await
        .expect("declaring a mixin must not fail the compile");
    assert!(
        !css.contains("$v + 2"),
        "the mixin body must not be emitted as CSS, got {css}"
    );
}

/// A `@function` declaration likewise contributes no rule, and the surrounding rules still come out —
/// the body is skipped, the file is not.
#[tokio::test]
async fn a_function_declaration_emits_no_rule_but_the_file_continues() {
    assert_eq!(
        compile("@function a() { @return 1 }\nb { c: d }")
            .await
            .expect("a function declaration compiles"),
        "b {\n  c: d;\n}\n"
    );
}

/// The boundary that keeps this from deleting working output: `@function --a()` is a **plain-CSS** at-rule
/// (`css/function.hrx::lowercase/result/sass_script`), whose body *is* emitted, un-evaluated. Treating it
/// as a Sass declaration would swallow a rule that passes today.
#[tokio::test]
async fn a_plain_css_function_at_rule_still_emits_its_body() {
    assert_eq!(
        compile("@function --a() {\n  result: $b;\n}")
            .await
            .expect("plain-CSS at-rule body"),
        "@function --a() {\n  result: $b;\n}\n"
    );
}

/// Inference from the two rules above rather than a fixture of its own, so it is pinned deliberately: a
/// `@mixin` written *inside* a style rule is not a CSS child block, so it neither settles nor flushes the
/// enclosing rule — `b: c` and `d: e` stay one rule (an opening *block* is what flushes the pending run,
/// and this opens no block in the output).
#[tokio::test]
async fn a_nested_declaration_does_not_split_the_enclosing_rule() {
    assert_eq!(
        compile("a { b: c; @mixin m { x: $v } d: e }")
            .await
            .expect("a mixin inside a rule compiles"),
        "a {\n  b: c;\n  d: e;\n}\n"
    );
}
