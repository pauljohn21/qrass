//! `@extend` tests (task 7.5, design D4): placeholder merging into extending
//! selectors, order independence (placeholder defined after the extend site),
//! `!optional`, interpolated placeholder names, empty placeholders that emit
//! nothing, and the element-plus `share-rule` / `extend-rule` double-mixin idiom.

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
fn extend_merges_placeholder_into_selector() {
    let out = css("%box { color: red; } .a { @extend %box; }");
    assert!(out.contains(".a {") && out.contains("color: red;"), "{out}");
    // The placeholder itself is never rendered.
    assert!(!out.contains('%'), "placeholder dropped\n---\n{out}");
}

#[test]
fn extend_multiple_selectors_share_placeholder() {
    // Two selectors extending one placeholder collapse into a single *grouped*
    // rule (dart-sass semantics): one declaration block shared by both selectors.
    let out = css("%box { color: red; } .a { @extend %box; } .b { @extend %box; }");
    assert!(out.contains(".a") && out.contains(".b"), "{out}");
    assert_eq!(
        out.matches("color: red;").count(),
        1,
        "shared declaration rendered once in the grouped rule\n---\n{out}"
    );
    assert!(!out.contains('%'), "{out}");
}

#[test]
fn extend_placeholder_defined_after_site() {
    // Order independence: the global pass resolves regardless of source order.
    let out = css(".a { @extend %box; } %box { color: red; }");
    assert!(out.contains(".a {") && out.contains("color: red;"), "{out}");
    assert!(!out.contains('%'), "{out}");
}

#[test]
fn extend_compound_placeholder_suffix_preserved() {
    // `%btn:hover` extended by `.a` becomes `.a:hover` (suffix kept).
    let out = css("%btn:hover { color: blue; } .a { @extend %btn; }");
    assert!(out.contains(".a:hover {"), "{out}");
    assert!(!out.contains('%'), "{out}");
}

#[test]
fn optional_extend_of_missing_placeholder_is_noop() {
    let out = css(".a { @extend %missing !optional; color: red; }");
    // `.a`'s own declaration survives; the dangling optional extend emits nothing.
    assert!(out.contains(".a {") && out.contains("color: red;"), "{out}");
}

#[test]
fn non_optional_extend_of_missing_placeholder_errors() {
    let msg = eval_err(".a { @extend %missing; }");
    assert!(msg.contains("not a known selector"), "{msg}");
}

#[test]
fn empty_placeholder_emits_nothing() {
    let out = css("%p { } .a { @extend %p; }");
    assert!(
        out.is_empty(),
        "empty placeholder yields no output\n---\n{out}"
    );
}

#[test]
fn element_plus_share_rule_extend_rule() {
    // Replicates element-plus mixins.scss: share-rule defines `%shared-<name>`
    // via `@at-root`, extend-rule does an interpolated `@extend ... !optional`.
    let src = r#"
@mixin share-rule($name) {
  $rule-name: '%shared-' + $name;
  @at-root #{$rule-name} {
    @content;
  }
}
@mixin extend-rule($name) {
  @extend #{'%shared-' + $name} !optional;
}

@include share-rule(scrollbar) {
  height: 10px;
  background: green;
}

.el-a { @include extend-rule(scrollbar); }
"#;
    let out = css(src);
    assert!(
        out.contains(".el-a {")
            && out.contains("height: 10px;")
            && out.contains("background: green;"),
        "extended selector inherits shared decls\n---\n{out}"
    );
    // Neither the placeholder nor the interpolated name leaks into output.
    assert!(!out.contains('%'), "{out}");
    assert!(!out.contains("shared-scrollbar"), "{out}");
}
