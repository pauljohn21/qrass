//! 在**词素**一级读调用语法（tasks 1.7a）：当一个 declaration 的值是一个我们拥有签名的内置函数调用时，
//! 它的实参被切分并交出去求值。
//!
//! 这里的一切由两条规则定，两条都是吃过亏才学会的（1.2b、0′.3b）：
//! • 一个实参前面的 `$name:` 是一个**标签**，从来不是一个引用；
//! • 分类器的粒度永远不可能比 lexer 更细。`(` 与 `)` 都不是结构字符，而 `(` 也不会刷出 pending 的那个词，
//!   所以源码里的 `math.div(6)` 到达时是一个单一词素 `Word("math.div(6")` —— 被调名、紧跟的 `(` 与它
//!   后面的数字焊在了一起。所以读一次调用必须**先把括的边界规范化**（[`brackets`]），那是词素的一个局部视图：上游
//!   什么都不变，而 `url(http://x)`/`-a-calc(` 保住它们已有的行为：CSS 原生函数从未进登记表。
//!   现量得带上阳性对照（只 grep 名字本身分不出“缺席”与“在表里”）：
//!   `grep -rn 'name: "var"' crates/sasspile/src/builtins/` = **0** 且 `name: "calc"` 也 = **0**，
//!   而同法对已登记的 `name: "math.div"` 与 `name: "rgb"` 交出 **2**。

use crate::brackets::trim_layout;
use crate::builtins;
use crate::lexer::Tok;

/// 一个调用的一个实参：它可选的 `$label:`，与组成它那个值的词素。
#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    pub label: Option<String>,
    pub toks: Vec<Tok>,
}

/// 从一个 token 串里取 `name(…)`，**仅当 `name` 解到一个已登记的内置函数签名**。
///
/// 签名这道闸就是本步的全部安全性：`color.contrast`（尚无签名）与任何 CSS 原生的 `var(…)`/`calc(…)`
/// 都到不了实参绑定，所以它们保住今天的逐字行为，而不是被一半求值。调用者因此必须把 `None` 当作
/// “不是我们认识的调用”（登记表就是 `builtins/mod.rs` 的 `BUILTINS = &[math::MATHS, color::COLORS]`，
/// 而 `signature` 的文档自己点名 `color.contrast`、`var`、`map.merge` 这三个是 `None`）。
pub fn known_call(toks: &[Tok]) -> Option<(String, Vec<Argument>)> {
    let (name, args) = split(&brackets(toks))?;
    let found = builtins::signature(&name);
    // “这次调用为什么仍是逐字？”的另一半：一个未知被调名是一个判定，而不是调用者的 bug，
    // 而实参个数才是分开这两种情形的东西。
    tracing::debug!(
        stage = "exec",
        module = "call",
        callee = name.as_str(),
        args = args.len(),
        signature = found.map_or("<none: kept verbatim>", |_| "<registered>"),
        "call decision"
    );
    found.map(|builtin| (builtin.name.to_string(), args))
}

/// 把一串 token 重切，让每一个括各占一个词素：`Word("math.div(6")` → `Word("math.div(")`、`Word("6")`。
///
/// 一个被调名保住它紧跟的 `(`（那正是标记它是被调名的东西），而一个 `)` 永远成为自己的 `Punct`。
/// 两括之间的文本原样复制，所以任何稍后要打印的东西都分不出这个规范化视图与源码的差别。
///
/// 与 [`crate::calculation`] 共用：一个 calculation 的 `calc(1px` 因同一个理由粘在一起，而两边需要的都是
/// 那唯一一次括规范化，不是第二份。
pub(crate) fn brackets(toks: &[Tok]) -> Vec<Tok> {
    toks.iter()
        .flat_map(|tok| match tok {
            Tok::Word(word) => cut(word),
            other => vec![other.clone()],
        })
        .collect()
}

/// 一个词在它所有的括处被切开，对它的字符只做一遍前向扫描。
fn cut(word: &str) -> Vec<Tok> {
    let (lexemes, pending) = word.chars().fold(
        (Vec::<Tok>::new(), String::new()),
        |(mut lexemes, mut pending), c| {
            match c {
                '(' => {
                    pending.push('(');
                    lexemes.push(Tok::Word(std::mem::take(&mut pending)));
                }
                ')' => {
                    if !pending.is_empty() {
                        lexemes.push(Tok::Word(std::mem::take(&mut pending)));
                    }
                    lexemes.push(Tok::Punct(')'));
                }
                other => pending.push(other),
            }
            (lexemes, pending)
        },
    );
    lexemes
        .into_iter()
        .chain((!pending.is_empty()).then_some(Tok::Word(pending)))
        .collect()
}

/// 把 `callee( … )` 分成被调名与它的顶层实参。
///
/// 这次调用必须是**整个**那一串：`math.div(6, 3) * 1px` 是一个单位表达式，它的结果带着一个本编译器
/// 尚未建模的复合单位，而发明一个部分值会打出一个错的数字。深度是在词素上计数的，不是递归进一棵树：
/// 一个 `…(` 词素开一层，一个 `Punct(')')` 关一层，而只有一个深度为 0 的 `,` 分隔实参。
fn split(toks: &[Tok]) -> Option<(String, Vec<Argument>)> {
    // 被调名是在那一串的**头部**找的，而不是把整串过滤一遍：括号内部的布局是实参自己的数据。
    // 在这里把它丢掉（本函数过去就是这么做的）会让 `rgb(18 52 86)` 的实参到达时是三个 Word
    // （`"18"`、`"52"`、`"86"`）而且一个分隔符都没有，于是 CSS Color 4 的单参数拼法在低于词素的任何
    // 词素那时已经没了。两端的仍要剥掉（`rgb( 1 )` 是一个实参 `1`），中间的间距保留。
    let meaningful = |range: &[Tok]| {
        range
            .iter()
            .position(|tok| !matches!(tok, Tok::Space(_) | Tok::Silent(_)))
            .zip(
                range
                    .iter()
                    .rposition(|tok| !matches!(tok, Tok::Space(_) | Tok::Silent(_))),
            )
    };
    let (head, tail) = meaningful(toks)?;
    let callee = match &toks[head] {
        Tok::Word(word) => word.clone(),
        _ => return None,
    };
    let name = callee.strip_suffix('(').filter(|name| !name.is_empty())?;
    if !matches!(&toks[tail], Tok::Punct(')')) {
        return None;
    }
    let inner = &toks[(head + 1)..tail];
    let arguments = top_level_groups(inner)
        .into_iter()
        .map(|arg| trim_layout(&arg))
        .filter(|arg| !arg.is_empty())
        .map(argument)
        .collect::<Vec<Argument>>();
    // `name()` 是一个真实的零实参调用；内部有词素却没切出任何实参，那条路就不是以调用应有的方式
    // 闭合的，所以这一串不被当作一次调用。
    if arguments.is_empty() && !inner.is_empty() {
        return None;
    }
    Some((name.to_string(), arguments))
}

/// 在一串的词素上，于每一个深度为 0 的逗号处切开。owned 的切片，调用之间不留下标游标。
///
/// 一个 calculation 的实参清单在同一批逗号上切分，所以这次切分在这里只住一份
/// （`crate::calculation::value` 是第二个调用者）。
pub(crate) fn top_level_groups(toks: &[Tok]) -> Vec<Vec<Tok>> {
    cut_at(toks, |tok| matches!(tok, Tok::Punct(',')))
}

/// 在任意分隔符上的同一个切法：顶层 `is_separator` 那些 token 之间的词素分组。深度是把正在构造的那个分组
/// 当作数据来计数的，用的是那唯一一条括律（[`crate::lexer::Tok::net`]），所以顶层分隔符不会切进嵌套的括。
/// 这条“切不开”有两个测试守着：`tests/calculation.rs:70` 的 `calc(clamp(1%, 2px, 3%))` 与
/// `tests/calculation.rs:323` 的 `math.div(1, min(10, 20))`；`var(--a, b)` 只是同一个形状的**推断例**，
/// 语料与测试里都没有它。`cut_at(.., |tok| matches!(tok, Tok::Space(_)))` 就是把一个空格分隔的实参
/// （`rgb(18 52 86)`）变成三段 token 串的办法 —— 见 [`crate::expr::space_run`]。
pub(crate) fn cut_at(toks: &[Tok], is_separator: impl Fn(&Tok) -> bool) -> Vec<Vec<Tok>> {
    let (groups, current) = toks.iter().fold(
        (Vec::<Vec<Tok>>::new(), Vec::<Tok>::new()),
        |(mut groups, mut current), tok| {
            if is_separator(tok) && crate::brackets::depth_of(&current) == 0 {
                groups.push(std::mem::take(&mut current));
            } else {
                current.push(tok.clone());
            }
            (groups, current)
        },
    );
    groups.into_iter().chain([current]).collect()
}

/// 单个实参：把一个开头的 `$label:`（一个名字，不是一个引用）从前面剥掉。
fn argument(toks: Vec<Tok>) -> Argument {
    let label = match toks.as_slice() {
        [Tok::Var(name), Tok::Punct(':'), ..] => Some(name.clone()),
        _ => None,
    };
    let value = if label.is_some() {
        toks[2..].to_vec()
    } else {
        toks
    };
    Argument { label, toks: value }
}
