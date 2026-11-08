//! 读 `calc( … )` **内部**的算术：扁平化读取，而不是解释执行。
//!
//! GC 式的做法是建一棵节点互指的树、再走两遍 —— 一遍化简、一遍打印。本模块不保留这种结构：一趟遍历把
//! 词素 run 变成 **带深度的文档序列表**（`level` 与 `owner` 是数据字段，所以嵌套成为索引，而不是调用栈），
//! 随后运行段 **最深层级优先** 归约 —— 保证一个分组被其父级看到时，它已经是一枚 owned 摘要。摘要持有折出的
//! 值、它打印的文本（在形成的当刻写下，绝不重新推导）以及它的优先级 —— 没有子指针、没有递归、没有遍历标志。
//!
//! ## 各条律，每条都追到一个 fixture
//!
//! *每一步都可算时，链折到底*：`calc(1px - 2px)` → `-1px`（`operator/minus::simplified`）、
//! `calc(1px + 20px - 300px + 4000px - 50000px)` → `-46279px` 与 `calc(2 * 3 + 4 / 5 - 6)` → `0.8`
//! （`operator/precedence::simplified/*`）、`calc(1px * 2)` → `2px`（`operator/times::simplified`）。一步
//! 算不动只停它自己：`calc(1px - 2%)` 照留（`minus::preserved/number`），复合单位留在壳内
//! （`Unit::is_plain`、`values/numbers/units/multiple::multiple_denominators`）。`+`/`-` 复用
//! [`Value::plus`]/[`Value::minus`]、`*`/`/` 复用 [`Value::times`]/[`Value::divided_by`]，所以单位律住在
//! 一个地方，与 `calc` 之外的算子共用。
//!
//! *括号在拼接的当刻由优先级决定*，绝不照抄、也绝不由第二遍重新推导：`calc((1 + var(--c)))` →
//! `calc(1 + var(--c))`（`operator/var::indirectly_parenthesized`）、`calc(1px + (2% - 3em))` →
//! `calc(1px + 2% - 3em)`（`precedence::preserved/additive/parens`）、`calc(1px * (2 / var(--c)))` →
//! `calc(1px * 2 / var(--c))`（`…/multiplicative/default/parens`），而
//! `calc(1px / (2 * var(--c)))`（`…/needs_parens/parens`）、`calc(1px * (2 + var(--c)))`
//! （`…/multiplicative_then_additive/parens`）与 `calc(1px - (2% + var(--c)))`
//! （`operator/minus::preserved/plus`）保住自己的。只装单个操作数的分组 **保住它自己的** 括号 —— 那段
//! 文本属于操作数（`var::directly_parenthesized`），这也是嵌套 `calc(var(--c))` 的落点
//! （`var::calculation`）；而装着一条链的嵌套 `calc( … )` 把它的各项拼进父级
//! （`precedence::preserved/additive/calculation`）。
//!
//! *读不出来的东西 decline*：一个词素无法交替成操作数与算子的 run —— 相邻操作数是一个 **空格分隔列表**、
//! 照写保留（`calc/space.hrx::var/before`：`calc(var(--c) 1)` → `calc(var(--c) 1)`）、`+`/`-` 缺了它要求
//! 空格的算子、在实参内永不闭合的括号分组、或求值失败的插值（把它变成文本会吞掉一个致命错误）。调用方
//! 随后原样保留源码文本。

use crate::brackets::{
    Entry, ROOT, Run, builtin_call, is_layout, merge_calls, opens, partition, spaced, trim_layout,
};
use crate::env::Env;
use crate::expr;
use crate::expr::BinOp;
use crate::lexer::Tok;
use crate::unit::Unit;
use crate::value::Value;

/// `calculation` 对单个实参的需求：它到底能不能读、是不是一个可以离开 `calc( … )` 壳的值、
/// 或它该在壳内打印什么。
pub enum Reading {
    Declined,
    Value(Value),
    Expression(Content),
}

/// 链的一项内容：它折到的值（当每步都可算时）、它打印的文本，以及让父级决定括号的优先级。`3` 是单个
/// 操作数。
///
/// 这也是 [`Value::Calculation`] 存的东西，所以**一个有界的 calculation 能拆回算术**：
/// `calc($number / 1px)` 配上 `$number: calc(1px * 1rad / 1ms / 1Hz)` 会抵消成
/// `calc(1rad / 1ms / 1Hz)`（`values/numbers/units/multiple::division/cancels/numerator/once`）——
/// 归一化后的文本本身表达不了这个。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Content {
    /// 链折到的量级与单位，当每步都可算时 —— 存成数据而不是 `Value`，好让 calculation 的内容无法指回
    /// 它所属的值
    pub(crate) folded: Option<(f64, Unit)>,
    pub(crate) printed: String,
    pub(crate) precedence: u8,
}

/// 一个独立值作为内容：它的 CSS 文本，以及它是数时的那个数。
fn content_of(value: &Value) -> Content {
    Content {
        folded: match value {
            Value::Number(value, unit) => Some((*value, unit.clone())),
            _ => None,
        },
        printed: value.to_css(),
        precedence: 3,
    }
}

/// 归约后链的一步：一个操作数，或拼起下一个的算子。
#[derive(Clone)]
enum Slot {
    Operand(Content),
    Join(BinOp),
}

/// 一个 `calc( … )` 壳（大小写不敏感）：唯一一个体被拼进父级的名字。
pub(crate) fn is_calc_name(callee: &str) -> bool {
    callee.eq_ignore_ascii_case("calc")
}

/// 读一个 calculation 实参。
pub fn read(toks: &[Tok], env: &Env) -> Reading {
    let flat = trim_layout(&expr::release_slashes(toks));
    // 一次调用是**一个操作数**：把 `var(--c)` 切成 `var(`、`--c`、`)` 会让它变成三个词素，外围就读不出
    // 链了（与 1.4a 的词素粘着括号同一条教训 —— 分类不能比词素更细）。单独的 `calc(` 保住它的括号，
    // 因为拼入嵌套 calculation 本身就是一条律（`precedence::preserved/additive/calculation`）。
    // 一个已注册的内建调用若**就是**整个实参，那是内建表的业务、不是本模块的：
    // `calc(math.round(2.3))` → `2`（`no_operator::function/sass/namespace`）。与其他操作数混排的形状
    // 没有任何输入/输出 fixture 给出答案，所以这样的 run decline，而不是发明一个。
    if let Some(value) = whole_call(&flat, env) {
        return match value {
            Value::Number(number, unit) if unit.is_plain() => {
                Reading::Value(Value::Number(number, unit))
            }
            value => Reading::Expression(content_of(&value)),
        };
    }
    let merged = merge_calls(&flat);
    if merged
        .iter()
        .any(|tok| matches!(tok, Tok::Word(word) if builtin_call(word)))
    {
        return Reading::Declined;
    }
    let runs = partition(&merged);
    let deepest = runs.iter().map(|run| run.level).max().unwrap_or(0);
    // 最深层优先，好让一个分组在其父级读它之前就是一份摘要。owner 是括号索引，而子级的括号总在父级
    // 之后，所以按层级排序的折叠就够了。
    let done = (0..=deepest).rev().collect::<Vec<u32>>().into_iter().fold(
        Vec::<(usize, Content)>::new(),
        |mut reduced, level| {
            let finished = runs
                .iter()
                .filter(|run| run.level == level)
                .filter_map(|run| reduce(run, &reduced, env).map(|term| (run.owner, term)))
                .collect::<Vec<(usize, Content)>>();
            reduced.extend(finished);
            reduced
        },
    );
    done.iter()
        .find(|(owner, _)| *owner == ROOT)
        .map(|(_, root)| finishing(root))
        .unwrap_or(Reading::Declined)
}

/// 一个折出的**plain** 数离开壳（`calc(1px * 2)` → `2px`，`operator/times::simplified`）；其余任何
/// 东西，包括复合单位（`1 / 1px / 1rad`），在壳内打印
/// （`values/numbers/units/multiple::multiple_denominators`）。
fn finishing(content: &Content) -> Reading {
    match &content.folded {
        Some((value, unit)) if unit.is_plain() => {
            Reading::Value(Value::Number(*value, unit.clone()))
        }
        _ => Reading::Expression(content.clone()),
    }
}

/// 归约一个 run：在加性算子（最松的一级）处切开，在每段的 `*`/`/` 算子（更紧的一级）处折叠，然后
/// 从左到右拼接各段。两次折叠，所以优先级不需要递归；`1 - 2 - 3` 出来是 `(1 - 2) - 3`，因为拼接
/// 按文档序走。
fn reduce(run: &Run, done: &[(usize, Content)], env: &Env) -> Option<Content> {
    let entries = run.entries.as_slice();
    let slots = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !is_layout(&entry.tok))
        .map(|(index, entry)| slot(entry, index, entries, done, env))
        .collect::<Option<Vec<Slot>>>()?;
    let pieces =
        slots
            .into_iter()
            .fold(Vec::<(Option<BinOp>, Vec<Slot>)>::new(), |mut acc, slot| {
                match slot {
                    Slot::Join(op) if matches!(op, BinOp::Add | BinOp::Sub) => {
                        acc.push((Some(op), Vec::new()));
                    }
                    other => {
                        if acc.is_empty() {
                            acc.push((None, Vec::new()));
                        }
                        if let Some((_, piece)) = acc.last_mut() {
                            piece.push(other);
                        }
                    }
                }
                acc
            });
    pieces
        .into_iter()
        .map(|(op, piece)| chain(piece).map(|term| (op, term)))
        .collect::<Option<Vec<(Option<BinOp>, Content)>>>()?
        .into_iter()
        .fold(None::<Content>, |left, (op, right)| match (left, op) {
            (None, _) => Some(right),
            (Some(left), Some(op)) => Some(join(left, op, right)),
            // 两段之间没有算子，那是列表、不是求和：这一 run decline。
            (Some(_), None) => None,
        })
}

/// 折叠一个乘性段：操作数与拼接算子必须交替，与 [`crate::operations`] 里完全一样，而这个严格的
/// 交替测试正是拒绝本模块无法建模的 run 的判据。
fn chain(piece: Vec<Slot>) -> Option<Content> {
    let operands = piece
        .iter()
        .step_by(2)
        .map(|slot| match slot {
            Slot::Operand(term) => Some(term.clone()),
            Slot::Join(_) => None,
        })
        .collect::<Option<Vec<Content>>>()?;
    let joins = piece
        .iter()
        .skip(1)
        .step_by(2)
        .map(|slot| match slot {
            Slot::Join(op) => Some(*op),
            Slot::Operand(_) => None,
        })
        .collect::<Option<Vec<BinOp>>>()?;
    if operands.is_empty() || joins.len() + 1 != operands.len() {
        return None;
    }
    let head = operands.first()?.clone();
    Some(
        operands
            .into_iter()
            .skip(1)
            .zip(joins)
            .fold(head, |left, (right, op)| join(left, op, right)),
    )
}

/// 一个词素作为一个槽：间距律接受的一个算子、一个已归约的分组，或一个操作数。
fn slot(
    entry: &Entry,
    index: usize,
    entries: &[Entry],
    done: &[(usize, Content)],
    env: &Env,
) -> Option<Slot> {
    if let Some(op) = operator(&entry.tok) {
        // `+`/`-` 只有在两侧都有空格时才是算子 —— 这条律让 `10px -5px` 保持为双项列表。`*` 与被
        // 放行的 `/` 没有这种歧义。
        return match (op, spaced(entries, index)) {
            (BinOp::Add | BinOp::Sub, false) => None,
            _ => Some(Slot::Join(op)),
        };
    }
    operand(entry, done, env).map(Slot::Operand)
}

/// 一个操作数：一个分组，由开启它的括号位置解析（`partition` 把它作为占位留在父 run 里），
/// 或单个词素。
fn operand(entry: &Entry, done: &[(usize, Content)], env: &Env) -> Option<Content> {
    if is_layout(&entry.tok) {
        return None;
    }
    if opens(&entry.tok) {
        return done
            .iter()
            .find(|(owner, _)| *owner == entry.index)
            .map(|(_, term)| parenthesised(term));
    }
    leaf(&entry.tok, env)
}

/// 围住单个操作数的分组保住自己的括号；围住一条链的分组由优先级重新推导
/// （`operator/var::directly_parenthesized` 对 `var::indirectly_parenthesized`）。
fn parenthesised(term: &Content) -> Content {
    match term.precedence {
        3 => Content {
            folded: term.folded.clone(),
            printed: format!("({})", term.printed),
            precedence: 3,
        },
        _ => term.clone(),
    }
}

/// 单个词素作为一个项。插值是一个 **值**（`calc(#{1px + 2px})` → `calc(3px)`，
/// `no_operator::interpolation/number`）；求值失败的函数体 decline，而不是变成文本 —— 把那个错误
/// 吞进字符串，正是致命 `Undefined variable` 曾消失的原因。
fn leaf(tok: &Tok, env: &Env) -> Option<Content> {
    let atom = |printed: String| {
        Some(Content {
            folded: None,
            printed,
            precedence: 3,
        })
    };
    match tok {
        Tok::Word(word) => Some(match crate::operations::number_lexeme(word) {
            Some((value, unit)) => number(value, &Unit::named(unit)),
            // 不是数字的词仍可能是 CSS 的四个 **calculation 常数**之一；其余任何东西都是语料照写的
            // 未知标识符（`…::constant/undefined/alone` 用 `calc(c)` 答 `calc(c)`）。
            None => constant(word).map_or_else(
                || Content {
                    folded: None,
                    printed: (*word).clone(),
                    precedence: 3,
                },
                |value| number(value, &Unit::unitless()),
            ),
        }),
        // 一个有界的名字经环境解析。一个有界的 **calculation** 以它已经是的内容进入 —— 折出的值、
        // 打印形式与优先级 —— 这才让 `calc($number / 1px)` 能与有界乘积抵消
        // （`values/numbers/units/multiple::division/cancels/numerator/once`）。带着自己名字的
        // calculation（`min( … )`）保住那个名字，因为只有 `calc` 会把体拼进父级。
        Tok::Var(name) => match env.get(name) {
            Ok(Value::Number(value, unit)) => Some(number(value, &unit)),
            Ok(Value::Calculation(callee, content)) => Some(if is_calc_name(&callee) {
                content.clone()
            } else {
                Content {
                    folded: content.folded.clone(),
                    printed: format!("{callee}({})", content.printed),
                    precedence: 3,
                }
            }),
            Ok(other) => atom(other.to_css()),
            Err(_) => atom(format!("${name}")),
        },
        // 带命名空间的引用要么是内建模块常数、要么什么都不是：已知的经 `module_value` 解析
        // （`calc(math.$pi)` → `3.1415926536`，`no_operator::variable/namespace`）；未知的没有一行
        // fixture 放进 calculation，所以 `upstream.$a` 保住自己的文本，与上面未绑定的 `$name` 处理一致，
        // 而不是变成对这个编译器从不读的文件的猜测。
        Tok::MemberVar(module, name) => match crate::builtins::module_value(module, name) {
            Some(Value::Number(value, unit)) => Some(number(value, &unit)),
            Some(other) => atom(other.to_css()),
            None => atom(format!("{module}.${name}")),
        },
        Tok::Interp(inner) => match expr::interpolated(inner, env).map(|built| built.eval(env)) {
            Some(Ok(value)) => atom(value.to_css()),
            _ => None,
        },
        Tok::Punct(_) | Tok::Op(_) | Tok::Space(_) | Tok::Silent(_) => None,
    }
}

fn number(value: f64, unit: &Unit) -> Content {
    Content {
        folded: Some((value, unit.clone())),
        printed: unit.calc_form(value),
        precedence: 3,
    }
}

/// 一个大小写不敏感、可带前导符号地命名 CSS 四个 calculation 常数之一的词：
/// `values/calculation/calc/constant.hrx` 读得 `calc(pi)` → `3.1415926536`、`calc(pI)` 同样、
/// `calc(e)` → `2.7182818285`、`calc(infinity)` 与 `calc(NaN)` 各自壳内返回（非有限数没有 plain CSS
/// 拼法 —— [`crate::value::Value::to_css`] 把它包起来），而 `calc(-infinity)` 保住符号。
///
/// **数值不在这里重复**：它们经 [`crate::builtins::module_value`] 而来，与 [`leaf`] 的 `math.$pi`
/// 臂同一扇门，所以 `pi` 在本 crate 只有一个值。本文件拥有的是**名字列表**，而且它有意比那张表窄：
/// 表里还有 `epsilon` 与 `max-safe-integer`，而没有任何 golden 把它们放进 calculation
/// （`calc(epsilon)`、`calc(max-safe-integer)` 全语料零命中），所以 `calc(epsilon)` 保持
/// `…::constant/undefined/alone` 所说的未知标识符身份。
fn constant(word: &str) -> Option<f64> {
    let (sign, name) = word
        .strip_prefix('-')
        .map_or((1.0, word), |tail| (-1.0, tail));
    ["pi", "e", "infinity", "nan"]
        .into_iter()
        .find(|spell| name.eq_ignore_ascii_case(spell))
        .and_then(|_| crate::builtins::module_value("math", &name.to_ascii_lowercase()))
        .and_then(|value| match value {
            Value::Number(number, _) => Some(number * sign),
            _ => None,
        })
}

/// 拼接两个项：两侧都是数且单位律一致时折出值，文本在这里一次性写下，带上这个位置要求的括号。
fn join(left: Content, op: BinOp, right: Content) -> Content {
    let level = if matches!(op, BinOp::Add | BinOp::Sub) {
        1
    } else {
        2
    };
    let printed = format!(
        "{} {} {}",
        wrap(&left, op, Side::Left),
        op.symbol(),
        wrap(&right, op, Side::Right)
    );
    match op.fold(&left, &right) {
        Some((value, unit)) => Content {
            folded: Some((value, unit.clone())),
            printed: unit.calc_form(value),
            precedence: level,
        },
        folded => Content {
            folded,
            printed,
            precedence: level,
        },
    }
}

/// 子级位于算子的哪一侧：`-` 与 `/` 在右侧不满足结合律。
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// 子级恰在丢掉这层括号会改变值的位置加括号。
fn wrap(child: &Content, parent: BinOp, side: Side) -> String {
    let up = match parent {
        BinOp::Add | BinOp::Sub => 1,
        BinOp::Mul | BinOp::Div => 2,
    };
    let needs = match side {
        Side::Left => child.precedence < up,
        Side::Right => {
            child.precedence < up
                || (child.precedence == up && matches!(parent, BinOp::Sub | BinOp::Div))
        }
    };
    if needs && child.precedence < 3 {
        format!("({})", child.printed)
    } else {
        child.printed.clone()
    }
}

impl BinOp {
    /// 值级的律，复用而不是重述。`None` 表示这一步不可算，这正是把这个运算留在打印文本里的东西。
    fn fold(self, left: &Content, right: &Content) -> Option<(f64, Unit)> {
        let (Some((a, au)), Some((b, bu))) = (&left.folded, &right.folded) else {
            return None;
        };
        let (left, right) = (Value::Number(*a, au.clone()), Value::Number(*b, bu.clone()));
        match self {
            BinOp::Add => left.plus(&right),
            BinOp::Sub => left.minus(&right),
            BinOp::Mul => left.times(&right),
            BinOp::Div => left.divided_by(&right),
        }
        .ok()
        .and_then(|value| match value {
            Value::Number(value, unit) => Some((value, unit)),
            _ => None,
        })
    }
}

fn operator(tok: &Tok) -> Option<BinOp> {
    let Tok::Op(sign) = tok else {
        return None;
    };
    match sign {
        '+' => Some(BinOp::Add),
        '-' => Some(BinOp::Sub),
        '*' => Some(BinOp::Mul),
        '/' => Some(BinOp::Div),
        _ => None,
    }
}

fn whole_call(run: &[Tok], env: &Env) -> Option<Value> {
    let Tok::Word(head) = run.first()? else {
        return None;
    };
    let closes_last = match run.last() {
        Some(Tok::Punct(')')) => true,
        Some(Tok::Word(word)) => word.ends_with(')') && !word.ends_with('('),
        _ => false,
    };
    if !builtin_call(head) || !closes_last {
        return None;
    }
    match expr::argument_expr(run, env).map(|built| built.eval(env)) {
        Some(Ok(value)) => Some(value),
        _ => None,
    }
}
