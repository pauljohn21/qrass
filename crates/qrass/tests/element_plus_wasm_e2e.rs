//! element-plus through the *preloaded* load path — the browser scenario
//! pinned on the host (task 10.2 payload, wasm contract): register the whole
//! `theme-chalk/src` SCSS tree in [`qrass::vfs`], then compile the real entry.
//! Every read is served from the registry (virtual-first), exactly what a
//! browser does with no file system at all; the output must still match the
//! official golden line-for-line. The genuine zero-`std::fs` proof of the same
//! payload runs in wasm under node (see the round's smoke); this pin keeps the
//! resolution order, the module bus and the fan-out honest over virtual files.

mod common;

use common::{css_diff, ep_entry, golden};
use qrass::{Options, compile, vfs};
use std::path::{Path, PathBuf};

/// Every `.scss` file under `dir`, in whatever order the walk yields.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("theme-chalk src readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("scss") {
            out.push(path);
        }
    }
}

#[test]
fn element_plus_compiles_with_the_whole_tree_preloaded() {
    let root = ep_entry().parent().expect("entry has a directory").to_path_buf();
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(files.len() > 100, "expected the full SCSS tree, got {}", files.len());
    for file in &files {
        vfs::register(file, std::fs::read_to_string(file).expect("source readable"));
    }

    // Virtual-first: each registered path resolves and reads from the registry.
    let css = compile(&ep_entry(), &Options::default())
        .expect("element-plus must compile from the preloaded tree");
    let baseline =
        std::fs::read_to_string(golden("element-plus/index.css")).expect("golden baseline");
    let diffs = css_diff::diff_report(&css, &baseline);
    assert!(
        diffs.is_empty(),
        "preloaded compile diverges from golden in {} line(s), first: {:?}",
        diffs.len(),
        diffs.first()
    );

    vfs::clear();
}
