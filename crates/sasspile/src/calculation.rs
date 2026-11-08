//! 一条计算的**壳律**：什么离开 `calc( … )`、什么卸掉一层壳、什么留在壳内。定位壳与规范化它的词素
//! 是 [`crate::brackets`] 的工作；本模块只是律本身，所以读起来是判定，而不是括号记账。
//!
//! 每条规则都从 fixture 引用，绝不推断：
//! * 单个可折叠的值离开它的壳 —— `calc(1px)` → `1px`（`no_operator::number/integer`）、
//!   `CaLc(1px)` → `1px`（`…::case_insensitive`）、`calc( 1px )` → `1px`（`…::extra_whitespace`）、
//!   `calc((1px))` → `1px`（`calc/parens::number`）；
//! * 嵌套计算卸掉**一层**壳，而那会露出下一次折叠 —— `calc(calc(1px))` → `1px`
//!   （`…::calculation/calc/simplified`）、`calc(clamp(1%, 2px, 3%))` → `clamp(1%, 2px, 3%)`
//!   （`…::calculation/clamp/preserved`）、`calc(min(1%, 2px))` → `min(1%, 2px)`；
//! * 其余一切保留外壳 —— `calc(var(--c))`、`calc(c())`、`calc(foobar)`、`calc((d))`
//!   （`calc/parens::{var/direct,double_preserved,identifier}`）；其中几行在第一条计算律存在之前就已
//!   通过，所以它们构成这条律的安全网；
//! * 内容本身由 [`crate::calc`] 归约 —— 扁平化，不是树：同单位折叠（`operator/plus::simplified`）、
//!   `*` 与 `/`（`operator/times::simplified`）、有理性单位（`values/numbers/units/multiple`），
//!   以及一条绑定计算重新进入算术（`…::division/cancels/numerator/once`）；
//! * `min`/`max`/`clamp` 在这里**不**当壳求值：裸名 `min(1%, 2px)` 保留文本是 1.4c 的律
//!   （`no_operator::function/css`），而 `calculation/min::simplified` 要的是*已注册*的内建
//!   （`::calculation/min/preserved` 对 `::min/simplified`）。
//!
//! 1.4a 故意留白的部分现已补上：化简与有理性单位随 1.4d 落地。

use crate::brackets::{Shell, find, is_calc, meaningful, open_at, resolved, without_leading_sign};
use crate::calc::{self, Content};
use crate::calls;
use crate::env::Env;
use crate::lexer::{Tok, render};
use crate::unit::Unit;
use crate::value::Value;

/// 一个实参最终塌成什么。几种形状保持分立，因为语料对它们的对待不同：只有数字/布尔离开壳，
/// 而内层计算的 **body** 是唯一顶替外壳的文本，而不是留在壳里。
#[derive(Clone)]
enum Outcome {
    Collapsed(Value),
    /// 内层计算的 body（不带它的外壳）：由外层壳的名字接管
    Nested(Content),
    /// 一个已带自己名字的计算 —— 作为内容交给外层壳，外层壳的壳让位于它
    Named(String),
    /// 可读但不是计算的内容
    Kept(String),
    /// `crate::calc` 折到折不动的链：其内容保住它到达的**值**，于是绑定计算可以再次被除或乘
    /// （`multiplication::cancels/*`）
    Folded(Content),
    /// 本切片读不出的东西；调用方保留 run 自己的文本
    Opaque,
}

/// 造出这条 run 所代表的计算值；当它不是本切片会读的计算时返回 `None`（未知被调者、未闭括号、
/// 调用外的运算）。
///
/// 那条事件就是本模块存在的理由——诊断：**到达时的词素形状**与经 [`open_at`] 切分之后的形状，
/// 再加上选了哪条判定。这条规则的每个错误版本都曾静默失败 —— 被调者判据什么都匹配不到，于是什么
/// 都没动 —— 只有这两个事实让它显形。
pub fn value(toks: &[Tok], env: &Env) -> Option<Value> {
    let shell = find(toks)?;
    let built = to_value(&shell, &arguments(&shell, env), env);
    // 故意放在宏外：`tracing` 在宏体内把 `Value` 解析成它自己的 field trait，所以在那里写
    // `Value::Calculation` 路径编译不过。
    let decision = match &built {
        None => "declined",
        Some(Value::Calculation(..)) => "preserved",
        Some(Value::Raw(..)) => "shell dropped",
        Some(_) => "collapsed",
    };
    let resolved = built.as_ref().map_or(String::new(), Value::to_css);
    tracing::debug!(
        stage = "exec",
        module = "calculation",
        name = shell.name.as_str(),
        arrived = %render(toks),
        cut = ?open_at(toks),
        decision,
        resolved = %resolved,
        "calculation read"
    );
    built
}

/// 壳律。包着**另一个带名计算**的 `calc` 卸掉自己的壳，把里层交出去 —— 哪怕内层算不出来，这正是
/// `calc(calc(1px))` 两步走到 `1px`、而 `calc(clamp(1%, 2px, 3%))` 停在 `clamp(1%, 2px, 3%)`
/// 的原因。包着普通内容的 `calc` 保留自己的壳（`calc(var(--c))`）。`min`/`max`/`clamp` 保留自己的
/// 名字、在这里从不被求值，因为 1.4a 拒绝猜这条律。
fn to_value(shell: &Shell, parts: &[Outcome], env: &Env) -> Option<Value> {
    if !is_calc(&shell.lower) {
        // `min`/`max`/`clamp` 壳保留自己的名字、从不求值：那是 1.4a「不猜」的一例。一个读不出的
        // 实参就让整条 run decline，文本因而与源码写下的完全一致 —— `calc` 之外不重写任何名字或
        // 间距（1.4c）。
        let readable = parts.iter().all(|part| !matches!(part, Outcome::Opaque));
        return readable.then(|| {
            Value::Calculation(
                shell.name.clone(),
                Content {
                    folded: one_argument_value(parts),
                    printed: rendered(parts),
                    precedence: 3,
                },
            )
        });
    }
    match parts {
        [Outcome::Collapsed(value)] => Some(value.clone()),
        // `calc( … )` 里一条折过的链同时保留壳*与*值：`calc(2px * 3px)` 印成 `calc(6px * 1px)`，
        // 但对要除它的人来说仍是数 `6px*px`（`calc/operator/units::multiplication`）。
        [Outcome::Folded(content)] => Some(Value::Calculation(shell.name.clone(), content.clone())),
        [Outcome::Nested(inner)] => Some(Value::Calculation(
            shell.name.clone(),
            Content {
                folded: inner.folded.clone(),
                printed: inner.printed.clone(),
                precedence: inner.precedence,
            },
        )),
        // 内层计算以自己名字印出：名字活下来，这层壳退场。
        [Outcome::Named(text)] => Some(Value::Raw(text.clone())),
        [Outcome::Kept(text)] => Some(Value::Calculation(
            shell.name.clone(),
            Content {
                folded: None,
                printed: text.clone(),
                precedence: 3,
            },
        )),
        [Outcome::Opaque] => decline_to_text(shell, env),
        many => Some(Value::Calculation(
            shell.name.clone(),
            Content {
                folded: one_argument_value(many),
                printed: rendered(many),
                precedence: 3,
            },
        )),
    }
}

/// 实参列表到达的值（当其中一部分带着值）：复合的乘/商可以同时作为文本印出、作为数使用。
fn folded(parts: &[Outcome]) -> Option<(f64, Unit)> {
    parts.iter().find_map(|part| match part {
        Outcome::Folded(content) | Outcome::Nested(content) => content.folded.clone(),
        Outcome::Collapsed(value) => match value.number() {
            Some(Value::Number(number, unit)) => Some((number, unit)),
            _ => None,
        },
        _ => None,
    })
}

/// [`folded`] 套上 arity 守卫：计算只有在实参列表是**一个部分**时才能认领值。`find_map` 停在第一个
/// 值对单实参是对的、对多实参就是捏造 —— `min(10, 20)` 不是 `10`，可 `math.div(1, min(10, 20))`
/// 印出过 `0.1`，`max(10, 20) * 1` 印出过 `10`。语料里没有任何一行把多实参计算放进算术
/// （`values/calculation/min.hrx::preserved/*` 都保留调用原文，也没有哪一行去除这样的壳），
/// 所以留下来的形状是本项目自己对「算不出来」的答案：按写下的文本（1.4a 的不猜律），落点是
/// `Value::number()` 给出 `None`、算术随之 decline。
fn one_argument_value(parts: &[Outcome]) -> Option<(f64, Unit)> {
    match parts {
        [single] => folded(std::slice::from_ref(single)),
        _ => None,
    }
}

/// 本切片读不成值的实参列表：它的规范化文本活下来，而如果那段文本本身是一个计算调用，就卸掉一层
/// 壳、让给里层（`::calculation/*/preserved`、`calc/parens::calculation`）。一个里面没有任何可读
/// 之物的裸 `min( … )` 反而 decline，这保住了 1.4a 留白的那些形状今天的逐字行为。
fn decline_to_text(shell: &Shell, env: &Env) -> Option<Value> {
    match find(&shell.inner) {
        Some(inner) => to_value(&inner, &arguments(&inner, env), env).map(|value| match value {
            Value::Calculation(name, body) => Value::Raw(format!("{name}({})", body.printed)),
            other => other,
        }),
        None if is_calc(&shell.lower) => Some(Value::Calculation(
            shell.name.clone(),
            Content {
                folded: None,
                printed: resolved(&shell.inner, env),
                precedence: 3,
            },
        )),
        None => None,
    }
}

/// 一条被保留的计算所携带的文本：它的引用是**已解析**的，因为 `calc` 实参仍按 SassScript 解析
/// （`::variable/number/simple_unit`，以及 0′.3b-2 的教训：special 调用的实参解析变量，而 `+`/`-`
/// 保持非算术语义）。一条不含任何动态之物的 run，与一条引用未绑定的 run，都回退到规范化的词素
/// 文本 —— `calc( … )` 里一个未绑定的 `$x` 保留自己的文本，永不成为致命错误
/// （`::variable/not_parsed_as_interpolation/*`）。
fn arguments(shell: &Shell, env: &Env) -> Vec<Outcome> {
    calls::top_level_groups(&shell.inner)
        .into_iter()
        .map(|group| classify(group, env))
        .collect()
}

/// 一个实参：先按结构读，再按值读。
fn classify(toks: Vec<Tok>, env: &Env) -> Outcome {
    let content = open_at(&toks);
    let run = meaningful(&content);
    if run.is_empty() {
        return Outcome::Opaque;
    }
    // 冗余括号**不**在这里剥：一个组活不活是它装着什么决定的性质，`calc::read` 从数据判定
    // （`parens::operation` 把 `calc((1px + 1%))` 降成 `calc(1px + 1%)`，而 `parens::var/direct`
    // 把 `calc((var(--c)))` 按写下保留）。
    if let Some(shell) = find(&content) {
        return match to_value(&shell, &arguments(&shell, env), env) {
            Some(value @ (Value::Number(..) | Value::Bool(..))) => Outcome::Collapsed(value),
            // 只有 `calc` 交出 body（那就是被卸掉的壳）；`clamp` 保留自己的名字 ——
            // `::calculation/clamp/preserved` 印的就是 `clamp(1%, 2px, 3%)`。
            Some(Value::Calculation(name, inner)) if is_calc(&name) => {
                Outcome::Nested(inner.clone())
            }
            Some(Value::Calculation(name, inner)) => {
                Outcome::Named(format!("{name}({})", inner.printed))
            }
            Some(value) => Outcome::Kept(value.to_css()),
            None => Outcome::Opaque,
        };
    }
    // 一个孤立的 `$var` 是一次**查找**：绑定的计算把自己的形式带出外层壳
    // （`::variable/calculation`：`$a: calc(1px + 1%)` 然后 `calc($a)` → `calc(1px + 1%)`），
    // 数字直接折叠出去（`syntax/extra_whitespace/number`），其余任何绑定值是留在壳内的内容 ——
    // 按它解析成的值印出，这正是 `::variable/unquoted_string` 把 `calc($a)` 变成 `calc(foobar)`
    // 的方式。*未绑定*的引用保留自己的文本，且不得成为致命错误
    // （`::variable/not_parsed_as_interpolation/*`）。
    if let [Tok::Var(name)] = run.as_slice() {
        return match env.get(name) {
            // 绑定计算的 *body* 才是顶替外层壳的东西，所以 `$a: calc(1px + 1%)` 上的 `calc($a)`
            // 印成 `calc(1px + 1%)` —— 一层壳，不是两层。
            Ok(Value::Calculation(_, inner)) => Outcome::Nested(inner.clone()),
            Ok(value) if collapsible(&value) => Outcome::Collapsed(value),
            // 带复合单位的绑定数字同样只属于计算：decline，而不是在任何 `calc( … )` 之外印出
            // 裸的 `1(px*rad)^-1`。
            Ok(Value::Number(_, unit)) if !unit.is_plain() => Outcome::Opaque,
            Ok(value) => Outcome::Kept(inside(&value)),
            Err(_) => Outcome::Opaque,
        };
    }
    // 粘着自己的数字的前导符号是**字面量**的一部分，不是运算：`::number/leading_plus` 要
    // `calc(+1px)` → `1px`，而词法器交来的是两个词素 `Op('+')` 与 `Word("1px")`；其他每个算子
    // 都由 `crate::calc` 读。
    let signed = without_leading_sign(&content);
    // 这个组随后按**表达式**读（`crate::calc`）：每一步都可算时就折叠，并按 Sass 自己的括号印出。
    // 它无法交替成操作数与算子的 run —— `foobar`、`c()`、`var(--c) 1` —— 留在文本路径上，于是
    // 1.4a 已经印对的东西一个都不动。
    let reading = calc::read(&signed, env);
    match reading {
        // 什么都没建模（`foobar`、`c()`、`var(--c)`、一个空格分隔列表、一个求值失败的插值 body）：
        // 保留源文本，与本模块之前逐字相同。
        calc::Reading::Declined => Outcome::Opaque,
        // 一个普通折叠值离开壳（`calc(1px * 2)` → `2px`）。
        calc::Reading::Value(value) if collapsible(&value) => Outcome::Collapsed(value),
        // 其余一切印在壳内 —— 一条停在不兼容步上的链、一个复合单位（`1 / 1px / 1rad`），
        // 或一个解析成功的插值（`calc(#{1px + 2px})` → `3px`）。
        calc::Reading::Value(value) => Outcome::Kept(inside(&value)),
        // 一条没有从壳里折叠出去的链，仍保住它到达的数。
        calc::Reading::Expression(content) => Outcome::Folded(content),
    }
}

/// 一个值能否离开自己的壳：**布尔**，或单位能在计算之外表达出来的数（见 [`Unit::is_plain`]）。
/// `Raw` 永远不合格，而这一条边界正是本簇已通过用例保持通过的原因（`calc(foobar)`、`calc(c())`、
/// `calc(var(--c))`、`calc(#{1px + 2px})`、`calc((#{1px + 2px}))`）。
///
/// `spelled` 判定是**保守选择，不是推出的律**，语料否定了它的一刀切版本：
/// `core_functions/color/color/degenerate::arg1/nan` 写下 `color(srgb calc(NaN) 0 0)` 而回答
/// `color(srgb 0 0 0)`，`core_functions/color/hsl/four_args/out_of_gamut::hue/degenerate/
/// positive_infinity` 把 `hsl(calc(infinity), 100%, 50%)` 回答为 `hsl(0, 100%, 50%)` —— 这些量
/// *确实*离开壳，因为消费方把它们夹到有限边界。两种替代都测过，都不干净：没有这个判定
/// `ledger diff oklab-solved-polar-v2 calc-constants` 读作 `regress=7 net=37`（七行在语料保留壳的
/// 地方印出裸 `infinity` / `infinity%`，例如 `…::arg1/positive_infinity` 与
/// `out_of_gamut::lightness/degenerate/positive_infinity`），加上它 `… calc-constants-final` 读作
/// `regress=0 net=14` 而那一版转绿的 23 行夹紧形状又变回红，**变回它们的基线形状**
///（`ledger changed` 对 `oklab-solved-polar-v2` 读 0，`ledger diff calc-constants calc-constants-v2`
/// 读 `regress=23 net=7`）。真正逐通道决定的律 —— `NaN`、`+∞`、`−∞` 哪个折叠、对着哪个边界夹 ——
/// 属于颜色构造子的夹紧律，不属于本壳判定；推出它是那 23 行点名的待办工作，在它落地之前本文件
/// 选择拒绝，而不是印出没有任何一行证明的值。
fn collapsible(value: &Value) -> bool {
    let spelled = match value {
        Value::Number(number, _) => number.is_finite(),
        _ => true,
    };
    matches!(value, Value::Bool(..) | Value::Number(..)) && value.is_plain() && spelled
}

/// 一个值在计算壳**之内**拿到的文本。没有 plain-CSS 写法的量按单位自己的计算形式写出
///（[`Unit::calc_form`]，它已经产出 `infinity * 1%`），因为 [`crate::value::Value::to_css`] 会补上
/// 它的 `calc( … )` 外壳，壳会再包一层成 `calc(calc(infinity))`。其余一切保留自己的 CSS 文本。
fn inside(value: &Value) -> String {
    match value {
        Value::Number(number, unit) if !number.is_finite() => unit.calc_form(*number),
        other => other.to_css(),
    }
}

/// 实参列表的可读部分，按语料分开它们的方式（`, `）连接。
fn rendered(parts: &[Outcome]) -> String {
    parts
        .iter()
        .map(|part| match part {
            Outcome::Collapsed(value) => value.to_css(),
            Outcome::Nested(inner) | Outcome::Folded(inner) => inner.printed.clone(),
            Outcome::Named(inner) | Outcome::Kept(inner) => inner.clone(),
            Outcome::Opaque => String::new(),
        })
        .collect::<Vec<String>>()
        .join(", ")
}
