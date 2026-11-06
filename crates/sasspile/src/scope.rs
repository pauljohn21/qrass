//! `Exec` 内核携带的那两个 **owned 数据值**：一个开着的块与一条定下来的规则。
//!
//! 它们与 [`crate::exec`] 的前进步分开住，是为了让执行*持有*什么能单独读懂：一个块是一个选择器、它那串
//! 待刷的声明（字段 `decls`），再加上这个块*是*什么（普通 CSS 还是一个可调用的本体）；一个规则是一个选择器
//! 加上一串声明。嵌套深度为什么是这个数据而不是调用栈、以及唯一那次真正的重入，都只住在
//! [`crate::exec`] 的页头（那里还带着它的限定句），这里不重述。
//!
//! 一个块里**没有任何循环态**。一个 `@for` 由那个拥有大括号的相（[`crate::structure::MsgAcc`]）剪出来，
//! 以一个已完成的事件到达执行，所以这里不需要为缓存一个本体或计数还没闭合的块而长出新字段。

use crate::selector::{PropName, Selector};
use crate::value::Value;

/// 一条解好了的 CSS 规则：从一个块里刷出来的**一串声明**。一个块可以产出好几条规则
/// （`sass-spec` 的 `css/style_rule` `declaration/interleaved/around_style_rule`），所以这不是“块整个本体”。
/// 一个 `@for` 在本体含有块时**逐轮**产出一条规则
/// （`sass-spec` 的 `directives/for/for.hrx::to_scope`），那是同一个事实从另一边的说法。
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: Selector,
    pub decls: Vec<(PropName, Value)>,
}

impl StyleRule {
    /// 把规则序列化为一个展开的 CSS 块。[`Value::to_css`] 仍是一个值变成文本的唯一地方，
    /// [`Selector::to_css`] 是一个头变成文本的唯一地方。
    pub fn to_css(&self) -> String {
        let body = self
            .decls
            .iter()
            .map(|(prop, value)| format!("  {}: {};\n", prop.as_str(), value.to_css()))
            .collect::<String>();
        format!("{} {{\n{}}}\n", self.selector.to_css(), body)
    }
}

/// 一个当前开着的块，作为流动态里的数据。
#[derive(Debug, Clone)]
pub(crate) struct Scope {
    pub(crate) selector: Selector,
    pub(crate) decls: Vec<(PropName, Value)>,
    /// 这个块是一个**普通 CSS at-rule** 的本体（`@function --a()`），或坐在它里面，所以它的声明是
    /// CSS 文本：一个未绑定的 `$x` 保持被写下而不抛 `UndefinedVariable`（`css/function.hrx` 里的**六条**
    /// `*/sass_script` case —— 现量：`grep -c "sass_script/input" sass-spec/spec/css/function.hrx` = 6，
    /// 且这六条不在本批 run 的失败名单里）。
    /// 这样一个本体内的插值仍是 SassScript（`lowercase/result/interpolation`：`#{1 + 1}` → `2`），
    /// 所以这里只抑制“致命”那一层。（`css/plain/function.hrx` 用 `@use "plain"` 说同一批本体；那些 case
    /// 仍红是因为缺 P5 的 loader，不是因为这里。）
    pub(crate) plain_css: bool,
    /// 这个块是一个**可调用声明**的本体（`@mixin name(…)` / `@function name(…)`），而 Sass 并不在它被写下的
    /// 地方*执行*它：不求值任何东西，不往外围环境里绑任何东西，也不出任何规则。调用它（`@include` 或一次
    /// 求值）才是会执行本体的那件事，而那属于 4.1/4.2。在此之前把一个本体当普通 CSS 处理，会因它自己的参数
    /// 而走成未绑定变量。两个数各自口径不同，不得读成同一件事（都是现量）：
    /// `cargo run -q -p ledger -- failures <run> "meta/get_mixin" --count` = **30** 是**该簇的红行数**
    /// （needle 同时匹配 key 与 detail）；而其中真正报出那个错的只有 **1** 行 ——
    /// `cargo run -q -p ledger -- failures <run> "meta/get_mixin" "Undefined variable" --count` = **1**，
    /// 即 `core_functions/meta/get_mixin/content::scope/fall_through`（`unexpected error "Undefined variable."`）。
    /// 另 20 行是 `expected error …`/未实现透传，其余是 `got ""` —— 其中 `same_module::dash_insensitive/*`
    /// 那两行也是 `got ""`，所以它们不是这个错的凭据。
    pub(crate) inert: bool,
}
