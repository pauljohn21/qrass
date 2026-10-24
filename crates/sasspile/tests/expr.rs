//! Unit tests for the expression evaluator foundation (standalone; the pipeline is
//! not yet wired to it, so these exercise `Expr::eval` directly).

use sasspile::{Env, Expr, Value};

#[test]
fn and_short_circuits_undefined_rhs() {
    // `false and #{$undefined}` → false, never evaluating the undefined variable.
    let env = Env::new();
    let expr = Expr::And(
        Box::new(Expr::Lit(Value::Bool(false))),
        Box::new(Expr::Interp(Box::new(Expr::Var("nope".into())))),
    );
    assert_eq!(expr.eval(&env).unwrap(), Value::Bool(false));
}

#[test]
fn or_short_circuits_truthy_rhs() {
    let env = Env::new();
    let expr = Expr::Or(
        Box::new(Expr::Lit(Value::Bool(true))),
        Box::new(Expr::Interp(Box::new(Expr::Var("nope".into())))),
    );
    assert_eq!(expr.eval(&env).unwrap(), Value::Bool(true));
}

#[test]
fn if_evaluates_only_the_taken_branch() {
    let env = Env::new();
    // if(false, <undefined>, Raw("fallback")) → the else branch, no error.
    let expr = Expr::If(
        Box::new(Expr::Lit(Value::Bool(false))),
        Box::new(Expr::Interp(Box::new(Expr::Var("nope".into())))),
        Box::new(Expr::Lit(Value::Raw("fallback".into()))),
    );
    assert_eq!(expr.eval(&env).unwrap(), Value::Raw("fallback".into()));
}

#[test]
fn undefined_variable_is_an_error() {
    let env = Env::new();
    assert!(Expr::Var("x".into()).eval(&env).is_err());
}

#[test]
fn variable_resolves_from_env() {
    let mut env = Env::new();
    env.insert("size".into(), Value::Raw("10px".into()));
    assert_eq!(
        Expr::Var("size".into()).eval(&env).unwrap(),
        Value::Raw("10px".into())
    );
}

#[test]
fn concat_joins_parts_with_spaces() {
    let env = Env::new();
    let expr = Expr::Concat(vec![
        Expr::Lit(Value::Raw("a".into())),
        Expr::Interp(Box::new(Expr::Lit(Value::Raw("b".into())))),
    ]);
    assert_eq!(expr.eval(&env).unwrap().to_css(), "a b");
}
