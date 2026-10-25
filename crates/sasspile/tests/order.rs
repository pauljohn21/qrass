//! Document order is a property of the **stream**, not of a key. Up to `Render` the chain is serial,
//! so rules reach it in the order their blocks settle (see `exec::State::step`: a pending
//! declaration run settles when a nested block opens, the rest when a block closes). No `order`
//! field, no `collect`, no `sort_by_key`, and no per-rule task hand-off — real concurrency returns
//! only at P5 (independent module subgraphs), where the extra constraint is that a queued inner
//! observable must not have produced anything before it was subscribed.
//!
//! The multi-thread runtime below is kept deliberately: it proves the pipeline is order-correct even
//! when the caller drives it from several workers.

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
    // A child block closes before its parent, yet the parent's own declaration run leads — it
    // settles when the child opens, not when the parent closes.
    assert_eq!(
        css("a { p: 1; b { q: 2 } c { r: 3 } }".to_string()).await,
        "a {\n  p: 1;\n}\na b {\n  q: 2;\n}\na c {\n  r: 3;\n}\n"
    );
}
