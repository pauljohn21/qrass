//! `meta.module-variables` (round 72, `core_functions/meta/module_variables.hrx`):
//! the namespace's user variables as a sorted map, dart's arity/type wording,
//! and the built-in-module empty case. (The `module-functions`/`module-mixins`
//! siblings still wait on first-class callable values.)

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn value(expr: &str) -> String {
    let src = format!(".a {{ p: {expr}; }}");
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

fn err(expr: &str) -> String {
    let src = format!(".a {{ p: {expr}; }}");
    let tokens = lex(0, "t.scss".into(), src.clone()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.clone(), tokens).expect("parse ok");
    eval_all(0, "t.scss".into(), src, program)
        .err()
        .unwrap_or_else(|| panic!("{expr} must fail"))
        .to_string()
}

#[test]
fn module_variables_wording() {
    assert_eq!(value("meta.module-variables(\"meta\")"), "()");
    assert!(err("meta.module-variables()").contains("Missing argument $module."));
    assert!(
        err("meta.module-variables(\"meta\", \"c\")")
            .contains("Only 1 argument allowed, but 2 were passed.")
    );
    assert!(
        err("meta.module-variables(1)")
            .contains("$module: 1 is neither a string nor a module reference.")
    );
    assert!(
        err("meta.module-variables(\"other\")")
            .contains("There is no module with namespace \"other\".")
    );
}
