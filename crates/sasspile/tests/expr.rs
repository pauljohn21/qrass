//! Unit tests for the expression evaluator foundation (standalone; the pipeline is
//! not yet wired to it, so these exercise `Expr::eval` directly).

use sasspile::{Env, Expr, Op, Value, parse_expr, value_to_css};

fn ev(src: &str, env: &Env) -> String {
    parse_expr(src).eval(env).map(|v| v.to_css()).unwrap_or_default()
}

#[test]
fn parse_expr_arithmetic() {
    let env = Env::new();
    assert_eq!(ev("1 + 2", &env), "3");
    assert_eq!(ev("2 * 3px", &env), "6px");
    assert_eq!(ev("10px - 3px", &env), "7px");
    assert_eq!(ev("2 + 3 * 4", &env), "14"); // precedence
    assert_eq!(ev("7 % 3", &env), "1");
}

#[test]
fn parse_expr_leaves_non_numeric_verbatim() {
    let env = Env::new();
    assert_eq!(ev("c - d", &env), "c - d"); // identifiers untouched
    assert_eq!(ev("1px 2px", &env), "1px 2px"); // space list, not arithmetic
    // a call parses to Func; unknown funcs Err here (render keeps them verbatim)
    assert!(parse_expr("url(http://x)").eval(&env).is_err());
}

#[test]
fn parse_expr_uses_variables() {
    let mut env = Env::new();
    env.insert("a".into(), Value::Number(2.0, "px".into()));
    assert_eq!(ev("$a + 3px", &env), "5px");
}

#[test]
fn parse_expr_builtin_functions() {
    let env = Env::new();
    assert_eq!(ev("math.div(6, 2)", &env), "3");
    assert_eq!(ev("abs(5px)", &env), "5px");
    assert_eq!(ev("ceil(1.2)", &env), "2");
    assert_eq!(ev("floor(1.9)", &env), "1");
}

#[test]
fn parse_expr_unknown_function_errors() {
    let env = Env::new();
    // unknown -> Err, so render keeps the value verbatim (CSS functions untouched)
    assert!(parse_expr("url(a)").eval(&env).is_err());
    assert!(parse_expr("calc(1 + 2)").eval(&env).is_err());
}

#[test]
fn parse_number_with_unit() {
    assert_eq!(Value::parse_number("10px"), Some(Value::Number(10.0, "px".into())));
    assert_eq!(Value::parse_number("-3"), Some(Value::Number(-3.0, String::new())));
    assert_eq!(Value::parse_number("abc"), None);
}

#[test]
fn arith_add_same_unit() {
    let env = Env::new();
    let e = Expr::Bin(
        Op::Add,
        Box::new(Expr::Lit(Value::Number(2.0, "px".into()))),
        Box::new(Expr::Lit(Value::Number(3.0, "px".into()))),
    );
    assert_eq!(e.eval(&env).unwrap().to_css(), "5px");
}

#[test]
fn arith_div_same_unit_is_unitless() {
    let env = Env::new();
    let e = Expr::Bin(
        Op::Div,
        Box::new(Expr::Lit(Value::Number(10.0, "px".into()))),
        Box::new(Expr::Lit(Value::Number(2.0, "px".into()))),
    );
    assert_eq!(e.eval(&env).unwrap().to_css(), "5");
}

#[test]
fn arith_incompatible_units_error() {
    let env = Env::new();
    let e = Expr::Bin(
        Op::Add,
        Box::new(Expr::Lit(Value::Number(1.0, "px".into()))),
        Box::new(Expr::Lit(Value::Number(1.0, "em".into()))),
    );
    assert!(e.eval(&env).is_err());
}

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

#[test]
fn value_var_substitution() {
    let mut env = Env::new();
    env.insert("x".into(), Value::Raw("3px".into()));
    assert_eq!(value_to_css("$x", &env).as_deref(), Some("3px"));
}

#[test]
fn value_unknown_bare_var_passthrough() {
    let env = Env::new();
    assert_eq!(value_to_css("-a-calc($c)", &env).as_deref(), Some("-a-calc($c)"));
}

#[test]
fn value_interp_glues_to_adjacent_text() {
    let mut env = Env::new();
    env.insert("k".into(), Value::Raw("2".into()));
    assert_eq!(value_to_css("col-#{$k}", &env).as_deref(), Some("col-2"));
}

#[test]
fn value_interp_unknown_var_is_none() {
    let env = Env::new();
    assert_eq!(value_to_css("#{$nope}", &env), None);
}

#[test]
fn value_normalises_comma_spacing() {
    let env = Env::new();
    assert_eq!(value_to_css("var(--c ,)", &env).as_deref(), Some("var(--c, )"));
    assert_eq!(value_to_css("a,b", &env).as_deref(), Some("a, b"));
}

#[test]
fn value_collapses_whitespace_and_keeps_url() {
    let env = Env::new();
    assert_eq!(value_to_css("url(http://x)  y", &env).as_deref(), Some("url(http://x) y"));
    assert_eq!(value_to_css("1px    2px", &env).as_deref(), Some("1px 2px"));
}
