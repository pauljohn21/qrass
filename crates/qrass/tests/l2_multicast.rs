//! L2 module orchestration (design D1): the `@use` singleton is one instantiation
//! broadcast to every parent, not a per-parent copy or re-run.
//!
//! The externally visible contract of a multicast is that the shared module's body
//! executes **exactly once** no matter how many parents import it, while each parent
//! still gets the full export surface. These tests drive the real loader against the
//! `tests/fixtures/modules` diamond (`diamond_a`/`diamond_b` → `shared`) so the
//! `Local` `BehaviorSubject` carrier in `module_bus` is exercised end to end — a
//! re-instantiation would duplicate `.shared-rule`, and a carrier that deadlocks or
//! loses its cached value would hang or error the run.

use std::path::{Path, PathBuf};

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn modules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modules")
}

/// Compile a virtual entry living in the modules dir, so a relative `@use "x"`
/// resolves against the on-disk fixtures.
fn css(src: &str) -> String {
    let name = modules_dir()
        .join("virtual.scss")
        .to_string_lossy()
        .into_owned();
    let tokens = lex(0, name.clone(), src.to_string()).expect("lex ok");
    let program = parse(0, name.clone(), src.to_string(), tokens).expect("parse ok");
    match eval_all(0, name, src.to_string(), program) {
        Ok(chunks) => chunks.join("\n"),
        Err(err) => panic!("eval ok for {src:?}, got {err}"),
    }
}

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

#[test]
fn diamond_instantiates_the_shared_module_once() {
    let out = css("@use \"diamond_a\";\n@use \"diamond_b\";");
    // The shared body published its rule exactly once for two parents.
    assert_eq!(
        count(&out, ".shared-rule"),
        1,
        "shared CSS duplicated: {out}"
    );
    assert_eq!(
        count(&out, "from-shared"),
        1,
        "shared CSS duplicated: {out}"
    );
    // …while each parent still bound the namespace and read the export.
    assert_eq!(count(&out, ".da"), 1, "parent A lost its own body: {out}");
    assert_eq!(count(&out, ".db"), 1, "parent B lost its own body: {out}");
    assert_eq!(
        count(&out, "value: 42"),
        2,
        "namespace export missing: {out}"
    );
}

#[test]
fn shared_module_publishes_at_its_first_load() {
    // The multicast settles at the *first* request, so the shared rule precedes the
    // parent that triggered it; the second parent joins the finished carrier and
    // contributes nothing new.
    let out = css("@use \"diamond_a\";\n@use \"diamond_b\";");
    let shared = out.find(".shared-rule").expect("shared rule present");
    let da = out.find(".da").expect("parent A rule present");
    let db = out.find(".db").expect("parent B rule present");
    assert!(
        shared < da,
        "shared must publish before its importer: {out}"
    );
    assert!(da < db, "import order must be preserved: {out}");
}

#[test]
fn repeated_use_of_the_same_module_stays_a_single_instantiation() {
    // One entry importing the same module twice through two different names still
    // resolves to the same canonical path, so exactly one instantiation is broadcast.
    let out = css("@use \"diamond_a\";\n@use \"diamond_a\" as second;");
    assert_eq!(
        count(&out, ".shared-rule"),
        1,
        "re-`@use` re-ran the body: {out}"
    );
    assert_eq!(
        count(&out, "value: 42"),
        1,
        "aliased parent lost its body: {out}"
    );
}

#[test]
fn a_cycle_is_still_detected_through_the_carrier() {
    // Opening a carrier before its body runs must not mask the load-stack check:
    // `_cyc_a` → `_cyc_b` → `_cyc_a` still reports the circular dependency.
    let name = modules_dir()
        .join("virtual.scss")
        .to_string_lossy()
        .into_owned();
    let src = "@use \"cyc_a\";";
    let tokens = lex(0, name.clone(), src.to_string()).expect("lex ok");
    let program = parse(0, name.clone(), src.to_string(), tokens).expect("parse ok");
    let err = eval_all(0, name, src.to_string(), program).expect_err("cycle must fail");
    assert!(
        err.to_string().contains("circular dependency"),
        "expected a cycle error, got {err}"
    );
}
