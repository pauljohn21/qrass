//! OpenSpec converge-sass-spec-to-100 M4 slice 3: a namespaced assignment
//! `ns.$var: value` at top level mutates the **defining module's** live cell
//! (`forward/member` `bare` — "Namespaced assignments always assign to the
//! defining module"). Every read path sees it afterwards: the ns view itself,
//! other importers of the same origin, and the defining module's own
//! functions (dart module vars are shared-mutable, not snapshot copies).

use qrass::compile;
use qrass::{error::SassError, Options, OutputStyle};

fn sass(root: &str, files: &[(&str, &str)]) -> Result<String, SassError> {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "qrass-fm-{}-{}",
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

const UP: &str = "$a: old;\n@function get-a() { @return $a }\n";

#[test]
fn ns_write_reaches_forwarding_importer_reads() {
    // `mid` forwards `up`; writing `mid.$a` must be visible through `mid.$a`
    // AND through `mid.get-a()` (the defining module's function body).
    let css = sass(
        "@use \"mid\";\nmid.$a: new;\nb {c: mid.$a; d: mid.get-a()}\n",
        &[("_up.scss", UP), ("_mid.scss", "@forward \"up\";\n")],
    )
    .expect("namespaced write");
    assert!(css.contains("c: new"), "{css}");
    assert!(css.contains("d: new"), "{css}");
}

#[test]
fn ns_write_is_visible_to_a_second_importer_of_the_origin() {
    // Two importers share one origin cell: `up`'s own read also sees it.
    let css = sass(
        "@use \"up\";\n@use \"mid\";\nmid.$a: new;\nb {c: up.$a; d: mid.get-a()}\n",
        &[("_up.scss", UP), ("_mid.scss", "@forward \"up\";\n")],
    )
    .expect("cross-importer visibility");
    assert!(css.contains("c: new"), "{css}");
    assert!(css.contains("d: new"), "{css}");
}

#[test]
fn direct_ns_write_updates_the_defining_module() {
    // Without any forwarder: `up.$a: x` mutates up's cell; its function reads
    // the updated value (`forward/member` `bare` `variable/*`).
    let css = sass(
        "@use \"up\";\nup.$a: set;\nb {c: up.$a; d: up.get-a()}\n",
        &[("_up.scss", UP)],
    )
    .expect("direct namespaced write");
    assert!(css.contains("c: set"), "{css}");
    assert!(css.contains("d: set"), "{css}");
}

#[test]
fn plain_local_assignment_is_not_a_member_write() {
    // `$x: v` with no dot keeps today's local scoping (no store involved).
    let css = sass("$a: one;\nb {c: $a}\n", &[]).expect("plain assignment");
    assert!(css.contains("c: one"), "{css}");
}

#[test]
fn prefixed_forward_write_reaches_the_base_variable() {
    // `@forward … as d-*` renames export; writing the renamed name mutates
    // the DEFINING module's base variable (`forward/member/as`
    // `variable_assignment/top_level`: `midstream.$d-a: new` then
    // `midstream.d-get-a()` reads `new value`).
    let css = sass(
        "@use \"mid\";\nmid.$d-a: new;\nb {c: mid.d-get-a(); d: mid.$d-a}\n",
        &[
            ("_up.scss", "$a: old;\n@function get-a() { @return $a }\n"),
            ("_mid.scss", "@forward \"up\" as d-*;\n"),
        ],
    )
    .expect("prefixed write-through");
    assert!(css.contains("c: new"), "{css}");
    assert!(css.contains("d: new"), "{css}");
}

#[test]
fn show_hide_name_renamed_members() {
    // rename applies FIRST, `show`/`hide` name the renamed members
    // (`as b-* show b-a` keeps; `-`/`_` interchangeable per `show b_a`).
    let css = sass(
        "@use \"mid\";\n@include mid.b-a;\n",
        &[
            ("_up.scss", "@mixin a() {c {d: e}}\n"),
            ("_mid.scss", "@forward \"up\" as b-* show b_a;\n"),
        ],
    )
    .expect("show on renamed name");
    assert!(css.contains("d: e"), "{css}");
}

#[test]
fn separator_equivalence_on_prefixed_reads() {
    // `as d_*` + `mid.$d-c` reads the prefixed variable across separators
    // (`as` `different_separator`).
    let css = sass(
        "@use \"mid\";\na {b: mid.$d-c}\n",
        &[("_up.scss", "$c: e;\n"), ("_mid.scss", "@forward \"up\" as d_*;\n")],
    )
    .expect("-/_ interchangeable");
    assert!(css.contains("b: e"), "{css}");
}
