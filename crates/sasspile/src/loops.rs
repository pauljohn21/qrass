//! 识别并枚举 `@for` 头 —— 执行中决定*一个循环是什么*的那部分，与 [`crate::exec`] 把一个事件移入
//! 流动状态的工作分开。
//!
//! **这里没有任何东西持有循环的 body。** 拥有花括号的那一相（[`crate::structure::MsgAcc`]）把 body
//! 切出并作为一个 owned [`crate::structure::Event`] 交上来，所以本模块陈述的两个事实 —— *这个头是
//! 不是 `@for`*（[`is_for`]）与*它命名什么区间*（[`for_loop`]）—— 都在词素上判定。这一分工把**缓冲**
//! 与**还欠几个块要关的计数**从执行器里拿走；它还没有拿走每遍对 body 的重灌，那由
//! `exec::State::unroll` 按实情说成与 design.md 第 88 行的 `flat_map`(一项 → body 子流) 之间的既存
//! 差别，而不是藏在一句“差别已消失”的断言后面。
//!
//! 区间*是*数据：[`Walk`] 携它为三个数字，按需生成。

use crate::SassError;
use crate::brackets;
use crate::env::Env;
use crate::lexer::Tok;
use crate::value::Value;

/// 一个被读出的 `@for` 头：它绑定的变量与它走的区间。body **不**是这个值的一部分 —— 它在旁边
/// 到达，由分界相切出。
#[derive(Debug, Clone)]
pub(crate) struct ForLoop {
    pub(crate) var: String,
    pub(crate) walk: Walk,
}

/// `@for` 走的区间，以**三个数字** —— 它的第一个值、有多少个、以及步长。
///
/// 保持为数据而不是物化：在头被读出的那一刻就把 span 收进 `Vec<i64>`，会在任何声明求值之前为每次
/// 迭代要一个八字节槽，于是 `@for $i from 1 through 1000000000000` 分配失败并把进程带走。移动 `to`
/// 的边界出于同一原因做了检查 —— `@for $i from 1 to -9223372036854775808` 曾回绕成一个巨大的正边界
/// 而不是 decline。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Walk {
    first: i64,
    count: i64,
    step: i64,
}

impl Walk {
    /// `from <first>` *direction* `<last>` 命名的区间：`through` 保留它的端点，`to` 丢掉它一个步长，
    /// 起点高于终点时向下走（`5 through 1` → 5,4,3,2,1），空区间不携带任何值（`1 to 1` 什么都不
    /// 贡献）。算术装不进 `i64` 时是 `None`，那 decline 这个头，而不是发明一个边界。
    fn bounded(first: i64, last: i64, inclusive: bool) -> Option<Self> {
        let step = if first <= last { 1 } else { -1 };
        let end = last.checked_sub(i64::from(!inclusive) * step)?;
        let count = end.checked_sub(first)?.checked_mul(step)?.checked_add(1)?;
        (count >= 0).then_some(Self { first, count, step })
    }

    /// 文档序的值，被索取时生成。`k` 绝不超过 `count - 1`（两个边界之间的距离），所以每个乘积都落在
    /// 它们之内。
    pub(crate) fn values(self) -> impl Iterator<Item = i64> {
        (0..self.count).map(move |k| self.first + k * self.step)
    }
}

/// 这个词素是不是 `@for` 关键字？两个读取器共用的唯一测试：[`is_for`] 拿它问一个头的第一个词素
/// （结构相能回答的问题），[`for_loop`] 拿它问它正在读的头。这个比较出现两种写法，就是一相可能
/// 切出 body 而另一相随后 decline 去跑它的方式。
fn for_keyword(word: &str) -> bool {
    word.eq_ignore_ascii_case("@for")
}

/// 这个块头是否**命名一个 `@for`**？[`for_loop`] 应用的第一个词素测试相同，但问的是结构相需要的
/// 更弱的问题：它无法知道边界是否可读（那需要活在执行器里的环境），只知道这个块被写成了一个循环
/// —— 所以一个 [`for_loop`] decline 的头仍被切出，然后作为它本来就是的普通块交回。
pub(crate) fn is_for(toks: &[Tok]) -> bool {
    let head = brackets::meaningful(toks);
    let [Tok::Word(word), ..] = head.as_slice() else {
        return false;
    };
    for_keyword(word)
}

/// 只识别 **`@for $i from <bound> through|to <bound>`** 与它的字面词素形状：`@for`、一个变量、
/// `from`、一个边界、一个方向词、一个边界。任何其他形状（`@each`、一个带括号的嵌套头、一个不是
/// 整数的边界）返回 `None` 并保留今天的行为，所以展开只发生在头被无猜测读出的地方。
///
/// 边界经环境读取，因为语料把它写成引用：`through $limit`（`directives/for/for.hrx::to_scope`）。
/// 区间**在这里一次性**决定 —— 同一个用例陈述了在 body 内重新绑定 `$limit` 不会移动循环的终点。
///
/// 金样：`directives/for/for.hrx::{inclusive_forward/scss, exclusive_forward, inclusive_backward,
/// negative_to_positive, empty/scss, to_scope}`。
pub(crate) fn for_loop(toks: &[Tok], env: &Env) -> Result<Option<ForLoop>, SassError> {
    let header = brackets::meaningful(toks);
    let [
        Tok::Word(keyword),
        Tok::Var(var),
        Tok::Word(from_kw),
        rest @ ..,
    ] = header.as_slice()
    else {
        return Ok(None);
    };
    if !for_keyword(keyword) || !from_kw.eq_ignore_ascii_case("from") {
        return Ok(None);
    }
    // 方向词是搜索得到的，而不是假定它坐在某个固定偏移上，所以一个多于一个词素的边界会被当成它
    // 本来的 run 读出，而不会错动其后每一个槽位。
    let turned = rest.iter().enumerate().find_map(|(offset, tok)| match tok {
        Tok::Word(word) => known_direction(word).map(|inclusive| (offset, inclusive)),
        _ => None,
    });
    let Some((offset, inclusive)) = turned else {
        return Ok(None);
    };
    let (Some(first), Some(last)) = (
        integer(&rest[..offset], env)?,
        integer(&rest[offset + 1..], env)?,
    ) else {
        return Ok(None);
    };
    Ok(Walk::bounded(first, last, inclusive).map(|walk| ForLoop {
        var: var.clone(),
        walk,
    }))
}

/// 方向词与它是否保留端点。两种拼写共用一张表，所以第三个词会被 decline，而不是被猜测。
fn known_direction(word: &str) -> Option<bool> {
    [("through", true), ("to", false)]
        .into_iter()
        .find(|(name, _)| word.eq_ignore_ascii_case(name))
        .map(|(_, inclusive)| inclusive)
}

/// 一个 `@for` 边界当作整数读出，或在语料允许程序失败的读法失败时给出一个 `Err`。
///
/// 一个**字面**边界严格按头部一贯读它的方式解析（`from -1` 把它的负号留在词素内，按
/// `for.hrx::negative_to_positive`）。一个**引用**边界在头部所在的位置经环境解析 —— `through $limit`
/// （`for.hrx::to_scope`）—— 且*只解析一次*，因为同一个用例陈述了在 body 内重新绑定 `$limit` 不会
/// 移动区间的终点。未绑定的名字**不会被折成一个 decline**：`Env::get` 抛出根 crate 计入致命类别的
/// `UndefinedVariable`，而用 `.ok()` 吞掉它正是本项目已两次实测过的“`.ok()` 吞掉真 `Err`”形状。
///
/// 不是整数的边界会 decline，而语料对这类边界说的两件事在这里被分开保管：
/// `for.hrx::error/{from_float,to_float,from_type,to_type,incompatible_units,unit_coersion_to_float}` 钉住
/// **消息**（`1.5 is not an int.`），那是 P7 的错误工作；而 `for.hrx::unit/same` 钉住一个
/// `1px through 5px` 的头**展开并保留它的单位** —— 这一读取本切片并不具备，所以它 decline，而不是
/// 丢掉 `px` 印出一个没人写过的数字。
fn integer(bound: &[&Tok], env: &Env) -> Result<Option<i64>, SassError> {
    let whole = match bound {
        [Tok::Word(word)] => word.parse::<i64>().ok(),
        [Tok::Var(name)] => match env.get(name)? {
            Value::Number(value, unit) if unit.is_unitless() && value.fract() == 0.0 => {
                Some(value as i64)
            }
            // 对单独数字的绑定以它的**源文本**携带 —— 拒绝把一个数字变成表达式是
            // `expr::from_tokens` 自己的律（正是它让 `w: 10px` 被精确写出），而 `for.hrx::to_scope`
            // 在根上写 `$limit: 4` —— 所以头部把那段文本读回它所说的那个整数。任何不解析成整数的
            // 东西都会 decline：`4px`、`1.5`、`"4"` 和 `4e0` 保持未读，而不是悄悄变成数字。印出
            // 值再扫描回来是一个权宜之计；移除它的形状是绑定自身携带的一个整数槽，作为未决项记录在
            // `tasks.md` 9.7。
            bound => bound.to_css().parse::<i64>().ok(),
        },
        _ => None,
    };
    Ok(whole)
}
