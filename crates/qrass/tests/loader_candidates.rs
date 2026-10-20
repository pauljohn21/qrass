//! Candidate-ordering contract tests for the filesystem loader (round 77,
//! `directives/use/error/load.hrx` + `directives/use/load.hrx`). Rules are
//! read off the golden tables: a URL that already carries a recognized
//! extension never grows one and never resolves through its own directory
//! index; same-level partial/non-partial pairs are ambiguous; a normal file
//! wins over a directory index; and a miss raises `Can't find stylesheet to
//! import.` Each test materializes a private tree under `target/tmp`
//! (test-local scratch) and drives the public compile API.

use qrass::Options;
use std::path::Path;

fn scratch_tree(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/tmp/loader_tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (path, content) in files {
        let target = root.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("mkdir ok");
        }
        std::fs::write(target, content).expect("write ok");
    }
    root
}

fn compile_in(root: &Path) -> Result<String, String> {
    qrass::compile(&root.join("input.scss"), &Options::default())
        .map_err(|e| e.to_string())
}

#[test]
fn extensionless_file_is_not_a_candidate() {
    // `use/error/load` no_extension: an extensionless `other` on disk does not
    // resolve; only recognized extensions load.
    let root = scratch_tree(
        "no_extension",
        &[("input.scss", "@use \"other\";\n"), ("other", "a {b: c}")],
    );
    let err = compile_in(&root).err().unwrap_or_default();
    assert!(err.contains("Can't find stylesheet to import."), "{err}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_stylesheet_raises_cant_find() {
    let root = scratch_tree("missing", &[("input.scss", "@use \"other\";\n")]);
    let err = compile_in(&root).err().unwrap_or_default();
    assert!(err.contains("Can't find stylesheet to import."), "{err}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dot_scss_url_does_not_read_its_own_directory_index() {
    // `dir_dot_scss`: `dir.scss` is a *directory* holding index.scss; the
    // extension already present forbids growing candidates or reading through.
    let root = scratch_tree(
        "dir_dot_scss",
        &[
            ("input.scss", "@use \"dir.scss\";\n"),
            ("dir.scss/index.scss", ".foo { a: b; }\n"),
        ],
    );
    let err = compile_in(&root).err().unwrap_or_default();
    assert!(err.contains("Can't find stylesheet to import."), "{err}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn partial_vs_nonpartial_is_ambiguous() {
    // `conflict/partial`: both `_other.scss` and `other.scss` exist.
    let root = scratch_tree(
        "conflict_partial",
        &[
            ("input.scss", "@use \"other\";\n"),
            ("_other.scss", "a {partial: true}\n"),
            ("other.scss", "a {partial: false}\n"),
        ],
    );
    let err = compile_in(&root).err().unwrap_or_default();
    assert!(
        err.contains("It's not clear which file to import. Found:")
            && err.contains("_other.scss")
            && err.contains("other.scss"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn index_pair_is_ambiguous() {
    // `conflict/index`: `other/_index.scss` vs `other/index.scss`.
    let root = scratch_tree(
        "conflict_index",
        &[
            ("input.scss", "@use \"other\";\n"),
            ("other/_index.scss", "a {partial: true}\n"),
            ("other/index.scss", "a {partial: false}\n"),
        ],
    );
    let err = compile_in(&root).err().unwrap_or_default();
    assert!(
        err.contains("It's not clear which file to import. Found:")
            && err.contains("other/_index.scss")
            && err.contains("other/index.scss"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn normal_file_wins_over_directory_index() {
    // `use/load` precedence/normal_before_index: `dir.scss` beats `dir/index.scss`.
    let root = scratch_tree(
        "normal_before_index",
        &[
            ("input.scss", "@use \"dir\";\n"),
            ("dir.scss", "a {index: false}\n"),
            ("dir/index.scss", "a {index: true}\n"),
        ],
    );
    let out = compile_in(&root).expect("normal file resolves before the index");
    assert!(out.contains("index: false"), "{out}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn scss_wins_over_css_sibling() {
    // `use/load` precedence/scss_before_css.
    let root = scratch_tree(
        "scss_before_css",
        &[
            ("input.scss", "@use \"other\";\n"),
            ("other.scss", "a {syntax: scss}\n"),
            ("other.css", "a {syntax: css}\n"),
        ],
    );
    let out = compile_in(&root).expect("scss wins");
    assert!(out.contains("syntax: scss") || out.contains("scss"), "{out}");
    let _ = std::fs::remove_dir_all(&root);
}
