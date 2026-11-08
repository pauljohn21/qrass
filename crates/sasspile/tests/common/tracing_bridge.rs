//! 开发期诊断视图在此取得订阅者 —— 这里是唯一决定一次运行如何被观察的地方，所以任何 `src/` 文件
//! 都不点名 OTel 类型（管线只用 `tracing` 宏）。
//!
//! 带 `--features trace` 时，既有 span 被桥接到 OpenTelemetry，并由 `opentelemetry-stdout` 打印：
//! 每个 span 在结束时写出、带上它的 `parent` id，于是 `compile → phase → decl` 无需 collector
//! 或网络跳就能嵌套。不带该 feature 时，同一条代码路径装一个普通 `fmt` 订阅者。两种情形下
//! 详细程度都由 `RUST_LOG` 决定，而默认停在 `info`，好让普通夹具运行不比从前更吵。
//!
//! OTel 那几个 crate 是 `[dev-dependencies]`：只到测试与示例，绝不进库。Cargo 无法把 dev-dependency
//! 做成条件依赖，所以 `trace` 门控的是*接线*，不是编译 —— 这里如实写明，而不是假装一个 feature
//! 能剥掉构建。

/// 环境没表态时的默认值：`info`，与夹具在任何 tracing 工作之前所记的一致，好让打开 `trace`
/// 永远不会悄悄把调用方埋进 DEBUG 输出。
fn filter() -> tracing_subscriber::filter::EnvFilter {
    tracing_subscriber::filter::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("info"))
}

/// 装一次订阅者。`try_init` 让它在测试已装过订阅者时依然安全。
#[cfg(feature = "trace")]
pub fn init() {
    use opentelemetry::trace::TracerProvider as _;
    // 三者都是 trait：`Layer` 提供 `with_filter`、`SubscriberExt` 提供分层、
    // `SubscriberInitExt` 提供在已存在订阅者时依然安全的 `.try_init()`。
    use tracing_subscriber::{
        layer::{Layer as _, SubscriberExt as _},
        util::SubscriberInitExt as _,
    };
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
        .build();
    // tracer 通过 `Arc` 握住整条管线，所以此后不再需要 provider 句柄；丢掉它会在 span 写完之前
    // 就把 exporter 关掉。
    let tracer = provider.tracer("sasspile");
    std::mem::forget(provider);
    let _ = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(tracing_subscriber::fmt::layer().with_filter(filter()))
        .try_init();
}

#[cfg(not(feature = "trace"))]
pub fn init() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter())
        .try_init();
}
