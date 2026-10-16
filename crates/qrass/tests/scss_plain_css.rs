//! Plain-CSS content enforcement (the goldens "plain CSS" mode).
//!
//! A `.css` source reached through `@use` is passed through verbatim, but any
//! SCSS-only construct inside it must be rejected the way the golden's CSS parser
//! would. These tests exercise the validator (crates/qrass/src/plain_css.rs)
//! through the real `@use` path against on-disk fixtures in `fixtures/modules`.

use std::path::{Path, PathBuf};

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn modules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modules")
}

/// Compile a virtual entry in the modules dir so a relative `@use "x"` resolves
/// against the on-disk fixtures (`x.scss` / `x.css` / `_x.scss` …).
fn css(src: &str) -> String {
    let name = modules_dir()
        .join("virtual.scss")
        .to_string_lossy()
        .into_owned();
    let tokens = lex(0, name.clone(), src.to_string()).expect("lex ok");
    let program = parse(0, name.clone(), src.to_string(), tokens).expect("parse ok");
    match eval_all(0, name, src.to_string(), program) {
        Ok(chunks) => chunks.join("\n"),
        Err(e) => panic!("eval ok for {src:?}, got {e}"),
    }
}

fn eval_err(src: &str) -> String {
    let name = modules_dir()
        .join("virtual.scss")
        .to_string_lossy()
        .into_owned();
    let tokens = lex(0, name.clone(), src.to_string()).expect("lex ok");
    let program = parse(0, name.clone(), src.to_string(), tokens).expect("parse ok");
    match eval_all(0, name, src.to_string(), program) {
        Ok(_) => panic!("expected eval error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn plain_css_rejects_scss_at_rule() {
    let e = eval_err("@use \"scss_atrule\";");
    assert!(
        e.contains("at-rule isn't allowed in plain CSS"),
        "`@mixin` in .css must be rejected\n---\n{e}"
    );
}

#[test]
fn plain_css_rejects_sass_variable() {
    let e = eval_err("@use \"scss_var\";");
    assert!(
        e.contains("variables aren't allowed in plain CSS"),
        "`$var` in .css must be rejected\n---\n{e}"
    );
}

#[test]
fn plain_css_rejects_interpolation() {
    let e = eval_err("@use \"scss_interp\";");
    assert!(
        e.contains("Interpolation isn't allowed in plain CSS"),
        "`#{{…}}` in .css must be rejected\n---\n{e}"
    );
}

#[test]
fn plain_css_rejects_scss_operator() {
    let e = eval_err("@use \"scss_oper\";");
    assert!(
        e.contains("Operators aren't allowed in plain CSS"),
        "`1px + 2px` in .css must be rejected\n---\n{e}"
    );
}

#[test]
fn plain_css_allows_css_math_functions() {
    // `calc`/`min`/`max`/`clamp` operands are literal CSS math, not SassScript.
    let out = css("@use \"css_calc_ok\";");
    assert!(
        out.contains("calc(100% - 3px * 2)"),
        "`calc(… - … * …)` must pass through\n---\n{out}"
    );
}

#[test]
fn plain_css_allows_custom_at_rule_verbatim() {
    // A custom `@function --a()` (any casing) is opaque CSS; its `$b` payload
    // must be preserved verbatim, not validated as a Sass variable.
    let out = css("@use \"css_custom_atrule\";");
    assert!(
        out.contains("result: $b;"),
        "custom at-rule body must survive verbatim\n---\n{out}"
    );
}

#[test]
fn plain_css_rejects_placeholder_selector() {
    let e = eval_err("@use \"scss_placeholder\";");
    assert!(
        e.contains("Placeholder selectors aren't allowed in plain CSS"),
        "`%ph` in .css must be rejected\n---\n{e}"
    );
}

#[test]
fn plain_css_rejects_top_level_combinator() {
    let e = eval_err("@use \"scss_combinator\";");
    assert!(
        e.contains("leading combinators aren't allowed in plain CSS"),
        "a root rule starting with `>` must be rejected\n---\n{e}"
    );
}

#[test]
fn plain_css_rejects_parent_selector_suffix() {
    let e = eval_err("@use \"scss_parent_suffix\";");
    assert!(
        e.contains("Parent selectors can't have suffixes in plain CSS"),
        "`&b` must be rejected\n---\n{e}"
    );
}

#[test]
fn plain_css_rejects_module_namespace() {
    let e = eval_err("@use \"scss_namespaced\";");
    assert!(
        e.contains("Module namespaces aren't allowed in plain CSS"),
        "`lib.$x` must report the namespace message\n---\n{e}"
    );
}

#[test]
fn plain_css_allows_valid_nesting_and_keyframes() {
    // Guard against false positives: CSS nesting's bare `&`/`&:hover` and a
    // keyframes `50%` step are valid and must pass through untouched.
    let out = css("@use \"css_nesting_ok\";");
    assert!(
        out.contains("@keyframes spin") && out.contains("&:hover") && out.contains("50%"),
        "valid nesting + keyframes must survive\n---\n{out}"
    );
}

#[test]
fn plain_css_rejects_map_literal() {
    // `(y: z)` is a `:` inside parens -> the golden's CSS parser: `expected ")"`.
    let e = eval_err("@use \"scss_map\";");
    assert!(
        e.contains("expected"),
        "map literal in .css must be rejected\n---\n{e}"
    );
}

#[test]
fn plain_css_rejects_named_argument() {
    // `f($name: v)` uses a Sass variable as a named argument, which plain CSS
    // has no syntax for.
    let e = eval_err("@use \"scss_named_arg\";");
    assert!(
        e.contains("variables aren't allowed in plain CSS"),
        "`$lightness:` named arg in .css must be rejected\n---\n{e}"
    );
}
