//! Host-side pins for the browser facade (`qrass-wasm`). The `*_text` layer is
//! the exact logic the `#[wasm_bindgen]` exports run in the browser, and its
//! error channel is a plain `String` so host tests can assert the message text.
//! The JS wrappers are pinned only on their Ok path: constructing a `JsError`
//! calls an imported JS function, which wasm-bindgen deliberately traps on
//! non-wasm targets, so their Err paths belong to the browser smoke test.

use qrass::{Options, compile_string};
use qrass_wasm::{
    clear_sources, compile_scss, compile_scss_text, compile_scss_with_style,
    compile_scss_with_style_text, register_source,
};

#[test]
fn facade_compiles_a_real_program() {
    let css = compile_scss_text(
        "in.scss",
        "@use \"sass:math\"; @mixin c { color: red } .a { .b { @include c; w: math.div(10px, 2); } }",
    )
    .expect("compile ok");
    assert!(css.contains(".a .b"), "nesting lost: {css}");
    assert!(css.contains("color: red"), "mixin body lost: {css}");
    assert!(css.contains("5px"), "builtin math lost: {css}");
}

#[test]
fn facade_matches_the_core_string_entry() {
    let src = ".x { color: darken(red, 10%) }";
    let via_facade = compile_scss_text("e.scss", src).expect("compile ok");
    let via_core =
        compile_string("e.scss".to_string(), src.to_string(), &Options::default()).expect("ok");
    assert_eq!(via_facade, via_core);
}

#[test]
fn facade_reports_load_errors_as_js_exceptions() {
    let err = compile_scss_text("e.scss", "@use \"definitely_missing\";")
        .expect_err("unresolvable @use must fail");
    assert!(
        err.contains("Couldn't resolve @use/@forward URL"),
        "unexpected wording: {err}"
    );
}

#[test]
fn facade_rejects_unknown_styles() {
    let err = compile_scss_with_style_text("e.scss", ".a { b: c }", "compressed")
        .expect_err("only expanded exists");
    assert!(
        err.contains("only 'expanded'"),
        "unexpected wording: {err}"
    );
    assert!(compile_scss_with_style_text("e.scss", ".a { b: c }", "expanded").is_ok());
    // The JS wrapper Ok path is host-callable (no JS imports are touched).
    assert!(compile_scss("e.scss", ".a { b: c }").is_ok());
    assert!(compile_scss_with_style("e.scss", ".a { b: c }", "expanded").is_ok());
}

#[test]
fn facade_compiles_multi_file_via_registered_sources() {
    // The browser story: JS preloads the partial tree, then one compile walks
    // `@use` across two virtual files with zero file-system access.
    clear_sources();
    register_source("/f/_vars.scss", "$brand: #06c; .b { color: $brand }");
    register_source("/f/_uses.scss", "@use \"vars\";\n.u { border-color: vars.$brand }");
    let css = compile_scss_text("/f/entry.scss", "@use \"uses\";\n").expect("virtual compile ok");
    assert!(css.contains(".b"), "transitive partial lost: {css}");
    assert!(css.contains(".u"), "partial body lost: {css}");
    assert!(css.contains("#0066cc"), "cross-module variable lost: {css}");
    clear_sources();
}
