//! Round 31 — `list.zip` inner separator + zero-arity, and `meta.inspect` of the
//! empty list.
//!
//! dart zips into an outer comma list whose inner element lists are always
//! **space**-separated (`zip(1 2 3, c d)` renders `1 c, 2 d`), accepts zero
//! arguments (`zip()` → `()`), and `meta.inspect` of an unbracketed empty list
//! prints the empty paren `()` rather than collapsing to the empty string.

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

fn value(expr: &str) -> String {
    let out = css(&format!("@use \"sass:list\";\n@use \"sass:meta\";\n.a {{ p: {expr}; }}"));
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
fn zip_inner_lists_are_space_separated() {
    // Outer comma list; each inner element list is space-separated regardless of
    // the inputs' own separators, and the result truncates to the shortest list.
    assert_eq!(value("list.zip(1 2 3, c d)"), "1 c, 2 d");
    assert_eq!(value("list.zip(1 2 3 4, c d)"), "1 c, 2 d");
    assert_eq!(value("list.zip((1, 2, 3), 4 5)"), "1 4, 2 5");
    assert_eq!(value("list.zip(1 2 3, c d e, red green blue)"), "1 c red, 2 d green, 3 e blue");
}

#[test]
fn zip_accepts_zero_lists_and_empty_lists_yield_empty() {
    assert_eq!(value("meta.inspect(list.zip())"), "()");
    assert_eq!(value("meta.inspect(list.zip((), 1 2 3))"), "()");
    assert_eq!(value("meta.inspect(list.zip(1 2 3, ()))"), "()");
}

#[test]
fn inspect_renders_the_empty_list_as_the_empty_paren() {
    // An unbracketed empty list has no separator text; dart's source form is ().
    assert_eq!(value("meta.inspect(list.join((), ()))"), "()");
    // A bracketed empty list keeps its brackets, and a single-element list is
    // unaffected by the empty-list special case.
    assert_eq!(value("meta.inspect([1])"), "[1]");
}
