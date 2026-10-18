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

// ─── selector.append: compound-junction splice (round 34) ─────────────────────
// append folds each argument list onto the previous result by concatenating the
// parent's LAST compound with the child's FIRST, keeping the child's remaining
// complexes (cross-product) and combinators, and returns a parsed selector-list.

#[test]
fn append_joins_compounds_and_cross_multiplies_lists() {
    let out = compile(
        "@use \"sass:selector\";\n\
         a { a: selector.append(\".c\", \".d\"); b: selector.append(\".c, .d\", \".e, .f\"); c: selector.append(\"c d\", \"e f\"); }",
    );
    assert!(out.contains("a: .c.d"), "{out}");
    assert!(out.contains("b: .c.e, .c.f, .d.e, .d.f"), "{out}");
    assert!(out.contains("c: c de f"), "{out}");
}

#[test]
fn append_preserves_leading_and_trailing_combinators() {
    let out = compile(
        "@use \"sass:selector\";\n\
         a { a: selector.append(\"> c\", \"d\"); b: selector.append(\"c\", \"d ~\"); c: selector.append(\"c > > d\", \"e\"); }",
    );
    assert!(out.contains("a: > cd"), "{out}");
    assert!(out.contains("b: cd ~"), "{out}");
    assert!(out.contains("c: c > > de"), "{out}");
}

#[test]
fn append_returns_a_structural_selector_list_value() {
    let out = compile(
        "@use \"sass:selector\";\n\
         $r: selector.append(\"c d, e f\", \"g\");\n\
         a { r: $r; s: $r == (c dg, e fg); }",
    );
    assert!(out.contains("r: c dg, e fg"), "{out}");
    assert!(out.contains("s: true"), "{out}");
}

#[test]
fn append_rejects_universal_namespaced_and_parent_bases() {
    // The child's first compound cannot splice into a non-empty parent when it
    // opens with a universal or a namespaced type; a parent selector is rejected
    // at parse; and zero arguments raise the arity guard.
    for (src, want) in [
        ("selector.append(\".c\", \"*\")", "Can't append * to .c."),
        ("selector.append(\"c\", \"|d\")", "Can't append |d to c."),
        ("selector.append(\".c\", \"&\")", "Parent selectors aren't allowed here."),
        ("selector.append()", "At least one selector must be passed."),
    ] {
        let scss = format!("@use \"sass:selector\";\na {{ b: {src}; }}");
        let msg = eval_err(&scss);
        assert!(msg.contains(want), "{src}: expected {want:?}, got {msg}");
    }
}

// ─── selector.replace: non-additive extension (round 35) ──────────────────────
// replace shares `extend`'s engine but drops the verbatim original whenever a
// selector actually matched, substitutes inside selector-list pseudos instead of
// growing them, and never trims results across sibling selectors.

#[test]
fn replace_substitutes_top_level_and_keeps_unmatched() {
    let out = compile(
        "@use \"sass:selector\";\n\
         a { a: selector.replace(\"c\", \"c\", \"d\"); b: selector.replace(\"c.d\", \"c\", \"e\"); c: selector.replace(\"c\", \"d\", \"e\"); }",
    );
    assert!(out.contains("a: d"), "{out}");
    assert!(out.contains("b: e.d"), "{out}");
    // `no_op`: `c` never matches `d`, so it survives verbatim.
    assert!(out.contains("c: c"), "{out}");
}

#[test]
fn replace_weaves_a_complex_replacement() {
    let out = compile(
        "@use \"sass:selector\";\n\
         a { a: selector.replace(\"c d\", \"d\", \"e f\"); }",
    );
    assert!(out.contains("a: c e f, e c f"), "{out}");
}

#[test]
fn replace_substitutes_inside_selector_list_pseudos() {
    // Unlike `extend` (which grows the argument list), `replace` swaps the match
    // in place: `:is(c)` -> `:is(d)` and `:not(c)` -> `:not(d)`.
    let out = compile(
        "@use \"sass:selector\";\n\
         a { a: selector.replace(\":is(c)\", \"c\", \"d\"); b: selector.replace(\":where(c)\", \"c\", \"d\"); c: selector.replace(\":not(c)\", \"c\", \"d\"); }",
    );
    assert!(out.contains("a: :is(d)"), "{out}");
    assert!(out.contains("b: :where(d)"), "{out}");
    assert!(out.contains("c: :not(d)"), "{out}");
}

#[test]
fn replace_is_not_additive_and_never_trims_siblings() {
    // `d c` extends to `d e`, which a broad `e` from the sibling `c` must NOT
    // absorb (extend would trim it; replace keeps both).
    let out = compile(
        "@use \"sass:selector\";\n\
         $r: selector.replace((c, d c), \"c\", \"e\");\n\
         a { v: $r; }",
    );
    assert!(out.contains("v: e, d e"), "{out}");
}

#[test]
fn replace_rejects_complex_original_and_parent_selector() {
    for (src, want) in [
        (
            "selector.replace(\"c\", \"d e\", \"f\")",
            "Can't extend complex selector d e.",
        ),
        (
            "selector.replace(\"&\", \"c\", \"d\")",
            "$selector: Parent selectors aren't allowed here.",
        ),
        (
            "selector.replace(\"c\", \"[d\", \"e\")",
            "$original: expected more input.",
        ),
    ] {
        let scss = format!("@use \"sass:selector\";\na {{ b: {src}; }}");
        let msg = eval_err(&scss);
        assert!(msg.contains(want), "{src}: expected {want:?}, got {msg}");
    }
}

/// Compile `src` and return the eval error's message (panics if it succeeds).
fn eval_err(src: &str) -> String {
    use qrass::eval::eval_all;
    use qrass::lexer::lex;
    use qrass::parser::parse;
    let tokens = lex(0, "t.scss".into(), src.to_string()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.to_string(), tokens).expect("parse ok");
    match eval_all(0, "t.scss".into(), src.to_string(), program) {
        Ok(_) => panic!("expected eval error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

/// `selector.nest` validates each argument as a selector: a non-string raises the
/// prefix-less `… is not a valid selector…`; a `&`-free malformed argument
/// surfaces `expected more input.`; no argument raises `$selectors: At least one
/// selector must be passed.`. (The `&`-position parent rules are a later round.)
#[test]
fn selector_nest_argument_errors() {
    let msg = eval_err(r##"a {b: selector.nest(1)}"##);
    assert!(msg.contains("1 is not a valid selector"), "{msg}");
    let msg = eval_err(r##"a {b: selector.nest("c", 1)}"##);
    assert!(msg.contains("1 is not a valid selector"), "{msg}");
    let msg = eval_err(r##"a {b: selector.nest("[c")}"##);
    assert!(msg.contains("expected more input."), "{msg}");
    let msg = eval_err(r##"a {b: selector.nest("c", "[d")}"##);
    assert!(msg.contains("expected more input."), "{msg}");
    let msg = eval_err(r##"a {b: selector.nest()}"##);
    assert!(
        msg.contains("$selectors: At least one selector must be passed."),
        "{msg}"
    );
}
