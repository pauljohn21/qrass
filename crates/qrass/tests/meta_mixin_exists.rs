//! Golden-pinned contract for `meta.mixin-exists` (round 93,
//! `core_functions/meta/mixin_exists`): the arity/type validation ladder and
//! the `$module` lookup semantics — bound namespace names answer from the
//! module's exported members, loaded built-ins own no mixins (false), and an
//! unbound or wrongly-spelled namespace raises dart's no-module error.

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

/// Evaluate a whole stylesheet; return the CSS on success or the error text.
fn outcome(src: &str) -> String {
    let tokens = lex(0, "t.scss".into(), src.to_string()).expect("lex ok");
    let program = parse(0, "t.scss".into(), src.to_string(), tokens).expect("parse ok");
    match eval_all(0, "t.scss".into(), src.to_string(), program) {
        Ok(out) => out.join("\n"),
        Err(e) => e.message,
    }
}

#[test]
fn validation_ladder() {
    assert!(outcome("@use \"sass:meta\"; a {b: meta.mixin-exists()}")
        .contains("Missing argument $name."));
    assert!(
        outcome("@use \"sass:meta\"; a {b: meta.mixin-exists(c, d, e)}")
            .contains("Only 2 arguments allowed, but 3 were passed.")
    );
    assert!(outcome("@use \"sass:meta\"; a {b: meta.mixin-exists(12px)}")
        .contains("$name: 12px is not a string."));
    assert!(
        outcome("@use \"sass:meta\"; a {b: meta.mixin-exists(c, 1)}")
            .contains("$module: 1 is neither a string nor a module reference.")
    );
}

#[test]
fn module_lookup() {
    // an unbound namespace raises dart's error
    assert!(
        outcome("@use \"sass:meta\"; a {b: meta.mixin-exists(\"c\", \"d\")}")
            .contains("There is no module with namespace \"d\".")
    );
    // a built-in namespace is only in scope once `@use`d
    assert!(outcome("@use \"sass:meta\"; a {b: meta.mixin-exists(\"c\", \"color\")}")
        .contains("There is no module with namespace \"color\"."));
    assert!(
        outcome("@use \"sass:meta\"; @use \"sass:color\"; a {b: meta.mixin-exists(\"c\", \"color\")}")
            .contains("b: false")
    );
    // exact spelling — `_` does not fold to the bound `a-b`
    assert!(outcome(
        "@use \"sass:meta\"; @use \"sass:color\" as a-b; a {b: meta.mixin-exists(\"c\", \"a_b\")}"
    )
    .contains("There is no module with namespace \"a_b\"."));
    // bare local lookups still answer
    assert!(
        outcome("@use \"sass:meta\"; @mixin local {} a {b: meta.mixin-exists(local)}")
            .contains("b: true")
    );
    assert!(
        outcome("@use \"sass:meta\"; a {b: meta.mixin-exists(non-existent)}")
            .contains("b: false")
    );
}
