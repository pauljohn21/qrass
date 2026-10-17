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

// ─── 8.2 map.* ──────────────────────────────────────────────────────────────────

#[test]
fn map_get_nested() {
    assert_eq!(value("map.get((a: (b: 42)), a, b)"), "42");
}

#[test]
fn map_get_missing_is_null() {
    // null drops the declaration entirely.
    let out = css(".a { p: map.get((a: 1), zzz); }");
    assert!(!out.contains("p:"), "{out}");
}

#[test]
fn map_has_key() {
    assert_eq!(value("map.has-key((a: 1, b: 2), a)"), "true");
    assert_eq!(value("map.has-key((a: 1), zzz)"), "false");
}

#[test]
fn map_keys_and_values() {
    assert_eq!(value("map.keys((a: 1, b: 2))"), "a, b");
    assert_eq!(value("map.values((a: 1, b: 2))"), "1, 2");
}

#[test]
fn map_merge_shallow() {
    assert_eq!(
        value("map.merge((a: 1, b: 2), (b: 3, c: 4))"),
        "(a: 1, b: 3, c: 4)"
    );
}

#[test]
fn map_deep_merge_recursive() {
    let out = value("map.deep-merge((a: (x: 1, y: 2)), (a: (y: 3, z: 4)))");
    assert_eq!(out, "(a: (x: 1, y: 3, z: 4))");
}

#[test]
fn map_set_and_remove() {
    assert_eq!(value("map.set((a: 1), b, 2)"), "(a: 1, b: 2)");
    assert_eq!(value("map.set((a: 1), a, 9)"), "(a: 9)");
    assert_eq!(value("map.remove((a: 1, b: 2), a)"), "(b: 2)");
}

// ─── 8.2 math.* ───────────────────────────────────────────────────────────────

#[test]
fn math_div_serialization_and_units() {
    assert_eq!(value("math.div(1, 3)"), "0.3333333333");
    assert_eq!(value("math.div(10px, 2)"), "5px");
}

#[test]
fn math_rounding() {
    assert_eq!(value("math.round(2.6)"), "3");
    assert_eq!(value("math.ceil(2.1)"), "3");
    assert_eq!(value("math.floor(2.9)"), "2");
    assert_eq!(value("math.abs(-4)"), "4");
}

#[test]
fn math_min_max_variadic() {
    assert_eq!(value("math.min(3, 1, 2)"), "1");
    assert_eq!(value("math.max(3, 1, 2)"), "3");
}

#[test]
fn math_percentage_and_power() {
    assert_eq!(value("math.percentage(0.25)"), "25%");
    assert_eq!(value("math.pow(2, 10)"), "1024");
    assert_eq!(value("math.sqrt(16)"), "4");
    assert_eq!(value("math.log(math.pow(2, 3), 2)"), "3");
}

#[test]
fn math_constants_resolve_without_use() {
    // `math.$pi` / `math.$e` come from the built-in module constant table.
    assert!(
        value("math.$pi").starts_with("3.141592653"),
        "{}",
        value("math.$pi")
    );
    assert!(
        value("math.$e").starts_with("2.71828"),
        "{}",
        value("math.$e")
    );
}

#[test]
fn math_trig_direct_returns_unitless_ratio() {
    // Angles accept a unit (deg/grad/rad/turn) or a unitless radian measure.
    assert_eq!(value("math.sin(1deg)"), "0.0174524064");
    assert_eq!(value("math.sin(1rad)"), "0.8414709848");
    assert_eq!(value("math.sin(1)"), "0.8414709848");
    assert_eq!(value("math.sin(1turn)"), "0");
    assert_eq!(value("math.cos(1rad)"), "0.5403023059");
    assert_eq!(value("math.tan(1rad)"), "1.5574077247");
}

#[test]
fn math_inverse_trig_returns_degrees() {
    assert_eq!(value("math.asin(0.5)"), "30deg");
    assert_eq!(value("math.asin(1)"), "90deg");
    assert_eq!(value("math.atan(1)"), "45deg");
    assert_eq!(value("math.atan2(1, 1)"), "45deg");
}

#[test]
fn math_non_finite_wraps_in_calc() {
    // A non-finite number is not a legal bare CSS number: the goldens emit it in
    // `calc()`. `math.div(1, 0)` is infinity, not an error.
    assert_eq!(value("math.div(1, 0)"), "calc(infinity)");
    assert_eq!(value("math.sin(math.div(1, 0))"), "calc(NaN)");
    assert_eq!(value("math.asin(2)"), "calc(NaN * 1deg)");
}

#[test]
fn math_hypot_unit_clamp_and_predicates() {
    assert_eq!(value("math.hypot(3, 4)"), "5");
    assert_eq!(value("math.hypot(3cm, 4mm * 10)"), "5cm");
    assert_eq!(value("math.clamp(0, 2, 1)"), "1");
    assert_eq!(value("math.is-unitless(1px)"), "false");
    assert_eq!(value("math.unit(1px)"), "\"px\"");
    assert_eq!(value("math.unit(1)"), "\"\"");
    assert_eq!(value("math.compatible(1px, 1in)"), "true");
    assert_eq!(value("math.compatible(1px, 1s)"), "false");
}

// ─── 8.2 color.* ──────────────────────────────────────────────────────────────

#[test]
fn color_channel_reads_rgb() {
    assert_eq!(value("color.channel(#ff8000, \"red\")"), "255");
    assert_eq!(value("color.channel(#ff8000, \"green\")"), "128");
    assert_eq!(value("color.channel(#ff8000, \"blue\")"), "0");
}

/// The deprecated single-channel accessors (`color.red` …) are shims over
/// `color.channel` reading one channel in its natural space; `whiteness`/
/// `blackness` recover exactly from the RGB extremes.
#[test]
fn color_channel_accessors() {
    assert_eq!(value("color.red(#ff8000)"), "255");
    assert_eq!(value("color.green(#ff8000)"), "128");
    assert_eq!(value("color.blue(#ff8000)"), "0");
    assert_eq!(value("color.hue(#ff0000)"), "0deg");
    assert_eq!(value("color.saturation(#ff0000)"), "100%");
    assert_eq!(value("color.lightness(#ff0000)"), "50%");
    // black = hwb(_, 0%, 100%), white = hwb(_, 0%, 0%); gray 50/50.
    assert_eq!(value("color.blackness(black)"), "100%");
    assert_eq!(value("color.blackness(white)"), "0%");
    assert_eq!(value("color.whiteness(black)"), "0%");
    assert_eq!(value("color.whiteness(#808080)"), "50.1960784314%");
    // Named-argument form folds `$color` into slot 0 like every other builtin.
    assert_eq!(value("color.red($color: #ed0000)"), "237");
}

#[test]
fn color_constructors() {
    // Explicit rgb()/hsl() constructors serialize in the functional form (the goldens
    // preserves representation origin: only a `#rrggbb` literal emits hex).
    assert_eq!(value("color.rgb(255, 0, 0)"), "rgb(255, 0, 0)");
    assert_eq!(value("color.hsl(0, 100%, 50%)"), "rgb(255, 0, 0)");
}

#[test]
fn color_adjust_scale_change() {
    assert_eq!(value("color.adjust(#000000, $red: 128)"), "#800000");
    assert_eq!(value("color.change(#000000, $red: 200)"), "#c80000");
    assert_eq!(value("color.scale(#000000, $red: 50%)"), "#800000");
}

#[test]
fn color_mix_and_meta() {
    assert_eq!(value("color.mix(#ff0000, #0000ff)"), "#800080");
    assert_eq!(value("color.alpha(#ff0000)"), "1");
    assert_eq!(value("color.ie-hex-str(#ff0000)"), "#FFFF0000");
}

// ─── 8.3 string.* ─────────────────────────────────────────────────────────────

#[test]
fn string_length_slice_index() {
    assert_eq!(value("string.length(\"hello\")"), "5");
    assert_eq!(value("string.slice(\"hello\", 2, 4)"), "ell");
    assert_eq!(value("string.index(\"hello\", \"ll\")"), "3");
}

#[test]
fn string_case_and_quotes() {
    assert_eq!(value("string.to-upper-case(\"abc\")"), "ABC");
    assert_eq!(value("string.to-lower-case(\"ABC\")"), "abc");
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
    assert_eq!(value("meta.keyword-get((a: 1, b: 2), b)"), "2");
    assert_eq!(value("meta.type-of(meta.keywords((a: 1)))"), "map");
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
    assert_eq!(val_prog(".a { p: meta.content-exists(); }"), "false");
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
    assert_eq!(value("string.to-upper-case($string: \"hi\")"), "HI");
}

/// A named argument mixed with a positional one into the *same* slot is a call
/// error (golden: "was passed both positionally and by name"), and an unknown
/// name on a fixed-arity builtin is rejected — but a variadic builtin leaves
/// undeclared names in the named map instead of erroring.
#[test]
fn named_argument_conflicts_and_unknowns() {
    let err = eval_err(".a { p: math.abs(-3, $number: 3); }");
    assert!(err.contains("both positionally and by name"), "{err}");
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
