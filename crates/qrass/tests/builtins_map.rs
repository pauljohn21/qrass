//! `map.*` builtin tests (split out of `builtins.rs` to keep each file within the
//! 500-line budget). Namespaced calls dispatch directly (no `@use`).

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
fn map_merge_nested_keypath() {
    // `$map1, $keys…, $map2`: shallow-merge `$map2` at the nested key path.
    assert_eq!(
        value("map.merge((c: (d: e, f: g)), c, (1: 2))"),
        "(c: (d: e, f: g, 1: 2))"
    );
    // Overriding collisions keep the leaf's order.
    assert_eq!(
        value("map.merge((c: (d: e, f: g)), c, (d: 1, f: 2))"),
        "(c: (d: 1, f: 2))"
    );
    // A missing or non-map step fabricates the intermediate map.
    assert_eq!(
        value("map.merge((c: 1), c, d, (e: f))"),
        "(c: (d: (e: f)))"
    );
    assert_eq!(value("map.merge((c: 1), c, (d: e))"), "(c: (d: e))");
}

#[test]
fn map_merge_named_and_errors() {
    assert_eq!(
        value("map.merge($map1: (c: d), $map2: (1: 2))"),
        "(c: d, 1: 2)"
    );
    let msg = eval_err(".a { p: map.merge(1, (c: d)); }");
    assert!(msg.contains("$map1: 1 is not a map."), "{msg}");
    let msg = eval_err(".a { p: map.merge((c: d), e, 1); }");
    assert!(msg.contains("$map2: 1 is not a map."), "{msg}");
    let msg = eval_err(".a { p: map.merge((c: d)); }");
    assert!(msg.contains("Expected $args to contain a key."), "{msg}");
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

#[test]
fn map_set_nested_keypath() {
    // `$map, $keys…, $value`: set the final argument at the nested leaf,
    // fabricating intermediate maps for a missing or non-map step.
    assert_eq!(
        value("map.set((c: ()), c, d, e, f)"),
        "(c: (d: (e: f)))"
    );
    assert_eq!(value("map.set((c: (d: e)), c, d, f)"), "(c: (d: f))");
    assert_eq!(
        value("map.set((c: (d: e)), c, f, g)"),
        "(c: (d: e, f: g))"
    );
    assert_eq!(value("map.set((c: 1), c, d, f)"), "(c: (d: f))");
    // named form still binds map/key/value to the first, second, last slots.
    assert_eq!(
        value("map.set($map: (c: d), $key: c, $value: e)"),
        "(c: e)"
    );
    let msg = eval_err(".a { p: map.set(1, c, d); }");
    assert!(msg.contains("$map: 1 is not a map."), "{msg}");
    let msg = eval_err(".a { p: map.set((c: d)); }");
    assert!(msg.contains("Expected $args to contain a key."), "{msg}");
    let msg = eval_err(".a { p: map.set((c: d), e); }");
    assert!(msg.contains("Expected $args to contain a value."), "{msg}");
}
