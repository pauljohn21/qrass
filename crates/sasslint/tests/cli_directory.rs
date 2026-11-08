//! CLI 必须扫**它被给定的那个目录**。第一版把 `argv[1]` 读成路径，于是文档里的
//! `sasslint lint crates/sasspile/src` 去扫了一个叫 `lint` 的不存在目录，打印 `count=0`，而同一份
//! 代码让 `gate_sasspile_is_clean` 变红。永远通过的闸比没有闸更糟，所以两半都钉在这里：具名目录
//! 里的植入违规必须非零退出，干净目录必须零退出。

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn scratch_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sasslint-cli-{label}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn the_lint_verb_lints_the_given_directory() {
    let dirty = scratch_dir("dirty");
    // 一个 batch shell：同行里出现 `scan_map(` 与 `Vec<` 正是 `no-batch-shell` 自己的签名——按规则
    // 原文的写法写。（`Vec::<u8>` turbofish **不**含 `Vec<`，本测试的第一稿就是这么植入了一个
    // 本就合规的违规。）
    fs::write(
        dirty.join("bad.rs"),
        "acc = acc.scan_map(|a, tok| -> Vec<Tok> { a.feed(tok) });\n",
    )
    .unwrap();

    let failure = Command::new(env!("CARGO_BIN_EXE_sasslint"))
        .args(["lint", dirty.to_str().unwrap()])
        .output()
        .expect("run sasslint");
    assert!(
        !failure.status.code().is_some_and(|code| code == 0),
        "a planted violation in the named directory must not pass: {:?}",
        String::from_utf8_lossy(&failure.stderr)
    );

    let clean = scratch_dir("clean");
    fs::write(clean.join("good.rs"), "let a = 1;\n").unwrap();
    let pass = Command::new(env!("CARGO_BIN_EXE_sasslint"))
        .args(["lint", clean.to_str().unwrap()])
        .output()
        .expect("run sasslint");
    assert!(
        pass.status.success(),
        "a clean directory must exit 0: {:?}",
        String::from_utf8_lossy(&pass.stderr)
    );

    // 缺陷的另一半：不存在的路径*不是*一张健康证明。
    let missing = Command::new(env!("CARGO_BIN_EXE_sasslint"))
        .args(["lint", dirty.join("nope").to_str().unwrap()])
        .output()
        .expect("run sasslint");
    assert!(
        !missing.status.success(),
        "a nonexistent directory must fail loudly instead of reporting count=0"
    );

    fs::remove_dir_all(dirty).ok();
    fs::remove_dir_all(clean).ok();
}
