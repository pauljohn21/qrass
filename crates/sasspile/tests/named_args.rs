//! Named arguments are labels, not references (the shape behind 1,611 invalid `Undefined variable`
//! errors). Every input below is copied from the fixture cited on it, so the boundary is the corpus's,
//! not an invented expectation.
//!
//! The contract pinned here is "a `$label:` names a parameter, it never becomes a variable reference" — for
//! a callee with a signature the call evaluates to the fixture's own output, and for one without, the run's
//! text survives. Either way the compile must not fail: an environment lookup for a label is the mistake that
//! once produced ~1,611 bogus `Undefined variable` errors, and these assertions exist so it cannot come back.

use sasspile::ErrorKind;
use sasspile::compile;

/// `core_functions/color/hwb/four_args::named` — every argument named. Sass resolves them against the
/// function's parameters; an environment lookup for `$hue` is simply wrong.
///
/// This assertion used to be weaker — the call was unregistered, so the test could only require that its text
/// survived. `hwb` now has a signature, so the labels are checked against the fixture's own `output.css`,
/// which is the stronger statement of the same contract: the `$hue:`/`$whiteness:`/`$blackness:`/`$alpha:`
/// labels bind to parameters in any order, none of them is looked up as a variable, and the call evaluates.
#[tokio::test]
async fn named_arguments_are_not_variable_references() {
    let css = compile("a {b: color.hwb($hue: 180, $whiteness: 30%, $blackness: 40%, $alpha: 0.4)}")
        .await
        .expect("named arguments must not fail the compile");
    assert_eq!(css, "a {\n  b: hsla(180, 33.3333333333%, 45%, 0.4);\n}\n");
}

/// `core_functions/map/has_key.hrx:69` — a named argument whose value is a **map literal**, so both
/// readings of `$map:` appear in one run and only the outer one is a label.
#[tokio::test]
async fn a_map_literal_inside_named_arguments_still_parses() {
    let css = compile("a {b: map.has-key($map: (c: d), $key: c)}")
        .await
        .expect("a call with a map argument must not fail");
    assert!(css.contains("map.has-key("), "got {css}");
}

/// `core_functions/color/is_powerless/named` — the callee is namespaced (`color.is-powerless`), so the
/// argument-start test cannot depend on the name being a plain identifier.
#[tokio::test]
async fn a_namespaced_callee_still_names_its_arguments() {
    compile("a {b: color.is-powerless($color: red, $channel: \"a\", $space: lab)}")
        .await
        .expect("namespaced call with named arguments");
}

/// The boundary that keeps this from becoming the blanket fallback the iron rule bans: a run **without**
/// a named argument still resolves its references, and an unbound one still aborts the stream.
#[tokio::test]
async fn a_real_reference_still_fails() {
    let err = compile("a { b: hwb($missing) }")
        .await
        .expect_err("an unbound reference in a call's argument position is still a reference");
    assert_eq!(err.kind, ErrorKind::UndefinedVariable);
}
