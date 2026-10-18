//! Selector parser (Phase 1) round-trip and error tests. The canonical rendering
//! is part of the contract: `parse_list(src).to_string()` must reproduce the exact
//! text the `selector.*` and `@extend` `.hrx` goldens expect, so a round-trip is
//! itself the correctness check for the AST/parser.

use qrass::selector_ast::{PseudoArg, Simple};
use qrass::selector_parse::{SelParseError, parse_complex_str, parse_list};

/// Assert the selector round-trips to `expected` (canonical text).
fn rt(src: &str, expected: &str) {
    let parsed = match parse_list(src) {
        Ok(parsed) => parsed,
        Err(e) => panic!("parse_list({src:?}) should succeed, got {e:?}"),
    };
    assert_eq!(parsed.to_string(), expected, "canonical render of {src:?}");
}

/// Assert the selector fails with exactly `kind`.
fn err(src: &str, kind: SelParseError) {
    match parse_list(src) {
        Ok(got) => panic!("expected {kind:?} for {src:?}, parsed ok as {got}"),
        Err(got) => assert_eq!(got, kind, "error kind for {src:?}"),
    }
}

#[test]
fn compounds_and_combinators_survive_the_round_trip() {
    rt("a", "a");
    rt(".a.b#c", ".a.b#c");
    rt("a b", "a b");
    rt("a > b", "a > b");
    rt("a + b", "a + b");
    rt("a ~ b", "a ~ b");
    rt("a > b c ~ d", "a > b c ~ d");
    rt("a, b", "a, b");
    rt("a .b > #c", "a .b > #c");
}

#[test]
fn namespace_forms_keep_their_bars() {
    rt("*", "*");
    rt("*|*", "*|*");
    rt("|*", "|*");
    rt("ns|*", "ns|*");
    rt("ns|c", "ns|c");
    rt("|c", "|c");
    rt("a|*", "a|*");
}

#[test]
fn attribute_selectors_render_as_written() {
    // The raw group is preserved verbatim: no quotes are added or removed.
    rt("[c=d]", "[c=d]");
    rt("[c=\"d\"]", "[c=\"d\"]");
    rt("[c]", "[c]");
    rt("[c^=d]", "[c^=d]");
    rt("[c$=d i]", "[c$=d i]");
    rt("a[b=c] d", "a[b=c] d");
}

#[test]
fn functional_pseudos_classify_their_argument() {
    rt(":hover", ":hover");
    rt(":not(.a)", ":not(.a)");
    rt(":is(.a, .b)", ":is(.a, .b)");
    rt(":has(> .a)", ":has(> .a)"); // leading relative combinator is retained (front-pinned)
    rt(":lang(en)", ":lang(en)");
    rt(":nth-child(2n + 1)", ":nth-child(2n+1)"); // <an+b> canonicalizes whitespace away
}

#[test]
fn the_of_keyword_only_splits_between_whitespace() {
    // `offset` must not be treated as an `of` clause; the arg parses as a selector.
    rt(":not(offset)", ":not(offset)");
    rt(":nth-child(2n + 1 of .a)", ":nth-child(2n+1 of .a)");
    let list = parse_list(":nth-child(3n of .a, .b)").unwrap();
    let Simple::PseudoClass { arg, .. } = &list.complexes[0].components[0].compound.simples[0]
    else {
        panic!("expected a pseudo-class");
    };
    match arg {
        PseudoArg::Nth { anbn, of } => {
            assert_eq!(anbn, "3n");
            assert_eq!(of.as_ref().unwrap().complexes.len(), 2);
        }
        other => panic!("expected Nth, got {other:?}"),
    }
}

#[test]
fn pseudo_elements_are_marked_by_colon_count() {
    let element = parse_complex_str("::before").unwrap();
    assert!(
        element.components[0]
            .compound
            .simples
            .iter()
            .any(Simple::is_pseudo_element)
    );
    // A single-colon pseudo-class is not an element.
    let class = parse_complex_str(":hover").unwrap();
    assert!(
        !class.components[0]
            .compound
            .simples
            .iter()
            .any(Simple::is_pseudo_element)
    );
    // Legacy single-colon pseudo-elements are still marked as elements.
    let legacy = parse_complex_str(":before").unwrap();
    assert!(
        legacy.components[0]
            .compound
            .simples
            .iter()
            .any(Simple::is_pseudo_element)
    );
}

#[test]
fn nested_selector_lists_are_exposed_in_order() {
    let list = parse_list(":is(.a, .b)").unwrap();
    let nested = list.selector_lists();
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].complexes.len(), 2);
    assert_eq!(nested[0].to_string(), ".a, .b");
}

#[test]
fn a_type_simple_is_found_but_a_class_is_not() {
    let with_type = parse_complex_str("a.b").unwrap();
    assert!(with_type.components[0].compound.type_simple().is_some());
    let only_class = parse_complex_str(".b").unwrap();
    assert!(only_class.components[0].compound.type_simple().is_none());
}

#[test]
fn a_parent_selector_is_rejected_wherever_it_appears() {
    err("& a", SelParseError::Parent);
    err("a &", SelParseError::Parent);
    err(":is(&)", SelParseError::Parent);
}

#[test]
fn truncated_input_is_rejected() {
    // A *dangling* combinator is legal input (the golden parses `b +` into the
    // structural value `(b "+",)`), so only genuinely incomplete input errors.
    rt("a >", "a >");
    rt("a +", "a +");
    err("", SelParseError::Truncated);
    err("a,", SelParseError::Truncated);
    err("[c", SelParseError::Truncated);
    err(":is(.a", SelParseError::Truncated);
}

#[test]
fn escapes_and_vendor_prefixes_round_trip() {
    rt("::-webkit-scrollbar", "::-webkit-scrollbar");
    rt(":-moz-any-link", ":-moz-any-link");
    rt(".\\@class", ".\\@class");
}

// ─── selector.parse builtins: the returned *value*, not just its text ────────
// `selector.parse` must hand back the parsed selector-list value (comma list of
// space lists with combinators as their own elements), the same shape unify /
// extend return — so `$result == (decomposed literal,)` holds while the value
// still serializes to the identical CSS text.

/// Compile `src` and return the joined CSS output.
fn compile(src: &str) -> String {
    use qrass::eval::eval_all;
    use qrass::lexer::lex;
    use qrass::parser::parse;
    let tokens = lex(0, "t.scss".into(), src.to_string()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.to_string(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src.to_string(), program)
        .expect("eval ok")
        .join("\n")
}

#[test]
fn parse_result_equals_the_decomposed_selector_literal() {
    // descendant / child / leading-bogus-combinator structures compare true.
    let out = compile(
        "@use \"sass:selector\";\n\
         $d: selector.parse(\"b c d\");\n\
         $c: selector.parse(\"b > c > d\");\n\
         $l: selector.parse(\"> b\");\n\
         a { d: $d == (b c d,); c: $c == (b \">\" c \">\" d,); l: $l == (\">\" b,); }",
    );
    assert!(out.contains("d: true"), "{out}");
    assert!(out.contains("c: true"), "{out}");
    assert!(out.contains("l: true"), "{out}");
}

#[test]
fn parse_result_still_serializes_as_the_selector() {
    let out = compile("@use \"sass:selector\";\n@use \"sass:meta\";\na { b: selector.parse(\"a, c > d\"); }");
    assert!(out.contains("b: a, c > d"), "{out}");
}
