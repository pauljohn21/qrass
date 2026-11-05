//! `sasslint` CLI：扫一个目录（默认 `crates/sasspile/src`）找铁律违规；逐条用 `tracing` 报告；只要
//! 找到就非零退出（一道 CI 闸）。
//!
//! 文档里的调用形式是 `sasslint lint <dir>`（`rust.md`），而这层壳的第一版把 `argv[1]` 读成**路径**——
//! 于是 `sasslint lint crates/sasspile/src` 去找一个叫 `lint` 的目录，什么也没找到，就永远报 `count=0`：
//! 一道永远通过的闸比没有闸更糟，因为它看着像一张健康证明，而同一份代码上的
//! `gate_sasspile_is_clean` 测试是红的。所以这个动词要被消耗掉，同时位置参数里的路径仍然接受。

use std::path::PathBuf;
use std::process::exit;

use sasslint::scan_dir;

fn main() {
    trace_setup();

    let arguments = std::env::args().skip(1).collect::<Vec<String>>();
    let root = arguments
        .iter()
        .find(|argument| argument.as_str() != "lint")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("crates/sasspile/src"));

    let span = tracing::info_span!("sasslint", path = %root.display());
    let _guard = span.enter();

    if !root.exists() {
        tracing::error!(path = %root.display(), "no such path — nothing was linted");
        exit(2);
    }

    let findings = scan_dir(&root);
    findings.iter().for_each(|f| {
        tracing::warn!(rule = f.rule, file = %f.file, line = f.line, text = %f.text, "violation")
    });
    tracing::info!(count = findings.len(), "sasslint complete");

    if !findings.is_empty() {
        exit(1);
    }
}

/// 环境所要求的过滤器，默认 `info`，好让闸的输出形状不变。
fn filter() -> tracing_subscriber::filter::EnvFilter {
    tracing_subscriber::filter::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("info"))
}

/// 装上 subscriber。开了 `--features trace` 时，`sasslint` 这个 span（它带着被扫的路径）也会桥到
/// OpenTelemetry 并由 `opentelemetry-stdout` 打印，于是“这道闸花了多久、扫的是什么”不必从进程外掐表
/// 也能读到。
#[cfg(feature = "trace")]
fn trace_setup() {
    use opentelemetry::trace::TracerProvider as _;
    use tracing_subscriber::{
        layer::{Layer as _, SubscriberExt as _},
        util::SubscriberInitExt as _,
    };
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
        .build();
    let tracer = provider.tracer("sasslint");
    // tracer 通过一个 `Arc` 持有这条管线；把 provider drop 掉会在 span 写出之前就关掉 exporter。
    std::mem::forget(provider);
    let _ = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(tracing_subscriber::fmt::layer().with_filter(filter()))
        .try_init();
}

#[cfg(not(feature = "trace"))]
fn trace_setup() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter())
        .try_init();
}
