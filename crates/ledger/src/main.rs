//! 账本 CLI 的薄壳：tracing 订阅者的装配 + 参数的接线；所有逻辑都住在 `ledger` 库里
//! （与 hrx runner 共用）。
//!
//! 开 `--features trace` 时这一次 run 还会有一份 OpenTelemetry 视图（见 [`trace_setup`]）。那个根 span
//! 不是装饰：`tracing-opentelemetry` 导出的是 **span**，而这层壳从前一个都不建，所以一次“带 trace 的”
//! CLI run 会导出一条空 trace。每次调用一个 span、带着哪条命令对着哪个数据库跑，是“这次调用干了什么”
//! 这个读数至少需要的东西。

use rusqlite::Connection;
use std::path::Path;
use tracing::info;

fn main() {
    trace_setup();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db = args
        .iter()
        .find(|a| a.starts_with("--db="))
        .map_or(ledger::DB, |a| a.trim_start_matches("--db="));
    let argv: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--db="))
        .cloned()
        .collect();
    let command = argv.first().map(String::as_str).unwrap_or("<none>");
    let span = tracing::info_span!("ledger", command, db = %db);
    let _guard = span.enter();
    match Connection::open(Path::new(db)).and_then(|conn| ledger::run(&conn, &argv)) {
        Ok(()) => {}
        Err(e) => info!(?e, "ledger error"),
    }
}

/// 环境要求的过滤器，默认 `info`，好让一次普通 run 打出的东西与任何 tracing 工作之前完全一样。
///
/// ANSI 样式是故意关掉的：这个二进制的输出**就是**那个读数，而混在里面的颜色代码弄坏了每一回对它下手的
/// `sed`/`awk` 切片（一个会话里三次，每次都产出一个看着合理的 `0`，非得靠阳性对照才抓得住）。
fn filter() -> tracing_subscriber::filter::EnvFilter {
    tracing_subscriber::filter::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("info"))
}

/// 装上 subscriber。开了 `trace` feature 时，这次 run 的 span 还额外桥到 OpenTelemetry，由
/// `opentelemetry-stdout` 打印，它不需要 collector 也不需要网络。
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
    let tracer = provider.tracer("ledger");
    // tracer 通过一个 `Arc` 持有 exporter 管线；在这里 drop 掉 provider 句柄会在根 span 来得及导出之前
    // 就把它关掉。
    std::mem::forget(provider);
    let _ = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_ansi(false)
                .with_filter(filter()),
        )
        .try_init();
}

#[cfg(not(feature = "trace"))]
fn trace_setup() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter())
        .with_target(false)
        .with_ansi(false)
        .try_init();
}
