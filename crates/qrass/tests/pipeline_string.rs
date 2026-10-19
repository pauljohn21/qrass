//! The in-memory entry (`qrass::compile_string`, `PipelineBuilder::source`): the
//! fs-free pipeline head the `wasm32-unknown-unknown` integration compiles
//! through — a browser has no files to read, only strings handed in by JS. On
//! native targets these pins keep the two entry forms behaving identically:
//! same modules bus, same fan-out, same `@use` resolution, same rendered text.

use std::path::{Path, PathBuf};

use qrass::error::SassError;
use qrass::{Options, compile, compile_string, pipeline};

fn modules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modules")
}

/// A virtual entry living in the modules dir: relative `@use` resolves against
/// its base directory exactly as an on-disk file's would.
fn virtual_name() -> String {
    modules_dir()
        .join("virtual.scss")
        .to_string_lossy()
        .into_owned()
}

fn css(src: &str) -> Result<String, SassError> {
    compile_string(virtual_name(), src.to_string(), &Options::default())
}

#[test]
fn inline_entry_compiles_a_real_program() {
    let out = css("@mixin c { color: red } .a { .b { @include c; } }").expect("compile ok");
    assert!(out.contains(".a .b"), "nesting lost: {out}");
    assert!(out.contains("color: red"), "mixin body lost: {out}");
}

#[test]
fn inline_entry_loads_builtin_modules() {
    // `sass:` namespaces never touch the file system — the wasm-fever path.
    let out = css("@use \"sass:math\"; .w { width: math.div(100px, 4); }").expect("compile ok");
    assert!(out.contains("25px"), "builtin call lost: {out}");
}

#[test]
fn inline_entry_drives_the_module_bus_and_fan_out() {
    // The diamond fixture exercises open/settle/share on the `Local` hub and the
    // branch fan-out — all reached from an in-memory entry.
    let out = css("@use \"diamond_a\";\n@use \"diamond_b\";").expect("compile ok");
    assert_eq!(
        out.matches(".shared-rule").count(),
        1,
        "shared module must instantiate exactly once: {out}"
    );
}

#[test]
fn inline_and_file_entries_produce_the_same_css() {
    let path = modules_dir().join("_shared.scss");
    let via_file = compile(&path, &Options::default()).expect("file compile ok");
    let content = std::fs::read_to_string(&path).expect("fixture readable");
    let via_string = compile_string(path.display().to_string(), content, &Options::default())
        .expect("string compile ok");
    assert_eq!(via_file, via_string);
}

#[test]
fn entry_and_source_are_mutually_exclusive() {
    let err = pipeline::build()
        .entry(modules_dir().join("_lib.scss"))
        .source("x.scss", "a { b: c }")
        .drain()
        .expect_err("both entry forms must be rejected");
    assert!(
        format!("{err}").contains("both an entry file and an inline source"),
        "unexpected wording: {err}"
    );
}
