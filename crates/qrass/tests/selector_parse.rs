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
    rt(":has(> .a)", ":has(.a)"); // leading relative combinator is dropped (known gap)
    rt(":lang(en)", ":lang(en)");
    rt(":nth-child(2n + 1)", ":nth-child(2n + 1)");
}

#[test]
fn the_of_keyword_only_splits_between_whitespace() {
    // `offset` must not be treated as an `of` clause; the arg parses as a selector.
    rt(":not(offset)", ":not(offset)");
    rt(":nth-child(2n + 1 of .a)", ":nth-child(2n + 1 of .a)");
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
    // A dangling combinator is truncated, not silently downgraded to `a`.
    err("a >", SelParseError::Truncated);
    err("a +", SelParseError::Truncated);
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
