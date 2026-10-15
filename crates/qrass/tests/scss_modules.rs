//! Module-loader tests (task 6.1): URL canonicalization order, `_`-prefixed
//! partials, directory `index.scss`, plain-CSS passthrough detection, the
//! not-found candidate list, and read-once deduplication through [`Loader`].

use std::path::{Path, PathBuf};

use qrass::eval::eval_all;
use qrass::lexer::lex;
use qrass::loader::{Loader, SourceKind, canonicalize};
use qrass::parser::parse;

fn modules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/modules")
}

/// Compile a virtual entry whose `name` sits in the modules dir so relative
/// `@use` resolves against the on-disk fixtures.
fn css(src: &str) -> String {
    let name = modules_dir()
        .join("virtual.scss")
        .to_string_lossy()
        .into_owned();
    let tokens = lex(0, name.clone(), src.to_string()).expect("lex ok");
    let program = parse(0, name.clone(), src.to_string(), tokens).expect("parse ok");
    match eval_all(0, name, src.to_string(), program) {
        Ok(chunks) => chunks.join("\n"),
        Err(e) => panic!("eval ok for {src:?}, got {e}"),
    }
}

fn eval_err(src: &str) -> String {
    let name = modules_dir()
        .join("virtual.scss")
        .to_string_lossy()
        .into_owned();
    let tokens = lex(0, name.clone(), src.to_string()).expect("lex ok");
    let program = parse(0, name.clone(), src.to_string(), tokens).expect("parse ok");
    match eval_all(0, name, src.to_string(), program) {
        Ok(_) => panic!("expected eval error for {src:?}"),
        Err(e) => e.to_string(),
    }
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

#[test]
fn resolves_partial_with_underscore_prefix() {
    let r = canonicalize("vars", &modules_dir()).expect("partial resolves");
    assert_eq!(name_of(&r.path), "_vars.scss");
    assert_eq!(r.kind, SourceKind::Scss);
}

#[test]
fn resolves_directory_index() {
    let r = canonicalize("sub", &modules_dir()).expect("index resolves");
    assert_eq!(name_of(&r.path), "index.scss");
    assert_eq!(r.path.parent().map(name_of).as_deref(), Some("sub"));
    assert_eq!(r.kind, SourceKind::Scss);
}

#[test]
fn detects_css_passthrough() {
    let r = canonicalize("plain", &modules_dir()).expect("css resolves");
    assert_eq!(name_of(&r.path), "plain.css");
    assert_eq!(r.kind, SourceKind::Css);
}

#[test]
fn literal_extension_is_tried_as_is() {
    let r = canonicalize("_vars.scss", &modules_dir()).expect("explicit scss resolves");
    assert_eq!(name_of(&r.path), "_vars.scss");
}

#[test]
fn not_found_lists_candidates() {
    let err = canonicalize("missing", &modules_dir()).expect_err("should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("Couldn't resolve"),
        "load error surfaced\n---\n{msg}"
    );
    assert!(
        msg.contains("missing.scss"),
        "candidate list present\n---\n{msg}"
    );
    assert!(
        msg.contains("_missing.scss"),
        "partial candidate\n---\n{msg}"
    );
}

#[test]
fn loader_dedupes_by_canonical_path() {
    let mut loader = Loader::default();
    let r = canonicalize("vars", &modules_dir()).expect("partial resolves");
    let a = loader.load_resolved(&r).expect("first load");
    let b = loader.load_resolved(&r).expect("second load");
    assert_eq!(a.file, b.file, "same canonical path registered once");
    assert_eq!(a.content(loader.files()), Some("$from-partial: 1;\n"));
}

// ─── 6.2 @use: namespace binding / instantiate-once / privacy ─────────────────

#[test]
fn use_binds_default_namespace() {
    let out = css("@use \"vars\";\n.a { color: vars.$from-partial; }");
    assert!(
        out.contains(".a {\n  color: 1;\n}"),
        "default namespace `vars`\n---\n{out}"
    );
}

#[test]
fn use_named_alias() {
    let out = css("@use \"vars\" as v;\n.a { color: v.$from-partial; }");
    assert!(
        out.contains(".a {\n  color: 1;\n}"),
        "`as v` alias\n---\n{out}"
    );
}

#[test]
fn use_as_star_merges_into_scope() {
    let out = css("@use \"vars\" as *;\n.a { color: $from-partial; }");
    assert!(
        out.contains(".a {\n  color: 1;\n}"),
        "`as *` merges plain var\n---\n{out}"
    );
}

#[test]
fn module_css_emitted_once() {
    let out = css("@use \"lib\";\n@use \"lib\" as l2;\n.use { width: l2.$public; }");
    assert_eq!(
        out.matches(".lib-rule").count(),
        1,
        "double @use runs module once\n---\n{out}"
    );
    assert!(
        out.contains(".use {\n  width: 7;\n}"),
        "second alias reads export\n---\n{out}"
    );
}

#[test]
fn private_var_not_visible_cross_module() {
    let msg = eval_err("@use \"lib\";\n.a { color: lib.$-secret; }");
    assert!(
        msg.contains("Undefined variable"),
        "private `$-secret` hidden\n---\n{msg}"
    );
}

#[test]
fn transitive_dependency_loads_once() {
    // dep @uses lib; entry @uses both → lib body runs once.
    let out = css("@use \"dep\";\n@use \"lib\";\n.top { color: lib.$public; }");
    assert_eq!(
        out.matches(".lib-rule").count(),
        1,
        "diamond lib runs once\n---\n{out}"
    );
    assert!(out.contains(".dep {"), "dep css present\n---\n{out}");
    assert!(
        out.contains(".top {\n  color: 7;\n}"),
        "entry reads lib\n---\n{out}"
    );
}

// ─── 6.3 @forward (show / hide / prefix) + @use … with ────────────────────────

#[test]
fn forward_show_reexports_only_listed() {
    let out = css("@use \"fwd_show\" as t;\n.a { width: t.$base; }");
    assert!(
        out.contains(".a {\n  width: 10px;\n}"),
        "forwarded $base\n---\n{out}"
    );
    assert!(
        out.contains(".cfg {"),
        "forwarded module CSS flows up\n---\n{out}"
    );
    let msg = eval_err("@use \"fwd_show\" as t;\n.a { color: t.$hover; }");
    assert!(
        msg.contains("Undefined variable"),
        "$hover hidden by show\n---\n{msg}"
    );
}

#[test]
fn forward_hide_excludes_listed() {
    let out = css("@use \"fwd_hide\" as t;\n.a { width: t.$base; }");
    assert!(
        out.contains(".a {\n  width: 10px;\n}"),
        "$base kept\n---\n{out}"
    );
    let msg = eval_err("@use \"fwd_hide\" as t;\n.a { color: t.$hover; }");
    assert!(
        msg.contains("Undefined variable"),
        "$hover excluded\n---\n{msg}"
    );
}

#[test]
fn forward_prefix_renames_members() {
    let out = css("@use \"fwd_prefix\" as t;\n.a { width: t.$c-base; }");
    assert!(
        out.contains(".a {\n  width: 10px;\n}"),
        "$base renamed $c-base\n---\n{out}"
    );
}

#[test]
fn use_with_config_consumes_default() {
    let out = css("@use \"config\" with ($base: 20px);");
    assert!(
        out.contains(".cfg {\n  margin: 20px;\n}"),
        "!default overridden by with\n---\n{out}"
    );
}

#[test]
fn use_with_unconsumed_config_errors() {
    let msg = eval_err("@use \"config\" with ($nope: 5);");
    assert!(
        msg.contains("did not declare"),
        "unconsumed config is an error\n---\n{msg}"
    );
}

// ─── 6.4 Circular @use detection + @import policy ─────────────────────────────

#[test]
fn circular_use_is_detected() {
    let msg = eval_err("@use \"cyc_a\";");
    assert!(
        msg.contains("circular") || msg.contains("Circular"),
        "cycle reported\n---\n{msg}"
    );
    assert!(
        msg.contains("cyc_a") && msg.contains("cyc_b"),
        "cycle path lists files\n---\n{msg}"
    );
}

#[test]
fn scss_import_is_rejected() {
    let msg = eval_err("@import \"lib\";");
    assert!(
        msg.contains("not supported"),
        "SCSS @import unsupported\n---\n{msg}"
    );
}

#[test]
fn css_import_passes_through() {
    let out = css("@import \"theme.css\";");
    assert!(
        out.contains("@import") && out.contains("theme.css"),
        "CSS @import passthrough\n---\n{out}"
    );
}
