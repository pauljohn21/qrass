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

/// Round-81 (`get_module.hrx` + `meta/load/*`): the module first-class value
/// — inspect spellings, `type-of`, identity equality, the arity/type error
/// family, and `meta.load` returning the reference without emitting CSS.
#[test]
fn module_reference_values() {
    vfs::clear();
    let out = compile_string(
        "/lc/gm.scss".to_string(),
        "@use \"sass:meta\";\n@use \"sass:math\";\n\n\
         a {
           inspect: meta.inspect(meta.get-module(meta));
           ty: meta.type-of(meta.get-module(math));
           eq: meta.get-module(meta) == meta.get-module(meta);
           ne: meta.get-module(meta) == meta.get-module(color);
           gve: meta.global-variable-exists(\"e\", $module: meta.get-module(math));
           loaded: meta.inspect(meta.load(\"sass:color\"));
         }",
        &Options::default(),
    )
    .expect("compile ok");
    assert!(out.contains("inspect: get-module(\"meta\");"), "{out}");
    assert!(out.contains("ty: module"), "{out}");
    assert!(out.contains("eq: true"), "{out}");
    assert!(out.contains("ne: false"), "{out}");
    assert!(out.contains("gve: true"), "{out}");
    assert!(out.contains("loaded: get-module(\"color\");"), "{out}");
    vfs::clear();
}

#[test]
fn get_module_error_spellings() {
    let err = load(
        "gmnull",
        "@use \"sass:meta\";\na {b: meta.get-module(null)}\n",
    )
    .err()
    .unwrap_or_default();
    assert!(
        err.contains("$module: null is neither a string nor a module reference."),
        "{err}"
    );
    let err = load("gmfew", "@use \"sass:meta\";\na {b: meta.get-module()}\n")
        .err()
        .unwrap_or_default();
    assert!(err.contains("Missing argument $module."), "{err}");
    let err = load(
        "gmmany",
        "@use \"sass:meta\";\na {b: meta.get-module(c, d)}\n",
    )
    .err()
    .unwrap_or_default();
    assert!(
        err.contains("Only 1 argument allowed, but 2 were passed."),
        "{err}"
    );
    let err = load(
        "gmnone",
        "@use \"sass:meta\";\na {b: meta.get-module(does-not-exist)}\n",
    )
    .err()
    .unwrap_or_default();
    assert!(
        err.contains("There is no module with namespace \"does-not-exist\"."),
        "{err}"
    );
}

#[test]
fn with_empty_map_loads() {
    // `load_css/with/empty`: `$with: ()` is dart's empty-map coercion of the
    // empty literal — the module loads unchanged (OpenSpec task 5.3).
    vfs::clear();
    vfs::register("/lc/_plain.scss", "a {b: c}\n");
    let body = "@use \"sass:meta\";\n@include meta.load-css(\"plain\", $with: ());\n";
    let out = compile_string(
        "/lc/idx.scss".to_string(),
        body.to_string(),
        &Options::default(),
    )
    .expect("$with: () must coerce to the empty map");
    assert!(out.contains("b: c"), "{out}");
    vfs::clear();
}
