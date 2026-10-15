//! Task group 9: expanded CSS serialization (`src/css.rs`). Covers the
//! structural contract — two-space indent, block order = source order, empty
//! rules dropped, the comment policy (`/* */` preserved, `//` dropped), and the
//! `!important` suffix — plus the number/color serializer boundary cases
//! (1/3, 0.0, -0.0, large-integers-without-exponent, `#fff` → `#ffffff`).

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

/// Compile source to the eval chunk stream joined the way the render stage does.
fn css(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src.into(), program)
        .expect("eval ok")
        .join("\n")
}

// ─── 9.1 Structure: indentation, nesting, source order ───────────────────────

#[test]
fn flat_rule_two_space_indent() {
    assert_eq!(css(".a { top: 10px; }"), ".a {\n  top: 10px;\n}");
}

#[test]
fn nested_rule_flattens_in_source_order() {
    // The parent's own declarations emit first, then the nested child as its own
    // resolved block — matching expanded source order.
    let out = css(".a {\n  top: 10px;\n  .b { left: 20px; }\n}");
    assert_eq!(out, ".a {\n  top: 10px;\n}\n.a .b {\n  left: 20px;\n}");
}

#[test]
fn multiple_declarations_keep_order() {
    let out = css(".a {\n  top: 1px;\n  left: 2px;\n  right: 3px;\n}");
    assert_eq!(out, ".a {\n  top: 1px;\n  left: 2px;\n  right: 3px;\n}");
}

// ─── 9.1 Empty rules and null declarations are dropped ───────────────────────

#[test]
fn empty_rule_is_dropped() {
    assert_eq!(css(".a {\n}\n").trim(), "");
}

#[test]
fn null_valued_declaration_is_dropped() {
    assert_eq!(css(".a { x: null; }").trim(), "");
}

// ─── 9.1 @media / @keyframes at-rule wrapping (merged block, indent) ─────────

#[test]
fn media_wraps_sibling_rules_merged() {
    let out = css("@media screen {\n  .a { top: 10px; }\n  .b { left: 20px; }\n}");
    assert_eq!(
        out,
        "@media screen {\n  .a {\n    top: 10px;\n  }\n  .b {\n    left: 20px;\n  }\n}"
    );
}

#[test]
fn keyframes_wraps_stops() {
    let out = css("@keyframes spin {\n  from { opacity: 0; }\n  to { opacity: 1; }\n}");
    assert_eq!(
        out,
        "@keyframes spin {\n  from {\n    opacity: 0;\n  }\n  to {\n    opacity: 1;\n  }\n}"
    );
}

// ─── 9.1 !important is carried into the value string ─────────────────────────

#[test]
fn important_suffix_preserved() {
    let out = css(".a { margin: 0 !important; }");
    assert_eq!(out, ".a {\n  margin: 0 !important;\n}");
}

// ─── 9.1 Comment policy: /* */ preserved, // dropped ─────────────────────────

#[test]
fn loud_comments_preserved_silent_dropped() {
    let src = "/* loud */\n.a { top: 10px; } // silent\n/* keep */\n";
    let out = css(src);
    assert_eq!(out, "/* loud */\n.a {\n  top: 10px;\n}\n/* keep */");
    assert!(!out.contains("silent"), "silent comment leaked: {out}");
}

#[test]
fn comment_inside_rule_interleaves_in_order() {
    let src = ".a {\n  /* inner */\n  top: 10px;\n}\n";
    assert_eq!(css(src), ".a {\n  /* inner */\n  top: 10px;\n}");
}

// ─── 9.2 Number serializer boundary cases ────────────────────────────────────

#[test]
fn fraction_ten_digits() {
    // math.div(1,3) → 0.333… serialized with ten fractional digits, no exponent.
    let out = css(".a { x: math.div(1, 3); }");
    assert_eq!(out, ".a {\n  x: 0.3333333333;\n}");
}

#[test]
fn zero_and_negative_zero_normalize() {
    assert_eq!(css(".a { x: 0.0; }"), ".a {\n  x: 0;\n}");
    assert_eq!(css(".a { x: -0.0; }"), ".a {\n  x: 0;\n}");
}

#[test]
fn trailing_zeros_trimmed() {
    assert_eq!(css(".a { x: 1.50; }"), ".a {\n  x: 1.5;\n}");
}

#[test]
fn large_number_has_no_exponent() {
    // 1e18 exceeds the integral fast-path yet must serialize as plain digits.
    let out = css(".a { x: 1000000000 * 1000000000; }");
    assert_eq!(out, ".a {\n  x: 1000000000000000000;\n}");
    assert!(!out.contains('e'), "exponent leaked: {out}");
}

// ─── 9.2 Color serializer boundary cases ─────────────────────────────────────

#[test]
fn short_hex_normalizes_to_six_digits() {
    assert_eq!(css(".a { c: #fff; }"), ".a {\n  c: #ffffff;\n}");
}

#[test]
fn named_color_preserved_as_keyword() {
    // dart-sass keeps a recognized color keyword verbatim (it does not hex-ify).
    assert_eq!(css(".a { c: red; }"), ".a {\n  c: red;\n}");
}

#[test]
fn rgb_function_serializes_as_rgb() {
    assert_eq!(
        css(".a { c: color.rgb(255, 0, 0); }"),
        ".a {\n  c: rgb(255, 0, 0);\n}"
    );
}

#[test]
fn hex_literal_serializes_as_hex() {
    assert_eq!(css(".a { c: #ff0000; }"), ".a {\n  c: #ff0000;\n}");
}

#[test]
fn translucent_color_serializes_as_rgba() {
    let out = css(".a { c: color.change(#ff0000, $alpha: 0.5); }");
    assert_eq!(out, ".a {\n  c: rgba(255, 0, 0, 0.5);\n}");
}
