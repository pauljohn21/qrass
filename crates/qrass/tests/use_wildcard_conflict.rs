//! OpenSpec converge-sass-spec-to-100 M4: two different origin files that
//! both export the same member name through `as *` make the *bare reference*
//! fail with dart's `This {variable,function,mixin} is available from
//! multiple global modules.` (`directives/use/error/member/*`; identical
//! values still conflict). One upstream reached both directly and through a
//! `@forward` keeps a single origin and must NOT conflict (element-plus
//! `common/var.scss` chains), and exports never leak `@use`-injected vars.

use qrass::error::SassError;
use qrass::{compile, Options, OutputStyle};

fn sass(root: &str, files: &[(&str, &str)]) -> Result<String, SassError> {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "qrass-wf-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, body) in files {
        std::fs::write(dir.join(name), body).unwrap();
    }
    std::fs::write(dir.join("input.scss"), root).unwrap();
    let out = compile(
        &dir.join("input.scss"),
        &Options {
            style: OutputStyle::Expanded,
            load_paths: vec![dir.clone()],
        },
    );
    let _ = std::fs::remove_dir_all(&dir);
    out.map(|css| css.trim().to_string())
}

const O1: &str = "$member: from other1;\n@function member() { @return from other1 }\n@mixin member { x {y: m1} }\n";
const O2: &str = "$member: from other2;\n@function member() { @return from other2 }\n@mixin member { x {y: m2} }\n";

#[test]
fn conflicting_variable_is_rejected() {
    let err = sass(
        "@use \"other1\" as *;\n@use \"other2\" as *;\na {b: $member}\n",
        &[("_other1.scss", O1), ("_other2.scss", O2)],
    )
    .expect_err("conflicting wildcard variable must fail");
    assert!(
        err.message.contains("This variable is available from multiple global modules."),
        "{}",
        err.message
    );
}

#[test]
fn conflicting_function_is_rejected() {
    let err = sass(
        "@use \"other1\" as *;\n@use \"other2\" as *;\na {b: member()}\n",
        &[("_other1.scss", O1), ("_other2.scss", O2)],
    )
    .expect_err("conflicting wildcard function must fail");
    assert!(
        err.message.contains("This function is available from multiple global modules."),
        "{}",
        err.message
    );
}

#[test]
fn conflicting_mixin_is_rejected() {
    let err = sass(
        "@use \"other1\" as *;\n@use \"other2\" as *;\na {@include member}\n",
        &[("_other1.scss", O1), ("_other2.scss", O2)],
    )
    .expect_err("conflicting wildcard mixin must fail");
    assert!(
        err.message.contains("This mixin is available from multiple global modules."),
        "{}",
        err.message
    );
}

#[test]
fn single_wildcard_module_still_resolves() {
    // The conflict is cross-origin only: one `as *` keeps working normally.
    let css = sass(
        "@use \"other1\" as *;\na {b: $member; c: member(); @include member}\n",
        &[("_other1.scss", O1)],
    )
    .expect("single wildcard module resolves");
    assert!(css.contains("from other1"), "{css}");
    assert!(css.contains("y: m1"), "{css}");
}

#[test]
fn same_value_across_modules_still_conflicts() {
    // `same_value/*` goldens: identical exports from two files conflict too.
    let err = sass(
        "@use \"other1\" as *;\n@use \"other2\" as *;\na {b: $member}\n",
        &[
            ("_other1.scss", "$member: same;\n"),
            ("_other2.scss", "$member: same;\n"),
        ],
    )
    .expect_err("same-valued dual export still conflicts");
    assert!(err.message.contains("multiple global modules"), "{}", err.message);
}

#[test]
fn forwarded_and_direct_reach_share_one_origin() {
    // element-plus shape: `up` defines, `mid` forwards, both are `@use as *`d.
    // One upstream member reached twice is NOT a conflict (the old naive
    // cross-module detection regressed the whole element-plus e2e on this).
    let css = sass(
        "@use \"up\" as *;\n@use \"mid\" as *;\na {b: $shared; @include shared}\n",
        &[
            ("_up.scss", "$shared: v;\n@mixin shared { x {y: s} }\n"),
            ("_mid.scss", "@forward \"up\";\n"),
        ],
    )
    .expect("dual reach of one origin does not conflict");
    assert!(css.contains("b: v"), "{css}");
    assert!(css.contains("y: s"), "{css}");
}

#[test]
fn use_star_does_not_leak_vars_into_module_exports() {
    // dart exports only the module's OWN top-level assignments; `@use … as *`
    // vars are locally visible but never re-exported (re-export needs
    // `@forward`). An importer must not see the mid module's injected vars.
    let err = sass(
        "@use \"mid2\" as *;\na {b: $injected}\n",
        &[
            ("_leaf.scss", "$injected: no-peek;\n"),
            ("_mid2.scss", "@use \"leaf\" as *;\n$own: yes;\n"),
        ],
    )
    .expect_err("injected upstream var is not exported through mid");
    assert!(err.message.contains("Undefined variable"), "{}", err.message);
}
