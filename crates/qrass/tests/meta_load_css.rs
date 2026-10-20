//! `meta.load-css($url, $with: …)` contract tests (round 80,
//! `core_functions/meta/load_css/*`). Rules read off the golden tables: the
//! target's CSS splices into the output without any namespace binding, and
//! the `$with:` configuration reports through load-css-specific spellings.

use qrass::vfs;
use qrass::{Options, compile_string};

fn load(entry: &str, body: &str) -> Result<String, String> {
    vfs::clear();
    // a minimal target so `load-css("other")` resolves through the registry
    vfs::register("/lc/_other.scss", "$a: c;\n");
    compile_string(
        format!("/lc/{entry}.scss"),
        body.to_string(),
        &Options::default(),
    )
    .map_err(|e| e.to_string())
}

#[test]
fn load_css_splices_css_without_bindings() {
    // member.hrx: only the CSS joins — the module's variables stay invisible
    vfs::register("/lc/_other.scss", "$c: loaded-value;\na {b: c}\n");
    let out = compile_string(
        "/lc/index.scss".to_string(),
        "@use \"sass:meta\";\n@include meta.load-css(\"other\");\n".to_string(),
        &Options::default(),
    )
    .expect("load ok");
    assert!(out.contains("a {") && out.contains("b: c"), "{out}");
    let err = compile_string(
        "/lc/index2.scss".to_string(),
        "@use \"sass:meta\";\n@include meta.load-css(\"other\");\nx {y: $c}\n".to_string(),
        &Options::default(),
    )
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    assert!(err.contains("Undefined variable"), "{err}");
    vfs::clear();
}

#[test]
fn with_configuration_reports_the_load_css_spellings() {
    // not_default: `$a was not declared with !default in the @used module.`
    let err = load(
        "nd",
        "@use \"sass:meta\";\n@include meta.load-css(\"other\", $with: (a: b));\n",
    )
    .err()
    .unwrap_or_default();
    assert!(
        err.contains("$a was not declared with !default in the @used module."),
        "{err}"
    );

    // core_module: `Built-in module sass:color can't be configured.`
    let err = load(
        "core",
        "@use \"sass:meta\";\n@include meta.load-css(\"sass:color\", $with: (a: b));\n",
    )
    .err()
    .unwrap_or_default();
    assert!(
        err.contains("Built-in module sass:color can't be configured."),
        "{err}"
    );
}

#[test]
fn with_argument_type_errors() {
    // `$with: 1 is not a map.` / `$with key: 1 is not a string.`
    let err = load(
        "ty",
        "@use \"sass:meta\";\n@include meta.load-css(\"other\", $with: 1);\n",
    )
    .err()
    .unwrap_or_default();
    assert!(err.contains("$with: 1 is not a map."), "{err}");
    let err = load(
        "key",
        "@use \"sass:meta\";\n@include meta.load-css(\"other\", $with: (1: null));\n",
    )
    .err()
    .unwrap_or_default();
    assert!(err.contains("$with key: 1 is not a string."), "{err}");
    let err = load("few", "@use \"sass:meta\";\n@include meta.load-css();\n")
        .err()
        .unwrap_or_default();
    assert!(err.contains("Missing argument $url."), "{err}");
}

#[test]
fn configured_twice_and_already_loaded() {
    // `The variable $a-b was configured twice.` (normalized collision)
    let err = load(
        "tw",
        "@use \"sass:meta\";\n@include meta.load-css(\"other\", $with: (a-b: c, a_b: c));\n",
    )
    .err()
    .unwrap_or_default();
    assert!(err.contains("was configured twice."), "{err}");
}
