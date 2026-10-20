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

/// Round-73 W2 (get_function / get_mixin / feature_exists goldens): the
/// callable lookups, their arity/type wording, equality semantics and the
/// feature table.
#[test]
fn callable_lookups() {
    // serialization + type-of (`inspect/function.hrx`, `get_function/meta`)
    assert_eq!(
        value("meta.inspect(meta.get-function(lighten))"),
        "get-function(\"lighten\")"
    );
    assert_eq!(value("meta.type-of(meta.get-function(lighten))"), "function");
    assert_eq!(
        value("meta.inspect(meta.get-mixin(load-css, meta))"),
        "get-mixin(\"load-css\")"
    );
    // module-qualified builtins resolve through the module's own table
    assert_eq!(
        value("meta.get-function(\"round\", $module: \"math\")"),
        "get-function(\"round\")"
    );
    // equality: builtins by name, user defs by locked reference identity
    assert_eq!(
        value("meta.get-function(lighten) == meta.get-function(lighten)"),
        "true"
    );
    assert_eq!(
        value("meta.get-function(lighten) == meta.get-function(darken)"),
        "false"
    );
    assert_eq!(value("meta.is-function(meta.get-function(lighten))"), "true");
    assert_eq!(value("meta.is-function(1)"), "false");
    // feature table is exact-spelling (`dash_sensitive` reads at_error false)
    assert_eq!(value("meta.feature-exists(at-error)"), "true");
    assert_eq!(value("meta.feature-exists(\"at-error\")"), "true");
    assert_eq!(value("meta.feature-exists(at_error)"), "false");
    assert_eq!(value("meta.feature-exists(unknown)"), "false");
    // dart's wording family
    assert!(err("meta.get-mixin()").contains("Missing argument $name."));
    assert!(
        err("meta.get-function(nonexistent)").contains("Function not found: nonexistent")
    );
    assert!(
        err("meta.get-mixin(\"c\", $module: \"d\")")
            .contains("There is no module with namespace \"d\".")
    );
    assert!(
        err("meta.get-function(c, $module: 1)")
            .contains("$module: 1 is neither a string nor a module reference.")
    );
    assert!(err("meta.get-mixin(c, true, d, e)").contains("Only 2 arguments allowed, but 4 were passed."));
    // arithmetic on a callable names its serialization (`values/mixins.hrx`)
    assert!(
        err("meta.get-function(rgb) / meta.get-function(lighten)")
            .contains("get-function(\"rgb\") isn't a valid CSS value.")
    );
}
