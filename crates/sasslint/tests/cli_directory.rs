//! The CLI must lint **the directory it was given**. The first version read `argv[1]` as the path, so the
//! documented `sasslint lint crates/sasspile/src` scanned a nonexistent directory named `lint` and printed
//! `count=0` while the same code made `gate_sasspile_is_clean` red. An always-passing gate is worse than no
//! gate, so both halves are pinned here: a planted violation in the named directory must exit non-zero, and
//! a clean directory must exit zero.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sasslint-cli-{label}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn the_lint_verb_lints_the_given_directory() {
    let dirty = scratch_dir("dirty");
    // A batch shell: `scan_map(` with a `Vec<` on the same line is `no-batch-shell`'s own signature —
    // written exactly as the rule reads it. (A `Vec::<u8>` turbofish does **not** contain `Vec<`, which is
    // how the first draft of this test planted a violation that was legitimately clean.)
    fs::write(
        dirty.join("bad.rs"),
        "acc = acc.scan_map(|a, tok| -> Vec<Tok> { a.feed(tok) });\n",
    )
    .unwrap();

    let failure = Command::new(env!("CARGO_BIN_EXE_sasslint"))
        .args(["lint", dirty.to_str().unwrap()])
        .output()
        .expect("run sasslint");
    assert!(
        !failure.status.code().is_some_and(|code| code == 0),
        "a planted violation in the named directory must not pass: {:?}",
        String::from_utf8_lossy(&failure.stderr)
    );

    let clean = scratch_dir("clean");
    fs::write(clean.join("good.rs"), "let a = 1;\n").unwrap();
    let pass = Command::new(env!("CARGO_BIN_EXE_sasslint"))
        .args(["lint", clean.to_str().unwrap()])
        .output()
        .expect("run sasslint");
    assert!(
        pass.status.success(),
        "a clean directory must exit 0: {:?}",
        String::from_utf8_lossy(&pass.stderr)
    );

    // The other half of the defect: a path that does not exist is *not* a clean bill of health.
    let missing = Command::new(env!("CARGO_BIN_EXE_sasslint"))
        .args(["lint", dirty.join("nope").to_str().unwrap()])
        .output()
        .expect("run sasslint");
    assert!(
        !missing.status.success(),
        "a nonexistent directory must fail loudly instead of reporting count=0"
    );

    fs::remove_dir_all(dirty).ok();
    fs::remove_dir_all(clean).ok();
}
