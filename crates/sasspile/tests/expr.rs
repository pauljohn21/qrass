//! Expression rendering tests (from structured tokens).

use sasspile::expr::Env;
use sasspile::expr::{Expr, Op};
use sasspile::lexer::Tok;
use sasspile::value::Value;
use sasspile::value_parse;

fn render(toks: &[Tok], env: &Env) -> Option<String> {
    Expr::render(toks, env)
}

#[test]
fn bare_var_resolves_or_passes_through() {
    let mut env = Env::new();
    env.insert("x".into(), Value::Raw("3px".into()));
    assert_eq!(render(&[Tok::Var("x".into())], &env).as_deref(), Some("3px"));
    let empty = Env::new();
    assert_eq!(render(&[Tok::Var("y".into())], &empty).as_deref(), Some("$y"));
}

#[test]
fn interp_glues_and_undefined_drops() {
    let mut env = Env::new();
    env.insert("k".into(), Value::Raw("2".into()));
    let toks = [Tok::Word("col-".into()), Tok::Interp(vec![Tok::Var("k".into())])];
    assert_eq!(render(&toks, &env).as_deref(), Some("col-2"));
    let bad = [Tok::Word("x".into()), Tok::Interp(vec![Tok::Var("nope".into())])];
    assert_eq!(render(&bad, &env), None);
}

#[test]
fn comma_and_space_normalise() {
    let env = Env::new();
    let toks = [Tok::Word("a".into()), Tok::Punct(','), Tok::Word("b".into())];
    assert_eq!(render(&toks, &env).as_deref(), Some("a, b"));
    let sp = [Tok::Word("1px   2px".into())];
    assert_eq!(render(&sp, &env).as_deref(), Some("1px 2px"));
}

// ---- task 1.2: value expression parser + arithmetic / comparison / lazy / builtins ----

/// Parse a value string and lazily eval it, returning CSS or None (unparseable / error).
fn v(s: &str, env: &Env) -> Option<String> {
    value_parse::parse(s).and_then(|e| e.eval(env).ok()).map(|v| v.to_css())
}

#[test]
fn arithmetic_add_and_units() {
    let env = Env::new();
    assert_eq!(v("10px + 5px", &env).as_deref(), Some("15px"));
    assert_eq!(v("3 + 4", &env).as_deref(), Some("7"));
    assert_eq!(v("10px + 3", &env).as_deref(), Some("13px")); // unitless side adopts unit
    assert_eq!(v("5s - 2s", &env).as_deref(), Some("3s"));
    assert_eq!(v("5px - 2", &env).as_deref(), Some("3px"));
    // left-assoc: (1 + 2) - 3 == 0, not 1 + (2 - 3)
    assert_eq!(v("1 + 2 - 3", &env).as_deref(), Some("0"));
    // incompatible units -> error -> None (caller keeps verbatim)
    assert_eq!(v("5px + 3s", &env), None);
}

#[test]
fn arithmetic_mul_div_via_builtin_or_direct() {
    let env = Env::new();
    // `*` bails in the auto parser (CSS-shorthand safety) -> verbatim
    assert_eq!(v("2 * 3px", &env), None);
    // math.div and Mul work through builtins / direct Expr
    assert_eq!(v("math.div(6, 2)", &env).as_deref(), Some("3"));
    assert_eq!(v("math.div(6px, 2px)", &env).as_deref(), Some("3"));
    let mul = Expr::Bin(
        Box::new(Expr::Lit(Value::Number(2.0, String::new()))),
        Op::Mul,
        Box::new(Expr::Lit(Value::Number(3.0, "px".into()))),
    );
    assert_eq!(mul.eval(&env).ok().map(|v| v.to_css()).as_deref(), Some("6px"));
}

#[test]
fn comparisons_yield_bool() {
    let env = Env::new();
    assert_eq!(v("5 > 3", &env).as_deref(), Some("true"));
    assert_eq!(v("5 <= 5", &env).as_deref(), Some("true"));
    assert_eq!(v("2px > 10px", &env).as_deref(), Some("false"));
    assert_eq!(v("5px > 3s", &env), None); // cross-unit compare -> error
    assert_eq!(v("abc > abd", &env).as_deref(), Some("false")); // lexicographic: "abc" < "abd"
}

#[test]
fn lazy_and_or_if() {
    let env = Env::new();
    assert_eq!(v("true and false", &env).as_deref(), Some("false"));
    assert_eq!(v("false and true", &env).as_deref(), Some("false"));
    assert_eq!(v("true or false", &env).as_deref(), Some("true"));
    assert_eq!(v("if(true, 1px, 2px)", &env).as_deref(), Some("1px"));
    assert_eq!(v("if(false, 1px, 2px)", &env).as_deref(), Some("2px"));
}

#[test]
fn builtin_functions() {
    let env = Env::new();
    assert_eq!(v("abs(-3px)", &env).as_deref(), Some("3px"));
    assert_eq!(v("min(3, 1, 2)", &env).as_deref(), Some("1"));
    assert_eq!(v("max(1, 5, 2)", &env).as_deref(), Some("5"));
    assert_eq!(v("foo(1, 2)", &env), None); // unknown name -> Err -> None
}

#[test]
fn unsafe_or_verbatim_values_bail() {
    let env = Env::new();
    assert_eq!(v("12px/1.5", &env), None);
    assert_eq!(v("1 / 2", &env), None);
    assert_eq!(v("a % b", &env), None);
    assert_eq!(v("#{$x}", &env), None);
    assert_eq!(v("url(http://x)", &env), None);
    assert_eq!(v("1px 2px", &env), None);
    assert_eq!(v("red", &env).as_deref(), Some("red"));
}

#[test]
fn vars_in_arithmetic() {
    let mut env = Env::new();
    env.insert("w".into(), Value::Number(3.0, "px".into()));
    assert_eq!(v("$w + 2px", &env).as_deref(), Some("5px"));
    assert_eq!(v("$nope + 2px", &env), None); // unbound -> verbatim $name -> non-numeric -> None
}

#[test]
fn math_domain_functions() {
    // gold values from sass-spec/spec/core_functions/math/*.hrx
    let env = Env::new();
    assert_eq!(v("math.round(1)", &env).as_deref(), Some("1"));
    assert_eq!(v("math.round(2.9)", &env).as_deref(), Some("3"));
    assert_eq!(v("math.round(16.5)", &env).as_deref(), Some("17"));
    assert_eq!(v("math.round(-5.4)", &env).as_deref(), Some("-5"));
    assert_eq!(v("math.ceil(1.2)", &env).as_deref(), Some("2"));
    assert_eq!(v("math.floor(1.9)", &env).as_deref(), Some("1"));
    assert_eq!(v("math.round(10.5px)", &env).as_deref(), Some("11px")); // unit preserved
    assert_eq!(v("math.sqrt(9)", &env).as_deref(), Some("3"));
    assert_eq!(v("math.pow(2, 10)", &env).as_deref(), Some("1024"));
    assert_eq!(v("math.abs(-3px)", &env).as_deref(), Some("3px"));
    assert_eq!(v("math.min(3, 1, 2)", &env).as_deref(), Some("1"));
    assert_eq!(v("math.max(1, 5, 2)", &env).as_deref(), Some("5"));
    // sqrt rejects units (Sass semantics) -> None (verbatim)
    assert_eq!(v("math.sqrt(9px)", &env), None);
}

#[test]
fn string_domain_functions() {
    // gold shapes from sass-spec/spec/core_functions/string/*.hrx
    let env = Env::new();
    assert_eq!(v("to-upper-case(\"foo\")", &env).as_deref(), Some("\"FOO\""));
    assert_eq!(v("to-lower-case(\"FOO\")", &env).as_deref(), Some("\"foo\""));
    assert_eq!(v("quote(bar)", &env).as_deref(), Some("\"bar\""));
    assert_eq!(v("unquote(\"baz\")", &env).as_deref(), Some("baz"));
    assert_eq!(v("length(\"hello\")", &env).as_deref(), Some("5"));
    assert_eq!(v("string.length(hello)", &env).as_deref(), Some("5"));
}
