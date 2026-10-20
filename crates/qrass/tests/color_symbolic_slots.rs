//! Symbolic-channel serialization for the modern space-separated `hsl`/`hsla`
//! spelling (round 83, `hsl/one_arg/special_functions/*` goldens): a
//! `calc()`/`var()`/`env()`/`attr()` slot — directly or through
//! `string.unquote("…")` — re-emits the call in the legacy comma form, while
//! relative-color `from` anchors keep the verbatim slash spelling.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn value(expr: &str) -> String {
    let src = format!("@use \"sass:string\"; .a {{ p: {expr}; }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    let out = eval_all(0, "t.scss".into(), src, program)
        .expect("eval ok")
        .join("\n");
    out.split_once("p: ")
        .map(|x| x.1.split_once(";\n").map(|y| y.0).unwrap_or(x.1))
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[test]
fn symbolic_slots_serialize_comma() {
    assert_eq!(
        value("hsl(1 string.unquote(\"calc(2%)\") 3% / 0.4)"),
        "hsl(1, calc(2%), 3%, 0.4)"
    );
    assert_eq!(
        value("hsl(1 2% string.unquote(\"calc(3%)\") / 0.4)"),
        "hsl(1, 2%, calc(3%), 0.4)"
    );
    assert_eq!(
        value("hsl(1 2% 3% / string.unquote(\"calc(0.4)\"))"),
        "hsl(1, 2%, 3%, calc(0.4))"
    );
    assert_eq!(
        value("hsl(calc(1px + 1%) 2% 3% / 0.4)"),
        "hsl(calc(1px + 1%), 2%, 3%, 0.4)"
    );
    assert_eq!(value("hsl(var(--foo) 2% 3%)"), "hsl(var(--foo), 2%, 3%)");
    assert_eq!(value("hsl(1 2% 3% / env(--c))"), "hsl(1, 2%, 3%, env(--c))");
}

#[test]
fn from_anchor_stays_verbatim() {
    // relative colors keep the literal slash form (`hsl(from … h s l/0.4)`)
    assert_eq!(
        value("hsl(from hsl(1 2% 3%) h s l / 0.4)"),
        "hsl(from hsl(1 2% 3%) h s l/0.4)"
    );
}
