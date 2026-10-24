//! Expression rendering tests (from structured tokens).

use sasspile::expr::Env;
use sasspile::lexer::Tok;
use sasspile::value::Value;
use sasspile::expr::Expr;

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
