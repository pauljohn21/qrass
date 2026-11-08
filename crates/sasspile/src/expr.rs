//! Expr —— 一个 declaration 值（或选择器）作为结构化表达式：由 `Tok` run 一趟前向组装而成 ——
//! 绝不重扫渲染出的文本 —— 并对照 owned [`Env`] 自底向上求值。
//!
//! P1 以**宁可拒绝也不猜**让语法保持诚实：一个 run 只有在毫无歧义时才成为 `Expr`。任何带 `/`、`*`、
//! `%` 的词素、以及代数尚未覆盖的任何形状，都返回 `None` 并保留逐字文本 —— 正是这一点保护了
//! `font: 12px/1.5`、`grid: 1/2` 与 CSS 原生的 `calc()`/`min()` 不被读成 Sass。`+`/`-` 的间距律由
//! Sass 自己给出：它们只有在两侧都有空白时才是算子，所以 `10px - 5px` 是减法，而 `10px -5px` 是
//! 双项列表。

use crate::SassError;
use crate::calls::Argument;
use crate::env::Env;
use crate::lexer::{Tok, canon, render, token_text};
use crate::operations::{arithmetic, concatenation, has_dynamic, number_lexeme, protected};
use crate::unit::Unit;
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    /// `*`，lexer 把它发成自己的词素，Sass 也总是把它读作乘法
    Mul,
    /// 除法，只在 `/` 不可能意味着别的场合认（见 [`crate::unit`] 与 `calculation`）：
    /// declaration 值里的 `/` 是斜杠分隔列表。
    Div,
}

/// 一个建立在 declaration 值词素之上的表达式。数字逐字携带它读到的单位文本。
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// 一个数字字面量：量级与**读它时带着的那个**单位（绝不是复合的 —— 复合单位只出自
    /// `*` 与 `/`，那是 [`BinOp`] 的产物）
    Num(f64, Unit),
    Binary(Box<Expr>, BinOp, Box<Expr>),
    /// 插值周围的逐字词素文本（`".col-"`、`"0 "`）
    Text(String),
    /// `$name` —— 查找键从不含 `$`（见 [`Env`]）
    Var(String),
    /// `math.$pi` —— 一个内建模块的常数，从 [`Tok::MemberVar`] 到达时就是一个词素。
    MemberVar(String, String),
    /// 一次对我们有签名的内建函数的调用（`1.7a`）；每个实参保留它的 `$label:`，
    /// 好让绑定可以按名而不是按位。
    Call {
        name: String,
        args: Vec<Argument>,
    },
    /// 一个插值 run 的各部分，求值时以**不多加空格**的方式拼接
    Concat(Vec<Expr>),
    /// 一个**读成单个值的空格分隔 run**：`rgb(18 52 86)` 与 `hsl(0 100% 50%)` 是 CSS Color 4 的
    /// 单实参拼法，Sass 把它读成一个空格列表、而不是三个实参
    /// （`core_functions/color/rgb/one_arg/no_alpha.hrx::unitless/numbers` → `rgb(18, 52, 86)`；
    /// 同一文件的 `$channels:` 行证明它是一个实参，因为一个标签恰好绑一个）。
    /// 与 [`Expr::Concat`] 不同，分隔符被**保留**，因为空格列表就是这么打印的。
    Space(Vec<Expr>),
    /// 一个**斜杠分隔列表**：CSS Color 4 写 `rgb(0 255 127 / 0)`，而语料把同一次调用写成
    /// `rgb(list.slash(0 255 127, 0))` —— `rgb/one_arg/alpha.hrx::{in_gamut/transparent,slash_list}`
    /// 要求同一种颜色 —— 所以 alpha 不是作用在最后一个通道上的算子，而是一个二元素斜杠列表的
    /// 第二个元素，其第一个元素本身是一个空格列表。
    Slash(Vec<Expr>),
    /// 由 `crate::calculation` 读出的 `calc( … )` / `min( … )` / `max( … )` / `clamp( … )`（任务 1.4a）。
    /// 它携带的是**已完成的值**，而不是词素树，因为那次读取需要环境（有界的 `$a` 正是
    /// `calc(1px)` 与 `calc(calc(1px))` 的差别），而 `Expr` 节点永远没有环境；
    /// 所以本变体从不经过 `eval` 的操作数路径。
    Calculation(Value),
}

/// Sass 的 **special** 调用，按厂商前缀之后的基础名匹配：它们的实参是纯 CSS，其中 `+`/`-` 不是
/// 算术，而未绑定的 `$x` 只是文本而不是错误（`css/functions/special/prefixed/lowercase::calc/script_like`
/// 保留 `-a-calc($c)`；同一家族的 `calc/interpolation` 仍求值 `#{0}`；`values/calculation/calc/no_operator::
/// variable/unquoted_string` 解析的是*已绑定的* `$a`）。仅仅未知的函数**不算** special：
/// `css/functions/not_special.hrx` 会算 `-c-type(1 + 1)` → `-c-type(2)`。
///
/// 随内建注册表增长（P1.7/P2）；`min`/`max`/`clamp` 被有意排除 —— 它们的变量照样解析。
const SPECIAL_CALLS: [&str; 4] = ["url", "calc", "element", "expression"];

/// 剥掉调用名的厂商前缀（`-a-calc` → `calc`）。是 `fn` 而不是闭包：闭包被省略的生命周期
/// 无法把返回的 `&str` 系到它的实参上。
fn base_call_name(name: &str) -> &str {
    name.rsplit('-').next().unwrap_or(name)
}

/// 一个 run 是否调用了 special（或 `progid:` 式的带命名空间）函数。名字匹配忽略大小写，
/// 因为语料也逐字保留 `-C-EXPRESSION($d)`
/// （`css/functions/special/prefixed/uppercase::expression/script_like`）。
pub fn special_call(toks: &[Tok]) -> bool {
    toks.iter().any(|tok| match tok {
        Tok::Word(word) => match word.strip_suffix('(') {
            Some(name) => {
                SPECIAL_CALLS
                    .iter()
                    .any(|call| base_call_name(name).eq_ignore_ascii_case(call))
                    || name.contains(':')
            }
            None => false,
        },
        _ => false,
    }) || toks.windows(2).any(|pair| {
        matches!(pair[0], Tok::Punct(':')) && matches!(&pair[1], Tok::Word(w) if w.ends_with('('))
    })
}

/// 一个 run 是否**使用命名实参** —— `f(1, $w: 30%)`、`color.hwb($hue: 180, …)`。此处的 `$w`
/// 是一个实参*标签*，不是变量引用，查它会给一个 Sass 接受的程序凭空造出 `Undefined variable`
/// 错误（`core_functions/color/hwb/four_args::named`、`core_functions/color/is_powerless/named`；
/// 在范围内计量为 1,611 条用例）。
///
/// 判据在词素级，因为 map 字面量用同样的形状写相反的意图：`($z: null)` **是**引用 `$z` 作为键。
/// lexer 能把它们分开 —— 调用的 `(` 粘在 callee 上（`Word("hwb(")`），而分组的 `(` 独立站着
/// （`Word("(")`）—— 所以一个实参只从粘着的 `(` 或 `,` 之后开始，这里空白无意义。
///
/// 装着它的 run 在 P1 根本不被当成表达式求值：它保留逐字文本，这也意味着它里面任何*其他*引用
/// 眼下保持未解析（调用求值随 P4 到来）。
pub fn named_argument(toks: &[Tok]) -> bool {
    let meaningful: Vec<&Tok> = toks
        .iter()
        .filter(|tok| !matches!(tok, Tok::Space(_)))
        .collect();
    let opens_argument = |tok: &&Tok| match tok {
        Tok::Word(word) => word.ends_with('(') && word.len() > 1,
        Tok::Punct(',') => true,
        _ => false,
    };
    let names_argument = |tok: &&Tok| matches!(tok, Tok::Var(_));
    let is_colon = |tok: &&Tok| matches!(tok, Tok::Punct(':'));
    meaningful.windows(3).any(|window| {
        opens_argument(&window[0]) && names_argument(&window[1]) && is_colon(&window[2])
    })
}

/// 从词素 run 组装 `Expr`，或在 run 装着 P1 不能读的形状时返回 `None`。
///
/// `env` 只服务 calculation 路径（[`crate::calculation`）—— 那是唯一在裁定所属值形状的同时解析引用
/// 的读取；其余路径都不带 env。
pub fn from_tokens(toks: &[Tok], env: &Env) -> Option<Expr> {
    // **先**试调用，因为它的实参住在括号内：`math.percentage(1%)` 带一个 `%`，而下面的 CSS 简写
    // 保护正是为了把它挡在圈外 —— 但这里 callee 有签名，run *就是*表达式。这之所以安全，是因为
    // `known_call` 只匹配注册表认识的名字，于是 `calc(1% + 1px)`、`font: 12px/1.5`、`grid: 1/2`
    // 原样落入保护。
    if let Some((name, args)) = crate::calls::known_call(toks) {
        return Some(Expr::Call { name, args });
    }
    // calculation 试在保护测试**之前** —— 保护是为 CSS 简写（`font: 12px/1.5`、`grid: 1/2`）存在的，
    // 否则 `calc(1% + 1px)` 会被它永远拒绝：在 `calc( … )` 里，`/`、`*` 或 `%` 是 calculation
    // 自己的语法，不是分隔符。规则集有意照语料所陈述的（`tasks.md` 1.4a）：单值塌缩、嵌套壳剥一层、
    // 其余原样保持所写的表达式。不在这四个裸名里的 `…(` 词素原样通过，于是 `-a-calc(` 与 `url(`
    // 保持旧读法。
    if let Some(value) = crate::calculation::value(toks, env) {
        return Some(Expr::Calculation(value));
    }
    if protected(toks) {
        return None;
    }
    if has_dynamic(toks) {
        // 即便在这里算术也先试，因为模块常数的乘积是算术、不是插值：
        // `math.$pi * 1e15` → `3141592653589793`（`core_functions/math/variables.hrx::pi`），
        // 而插值臂曾把常数与一个字面 `* 1e15` 并排打印。
        // 它靠 [`operand_expr`] 保持狭窄：动态部分是 `$var` 或 `#{…}` 的 run 没有操作数读法，
        // 于是 `arithmetic` 因计数守卫 decline，run 走与以前相同的路径 —— 没有任何金样乘过
        // 用户变量，这里也不发明一个。
        arithmetic(toks, env).or_else(|| interpolation(toks, env))
    } else {
        arithmetic(toks, env).or_else(|| concatenation(toks))
    }
}

/// 一个运算的单个**操作数**：有注册的调用、calculation、或数字词素。
///
/// 它有意比 [`from_tokens`] 窄：切分后 run 的操作数在这里一次性读出，所以任何 run 都不会重新进入
/// 自己的切分，而**用户**变量或插值操作数不是本路径的事 ——
/// [`crate::operations::arithmetic`] 在一个 span 在本函数读不出时会丢弃整个 run，这正是这样的
/// run 回落到 [`from_tokens`] 插值臂的方式。把它放宽到包含调用与 calculation，正是
/// `math.div(6, 3px) * 1px` 成为一次运算的原因
/// （`core_functions/math/div.hrx::unit/denominator` → `2`），此前调用词素把 run 标成不可读，
/// 整个声明留在逐字文本。
pub(crate) fn operand_expr(toks: &[Tok], env: &Env) -> Option<Expr> {
    if let Some((name, args)) = crate::calls::known_call(toks) {
        return Some(Expr::Call { name, args });
    }
    if let Some(value) = crate::calculation::value(toks, env) {
        return Some(Expr::Calculation(value));
    }
    let meaningful = toks
        .iter()
        .filter(|tok| !matches!(tok, Tok::Space(_)))
        .collect::<Vec<&Tok>>();
    match meaningful.as_slice() {
        [Tok::Word(word)] => {
            number_lexeme(word).map(|(value, unit)| Expr::Num(value, Unit::named(unit)))
        }
        // 一个内建模块常数是算术的操作数：`math.$pi * 1e15` → `3141592653589793`
        // （`core_functions/math/variables.hrx::pi`）—— 没有这一臂，乘积会 decline，run 打出
        // 常数旁边跟一个字面 `* 1e15`。普通 `$x` **不**加进来：语料里没有金样乘用户变量，
        // 所以那条配对律不是本函数该发明的。
        [Tok::MemberVar(module, name)] => match crate::builtins::module_value(module, name) {
            Some(Value::Number(value, unit)) => Some(Expr::Num(value, unit)),
            _ => None,
        },
        _ => None,
    }
}

/// 一个实参的表达式。与 declaration 值不同，实参即使只是单个数字**也是**表达式
/// （`math.div(6, 3)` → `2`），所以 [`crate::exec::State::evaluated`] 施加给整个 declaration 值的
/// “裸数字还算不上表达式”的过滤器在这里不适用。
pub fn argument_expr(toks: &[Tok], env: &Env) -> Option<Expr> {
    let divided = release_slashes(toks);
    from_tokens(&divided, env).or_else(|| {
        // 实参里的单词素即便在 declaration 值里会被 `protected` 规则拒绝，也读成数字 ——
        // 那条规则是为 CSS 简写（`font: 12px/1.5`）存在的，而 `math.percentage(1%)` 不是：
        // 语料要的是正文给出的 `$number: Expected 1% to have no units.`，而不是一个未读的值。
        let meaningful = divided
            .iter()
            .filter(|tok| !matches!(tok, Tok::Space(_)))
            .collect::<Vec<&Tok>>();
        match meaningful.as_slice() {
            [Tok::Word(word)] => {
                number_lexeme(word).map(|(value, unit)| Expr::Num(value, Unit::named(unit)))
            }
            // 多于一个词素，于是剩下唯一的读法是空格分隔的那个。
            _ => space_run(&divided, env),
        }
    })
}

/// 一个**空格和/或斜杠分隔的实参**，只在词素边界上切：组件是词素 run，从来不是字符串的碎片，
/// 这正是把 `18 52 86` 读成三个数与把碰巧含空格的文本 `split(' ')` 的区别。每个组件都必须
/// 读成操作数，否则整个 run 不可读 —— 这正是让语料的 `missing/*` 行保持诚实的机制：
/// `rgb(none 52 66)` 按原样打印（`none` 还是本编译器没建模的值），它不会变成一个有洞的颜色。
///
/// `/` 只有在自成**组件**时（`0 255 127 / 0.3`，两侧都有空白）才算**列表分隔符**。粘在组件里的
/// `/`（`127/0.3`）属于 [`release_slashes`] 要暴露的算术；任何位置的 `+`/`-`/`*` 也意味着这不是
/// 列表 —— 否则猜错会把 `rgb(0 255 127 / 0.3)` 变成 `86` 除以 `0.3`。
pub(crate) fn space_run(toks: &[Tok], env: &Env) -> Option<Expr> {
    let parts = crate::calls::cut_at(toks, |tok| matches!(tok, Tok::Space(_)))
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<Vec<Tok>>>();
    let operator = |tok: &Tok| {
        matches!(
            tok,
            Tok::Op('/') | Tok::Op('*') | Tok::Op('+') | Tok::Op('-')
        )
    };
    // 分隔符是**自成**组件的 —— 两侧都有版面的 `/`。其余带算子的东西（`127/0.3`、`2 * 3`）
    // 是算术、不是列表，所以整个 run 在这里被拒绝并留在文本。
    let separator = |part: &[Tok]| part.len() == 1 && matches!(part[0], Tok::Op('/'));
    let edge = |part: &Vec<Tok>| separator(part);
    if parts.first().is_some_and(edge) || parts.last().is_some_and(edge) {
        return None;
    }
    if parts
        .iter()
        .any(|part| !separator(part) && part.iter().any(operator))
    {
        return None;
    }
    // 把 run 收成分组。写成对 parts 的 fold，而不是可变下标游走：结构的深度是累加器携带的数据。
    let groups = parts
        .iter()
        .fold(Vec::<Vec<&Vec<Tok>>>::new(), |mut groups, part| {
            if separator(part) {
                groups.push(Vec::new());
            } else if let Some(last) = groups.last_mut() {
                last.push(part);
            } else {
                groups.push(vec![part]);
            }
            groups
        });
    if groups.len() > 2 {
        return None;
    }
    let read = groups
        .iter()
        .map(|group| {
            let values = group
                .iter()
                .map(|part| list_operand(part, env))
                .collect::<Option<Vec<Expr>>>()?;
            match values.len() {
                1 => values.into_iter().next(),
                0 => None,
                _ => Some(Expr::Space(values)),
            }
        })
        .collect::<Option<Vec<Expr>>>()?;
    match read.len() {
        2 => Some(Expr::Slash(read)),
        // 单组只有在装着多个组件时才有意义；孤立词素是调用方的事，不是这次读取的事。
        1 if matches!(read.as_slice(), [Expr::Space(..)]) => read.into_iter().next(),
        _ => None,
    }
}

/// 一个**空格 run** 的单个组件。算术用 [`operand_expr`] 读它的操作数，那里裸词不是数、run 会
/// decline；列表是另一条律 —— CSS Color 4 的 `color(srgb 0.2 0.4 0.8)` 与字体栈都拿裸词当组件，
/// Sass 把裸词读作不带引号的字符串。把这次读法分离出来正是保护算术那一次：
/// `operators/plus::syntax/whitespace/both` 写着 `c + d` 而期望拼接 `cd`，那只有现有的操作数路径
/// 产生，所以当初放宽 [`operand_expr`] 本身把那对弄回归了。
pub(crate) fn list_operand(toks: &[Tok], env: &Env) -> Option<Expr> {
    operand_expr(toks, env).or_else(|| {
        let meaningful = toks
            .iter()
            .filter(|tok| !matches!(tok, Tok::Space(_)))
            .collect::<Vec<&Tok>>();
        // 带括号的词是本切片还没读的调用（`url(…`），不是字符串。
        match meaningful.as_slice() {
            [Tok::Word(word)] if !word.is_empty() && !word.contains('(') => {
                Some(Expr::Text(word.clone()))
            }
            _ => None,
        }
    })
}

/// 调用实参的词素，把每个 `/` 都放行成自己的算子：`Word("1/1px")` 变成 `1`、`/`、`1px`，
/// 于是 `math.unit(1/1px)` 能走到给出 `"px^-1"` 的除法
/// （`core_functions/math/unit::one_denominator`）。
///
/// 这正是 1.4d 要求的情景裁定 —— `/` **在这里**与 `calc( … )` 里意味着除法，而 declaration 值
/// 保留它为列表分隔符（`a {b: 1/2}` → `1/2`、`font: 12px/1.5`），URL 把它留在字面量里。
/// 实参之外没有东西被重切，引号词素从不被切，因为 `"a/b"` 是文本。`Expr::Call` 的逐字臂仍渲染
/// **原始** `arg.toks`，所以本函数读不出的 run 逐字节照旧出来。
pub(crate) fn release_slashes(toks: &[Tok]) -> Vec<Tok> {
    let quotable = |word: &str| word.starts_with('"') || word.starts_with('\'');
    toks.iter()
        .flat_map(|tok| match tok {
            Tok::Word(word) if word.contains('/') && !quotable(word) => word
                .split('/')
                .enumerate()
                .flat_map(|(index, part)| {
                    (index > 0)
                        .then_some(Tok::Op('/'))
                        .into_iter()
                        .chain((!part.is_empty()).then(|| Tok::Word(part.to_string())))
                })
                .collect(),
            other => vec![other.clone()],
        })
        .collect()
}

/// 只有插值形状 —— 一个**选择器**可以用的东西，因为选择器的词素绝不能被读成数字
/// （`.2xl` 是标识符，不是 `0.2xl`）。
pub fn interpolated(toks: &[Tok], env: &Env) -> Option<Expr> {
    has_dynamic(toks)
        .then(|| interpolation(toks, env))
        .flatten()
}

/// 一个插值 run 折成待拼接的部分：字面文本、`$var` 读取、嵌套的 `#{ … }` 体。由一个 `fold`
/// 构建，所以没有命令式循环驱动形状。
///
/// 嵌套的 `#{ … }` 体走与 declaration 值相同的路径（含 `env`），所以 `#{calc(1px)}` 的塌缩
/// 与一个 declaration 的 `calc(1px)` 完全一样。
fn interpolation(toks: &[Tok], env: &Env) -> Option<Expr> {
    let parts = toks
        .iter()
        .fold((Vec::new(), String::new()), |(mut parts, mut lit), tok| {
            let flush = |parts: &mut Vec<Expr>, lit: &mut String| {
                if !lit.is_empty() {
                    parts.push(Expr::Text(std::mem::take(lit)));
                }
            };
            match tok {
                Tok::Var(name) => {
                    flush(&mut parts, &mut lit);
                    parts.push(Expr::Var(name.clone()));
                }
                Tok::MemberVar(module, name) => {
                    flush(&mut parts, &mut lit);
                    parts.push(Expr::MemberVar(module.clone(), name.clone()));
                }
                Tok::Interp(inner) => {
                    flush(&mut parts, &mut lit);
                    parts.push(
                        from_tokens(inner, env).map_or(Expr::Text(canon(&render(inner))), |body| {
                            Expr::Concat(vec![body])
                        }),
                    );
                }
                other => lit.push_str(&token_text(other)),
            }
            (parts, lit)
        });
    let (mut parts, lit) = parts;
    if !lit.is_empty() {
        parts.push(Expr::Text(lit));
    }
    if parts.is_empty() {
        return None;
    }
    Some(Expr::Concat(parts))
}

impl BinOp {
    /// 算子自己的字符，用于把一个 calculation 打回文本。
    pub(crate) fn symbol(self) -> char {
        match self {
            BinOp::Add => '+',
            BinOp::Sub => '-',
            BinOp::Mul => '*',
            BinOp::Div => '/',
        }
    }

    /// 把一个算子施加给两个已求值的操作数，单位律留给 `Value`。
    fn apply(self, lhs: &Value, rhs: &Value) -> Result<Value, SassError> {
        match self {
            BinOp::Add => lhs.plus(rhs),
            BinOp::Sub => lhs.minus(rhs),
            BinOp::Mul => lhs.times(rhs),
            BinOp::Div => lhs.divided_by(rhs),
        }
    }
}

impl Expr {
    /// 对照 owned 环境自底向上求值。错误是 Sass 自己的（未定义变量、不兼容单位）；
    /// 由调用方决定它们在流里走多远。
    pub fn eval(&self, env: &Env) -> Result<Value, SassError> {
        match self {
            Expr::Num(value, unit) => Ok(Value::Number(*value, unit.clone())),
            Expr::Text(text) => Ok(Value::Raw(text.clone())),
            Expr::Var(name) => env.get(name),
            // 一个内建模块常数不查环境即可解析；其余经点号命名的名字属于本编译器没有的文件载入
            // 模块（`upstream.$a`、`utils.$empty-map` —— P5），在那里猜会造出语料没有陈述的
            // `Undefined variable`，所以 run 保留文本。
            Expr::MemberVar(module, name) => crate::builtins::module_value(module, name)
                .ok_or_else(|| SassError::new("no built-in module member under that name")),
            Expr::Call { name, args } => args
                .iter()
                .map(|arg| {
                    // 一个（还）不是表达式的实参在 Sass 里仍然是一个**值** —— 一个不带引号的
                    // 标识符、一个颜色、一个列表 —— 所以它作为文本携带，留给内建自己的参数检查，
                    // 正是那产生语料的 `$number: c is not a number.` 而不是静默跳过调用。
                    let value = match argument_expr(&arg.toks, env) {
                        Some(expr) => expr.eval(env)?,
                        // 一个**还**变不成表达式的实参 —— 带 `/` 或 `*` 的，即 `1/1px`、
                        // `1px * 1em` 这类复合单位，Sass 是当数对待的 —— 不是类型错误的证据。
                        // 整个调用 decline 并保留逐字文本；宣称 `1/1px is not a number.` 会是
                        // 发明一个失败（实测：第一版在 8 条复合用例上确实报的就是这个）。
                        None if protected(&arg.toks) => {
                            return Err(SassError::new("argument needs algebra P1 does not have"));
                        }
                        None => Value::Raw(canon(&render(&arg.toks))),
                    };
                    Ok((arg.label.clone(), value))
                })
                .collect::<Result<Vec<(Option<String>, Value)>, SassError>>()
                .and_then(|given| crate::builtins::evaluate(name, given)),
            Expr::Binary(lhs, op, rhs) => op.apply(&lhs.eval(env)?, &rhs.eval(env)?),
            Expr::Concat(parts) => parts
                .iter()
                .map(|part| part.eval(env).map(|value| value.to_css()))
                .collect::<Result<Vec<String>, SassError>>()
                .map(|text| Value::Raw(canon(&text.concat()))),
            // 每个组件保留自己的打印形式、分隔符留在原地：这就是空格列表的读法
            // （未被转换的 `rgb(18 52 86)` 按原样打印，`values/lists/*` 保留空格），
            // 也是颜色构造函数能结构化地拿同一个值的原因。
            Expr::Space(parts) => parts
                .iter()
                .map(|part| part.eval(env))
                .collect::<Result<Vec<Value>, SassError>>()
                .map(Value::Space),
            Expr::Slash(parts) => parts
                .iter()
                .map(|part| part.eval(env))
                .collect::<Result<Vec<Value>, SassError>>()
                .map(Value::Slash),
            // 读一个 calculation 时已对照环境解析过它 —— 所以该变体携带一个值而不是词素树 ——
            // 于是求值它就是恒等。
            Expr::Calculation(value) => Ok(value.clone()),
        }
    }
}
