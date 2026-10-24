//! Diagnostic probe: prints the per-value `value_to_css` result (and a compile) under
//! a `tracing` span tree so the value-layer divergences are visible, not guessed.
//! Run:  RUST_LOG=debug cargo test -p sasspile --test trace_probe -- --nocapture

use sasspile::{Env, Value, compile_string, value_to_css};

#[test]
fn dump_value_trace() {
    #[cfg(feature = "trace")]
    sasspile::init_trace();
    #[cfg(not(feature = "trace"))]
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".into()),
        )
        .with_test_writer()
        .try_init()
        .ok();

    let mut env = Env::new();
    env.insert("k".into(), Value::Raw("2".into()));
    env.insert("x".into(), Value::Raw("3px".into()));

    for src in [
        "col-#{$k}",
        "var(--c ,)",
        "-a-calc($c)",
        "1px    2px",
        "#{$nope}",
        "$x",
        "url(http://x)",
    ] {
        let out = value_to_css(src, &env);
        tracing::info!(src, ?out, "VALUE_PROBE");
    }

    let css = compile_string("a { b: c; }");
    tracing::info!(?css, "COMPILE_PROBE");
}
