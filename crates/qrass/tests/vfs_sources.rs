//! `crate::vfs` — the in-memory source registry behind the wasm contract.
//!
//! Pins the three properties the browser integration relies on: registered
//! sources satisfy the full Sass resolution order (literal → `_` partial →
//! directory `_index`) without any disk access; the module bus and branch
//! fan-out behave identically over virtual files (a diamond instantiates its
//! shared partial exactly once); and an **empty** registry changes nothing —
//! unregistered paths still fall through to `std::fs`.

use std::path::{Path, PathBuf};

use qrass::{Options, compile, compile_string, vfs};

fn virtual_diamond() {
    vfs::register("/v/_shared.scss", ".shared-rule { color: from-shared }");
    vfs::register("/v/_a.scss", "@use \"shared\";\n.a { color: red }");
    vfs::register("/v/_b.scss", "@use \"shared\";");
}

#[test]
fn diamond_resolves_and_instantiates_once_without_disk() {
    // The entry name lives under /v (no such directory on disk): every @use can
    // only resolve through the registry.
    vfs::clear();
    virtual_diamond();
    let out = compile_string(
        "/v/index.scss".to_string(),
        "@use \"a\";\n@use \"b\";\n".to_string(),
        &Options::default(),
    )
    .expect("virtual compile ok");
    assert_eq!(
        out.matches(".shared-rule").count(),
        1,
        "shared partial must instantiate exactly once: {out}"
    );
    assert!(out.contains("from-shared"), "shared CSS lost: {out}");
    vfs::clear();
}

#[test]
fn directory_index_candidate_hits_the_registry() {
    // `@use "kit"` must find the directory-index form `kit/_index.scss` (the
    // Sass candidate order: literal, `_` partial, dir `_index`, dir `index`).
    vfs::clear();
    vfs::register("/p/kit/_index.scss", ".kit { display: flex }");
    let out = compile_string(
        "/p/main.scss".to_string(),
        "@use \"kit\";\n".to_string(),
        &Options::default(),
    )
    .expect("index candidate resolved");
    assert!(out.contains(".kit"), "index rule lost: {out}");
    vfs::clear();
}

#[test]
fn dot_dot_segments_share_the_canonical_key() {
    // One module, two spellings: the entry lives in `/x/y/`, so `../m/deep`
    // lexically resolves to `/x/m/_deep.scss` — the registered key must match
    // the *normalized* candidate, not the raw `..`-carrying path.
    vfs::clear();
    vfs::register("/x/m/_deep.scss", ".deep { z-index: 1 }");
    let out = compile_string(
        "/x/y/entry.scss".to_string(),
        "@use \"../m/deep\";\n".to_string(),
        &Options::default(),
    )
    .expect("../ URL normalizes to the registered key");
    assert!(out.contains(".deep"), "normalized hit lost: {out}");
    vfs::clear();
}

#[test]
fn the_file_entry_works_on_purely_virtual_sources() {
    // Even `qrass::compile(&path)` — the CLI entry — runs with zero disk reads
    // once the entry itself is registered.
    vfs::clear();
    vfs::register("/v/entry.scss", "@use \"a\";\n.c { quotes: \"no\" }");
    virtual_diamond();
    let path = PathBuf::from("/v/entry.scss");
    let out = compile(&path, &Options::default()).expect("virtual entry via compile()");
    assert!(out.contains(".c"), "entry body lost: {out}");
    assert_eq!(out.matches(".shared-rule").count(), 1, "{out}");
    vfs::clear();
}

#[test]
fn empty_registry_falls_through_to_the_real_file_system() {
    // Regression guard for every existing suite: nothing registered → disk
    // resolution unchanged (fixtures directory from the L2 tests).
    vfs::clear();
    let modules = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modules");
    let name = modules.join("virtual.scss").to_string_lossy().into_owned();
    let out = compile_string(
        name,
        "@use \"diamond_a\";\n@use \"diamond_b\";\n".to_string(),
        &Options::default(),
    )
    .expect("on-disk compile still works");
    assert_eq!(out.matches(".shared-rule").count(), 1, "{out}");
}
