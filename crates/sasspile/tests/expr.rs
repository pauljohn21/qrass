//! Expression rendering tests (from structured tokens).

use sasspile::expr::Env;
use sasspile::expr::{call_builtin, Expr, Op};
use sasspile::lexer::Tok;
use sasspile::value::{Sep, Value};
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

fn raw(s: &str) -> Value {
    Value::Raw(s.into())
}
fn list(items: &[&str], sep: Sep) -> Value {
    Value::List(items.iter().map(|s| raw(s)).collect(), sep)
}
fn cb(name: &str, args: Vec<Value>) -> Option<String> {
    call_builtin(name, args).ok().map(|v| v.to_css())
}

#[test]
fn list_domain_functions() {
    // gold semantics from sass-spec/spec/core_functions/list/nth.hrx etc.
    let l = list(&["c", "d"], Sep::Space);
    assert_eq!(cb("list.nth", vec![l.clone(), Value::Number(1.0, String::new())]).as_deref(), Some("c"));
    assert_eq!(cb("list.nth", vec![l.clone(), Value::Number(2.0, String::new())]).as_deref(), Some("d"));
    assert_eq!(cb("list.nth", vec![l.clone(), Value::Number(-1.0, String::new())]).as_deref(), Some("d"));
    assert_eq!(cb("list.nth", vec![l.clone(), Value::Number(0.0, String::new())]), None); // out of range -> Err
    assert_eq!(cb("list.length", vec![l.clone()]).as_deref(), Some("2"));
    assert_eq!(cb("length", vec![l.clone()]).as_deref(), Some("2")); // dispatches on list
    assert_eq!(
        cb("list.set-nth", vec![l.clone(), Value::Number(1.0, String::new()), raw("x")]).as_deref(),
        Some("x d")
    );
    assert_eq!(cb("list.append", vec![l.clone(), raw("e")]).as_deref(), Some("c d e"));
    assert_eq!(
        cb("list.join", vec![l.clone(), list(&["e", "f"], Sep::Space)]).as_deref(),
        Some("c d e f")
    );
    assert_eq!(cb("list.index-of", vec![l.clone(), raw("d")]).as_deref(), Some("2"));
    assert_eq!(cb("list.index-of", vec![l.clone(), raw("z")]).as_deref(), Some("")); // Null -> ""
    assert_eq!(cb("list.separator", vec![l.clone()]).as_deref(), Some("space"));
    assert_eq!(cb("list.separator", vec![list(&["a", "b"], Sep::Comma)]).as_deref(), Some("comma"));
}

fn num(s: &str) -> Value {
    Value::Number(s.parse().unwrap(), String::new())
}
fn map(pairs: &[(&str, &str)]) -> Value {
    Value::Map(pairs.iter().map(|(k, v)| (raw(k), raw(v))).collect())
}

#[test]
fn map_domain_functions() {
    // gold semantics from sass-spec/spec/core_functions/map/*.hrx
    let m = map(&[("a", "1"), ("b", "2")]);
    assert_eq!(cb("map.get", vec![m.clone(), raw("a")]).as_deref(), Some("1"));
    assert_eq!(cb("map.get", vec![m.clone(), raw("z")]).as_deref(), Some("")); // missing -> null
    assert_eq!(cb("map.has-key", vec![m.clone(), raw("b")]).as_deref(), Some("true"));
    assert_eq!(cb("map.has-key", vec![m.clone(), raw("z")]).as_deref(), Some("false"));
    assert_eq!(cb("map.keys", vec![m.clone()]).as_deref(), Some("a, b"));
    assert_eq!(cb("map.values", vec![m.clone()]).as_deref(), Some("1, 2"));
    assert_eq!(
        cb("map.merge", vec![m.clone(), map(&[("a", "9"), ("c", "3")])]).as_deref(),
        Some("(a: 9, b: 2, c: 3)")
    );
    assert_eq!(
        cb("map.set", vec![m.clone(), raw("a"), raw("9")]).as_deref(),
        Some("(a: 9, b: 2)")
    );
    assert_eq!(cb("map.remove", vec![m.clone(), raw("a")]).as_deref(), Some("(b: 2)"));
    // non-map arg -> Err -> None (verbatim)
    assert_eq!(cb("map.keys", vec![num("3")]), None);
}

#[test]
fn color_domain_functions() {
    let c = |s: &str| Value::Raw(s.into());
    assert_eq!(cb("color.red", vec![c("#ff0000")]).as_deref(), Some("255"));
    assert_eq!(cb("color.green", vec![c("#00ff00")]).as_deref(), Some("255"));
    assert_eq!(cb("color.blue", vec![c("#0000ff")]).as_deref(), Some("255"));
    assert_eq!(cb("red", vec![c("red")]).as_deref(), Some("255")); // named
    assert_eq!(cb("color.red", vec![c("#f00")]).as_deref(), Some("255")); // short hex
    assert_eq!(cb("color.alpha", vec![c("#00000000")]).as_deref(), Some("0"));
    assert_eq!(cb("color.alpha", vec![c("#ff0000")]).as_deref(), Some("1"));
    assert_eq!(cb("color.hue", vec![c("#ff0000")]).as_deref(), Some("0deg"));
    assert_eq!(cb("color.saturation", vec![c("#ff0000")]).as_deref(), Some("100"));
    assert_eq!(cb("color.lightness", vec![c("#ff0000")]).as_deref(), Some("50"));
    assert_eq!(cb("color.mix", vec![c("red"), c("blue")]).as_deref(), Some("#800080"));
    // rgb() form
    assert_eq!(cb("color.blue", vec![c("rgb(0, 0, 200)")]).as_deref(), Some("200"));
    // non-colour arg -> Err -> None
    assert_eq!(cb("color.red", vec![c("nonsense")]), None);
}

#[test]
fn meta_type_of() {
    // gold: sass-spec/spec/core_functions/meta/type_of.hrx
    let t = |v| cb("meta.type-of", vec![v]);
    assert_eq!(t(Value::Bool(true)).as_deref(), Some("bool"));
    assert_eq!(t(Value::Number(1.0, String::new())).as_deref(), Some("number"));
    assert_eq!(t(Value::Color("red".into())).as_deref(), Some("color"));
    assert_eq!(t(Value::Str { text: "x".into(), quoted: true }).as_deref(), Some("string"));
    assert_eq!(t(Value::Null).as_deref(), Some("null"));
    assert_eq!(t(list(&["a", "b"], Sep::Space)).as_deref(), Some("list"));
    assert_eq!(t(map(&[("a", "b")])).as_deref(), Some("map"));
    assert_eq!(t(Value::Raw("foo".into())).as_deref(), Some("string"));
}

#[test]
fn selector_nest_and_parse() {
    // gold: sass-spec/core_functions/selector/nest + parse
    let s = |x: &str| Value::Raw(x.into());
    assert_eq!(cb("selector.nest", vec![s("c, d e"), s("f")]).as_deref(), Some("c f, d e f"));
    assert_eq!(cb("selector.nest", vec![s("a"), s("b"), s("c")]).as_deref(), Some("a b c"));
    assert_eq!(cb("selector.parse", vec![s("a b")]).as_deref(), Some("a b"));
    assert_eq!(cb("selector.parse", vec![s("a b, c")]).as_deref(), Some("a b, c"));
    // parent selector not allowed in parse -> Err -> None
    assert_eq!(cb("selector.parse", vec![s("&")]), None);
}
