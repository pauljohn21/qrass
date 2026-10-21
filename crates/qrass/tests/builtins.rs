//! Built-in function library tests (task group 8). Every registry entry gets at
//! least one case; the two contract assertions called out in tasks.md are
//! `math.div(1, 3)` serializing to `0.3333333333` and an un-registered function
//! raising "Undefined function" that carries the namespace/module name.
//!
//! Builtins are dispatched directly by the registry — a namespaced call such as
//! `math.div(...)` needs no `@use` (only module *variables* like `math.$pi` do),
//! so these tests exercise the pure dispatch path.

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

/// Evaluate `expr` as the value of declaration `p` and return its serialized CSS.
fn value(expr: &str) -> String {
    let out = css(&format!(".a {{ p: {expr}; }}"));
    let rest = out
        .split_once("p: ")
        .unwrap_or_else(|| panic!("no declaration emitted for {expr:?}: {out}"))
        .1;
    rest.split_once(";\n")
        .map(|x| x.0)
        .unwrap_or(rest)
        .trim()
        .to_string()
}

// ─── 8.1 dispatch contract: undefined function / arity / name normalization ─────

#[test]
fn undefined_namespaced_function_names_the_module() {
    let msg = eval_err(".a { p: map.nonexistent(1); }");
    assert!(msg.contains("Undefined function"), "{msg}");
    assert!(
        msg.contains("map"),
        "message must carry the module name: {msg}"
    );
}

#[test]
fn undefined_bare_function_passes_through_as_plain_css() {
    // the goldens emit calls it doesn't recognize as plain CSS (so browser-native
    // functions like `var`/`calc` work untouched); a bare unknown call is kept
    // verbatim rather than raising. Only *namespaced* unknowns error (see above).
    assert_eq!(value("totally_made_up(1)"), "totally_made_up(1)");
}

#[test]
fn wrong_arity_is_reported() {
    // dart-shaped shortfall wording: the missing slot is named (round 23).
    let msg = eval_err(".a { p: math.div(1); }");
    assert!(msg.contains("Missing argument $number2."), "{msg}");
    // dart-shaped overflow wording when past the fixed maximum.
    let msg = eval_err(".a { p: math.div(1, 2, 3); }");
    assert!(
        msg.contains("Only 2 arguments allowed, but 3 were passed."),
        "{msg}"
    );
}

#[test]
fn hyphen_alias_and_namespace_share_impl() {
    // `map-get` (legacy global) and `map.get` resolve to the same implementation.
    assert_eq!(value("map-get((a: 1), a)"), value("map.get((a: 1), a)"));
    assert_eq!(value("map.get((a: 1), a)"), "1");
}

// ─── 8.2 map.* / math.* / color.* ───────────────────────────────────────────────
// These three namespaces were split into tests/builtins_map.rs,
// tests/builtins_math.rs and tests/builtins_color.rs to keep every file within the
// 500-line budget; the shared css/value/eval_err helpers are duplicated there.

// ─── 8.3 string.* ─────────────────────────────────────────────────────────────

#[test]
fn string_length_slice_index() {
    assert_eq!(value("string.length(\"hello\")"), "5");
    assert_eq!(value("string.slice(\"hello\", 2, 4)"), "\"ell\"");
    assert_eq!(value("string.index(\"hello\", \"ll\")"), "3");
}

#[test]
fn string_case_and_quotes() {
    // to-upper/lower-case inherit the input's quoting (goldens: `"abc"` -> `"ABC"`).
    assert_eq!(value("string.to-upper-case(\"abc\")"), "\"ABC\"");
    assert_eq!(value("string.to-lower-case(\"ABC\")"), "\"abc\"");
    assert_eq!(value("string.unquote(\"hi\")"), "hi");
}

/// `string.insert` clamps out-of-range indices to the nearest edge and indexes by
/// Unicode codepoint (an astral emoji is one char). The result mirrors `$string`'s
/// quoting, so quoted inputs yield a quoted output.
#[test]
fn string_insert_indexing() {
    assert_eq!(value("string.insert(\"cde\", \"f\", 0)"), "\"fcde\"");
    assert_eq!(value("string.insert(\"cde\", \"f\", 2)"), "\"cfde\"");
    assert_eq!(value("string.insert(\"cde\", \"f\", -2)"), "\"cdfe\"");
    assert_eq!(value("string.insert(\"cde\", \"f\", 100)"), "\"cdef\"");
    assert_eq!(value("string.insert(\"cde\", \"f\", -100)"), "\"fcde\"");
    // Named arguments fold into positional slots.
    assert_eq!(
        value("string.insert($string: \"cde\", $insert: \"f\", $index: 2)"),
        "\"cfde\""
    );
}

// ─── 8.3 list.* ───────────────────────────────────────────────────────────────

#[test]
fn list_length_nth_index() {
    assert_eq!(value("list.length(10px 20px 30px)"), "3");
    assert_eq!(value("list.nth(a b c, 2)"), "b");
    assert_eq!(value("list.nth(a b c, -1)"), "c");
    assert_eq!(value("list.index(a b c, b)"), "2");
}

/// Bracketed (`[…]`) list literals round-trip through serialization and drive
/// `list.is-bracketed`; `join`/`append` propagate `list1`'s bracketedness (join
/// gated by `$bracketed`, default true), and a single-element space `list1`
/// defers its separator to `list2`.
#[test]
fn list_bracketed_lists() {
    // Literal round-trip.
    assert_eq!(value("[1 2 3]"), "[1 2 3]");
    assert_eq!(value("[1, 2]"), "[1, 2]");
    // Predicate reads the bracket flag; unbracketed forms are false.
    assert_eq!(value("list.is-bracketed([1])"), "true");
    assert_eq!(value("list.is-bracketed([])"), "true");
    assert_eq!(value("list.is-bracketed(1 2 3)"), "false");
    assert_eq!(value("list.is-bracketed((1,))"), "false");
    // Propagation: result bracketed iff list1 is (and $bracketed holds).
    assert_eq!(value("list.join([1], [2])"), "[1 2]");
    assert_eq!(value("list.join((1,), [2])"), "1, 2");
    assert_eq!(value("list.join([1], (2,))"), "[1, 2]");
    assert_eq!(value("list.join([1], [2], $bracketed: false)"), "1 2");
    assert_eq!(value("list.append([1 2], 3)"), "[1 2 3]");
    // A bracketed *space* run `[1 2]` is a flat 2-item list, not a one-element
    // list wrapping a nested space list (parser fix): length / nth read the run.
    assert_eq!(value("list.length([1 2])"), "2");
    assert_eq!(value("list.length([1 2 3])"), "3");
    assert_eq!(value("list.nth([10 20 30], 2)"), "20");
    assert_eq!(value("list.length([1, 2])"), "2");
}

#[test]
fn inspect_parenthesizes_nested_lists() {
    // `inspect` (source form) keeps a nested list's structure by parenthesizing it
    // when it would otherwise merge into the parent, unlike flat CSS output.
    assert_eq!(value("meta.inspect((1 2, 3 4))"), "1 2, 3 4"); // comma parent, space child: flat
    assert_eq!(value("meta.inspect(((1 2) (3 4)))"), "(1 2) (3 4)"); // space parent, space child: parens
    assert_eq!(value("meta.inspect(((1, 2) (3, 4)))"), "(1, 2) (3, 4)"); // comma child in space parent: parens
    assert_eq!(value("meta.inspect(((), ()))"), "(), ()"); // empty children: `()`
    assert_eq!(value("meta.inspect([1 2])"), "[1 2]"); // a bracketed run stays bracketed
    assert_eq!(value("meta.inspect(([1, 2] [3, 4]))"), "[1, 2] [3, 4]");
}

#[test]
fn list_append_join_separator() {
    assert_eq!(value("list.append(1 2, 3)"), "1 2 3");
    assert_eq!(value("list.join((a, b), (c, d))"), "a, b, c, d");
    assert_eq!(value("list.separator((a, b))"), "comma");
    assert_eq!(value("list.set-nth(a b c, 2, z)"), "a z c");
}

// ─── 8.3 selector.* ───────────────────────────────────────────────────────────

#[test]
fn selector_nest_parse_simple() {
    assert_eq!(value("selector.nest(\".a\", \".b\")"), ".a .b");
    assert_eq!(value("selector.nest(\".a\", \"&.b\")"), ".a.b");
    assert_eq!(value("selector.parse(\".a, .b\")"), ".a, .b");
    assert_eq!(
        value("selector.simple-selectors(\".a.b:hover\")"),
        ".a, .b, :hover"
    );
}

// ─── 8.3 meta.* (pure) ────────────────────────────────────────────────────────

#[test]
fn meta_type_of_and_inspect() {
    assert_eq!(value("meta.type-of(1px)"), "number");
    assert_eq!(value("meta.type-of(a)"), "string");
    assert_eq!(value("meta.type-of((a: 1))"), "map");
    assert_eq!(value("meta.inspect(42)"), "42");
}

#[test]
fn meta_keywords_and_get() {
    // keyword-get still reads a plain map (legacy spelling).
    assert_eq!(value("meta.keyword-get((a: 1, b: 2), b)"), "2");
    // meta.keywords requires a real rest-argument ArgList: it returns the named
    // part as a map, and a caller-supplied named arg reaches it via `$args...`.
    let src = "@use 'sass:meta';\n@function kw($args...) { @return meta.keywords($args); }\n\
               a { x: meta.inspect(kw($c: d, $e: f)); y: meta.inspect(kw(1, 2)); }";
    let out = css(src);
    assert!(out.contains("x: (c: d, e: f)"), "{out}");
    // A positional-only call yields an empty keyword map.
    assert!(out.contains("y: ()"), "{out}");
    // keyword-get reads a name out of the ArgList's keyword part.
    let src2 = "@use 'sass:meta';\n@function kg($args...) { @return meta.keyword-get($args, c); }\n\
                a { v: kg($c: 7); }";
    let out2 = css(src2);
    assert!(out2.contains("v: 7"), "{out2}");
    // Passing a non-ArgList (a plain list) to meta.keywords is a golden error.
    let err = eval_err("a { b: meta.keywords(1 2 3); }");
    assert!(err.contains("is not an argument list"), "{err}");
}

// ─── 8.3 global.* ─────────────────────────────────────────────────────────────

#[test]
fn global_if_and_units() {
    assert_eq!(value("if(true, yes, no)"), "yes");
    assert_eq!(value("if(false, yes, no)"), "no");
    assert_eq!(value("unit(42px)"), "\"px\"");
    assert_eq!(value("unitless(42)"), "true");
    assert_eq!(value("comparable(1px, 2px)"), "true");
    assert_eq!(value("feature-exists(global-keyframes)"), "false");
}

// ─── env-dependent meta (intercepted in eval_call) ─────────────────────────────

/// Evaluate `prog` and return the emitted error message (panics on success).
fn err_prog(prog: &str) -> String {
    match qrass::eval::eval_all(0, "t.scss".into(), prog.to_string(), {
        let tokens = qrass::lexer::lex(0, "t.scss".into(), prog.to_string()).expect("lex");
        qrass::parser::parse(0, "t.scss".into(), prog.to_string(), tokens).expect("parse")
    }) {
        Ok(out) => panic!("expected error, got: {}", out.join("\n")),
        Err(e) => e.message,
    }
}

/// Evaluate a full `prog` whose last rule emits `p`, returning `p`'s CSS value.
fn val_prog(prog: &str) -> String {
    let out = css(prog);
    let rest = out
        .split_once("p: ")
        .unwrap_or_else(|| panic!("no declaration emitted in {out}"))
        .1;
    rest.split_once(";\n")
        .map(|x| x.0)
        .unwrap_or(rest)
        .trim()
        .to_string()
}

#[test]
fn variable_and_global_exists() {
    assert_eq!(
        val_prog("$x: 1; .a { p: meta.variable-exists(\"x\"); }"),
        "true"
    );
    assert_eq!(
        val_prog("$x: 1; .a { p: meta.variable-exists(\"zzz\"); }"),
        "false"
    );
    assert_eq!(
        val_prog("$x: 1; .a { p: meta.global-variable-exists(\"x\"); }"),
        "true"
    );
}

#[test]
fn mixin_and_function_exist() {
    assert_eq!(
        val_prog("@mixin foo {} .a { p: meta.mixin-exists(\"foo\"); }"),
        "true"
    );
    assert_eq!(
        val_prog("@function foo() { @return 1; } .a { p: meta.function-exists(\"foo\"); }"),
        "true"
    );
    // A registered builtin is also visible to function-exists.
    assert_eq!(
        val_prog(".a { p: meta.function-exists(\"map-get\"); }"),
        "true"
    );
}

#[test]
fn content_exists_is_false_outside_include() {
    // golden `meta/content_exists` `error/outside_mixin`: calling outside a
    // mixin body is an error, not a silent false (round 94).
    assert!(
        err_prog(".a { p: meta.content-exists(); }")
            .contains("content-exists() may only be called within a mixin.")
    );
}

/// The named-argument dispatcher folds `$name:`-style arguments into the
/// positional slot each registered parameter declares, so every namespace
/// reads its inputs positionally regardless of how the caller spelled them.
#[test]
fn named_arguments_fold_into_positional_slots() {
    // `math` declares ($number) / ($min,$number,$max) …
    assert_eq!(value("math.abs($number: -3)"), "3");
    assert_eq!(value("math.clamp($min: 0, $number: 5, $max: 10)"), "5");
    // `color` — $color folds to slot 0; channel kwargs stay in the named map.
    assert_eq!(value("color.channel($color: red, $channel: \"red\")"), "255");
    assert_eq!(value("color.alpha($color: rgba(0, 0, 0, 0.5))"), "0.5");
    assert_eq!(value("color.opacity($color: rgba(0, 0, 0, 0.5))"), "0.5");
    assert_eq!(value("color.ie-hex-str($color: red)"), "#FFFF0000");
    // `selector` / `map` / `list` / `string` all reach the same impl by name.
    assert_eq!(value("selector.parse($selector: \"a > b\")"), "a > b");
    assert_eq!(value("map.get($map: (a: 1), $key: a)"), "1");
    assert_eq!(value("list.nth($list: (10 20 30), $n: 2)"), "20");
    assert_eq!(value("string.to-upper-case($string: \"hi\")"), "\"HI\"");
}

/// A named argument mixed with a positional one into the *same* slot is a call
/// error (golden: "Argument $x was passed both by position and by name."), and an
/// unknown name on a fixed-arity builtin is rejected — but a variadic builtin
/// leaves undeclared names in the named map instead of erroring.
#[test]
fn named_argument_conflicts_and_unknowns() {
    let err = eval_err(".a { p: math.abs(-3, $number: 3); }");
    assert!(
        err.contains("Argument $number was passed both by position and by name."),
        "{err}"
    );
    let err = eval_err(".a { p: math.abs($number: 3, $bogus: 1); }");
    assert!(err.contains("No such argument"), "{err}");
}

// ─── 8.4 legacy color ops (round 25): color.complement / grayscale / invert ──────

#[test]
fn legacy_color_ops_match_golden_form() {
    // Hue rotation renders back to a CSS keyword when the channels hit one.
    assert_eq!(value("color.complement(red)"), "aqua");
    // Desaturation of a saturated color lands on a half-byte gray → `rgb(%)` form.
    assert_eq!(value("color.grayscale(red)"), "rgb(50%, 50%, 50%)");
    // A full-channel invert of a hex literal round-trips as hex.
    assert_eq!(value("color.invert(#abcdef)"), "#543210");
    // Weight scales the inversion (50% blends toward the complement).
    assert_eq!(value("color.invert(black, 50%)"), "rgb(50%, 50%, 50%)");
}

#[test]
fn bare_filter_functions_are_not_shadowed() {
    // `invert`/`grayscale` are also CSS filter functions: the *bare* names stay
    // unregistered so a non-color call passes through verbatim as plain CSS
    // (registering them as builtins regressed the goldens' passthrough cases).
    assert_eq!(value("invert(var(--c))"), "invert(var(--c))");
    assert_eq!(value("grayscale(0.5)"), "grayscale(0.5)");
}

// ─── round 26: string.split / map.deep-remove ────────────────────────────────

#[test]
fn string_split_forms_a_bracketed_list() {
    // substring separator, codepoint (empty) separator, limit, and quote-style
    // inheritance from the source string.
    assert_eq!(value(r#"string.split("a, b, c", ", ")"#), r#"["a", "b", "c"]"#);
    assert_eq!(value(r#"string.split("a/b/c", "/", 1)"#), r#"["a", "b/c"]"#);
    assert_eq!(value(r#"string.split("Helvetica", "")"#), r#"["H", "e", "l", "v", "e", "t", "i", "c", "a"]"#);
    // separator never matches → one-element list holding the whole string.
    assert_eq!(value(r#"string.split("a, b, c", "&")"#), r#"["a, b, c"]"#);
    // an unquoted source yields unquoted elements.
    assert_eq!(value("string.split(abc, \"\")"), "[a, b, c]");
}

#[test]
fn map_deep_remove_walks_the_key_path() {
    // found nested key empties the child but keeps the parent; a miss anywhere
    // on the path leaves the map untouched.
    assert_eq!(value("map.deep-remove((c: (d: e)), c, d)"), "(c: ())");
    assert_eq!(value("map.deep-remove((c: (d: e, f: g)), c, d)"), "(c: (f: g))");
    assert_eq!(value("map.deep-remove((c: (d: e)), c, e)"), "(c: (d: e))");
    assert_eq!(value("map.deep-remove((c: d), c)"), "()");
}

// ─── round 27: color.hwb (3-arg legacy constructor) ──────────────────────────

#[test]
fn color_hwb_builds_a_legacy_color() {
    // integer-byte results serialize as keyword/#hex, fractional ones as hsl —
    // the same legacy form the goldens use, reproduced via legacy="hwb" + the
    // verbatim HSL triple. Hue units convert to degrees.
    assert_eq!(value("color.hwb(60, 0%, 0%)"), "yellow");
    assert_eq!(value("color.hwb(60, 0%, 20%)"), "#cccc00");
    assert_eq!(value("color.hwb(0deg, 30%, 40%)"), "hsl(0, 33.3333333333%, 45%)");
    // alpha drives the hsla() form.
    assert_eq!(value("color.hwb(180, 30%, 40%, 0.5)"), "hsla(180, 33.3333333333%, 45%, 0.5)");
}

#[test]
fn bare_hwb_is_not_claimed_as_a_builtin() {
    // The bare global `hwb()` must stay unregistered so a modern `hwb(…)` literal
    // is left to the existing Color-4 literal path (which normalizes the hue to
    // `180deg`) rather than the `color.hwb` builtin's legacy-hsl serialization.
    // Registering a bare `hwb` builtin would shadow the literal (round-25 lesson).
    assert_eq!(value("hwb(180 30% 40% / 0.5)"), "hwb(180deg 30% 40% / 0.5)");
}

// ─── round 28: legacy rgb() special-function passthrough ─────────────────────

#[test]
fn rgb_passes_unresolvable_channels_through_verbatim() {
    // A calc()/var()/env()/attr() channel is not resolvable to a byte, so the
    // legacy rgb() returns the rgb(…) call as a literal (comma-joined, keeping
    // the `rgb(` name even when an alpha is present) rather than a Color.
    assert_eq!(value(r#"rgb(string.unquote("calc(1)"), 2, 3)"#), "rgb(calc(1), 2, 3)");
    assert_eq!(value("rgb(var(--foo), 2, 3)"), "rgb(var(--foo), 2, 3)");
    assert_eq!(value("rgb(1, 2, 3, var(--a))"), "rgb(1, 2, 3, var(--a))");
}

#[test]
fn rgb_none_channel_computes_not_passes_through() {
    // `none` is NOT an unresolvable special function — it resolves to 0 and
    // builds a concrete color (this regressed when none was misclassified).
    assert_eq!(value("rgb(255, none, 255)"), "rgb(255, 0, 255)");
    assert_eq!(value("rgb(none, none, none)"), "rgb(0, 0, 0)");
}

// ─── round 29: bare `%` parses as a value token ──────────────────────────────

#[test]
fn bare_percent_is_a_value_token() {
    // A lone `%` is plain-CSS passthrough (the `attr(c, %)` type and `b: %`),
    // distinct from a `50%` percentage (which lexes as a single Number-with-unit).
    assert_eq!(value("%"), "%");
    assert_eq!(value("c %"), "c %");
    assert_eq!(value("% c"), "% c");
    assert_eq!(value("c(%)"), "c(%)");
    // combined with the round-28 rgb() passthrough: an unresolvable attr channel.
    assert_eq!(value("rgb(attr(c, %), 2, 3)"), "rgb(attr(c, %), 2, 3)");
}

/// Round-70(b) (function_exists goldens): `-`/`_` alias equivalence and the
/// `($name, $module)` module-scoped form.
#[test]
fn function_exists_aliases_and_module_form() {
    assert_eq!(
        val_prog("@function a_b() { @return null; } .a { p: meta.function-exists(a-b); }"),
        "true"
    );
    assert_eq!(
        val_prog("@function a-b() { @return null; } .a { p: meta.function-exists(a_b); }"),
        "true"
    );
    assert_eq!(
        val_prog(".a { p: meta.function-exists(\"rgb\", \"list\"); }"),
        "false"
    );
    assert_eq!(
        val_prog(".a { p: meta.function-exists(\"red\", \"color\"); }"),
        "true"
    );
}
