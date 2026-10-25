//! The concurrency contract. Items are flattened and then worked on **independently**:
//! `flat_map` defaults to `usize::MAX` concurrency and `Shared::from_stream` hands its stream
//! to the scheduler, which on native is `tokio::spawn`. So the merge must still come out in
//! exact document order — carried by the owned `order` key, not by a serialised fan-out and
//! not by recursive descent.

use sasspile::compile;

async fn css(input: String) -> String {
    compile(input).await.expect("concurrent pipeline compiles")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_independent_items_still_merge_in_document_order() {
    // TEMPORARY evidence instrumentation: prove the fan-out really runs on several threads.
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
    // A child block closes before its parent, yet the parent's own declarations must lead.
    assert_eq!(
        css("a { p: 1; b { q: 2 } c { r: 3 } }".to_string()).await,
        "a {\n  p: 1;\n}\na b {\n  q: 2;\n}\na c {\n  r: 3;\n}\n"
    );
}
