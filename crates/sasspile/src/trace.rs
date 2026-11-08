//! 流水线自己的诊断外壳 —— 唯一决定「什么是可观测的」的地方，所以其他模块除宏之外不 import `tracing` 类型。
//!
//! 两个层级，因为它们回答不同的问题：
//! * [`Phase`] —— 一条编译阶段，按条目数与毫秒计。它必须活得比 `subscribe` 长，所以句柄住在该阶段的
//!   observer 里（见 `crate::phases`）；
//! * [`ValueRead`] —— 一条声明的值：源码写了什么、它变成了什么、以及本编译器到底读没读它。每一轮
//!   合规对到最后都在问这个问题（「为什么这行还是逐字？」），而今天回答它意味着加一个临时探针并重编。
//!
//! 两者都在 `DEBUG` 打开，所以正常运行只付字段借用的代价，由 `RUST_LOG` 拍板。
//!
//! 字段*清单*是 `.qoder/rules/rust.md` 里的流水线约定，这里不复述；这里定死的是**每个字段在每个面上
//! 指什么**，因为一行日志只有在同名永远同义时才可搜：
//! * `phase` span —— `stage` 是阶段，`module` 是它驱动工作的代码模块，`items` 与 `elapsed_ms`
//!   是其产出与耗时。它不带 `id`：每次编译每个阶段恰好一条，所以命名「哪份程序」的身份在外层
//!   `compile` span（[`source_label`]）上，四个阶段嵌套其下；
//! * `value` span —— `module` 是该值所属的属性或变量，`id` 是它所在的块，另有 `raw_value`/`resolved`
//!   这一对，把 decline 显示成 decline 而不是一段空字符串；
//! * 决策事件（`call`、`calculation read`）—— `module` 是说话的子阶段。
//!
//! 两个面共用 `module` 但指称不同 —— 阶段的代码模块与值的所属属性 —— 因为它们回答不同的问题
//! （`stage`+`module` 串起流水线阶段，值那一对只问一行 CSS）；这一点写明而不是留白。没有任何面把
//! *选择器*放进 `module`：选择器有自己的名字，而一个含义逐行变化的字段是没人能 grep 的字段。

/// 流在 trace 里被称呼用的标签：第一行非空行，截短。这里不编造源码的其他信息，所以空程序明说，
/// 而不是打一个空字段。
pub(crate) fn source_label(program: &str) -> String {
    let first = program.lines().find(|line| !line.trim().is_empty());
    first.map_or_else(
        || "<empty>".to_string(),
        |line| line.trim().chars().take(40).collect::<String>(),
    )
}

/// 一个编译阶段：span 加上 observer 创建时起的时钟。
pub(crate) struct Phase {
    span: tracing::Span,
    started: std::time::Instant,
}

impl Phase {
    /// `stage` 是流水线里的阶段，`module` 是干这份工作的那个代码模块 —— 两者不同，
    /// 因为 `Render` 序列化的正是 `crate::exec` 建好的东西。
    pub(crate) fn new(stage: &'static str, module: &'static str) -> Self {
        Self {
            span: tracing::debug_span!(
                "phase",
                stage,
                module,
                items = tracing::field::Empty,
                elapsed_ms = tracing::field::Empty
            ),
            started: std::time::Instant::now(),
        }
    }

    /// 在其下干活的 span，好让阶段发出的东西嵌套在它里面。
    pub(crate) fn enter(&self) -> tracing::span::Entered<'_> {
        self.span.enter()
    }

    /// 结束这个阶段：记下它的产出与耗时，然后放掉这个句柄 —— 放掉就是关闭 span。把 span 换成空的，
    /// 也让第二次调用（一个既失败*又*走完的阶段）成为 no-op，而不是二次关闭。
    ///
    /// 耗时是带小数的毫秒：强制字段名是 `elapsed_ms`，但整数毫秒会把每个小阶段都报成 `0`，
    /// 读起来像「瞬时」，那是谎。
    pub(crate) fn close(&mut self, items: usize) {
        self.span.record("items", items as i64);
        self.span.record("elapsed_ms", millis(self.started));
        self.span = tracing::Span::none();
    }
}

/// 带小数的毫秒时长 —— 流水线约定点名的单位。
fn millis(since: std::time::Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1_000.0
}

/// 一条声明值的读取。关闭发生在 drop，所以 `?` 的路径也被记录 —— 失败恰恰是诊断必须不吞的情形。
pub(crate) struct ValueRead {
    span: tracing::Span,
    started: std::time::Instant,
}

impl ValueRead {
    /// `module` 是该值所属的东西（属性名，或绑定所命名的变量），`id` 是它所在的块，
    /// `raw_value` 是这段 run 的源码文本。
    pub(crate) fn begin(module: &str, id: &str, raw_value: &str) -> Self {
        Self {
            span: tracing::debug_span!(
                "value",
                module,
                id,
                raw_value,
                resolved = tracing::field::Empty,
                elapsed_ms = tracing::field::Empty
            ),
            started: std::time::Instant::now(),
        }
    }

    /// 这次读取产出了什么：一个值、一次显式 decline、或一个走 `Err` 通道的失败。
    pub(crate) fn record(
        self,
        outcome: &Result<Option<crate::value::Value>, crate::SassError>,
    ) -> Self {
        let resolved = match outcome {
            Ok(Some(value)) => value.to_css(),
            Ok(None) => "<declined: kept verbatim>".to_string(),
            Err(error) => format!("<error: {}>", error.message),
        };
        self.span.record("resolved", resolved.as_str());
        self
    }
}

impl Drop for ValueRead {
    fn drop(&mut self) {
        self.span.record("elapsed_ms", millis(self.started));
    }
}
