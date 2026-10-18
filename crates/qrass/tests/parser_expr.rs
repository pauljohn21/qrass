//! Expression-layer tests (task group 4): operator precedence, unary, calls,
//! namespaced calls, argument tables (positional / named / splat), parameter
//! tables with defaults & rest, map / comma / space lists, paren groups,
//! interpolation, and literals. Exercised through the public `parse` API by
//! embedding an expression as a variable value (`$v: <expr>;`).

use qrass::ast::{MixinDecl, Stmt};
use qrass::ast_expr::{Arg, BinOp, Expr, ListSep, Literal, UnaryOp};
use qrass::lexer::lex;
use qrass::parser::parse;

/// Parse `$v: <expr>;` and return the folded value expression.
fn expr_of(src: &str) -> Expr {
    let full = format!("$v: {src};");
    let tokens = lex(0, "t.scss".into(), full.clone()).expect("lex ok");
    let prog = parse(0, "t.scss".into(), full, tokens).expect("parse ok");
    match &prog[0] {
        Stmt::Variable(v) => v.value.clone(),
        other => panic!("expected Variable, got {other:?}"),
    }
}

/// Parse and expect an error from the value of `$v: <expr>;`.
fn expr_err(src: &str) -> String {
    let full = format!("$v: {src};");
    let tokens = lex(0, "t.scss".into(), full.clone()).expect("lex ok");
    match parse(0, "t.scss".into(), full, tokens) {
        Ok(p) => panic!("expected error for {src:?}, got {p:?}"),
        Err(e) => e.to_string(),
    }
}

fn num(value: f64, unit: Option<&str>) -> Expr {
    Expr::Literal(Literal::Number {
        value,
        unit: unit.map(String::from),
    })
}

// ─── Literals ────────────────────────────────────────────────────────────────

#[test]
fn number_with_unit() {
    assert_eq!(expr_of("10px"), num(10.0, Some("px")));
    assert_eq!(expr_of(".25"), num(0.25, None));
    assert_eq!(expr_of("1e3"), num(1000.0, None));
    assert_eq!(expr_of("50%"), num(50.0, Some("%")));
}

#[test]
fn keyword_literals() {
    assert_eq!(expr_of("true"), Expr::Literal(Literal::Bool(true)));
    assert_eq!(expr_of("false"), Expr::Literal(Literal::Bool(false)));
    assert_eq!(expr_of("null"), Expr::Literal(Literal::Null));
    assert_eq!(
        expr_of("solid"),
        Expr::Literal(Literal::Ident("solid".into()))
    );
}

#[test]
fn color_and_string_literals() {
    assert_eq!(
        expr_of("#0d6efd"),
        Expr::Literal(Literal::Color("#0d6efd".into()))
    );
    assert_eq!(
        expr_of("\"hi\""),
        Expr::Literal(Literal::Str {
            text: "hi".into(),
            quote: qrass::token::Quote::Double
        })
    );
}

#[test]
fn variable_reference() {
    match expr_of("$width") {
        Expr::Variable(name, _) => assert_eq!(name, "width"),
        other => panic!("expected Variable, got {other:?}"),
    }
}

// ─── Operator precedence ─────────────────────────────────────────────────────

#[test]
fn mul_binds_tighter_than_add() {
    // 1 + 2 * 3 → Add(1, Mul(2, 3))
    let e = expr_of("1 + 2 * 3");
    let Expr::Binary {
        op: BinOp::Add,
        left,
        right,
        ..
    } = e
    else {
        panic!("top should be Add, got {e:?}")
    };
    assert_eq!(*left, num(1.0, None));
    assert!(
        matches!(*right, Expr::Binary { op: BinOp::Mul, .. }),
        "right should be Mul, got {right:?}"
    );
}

#[test]
fn parens_override_precedence() {
    // (1 + 2) * 3 → Mul(Paren(Add(1,2)), 3)
    let e = expr_of("(1 + 2) * 3");
    let Expr::Binary {
        op: BinOp::Mul,
        left,
        ..
    } = e
    else {
        panic!("top should be Mul, got {e:?}")
    };
    assert!(
        matches!(*left, Expr::Paren(..)),
        "left should be Paren, got {left:?}"
    );
}

#[test]
fn and_binds_tighter_than_or() {
    // true and false or true → Or(And(true,false), true)
    let e = expr_of("true and false or true");
    let Expr::Binary {
        op: BinOp::Or,
        left,
        ..
    } = e
    else {
        panic!("top should be Or, got {e:?}")
    };
    assert!(
        matches!(*left, Expr::Binary { op: BinOp::And, .. }),
        "left should be And, got {left:?}"
    );
}

#[test]
fn comparison_operator() {
    let e = expr_of("1 > 0");
    assert!(
        matches!(e, Expr::Binary { op: BinOp::Gt, .. }),
        "expected Gt, got {e:?}"
    );
}

#[test]
fn unary_not_and_negation() {
    assert!(matches!(
        expr_of("not true"),
        Expr::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));
    // `-5` is a negative number literal (the lexer folds `-`+digit, matching sass).
    assert_eq!(expr_of("-5"), num(-5.0, None));
    // `-` before a non-number is a unary negation operator.
    match expr_of("-$x") {
        Expr::Unary {
            op: UnaryOp::Neg,
            operand,
            ..
        } => {
            assert!(
                matches!(*operand, Expr::Variable(..)),
                "operand should be Variable, got {operand:?}"
            );
        }
        other => panic!("expected Unary Neg, got {other:?}"),
    }
}

// ─── Lists / maps / interpolation ────────────────────────────────────────────

#[test]
fn space_separated_list() {
    match expr_of("0 auto") {
        Expr::List {
            items,
            sep: ListSep::Space,
            ..
        } => assert_eq!(items.len(), 2),
        other => panic!("expected space List, got {other:?}"),
    }
}

#[test]
fn comma_separated_list() {
    match expr_of("1, 2, 3") {
        Expr::List {
            items,
            sep: ListSep::Comma,
            ..
        } => assert_eq!(items.len(), 3),
        other => panic!("expected comma List, got {other:?}"),
    }
}

#[test]
fn map_literal() {
    match expr_of("(a: 1, b: 2)") {
        Expr::Map { pairs, .. } => {
            assert_eq!(pairs.len(), 2);
            assert_eq!(pairs[1].1, num(2.0, None));
        }
        other => panic!("expected Map, got {other:?}"),
    }
}

#[test]
fn interpolation_in_value() {
    assert!(
        matches!(expr_of("#{$a}px"), Expr::Interpolated { .. }),
        "expected Interpolated"
    );
}

// ─── Calls, arguments, parameters ────────────────────────────────────────────

#[test]
fn bare_function_call() {
    match expr_of("nth($l, 1)") {
        Expr::FunctionCall(f) => {
            assert_eq!(f.name, "nth");
            assert_eq!(f.namespace, None);
            assert_eq!(f.args.len(), 2);
            assert!(matches!(f.args[0], Arg::Positional(Expr::Variable(..))));
        }
        other => panic!("expected FunctionCall, got {other:?}"),
    }
}

#[test]
fn namespaced_call() {
    match expr_of("map.get($m, \"k\")") {
        Expr::FunctionCall(f) => {
            assert_eq!(f.namespace.as_deref(), Some("map"));
            assert_eq!(f.name, "get");
        }
        other => panic!("expected namespaced FunctionCall, got {other:?}"),
    }
}

#[test]
fn namespaced_member_var() {
    match expr_of("vars.$primary") {
        Expr::Member {
            namespace, name, ..
        } => {
            assert_eq!(namespace, "vars");
            assert_eq!(name, "primary", "member name drops the leading $");
        }
        other => panic!("expected Member, got {other:?}"),
    }
}

#[test]
fn named_and_splat_args() {
    match expr_of("f(1, $k: 2, $rest...)") {
        Expr::FunctionCall(call) => {
            assert_eq!(call.args.len(), 3, "args: {:?}", call.args);
            assert!(matches!(call.args[0], Arg::Positional(_)));
            assert!(matches!(&call.args[1], Arg::Named { name, .. } if name == "k"));
            assert!(matches!(call.args[2], Arg::Splat(_)));
        }
        other => panic!("expected FunctionCall, got {other:?}"),
    }
}

#[test]
fn mixin_params_with_default_and_rest() {
    let src = "@mixin m($a, $b: 2, $rest...) { color: red; }";
    let tokens = lex(0, "t.scss".into(), src.into()).expect("lex ok");
    let prog = parse(0, "t.scss".into(), src.into(), tokens).expect("parse ok");
    let Stmt::Mixin(MixinDecl { params, .. }) = &prog[0] else {
        panic!("expected Mixin, got {:?}", prog[0])
    };
    assert_eq!(params.len(), 3);
    assert!(params[0].default.is_none() && !params[0].rest, "$a plain");
    assert!(
        params[1].default.is_some() && !params[1].rest,
        "$b has default"
    );
    assert!(params[2].rest, "$rest is rest");
}

// ─── Errors ──────────────────────────────────────────────────────────────────

#[test]
fn empty_value_is_error() {
    assert!(!expr_err("").is_empty(), "empty value must report an error");
}

#[test]
fn unclosed_call_paren_is_error() {
    assert!(
        !expr_err("nth(1").is_empty(),
        "unclosed '(' must report an error"
    );
}
