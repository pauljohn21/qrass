//! sasspile —— 一个按**推（rxrust）流**架构的净室 SCSS→CSS 编译器：源文本把 owned 值*向前*推过
//! 各阶段内核（`from_iter → transform ×Lexer/Structurize/Exec/Render → reduce_initial →
//! into_future`），**从不**被当成实体化 AST 拉取、遍历。
//!
//! 采纳推式教条（`dart-sass = pull / rxrust = push`）之后，本 crate 被重置为从零开始的骨架；
//! 旧的拉式代码（parse→AST→递归下降）已删除，以防退回。流水线按
//! `openspec/changes/redesign-sasspile-rxrust-shared-stream/design.md` 逐阶段重建（P0–P8），
//! 受 `.qoder/rules/rust.md` 约束；行为基准**只有** `sass-spec/*.hrx` + `element-plus`。

use std::fmt;

use rxrust::prelude::*;

mod axes;
mod brackets;
mod builtins;
mod calc;
mod calculation;
mod calls;
mod color;
mod env;
mod exec;
mod expr;
mod lexer;
mod loops;
mod operations;
mod phases;
mod scope;
mod selector;
mod structure;
mod trace;
mod unit;
mod value;

use crate::phases::{Exec, Lexer, Render, Structurize};

/// 编译为什么失败。这个区分不是装饰：语料对不同类别的处理不同 —— 未绑定变量必须中断
/// （`sass-spec` 的 `directives/use/error/member/missing`：`Error: Undefined variable.`），
/// 而解析不动的 run 或按字面打印的调用是**已定义的透传**（`css/functions/special/**`、
/// `css/custom_properties/simple`、`css/selector/escaping::dollar_char`）。所以失败永远不能
/// 被一句「原样输出源码」的兜底合并掉。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// 这个位置上的变量引用没有绑定
    UndefinedVariable,
    /// 已知内置函数的实参个数少于其签名参数个数；消息是语料自己的
    /// （`Missing argument $number2.`，`core_functions/math/div.hrx::error/too_few_args`）
    MissingArgument,
    /// 已知内置函数的实参个数多于其签名容纳的个数，同样是语料的原话
    /// （`Only 2 arguments allowed, but 3 were passed.`，`div.hrx::error/too_many_args`）
    TooManyArguments,
    /// 内置函数的 number 参数拿到的是别的值类型；仍是语料的原话
    /// （`$number: c is not a number.`，`core_functions/math/round.hrx::error/type`）
    NotANumber,
    /// 两个**已知**量纲不同的数字被拿去比较；语料在消息里点名两者
    /// （`1px and 2s have incompatible units.`，`core_functions/math/max.hrx::error/incompatible_units`）。
    /// 未收录的单位永远不产生这条 —— 选择 decline 保留原文，与宣称「量纲冲突」不是一回事
    IncompatibleUnits,
    /// number 参数要求特定的单位（或不要单位）；仍是语料的原话
    /// （`$number: Expected 1% to have no units.`，`core_functions/math/percentage.hrx::error/unit`）
    WrongUnits,
    /// 颜色参数拿到的不是颜色，语料把这句话写了出来
    /// （`$color: 1 is not a color.`，`core_functions/color/change/error/type.hrx::color`）。
    /// 只有**数字**换来这条消息：处在本编译器没有的空间里的颜色（`color(srgb …)`、`lab(…)`）
    /// 是 decline，因为用 sRGB 回答它就是一个 pass 数看不见的错值。
    NotAColor,
    /// string 参数拿到非字符串（`$space: 1 is not a string.`，
    /// `core_functions/color/scale/error/type.hrx::space`）
    NotAString,
    /// 调用点名了其颜色答不了的通道，或给出的份额越出语料允许的范围 ——
    /// `$hue: Channel isn't scalable.`（`core_functions/color/scale/error/args.hrx::unknown`）与
    /// `$saturation: Expected -100.001% to be within -100% and 100%.`（`core_functions/color/scale/error/bounds.hrx::legacy/too_low`）。
    /// 两者说的都是**通道实参**，所以共用一个 kind；被拒绝的是哪个名字，文本里写着。
    Channel,
    /// 点名的 `$label:` 不是被调签名里的参数。不致命：run 保留自己的文本。这个 kind 存在的意义
    /// 是让读者知道**不要**把 run 再走一遍插值路径 —— 实参列表里的 `$name:` 是在*命名*一个参数，
    /// 不是引用一个变量；把它当变量解析就是那 ~1,611 条假 `Undefined variable` 背后的错误
    /// （`expr::named_argument`）。只有注册表看得见这些标签，所以只有它能说这句话。
    UnknownParameter,
    /// 声明左侧是一个**内置模块成员** —— `math.$pi: 0;`。内置模块的常量是只读的：
    /// `Cannot modify built-in variable.`（`core_functions/math/variables.hrx::error/assignment/*`）。
    /// 只有模块表真正拥有的名字换来这条；其余 `foo.$bar:` 仍按普通属性读。
    ReadOnlyVariable,
    /// 其余一切
    Other,
}

/// 会让流中断的那些 kind。未绑定变量是致命的，因为语料这么说
/// （`directives/use/error/member/missing`、`core_functions/meta/call.hrx::error/if_args`：两者都要
/// `Error: Undefined variable.`）。
///
/// 三条守卫翻转已经落地的行为，每一条都是语料的形状而不是猜测：
/// 1. 块**头部**只解析 `#{ … }` —— `@for $i from …` 的头*绑定* `$i`；在那里查它会为 ~47 条用例
///    凭空造出错（`directives/for/{whitespace,comment}.hrx`）；
/// 2. **special** 调用里未绑定的 `$x` 是纯 CSS 文本（`expr::special_call`：`url`/`calc`/`element`/
///    `expression`/`progid:` 的实参），而*已绑定*的仍解析
///    （`values/calculation/calc/no_operator::variable/unquoted_string`），`min`/`max`/`clamp`
///    则根本不是 special（`css/functions/not_special.hrx` 会算 `-c-type(1 + 1)`）；
/// 3. **纯 CSS at 规则体**里的声明（`@function --a()`）把 `$x` 留作文本
///    （`exec::plain_css_body`；`css/function.hrx::{lowercase,uppercase}/result/sass_script`），
///    而同一体里的 `#{1 + 1}` 仍求值（`css/function.hrx::lowercase/result/interpolation`）。
///
/// **以逐用例唯一 key + 错误消息基准量过**（见 `tests/hrx.rs`）：hrx 599 → **600**、
/// `regress=0`；唯一收益是 `core_functions/meta/call::error/if_args`。旧的「599 → 984（+385）」
/// 是粗基准的产物：harness 曾把**任何**错误都当成满足 `error` 用例，而 385 条里有 384 条写的是
/// 另一句消息（`Expected identifier.`、`$red: Expected 0.5px to have unit "%" or no units.`、
/// `DEPRECATION WARNING [global-builtin]: Global built-in functions are deprecated and will be removed in Dart Sass 3.0.0.`）—— 我们还不产生这些消息。
/// 在 P1 的单位代数与 P7 的诊断发出基准点名的消息之前，那些行保持 FAIL。
const FATAL: &[ErrorKind] = &[
    ErrorKind::UndefinedVariable,
    ErrorKind::MissingArgument,
    ErrorKind::TooManyArguments,
    ErrorKind::NotANumber,
    ErrorKind::IncompatibleUnits,
    ErrorKind::WrongUnits,
    ErrorKind::NotAColor,
    ErrorKind::NotAString,
    ErrorKind::Channel,
    ErrorKind::ReadOnlyVariable,
];

/// 编译器错误走流的**真** `Err` 通道（`Observer::error`），绝不作为能被兜底悄悄吞掉的值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SassError {
    pub message: String,
    pub kind: ErrorKind,
}

impl SassError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ErrorKind::Other,
        }
    }

    pub(crate) fn typed(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind,
        }
    }

    /// 这次失败必须终止流，而不是退成逐字文本吗？
    pub(crate) fn is_fatal(&self) -> bool {
        FATAL.contains(&self.kind)
    }
}

impl fmt::Display for SassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SassError {}

/// 公开入口 —— 也是**唯一**的对外 API。一条冷的**推**链只写在这里：没有任何阶段函数接收或
/// 返回 observable 类型，没有 `box_it()` 接缝。
///
/// 每个阶段都是一个框架形状的算子内核（`crate::phases`）：惰性 `struct{source}` + 住在自己
/// `Observer` 里的状态 + 三份 impl。内核把它产出的每一项直接发进 `observer.next(..)`，
/// 所以不会有东西被攒成 `Vec` 再摊平一次。
///
/// 是摊平，不是递归：`Exec` 把程序 `State`（打开的块、环境）留在自己的 observer 里，以
/// `&mut self` 向前推进，只发出一个事件了结的那条规则 —— 所以没有整份状态的克隆搭流走，
/// 也没有 `Arc<Env>` 快照被夹带在声明旁边；声明仍在它出现的位置上被求值。
///
/// 顺序是**流**的属性，不是 key 的属性：到 `Render` 为止链条是串行的，规则按文档序到达
/// `Render`，CSS 也按那个顺序出来 —— 没有 `collect`、没有 `sort_by_key`、没有按规则的
/// 任务交接。真并发只留给互相独立的模块子图（P5），那里的约束是：内层 observable 被订阅
/// 之前不能产出任何东西（`merge_all` 会丢掉排队内层在等待期间的发射）。CSS 文本只出现在
/// 那一个终点。
pub async fn compile(content: impl Into<String>) -> Result<String, SassError> {
    let program = content.into();
    // 命名「这条日志属于哪份程序」的 `id` 住在这里：阶段 span 除了 stage 之外没有别的身份，
    // 四条阶段 span 都嵌套在它里面并继承它。
    let _span = tracing::info_span!("compile", id = %trace::source_label(&program)).entered();
    let resolved = Shared::from_iter(program.chars().collect::<Vec<char>>())
        .transform(|source| Lexer { source })
        .transform(|source| Structurize { source })
        .transform(|source| Exec { source })
        .transform(|source| Render { source })
        .reduce_initial(String::new(), |mut css, chunk| {
            css.push_str(&chunk);
            css
        })
        .into_future()
        .await;
    // 流的失败以 `Ok(Err(SassError))` 到达，并**原样**返回 —— 旧的终点给每次失败重新贴标签
    // （`map_err(|_| SassError::new("reduce"))`），那正是被真通道替换掉的「吞掉这个错误」形状。
    // `reduce_initial` 总会发出它的种子，所以外层只剩「空程序」这一种错误，而空程序合法地
    // 编译成 `""`。
    match resolved {
        Ok(compiled) => compiled,
        Err(rxrust::IntoFutureError::Empty) => Ok(String::new()),
        Err(_) => Err(SassError::new("stream produced no result")),
    }
}
