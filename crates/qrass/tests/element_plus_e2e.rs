//! element-plus end-to-end golden conformance (task 10.2 / 10.4). Compiles the
//! real `element-plus/packages/theme-chalk/src/index.scss` through the public
//! [`qrass::compile`] surface and runs two cheap smoke gates plus a strict
//! golden comparison:
//!
//! * `element_plus_compiles_nonempty` — the pipeline must finish with zero
//!   errors and non-empty CSS;
//! * `element_plus_css_is_well_formed` — [`css_validate`] structural checks;
//! * `element_plus_component_coverage` — every `.el-*` class selector present
//!   in the golden baseline must also appear in our output;
//! * `element_plus_matches_golden` — whitespace-normalized line-for-line
//!   equality against the dart-sass `index.css` baseline.
//!
//! All four are named `element_plus*` so `cargo test -p qrass element_plus`
//! selects exactly this suite.

mod common;

use common::{css_diff, css_validate};
use qrass::{compile, Options};
use std::collections::HashSet;
use tracing::info;

/// Compile the element-plus entry to expanded CSS, or panic with the error.
fn compiled() -> String {
    let out = compile(&common::ep_entry(), &Options::default())
        .expect("element-plus index.scss must compile without errors");
    assert!(!out.trim().is_empty(), "compiled CSS must be non-empty");
    out
}

/// Read the golden baseline as a String (panics with the path if missing).
fn golden_text(rel: &str) -> String {
    let path = common::golden(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing golden {}: {e}", path.display()))
}

/// Collect every `.el-*` class-selector token from a CSS document.
fn el_selectors(css: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    let chars: Vec<char> = css.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        // A class token starts at `.el-` and continues over [A-Za-z0-9_-].
        if chars[i] == '.'
            && chars.get(i + 1) == Some(&'e')
            && chars.get(i + 2) == Some(&'l')
            && chars.get(i + 3) == Some(&'-')
        {
            let start = i;
            let mut j = i + 4;
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric() || chars[j] == '-' || chars[j] == '_')
            {
                j += 1;
            }
            set.insert(chars[start..j].iter().collect());
            i = j;
        } else {
            i += 1;
        }
    }
    set
}

#[test]
fn element_plus_compiles_nonempty() {
    let css = compiled();
    info!(len = css.len(), lines = css.lines().count(), "element-plus compiled");
}

#[test]
fn element_plus_css_is_well_formed() {
    let css = compiled();
    if let Err(e) = css_validate::validate(&css) {
        panic!("element-plus CSS failed structural validation: {e}");
    }
}

#[test]
fn element_plus_component_coverage() {
    let css = compiled();
    let golden = golden_text("element-plus/index.css");
    let ours = el_selectors(&css);
    let want = el_selectors(&golden);
    let missing: Vec<&String> = want.iter().filter(|s| !ours.contains(*s)).collect();
    assert!(
        missing.is_empty(),
        "{} `.el-*` selectors from golden are missing in our output, e.g. {:?}",
        missing.len(),
        &missing[..missing.len().min(20)]
    );
}

#[test]
fn element_plus_matches_golden() {
    let css = compiled();
    let golden = golden_text("element-plus/index.css");
    let diffs = css_diff::diff_report(&css, &golden);
    if let Some((diff, ctx_golden, ctx_ours)) = css_diff::first_diff(&css, &golden, 4) {
        panic!(
            "element-plus output diverges from golden: {} differing line(s), first at:\n{}\ngolden context:\n  {}\nour context:\n  {}",
            diffs.len(),
            diff.describe(),
            ctx_golden.join("\n  "),
            ctx_ours.join("\n  "),
        );
    }
}
