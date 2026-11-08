//! 文档序是**流**的属性，不是键的属性。到 `Render` 为止链条是串行的，所以规则按各自块
//! 落定的次序到达（见 `exec::State::step`：待结的 declaration 运行在嵌套块开启时落定，
//! 其余在块闭合时）。没有 `order` 字段、没有 `collect`、没有 `sort_by_key`、也没有逐规则
//! 的任务交接 —— 真正的并发只随 P5（独立模块子图）回来，那里的额外约束是排队的内层
//! observable 在被订阅之前不得已产出任何东西。
//!
//! 下面有意保留多线程运行时：它证明即使调用方从多个 worker 驱动，管线依然保持序正确。

use sasspile::compile;

async fn css(input: String) -> String {
    compile(input).await.expect("concurrent pipeline compiles")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_independent_items_still_merge_in_document_order() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let scss = (1..=32)
        .map(|n| format!(".c{n} {{ o: {n}; }}"))
        .collect::<String>();
    let want = (1..=32)
        .map(|n| format!(".c{n} {{\n  o: {n};\n}}\n"))
        .collect::<String>();
    assert_eq!(css(scss).await, want);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn nested_blocks_close_early_but_still_merge_parent_first() {
    // 子块比父块先闭合，然而父块自己的 declaration 运行仍在前 —— 它随子块开启而落定，
    // 而不是随父块闭合。
    assert_eq!(
        css("a { p: 1; b { q: 2 } c { r: 3 } }".to_string()).await,
        "a {\n  p: 1;\n}\na b {\n  q: 2;\n}\na c {\n  r: 3;\n}\n"
    );
}
