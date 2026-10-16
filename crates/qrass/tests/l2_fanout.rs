//! L2 branch fan-out (design D1, manual §2): the top-level `@use`/`@forward`
//! branches of one module body are acquired by a `Shared` `flat_map` stage
//! (`src/branch_fanout.rs`) *before* the sequential walk reaches them, and
//! `load_module` consumes that cache instead of probing the file system again.
//!
//! Speculation is only safe if it is invisible, so these tests pin the two
//! properties the fan-out could break and that the sequential-walk tests in
//! `scss_modules.rs` / `l2_multicast.rs` cannot see:
//!
//!  * **errors stay data** — a branch that fails to resolve or to parse must
//!    surface at its *own* statement, in statement order, with the same message
//!    as before, never at fan-out time (`Result` in the value channel, manual §14
//!    red line 5);
//!  * **the base dir is the writing body's** — the stage runs per frame, so a
//!    nested module's relative branch resolves against *its* directory, and the
//!    branches the stage deliberately skips (already a settled singleton, already
//!    on the cycle stack) must still produce the singleton/cycle behaviour.

use std::path::{Path, PathBuf};

use qrass::error::SassError;
use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::parser::parse;

fn modules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modules")
}

/// Compile a virtual entry living in the modules dir, so a relative `@use`
/// resolves against the on-disk fixtures.
fn compile(src: &str) -> Result<String, SassError> {
    let name = modules_dir()
        .join("virtual.scss")
        .to_string_lossy()
        .into_owned();
    let tokens = lex(0, name.clone(), src.to_string()).expect("lex ok");
    let program = parse(0, name.clone(), src.to_string(), tokens).expect("parse ok");
    eval_all(0, name, src.to_string(), program).map(|chunks| chunks.join("\n"))
}

fn css(src: &str) -> String {
    match compile(src) {
        Ok(out) => out,
        Err(err) => panic!("eval ok for {src:?}, got {err}"),
    }
}

/// The full rendered error: message, note (candidate list), and trace.
fn render(src: &str) -> String {
    match compile(src) {
        Ok(out) => panic!("expected eval error for {src:?}, got {out}"),
        Err(err) => err.to_string(),
    }
}

// ─── errors stay data ────────────────────────────────────────────────────────

#[test]
fn the_first_unresolvable_branch_owns_the_error() {
    // Both ghosts are probed by the same fan-out batch, so whichever order the
    // concurrent stage merges them in, the walk must still blame the first
    // statement — the second failure has to stay parked as data.
    let err = render("@use \"ghost_a\";\n@use \"ghost_b\";");
    assert!(err.contains("ghost_a"), "first branch must be reported\n---\n{err}");
    assert!(
        !err.contains("ghost_b"),
        "a later branch must not jump the queue\n---\n{err}"
    );
}

#[test]
fn an_unresolvable_late_branch_is_reported_after_the_early_one_loaded() {
    // The early branch is a hit in the cache and must still be instantiated; the
    // failure may not turn into a different error (e.g. a missing namespace).
    let err = render("@use \"shared\";\n@use \"ghost\";");
    assert!(err.contains("ghost"), "the failing branch is reported\n---\n{err}");
    assert!(
        !err.contains("undefined variable"),
        "the settled branch must not leak into the error\n---\n{err}"
    );
}

#[test]
fn a_parse_error_inside_a_branch_locates_in_that_branch() {
    let err = render("@use \"fo_bad_syntax\";");
    assert!(
        err.contains("_fo_bad_syntax.scss"),
        "the front-end failure must name its own file\n---\n{err}"
    );
    assert!(
        !err.contains("Couldn't resolve"),
        "a readable file must not degrade into a load error\n---\n{err}"
    );
}

#[test]
fn statement_order_beats_failure_kind() {
    // The fan-out collects a parse failure and a resolve failure in the same
    // batch; the walk still has to hit the SCSS syntax error first.
    let err = render("@use \"fo_bad_syntax\";\n@use \"ghost\";");
    assert!(
        err.contains("_fo_bad_syntax.scss"),
        "the first statement owns the error\n---\n{err}"
    );
    assert!(!err.contains("ghost"), "the parked failure must stay parked\n---\n{err}");
}

// ─── the base dir is the writing body's ──────────────────────────────────────

#[test]
fn a_nested_frame_fans_out_against_its_own_directory() {
    // `sub/_fo_nested.scss` writes `@use "sub-ghost"`: the fan-out inside that
    // frame must resolve relative to `sub/`, so the candidate list proves the
    // base dir while the error stays a single load failure.
    let err = render("@use \"sub/fo_nested\";");
    assert!(err.contains("sub-ghost"), "the branch is reported\n---\n{err}");
    let sub = modules_dir().join("sub").join("_sub-ghost.scss");
    assert!(
        err.contains(&sub.to_string_lossy().to_string()),
        "candidates must be probed under the writing module's dir\n---\n{err}"
    );
}

// ─── skipped branches keep their sequential semantics ────────────────────────

#[test]
fn a_settled_branch_is_skipped_and_still_shared_once() {
    // `shared` is open by the time the second parent's fan-out runs, so stage A
    // drops it before reading — and the load must still join the multicast.
    let out = css("@use \"diamond_a\";\n@use \"diamond_b\";");
    assert_eq!(out.matches(".shared-rule").count(), 1, "shared ran twice: {out}");
    assert_eq!(out.matches(".da").count(), 1, "parent A lost its body: {out}");
    assert_eq!(out.matches(".db").count(), 1, "parent B lost its body: {out}");
}

#[test]
fn a_branch_on_the_load_stack_still_reports_the_cycle() {
    // Stage A skips `_cyc_a` while it is on the stack, leaving no cache entry;
    // `load_module` then falls back to its own resolution and cycle check.
    let err = render("@use \"cyc_a\";");
    assert!(
        err.contains("circular dependency"),
        "the skip must not swallow the cycle\n---\n{err}"
    );
}

// ─── acquisition stays byte-identical ────────────────────────────────────────

#[test]
fn a_css_branch_passes_through_the_cache_verbatim() {
    let out = css("@use \"plain\";\n@use \"shared\";");
    assert!(
        out.contains(".css-passthrough { color: blue; }"),
        "plain CSS payload altered: {out}"
    );
    assert_eq!(out.matches(".shared-rule").count(), 1, "scss body: {out}");
}

#[test]
fn a_css_branch_that_does_not_parse_still_passes_through() {
    // The `.css` path *ignores* the parked parse failure — exotic CSS the SCSS
    // front-end cannot read must keep flowing through untouched.
    let out = css("@use \"exotic\";");
    assert!(
        out.contains(".unterminated-css { color: red;"),
        "unparsable CSS was rejected: {out}"
    );
}

#[test]
fn a_hit_and_a_miss_acquire_the_same_source() {
    // Two spellings of one file in the same body share a cache key only for the
    // first use; the second falls back to the inline acquisition, so both parents
    // must bind the very same singleton value.
    let out = css("@use \"shared\" as a;\n@use \"shared\" as b;\n.x { value: a.$answer + b.$answer; }");
    assert!(out.contains("value: 84"), "hit and miss disagree: {out}");
    assert_eq!(out.matches(".shared-rule").count(), 1, "body ran twice: {out}");
}
