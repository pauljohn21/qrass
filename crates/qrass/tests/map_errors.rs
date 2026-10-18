//! `map.*` argument-count and type-error contract, read off the sass-spec
//! `core_functions/map/*` goldens. Kept in its own file so the builtins suite
//! stays under the 500-line budget while the map error surface grows.
//!
//! Builtins dispatch directly (no `@use` needed for namespaced function calls),
//! so these exercise the pure registry path plus `builtins::fold_named`.

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

// ─── map.get / map.has-key: min 2 (map + key), $map must be a map ──────────────

#[test]
fn map_get_requires_key_and_map() {
    // Nested descent still works.
    assert_eq!(value("map.get((a: (b: c)), a, b)"), "c");
    let msg = eval_err(".a { p: map.get(1, 2); }");
    assert!(msg.contains("$map: 1 is not a map."), "{msg}");
    // `get($map, $key, $keys…)`: a lone map is missing `$key`, not a silent null.
    let msg = eval_err(".a { p: map.get((a: 1)); }");
    assert!(msg.contains("Missing argument $key."), "{msg}");
}

#[test]
fn map_has_key_requires_key_and_map() {
    assert_eq!(value("map.has-key((a: (b: c)), a, b)"), "true");
    let msg = eval_err(".a { p: map.has-key(1, 2); }");
    assert!(msg.contains("$map: 1 is not a map."), "{msg}");
    let msg = eval_err(".a { p: map.has-key((a: 1)); }");
    assert!(msg.contains("Missing argument $key."), "{msg}");
}

// ─── map.keys / values / remove / deep-remove: $map type error wording ─────────

#[test]
fn map_single_map_args_reject_non_maps() {
    let msg = eval_err(".a { p: map.keys(1); }");
    assert!(msg.contains("$map: 1 is not a map."), "{msg}");
    let msg = eval_err(".a { p: map.values(1); }");
    assert!(msg.contains("$map: 1 is not a map."), "{msg}");
    let msg = eval_err(".a { p: map.remove(1); }");
    assert!(msg.contains("$map: 1 is not a map."), "{msg}");
    let msg = eval_err(".a { p: map.deep-remove(1, 2); }");
    assert!(msg.contains("$map: 1 is not a map."), "{msg}");
}

#[test]
fn map_remove_positional_and_named_collides() {
    // A named `$key` that also fills a positional slot is a call error, with
    // dart's exact wording (builtins::fold_named).
    let msg = eval_err(".a { p: map.remove((c: d, e: f), c, $key: e); }");
    assert!(
        msg.contains("Argument $key was passed both by position and by name."),
        "{msg}"
    );
}

// ─── map.deep-merge: fixed 2-arity, empty `()` merged as an empty map ─────────

#[test]
fn map_deep_merge_arity() {
    let msg = eval_err(".a { p: map.deep-merge(1, (c: d)); }");
    assert!(msg.contains("$map1: 1 is not a map."), "{msg}");
    let msg = eval_err(".a { p: map.deep-merge((c: d), 1); }");
    assert!(msg.contains("$map2: 1 is not a map."), "{msg}");
    let msg = eval_err(".a { p: map.deep-merge((c: d)); }");
    assert!(msg.contains("Missing argument $map2."), "{msg}");
    let msg = eval_err(".a { p: map.deep-merge((c: d), (e: f), (g: h)); }");
    assert!(
        msg.contains("Only 2 arguments allowed, but 3 were passed."),
        "{msg}"
    );
}

#[test]
fn map_deep_merge_empty_value_keeps_left() {
    // The empty `()` on the right is an empty map; merging it leaves the left's
    // nested map intact (rather than replacing it with `()`).
    assert_eq!(
        value("map.deep-merge((c: (d: e)), (c: ()))"),
        "(c: (d: e))"
    );
    assert_eq!(
        value("map.deep-merge((a: (x: 1, y: 2)), (a: (y: 3, z: 4)))"),
        "(a: (x: 1, y: 3, z: 4))"
    );
}
