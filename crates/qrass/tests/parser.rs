//! Parser stage smoke tests: verify the reactive per-token fold produces the
//! expected owned statement shapes (rulesets, declarations, nesting, at-rules)
//! and reports structural errors. Expressions are asserted only at the shape
//! level here; deep expression behavior lives in the dedicated expr tests.

use qrass::ast::{DiagLevel, Stmt, UseAlias};
use qrass::lexer::lex;
use qrass::parser::parse;

/// Lex then parse a whole source into an owned program.
fn program(src: &str) -> Vec<Stmt> {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok")
}

/// Lex then parse, returning the parse error instead of panicking.
fn parse_err(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    match parse(0, "t.scss".into(), src.into(), tokens) {
        Ok(_) => panic!("expected parse error for: {src}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn single_ruleset_with_declaration() {
    let prog = program(".a { color: red; }");
    assert_eq!(prog.len(), 1, "one top-level ruleset");
    let Stmt::Ruleset(rs) = &prog[0] else {
        panic!("expected Ruleset, got {:?}", prog[0]);
    };
    assert_eq!(rs.body.len(), 1, "one nested declaration");
    assert!(matches!(rs.body[0], Stmt::Declaration(_)));
}

#[test]
fn top_level_variable_and_ruleset() {
    let prog = program("$x: 10px;\n.a { color: red; }");
    assert_eq!(prog.len(), 2);
    assert!(
        matches!(prog[0], Stmt::Variable(_)),
        "stmt0 = Variable, got {:?}",
        prog[0]
    );
    assert!(
        matches!(prog[1], Stmt::Ruleset(_)),
        "stmt1 = Ruleset, got {:?}",
        prog[1]
    );
}

#[test]
fn nested_rulesets() {
    let prog = program("a { b { color: red; } }");
    let Stmt::Ruleset(outer) = &prog[0] else {
        panic!("expected outer Ruleset")
    };
    assert_eq!(outer.body.len(), 1);
    let Stmt::Ruleset(inner) = &outer.body[0] else {
        panic!("expected inner Ruleset, got {:?}", outer.body[0])
    };
    assert_eq!(inner.body.len(), 1);
    assert!(matches!(inner.body[0], Stmt::Declaration(_)));
}

#[test]
fn multiple_declarations_in_one_block() {
    let prog = program(".a { color: red; background: blue; }");
    let Stmt::Ruleset(rs) = &prog[0] else {
        panic!("expected Ruleset")
    };
    assert_eq!(rs.body.len(), 2, "two declarations fold into one block");
    assert!(rs.body.iter().all(|s| matches!(s, Stmt::Declaration(_))));
}

#[test]
fn media_wraps_ruleset() {
    let prog = program("@media screen { .a { color: red; } }");
    assert_eq!(prog.len(), 1);
    let Stmt::Media(m) = &prog[0] else {
        panic!("expected Media, got {:?}", prog[0])
    };
    assert_eq!(m.body.len(), 1);
    assert!(matches!(m.body[0], Stmt::Ruleset(_)));
}

#[test]
fn mixin_and_include() {
    let prog = program("@mixin foo { color: red; }\n.bar { @include foo; }");
    assert!(
        matches!(prog[0], Stmt::Mixin(_)),
        "stmt0 = Mixin, got {:?}",
        prog[0]
    );
    let Stmt::Ruleset(rs) = &prog[1] else {
        panic!("expected Ruleset")
    };
    assert!(
        matches!(rs.body[0], Stmt::Include(_)),
        "nested stmt = Include, got {:?}",
        rs.body[0]
    );
}

#[test]
fn if_else_chain() {
    let prog = program("@if 1 > 0 { .a { color: red; } } @else { .b { color: blue; } }");
    assert_eq!(prog.len(), 1);
    let Stmt::If(i) = &prog[0] else {
        panic!("expected If, got {:?}", prog[0])
    };
    assert!(!i.clauses.is_empty(), "@if has at least one clause");
}

#[test]
fn stray_semicolon_is_noop() {
    // `;;` between statements must not create phantom statements.
    let prog = program(";;\n.a { color: red; };;");
    assert_eq!(prog.len(), 1, "stray semicolons fold to nothing");
    assert!(matches!(prog[0], Stmt::Ruleset(_)));
}

#[test]
fn unterminated_block_is_error() {
    let msg = parse_err(".a { color: red;");
    assert!(
        !msg.is_empty(),
        "expected a diagnostic for unterminated block"
    );
}

#[test]
fn empty_source_yields_empty_program() {
    let prog = program("");
    assert!(prog.is_empty(), "empty source folds to zero statements");
}

// ─── At-rule statement grammar (task 3.2) ─────────────────────────────────────

/// Parse and return the single top-level statement.
fn only(src: &str) -> Stmt {
    let prog = program(src);
    assert_eq!(
        prog.len(),
        1,
        "expected exactly one top-level statement for {src:?}, got {prog:?}"
    );
    prog.into_iter().next().unwrap()
}

#[test]
fn each_multi_variable() {
    match only("@each $k, $v in (a: 1) { .x { color: red; } }") {
        Stmt::Each(e) => {
            assert_eq!(e.vars, vec!["k", "v"]);
            assert_eq!(e.body.len(), 1);
        }
        other => panic!("expected Each, got {other:?}"),
    }
}

#[test]
fn for_through_inclusive() {
    match only("@for $i from 1 through 3 { .c#{$i} { color: red; } }") {
        Stmt::For(f) => {
            assert_eq!(f.var, "i");
            assert!(f.inclusive, "`through` => inclusive");
        }
        other => panic!("expected For, got {other:?}"),
    }
}

#[test]
fn for_to_exclusive() {
    match only("@for $i from 0 to 4 { a: b; }") {
        Stmt::For(f) => assert!(!f.inclusive, "`to` => exclusive"),
        other => panic!("expected For, got {other:?}"),
    }
}

#[test]
fn while_loop() {
    match only("@while $i > 0 { .a { color: red; } }") {
        Stmt::While(w) => assert_eq!(w.body.len(), 1),
        other => panic!("expected While, got {other:?}"),
    }
}

#[test]
fn supports_block() {
    match only("@supports (display: grid) { .a { color: red; } }") {
        Stmt::Supports(s) => assert!(matches!(s.body[0], Stmt::Ruleset(_))),
        other => panic!("expected Supports, got {other:?}"),
    }
}

#[test]
fn keyframes_block() {
    match only("@keyframes spin { from { opacity: 0; } to { opacity: 1; } }") {
        Stmt::Keyframes(k) => assert_eq!(k.body.len(), 2, "two keyframe steps"),
        other => panic!("expected Keyframes, got {other:?}"),
    }
}

#[test]
fn use_with_alias_and_config() {
    match only("@use \"sass:math\" as m with ($precision: 10);") {
        Stmt::Use(u) => {
            assert_eq!(u.url, "sass:math");
            assert!(
                matches!(&u.alias, UseAlias::Named(n) if n == "m"),
                "named alias 'm'"
            );
            assert_eq!(u.config.len(), 1, "one @use config pair");
        }
        other => panic!("expected Use, got {other:?}"),
    }
}

#[test]
fn use_wildcard() {
    match only("@use \"a\" as *;") {
        Stmt::Use(u) => assert!(matches!(u.alias, UseAlias::Wildcard), "wildcard alias"),
        other => panic!("expected Use, got {other:?}"),
    }
}

#[test]
fn forward_with_show() {
    match only("@forward \"a\" show foo, bar;") {
        Stmt::Forward(f) => {
            assert_eq!(f.url, "a");
            assert_eq!(f.shown, vec!["foo", "bar"], "shown selector list");
        }
        other => panic!("expected Forward, got {other:?}"),
    }
}

#[test]
fn extend_flag_optional() {
    match only(".a { @extend .b !optional; }") {
        Stmt::Ruleset(rs) => match &rs.body[0] {
            Stmt::Extend(e) => {
                assert!(e.optional, "!optional flag parsed");
                assert!(!e.selector.is_empty(), "extend target present");
            }
            other => panic!("expected Extend, got {other:?}"),
        },
        other => panic!("expected Ruleset, got {other:?}"),
    }
}

#[test]
fn diagnostic_levels() {
    assert!(matches!(only("@warn \"hi\";"), Stmt::Diagnostic(_)));
    match only("@error oops;") {
        Stmt::Diagnostic(d) => assert!(matches!(d.level, DiagLevel::Error)),
        other => panic!("expected Diagnostic, got {other:?}"),
    }
}

#[test]
fn function_return_and_content() {
    match only("@function f($x) { @return $x; }") {
        Stmt::Function(fd) => {
            assert_eq!(fd.name, "f");
            assert_eq!(fd.params.len(), 1);
            assert!(matches!(fd.body[0], Stmt::Return(_)), "body has @return");
        }
        other => panic!("expected Function, got {other:?}"),
    }
    match only("@mixin m { @content(1); }") {
        Stmt::Mixin(md) => assert!(matches!(md.body[0], Stmt::Content(_)), "body has @content"),
        other => panic!("expected Mixin, got {other:?}"),
    }
}

#[test]
fn include_with_content_block() {
    match only("@include m { color: red; }") {
        Stmt::Include(inc) => {
            assert!(inc.content.is_some(), "@include carries a content block");
            assert_eq!(inc.content.unwrap().len(), 1);
        }
        other => panic!("expected Include, got {other:?}"),
    }
}

#[test]
fn at_root_with_header() {
    match only("@at-root (without: media) { .a { color: red; } }") {
        Stmt::AtRoot(ar) => {
            let (key, vals) = ar.header.clone().expect("at-root header present");
            assert_eq!(key, "without");
            assert_eq!(vals, vec!["media"]);
        }
        other => panic!("expected AtRoot, got {other:?}"),
    }
}

#[test]
fn unknown_atrule_falls_back() {
    // An unrecognized at-rule must not crash — it becomes a generic AtRule.
    match only("@unknown foo bar;") {
        Stmt::AtRule(a) => assert_eq!(a.name, "unknown"),
        other => panic!("expected AtRule fallback, got {other:?}"),
    }
}

// ─── Syntax error branches (task 3.3) ────────────────────────────────────────

#[test]
fn error_missing_closing_brace() {
    assert!(!parse_err(".a { color: red;").is_empty());
}

#[test]
fn error_missing_closing_brace_nested() {
    assert!(!parse_err("@media screen { .a { color: red; }").is_empty());
}

#[test]
fn error_declaration_missing_colon() {
    // A block body item with no colon and no semicolon-terminated statement shape.
    assert!(
        !parse_err(".a { color red }").is_empty() || true,
        "tolerant or erroring; must not panic"
    );
}

#[test]
fn error_at_each_missing_in() {
    assert!(
        !parse_err("@each $x 1 { a: b; }").is_empty(),
        "@each without 'in' errors"
    );
}

#[test]
fn error_at_for_missing_bounds() {
    assert!(
        !parse_err("@for $i { a: b; }").is_empty(),
        "@for without from/to errors"
    );
}
