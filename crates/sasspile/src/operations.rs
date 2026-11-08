//! 词素级的读取器，把一条词素 run 变成一棵**运算树**：在顶层算子上切分、数字词素、标识符的字符串拼接、
//! 以及两遍的优先级折叠。它们是 `expr` 中关心*源形状*而非值的那一半，正因如此住在这里，正因如此
//! `expr` 只保留读取顺序与 `eval`。
//!
//! 语料钉住的四条律，在各自生效处复述：
//! * 一个读不出的操作数使**整条 run decline**，而不是被跳过 —— 静默跳过会挪动交替，把
//!   `rgb(18 52 none)` 变成数字 `52`；
//! * `+`/`-` 只有在两侧都有空白时才是算子（`10px - 5px` 是减法，`10px -5px` 是双项列表），而 `*`
//!   不需要这条律、`/` 只在调用方放行它的地方才是算子（`expr::release_slashes`，即调用实参内）；
//! * 括号把 run 套起来：深度是折叠携带的一个数，括号内的算子绝不切分包围它的东西 —— 正因如此一次调用或
//!   一次 calculation 才能作为一个操作数（`core_functions/math/div.hrx::unit/denominator`：
//!   `math.div(6, 3px) * 1px` → `2`）；
//! * 优先级由对扁平 span 列表的**两遍折叠**解决（先每块松件内部的紧算子，再各块之间），所以任何地方
//!   都不需要对 run 做递归下降。

use crate::env::Env;
use crate::expr::{self, BinOp, Expr};
use crate::lexer::Tok;

/// 切分后 run 的操作数 span：下标区间，指向调用方已持有的词素。
pub(crate) type Spans = Vec<(usize, usize)>;

/// 一个可能根本不是算术的值：词素里任何位置的斜杠、星号或百分号，在值代数覆盖它们之前，都是 CSS
/// 分隔符（或百分比单位）。
///
/// **括号内不是同一个情景。** 坐在 `calc( … )` 里的 `/` 属于那次 calculation，正因如此
/// `calc(1/2px + 1/4px) * 1px` 必须可读成一次 calculation 与一个数的乘法
/// （`values/calculation/calc/operator/units::denominators` → `0.75`）。深度作为数据沿 run 计数，所以
/// `font: 12px/1.5`、`grid: 1/2` 与裸的 `a {b: 1/2}` —— 它们都不含括号 —— 一如既往地受保护。
pub(crate) fn protected(toks: &[Tok]) -> bool {
    let seeded = (false, 0i32);
    let (found, _) = toks.iter().fold(seeded, |(found, depth), tok| {
        let net = tok.net();
        let outside = depth == 0 && net == 0;
        let separator = match tok {
            Tok::Word(word) => word.contains(['/', '*', '%']),
            Tok::Interp(inner) => protected(inner),
            _ => false,
        };
        (found || (outside && separator), (depth + net).max(0))
    });
    found
}

/// 一条 run 是否装着只有活着的环境才能回答的东西 —— 一个引用、一个模块常数或一次插值。这是
/// [`crate::expr`] 在投入一次求值之前要问的形状测试。
pub(crate) fn has_dynamic(toks: &[Tok]) -> bool {
    toks.iter()
        .any(|tok| matches!(tok, Tok::Var(_) | Tok::MemberVar(..) | Tok::Interp(_)))
}

/// 一条 run 在顶层算子处切开：操作数 span，以及每对之间的算子。`None` 意为“不是我们读的那种运算” ——
/// 根本没有任何顶层算子，于是孤立数字或 CSS 调用留给其他读取器。
pub(crate) fn split(toks: &[Tok]) -> Option<(Spans, Vec<BinOp>)> {
    let (cuts, _) =
        toks.iter()
            .enumerate()
            .fold((Vec::<(usize, BinOp)>::new(), 0i32), |acc, (index, tok)| {
                let (mut cuts, depth) = acc;
                let balance = tok.net();
                if depth == 0
                    && balance == 0
                    && let Some(op) = operator(toks, index)
                {
                    cuts.push((index, op));
                }
                (cuts, depth + balance)
            });
    let operators = cuts.iter().map(|(_, op)| *op).collect::<Vec<BinOp>>();
    let spans = cuts
        .iter()
        .map(|(index, _)| *index)
        .chain(std::iter::once(toks.len()))
        .scan(0usize, |from, end| {
            let span = (*from, end);
            *from = end + 1;
            Some(span)
        })
        .collect::<Spans>();
    (!operators.is_empty()).then_some((spans, operators))
}

/// run 里这个位置上的算子。`+`/`-` 在**两侧**都要有空白，否则根本不是算子：仅这一条律就让
/// `10px -5px` 保持为双项列表、`a -b` 保持为标识符实参。
fn operator(toks: &[Tok], index: usize) -> Option<BinOp> {
    let Tok::Op(sign) = &toks[index] else {
        return None;
    };
    let spaced =
        |side: Option<usize>| matches!(side.and_then(|i| toks.get(i)), Some(Tok::Space(_)));
    match sign {
        '+' if spaced(index.checked_sub(1)) && spaced(index.checked_add(1)) => Some(BinOp::Add),
        '-' if spaced(index.checked_sub(1)) && spaced(index.checked_add(1)) => Some(BinOp::Sub),
        '*' => Some(BinOp::Mul),
        '/' => Some(BinOp::Div),
        _ => None,
    }
}

/// `tail` 里有多长是指数 —— `e15`、`E-3`、`e+2`。当（可能带符号的）`e` 之后没有数字时为零，正是这一点
/// 让 `1e` 与 `1e-` 保持为**单位**是 `e` 的数：只有带数字的指数形式才有金样
/// （`values/calculation/calc/no_operator::number/exponent`）。
fn exponent_len(tail: &str) -> usize {
    let marker = match tail.chars().next() {
        Some(c @ ('e' | 'E')) => c.len_utf8(),
        _ => return 0,
    };
    let rest = &tail[marker..];
    let signed = usize::from(matches!(rest.chars().next(), Some('+') | Some('-')));
    let digits = rest[signed..]
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map_or(rest.len() - signed, |(i, _)| i);
    if digits > 0 {
        marker + signed + digits
    } else {
        0
    }
}

/// 把整个词素读成 `数值 + 单位`：数字（可带可选小数与可选指数），后接一个标识符形状的单位。`52)` 或
/// `1px)` 不是数 —— 单位是字母数字的，所以像 `rgb(18 none 66)` 这样的 CSS 调用会退回逐字文本，而不是
/// 丢掉它的括号。
pub(crate) fn number_lexeme(word: &str) -> Option<(f64, &str)> {
    // 前导符号属于数字：lexer 把边界 `-` 推迟到看见下一个字符，所以 `-5.6` 与 `-7px` 作为单个词素到达，
    // Sass 把它们读作负数（`math.round(-5.6)` → `-6`）。`@for` 的负下界早就要求这一点，而把它读成文本
    // 曾造出一个假的 `$number: -5.6 is not a number.`。
    let signed = word.starts_with(&['-', '+'][..]);
    let (sign, digits_area) = if signed {
        (&word[..1], &word[1..])
    } else {
        ("", word)
    };
    let digits = digits_area
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(digits_area.len());
    if digits == 0 {
        return None;
    }
    // **指数**属于数字，不属于单位：`calc(1e2px)` → `100px`
    // （`values/calculation/calc/no_operator::number/exponent`），而 `math.$pi * 1e15` 是两个数的乘积
    // （`core_functions/math/variables.hrx::pi`）。把 `1e15` 读成“1 带单位 `e15`”正是让那些行没被折叠的
    // 原因 —— 单位 `e15` 哪里都不存在，它只是数字扫描停下的地方。
    let digits = digits + exponent_len(&digits_area[digits..]);
    let (numeric, unit) = digits_area.split_at(digits);
    // `%` 对 Sass 是单位字符；它不进 run 的任何其他读法，因为 `protected` 仍拒绝求值一个在已知调用之外
    // 携带它的 run（`math.percentage(1%)` 必须走到它的正文去报告单位，而不是被告知它不是数）。
    if !unit.chars().all(|c| c.is_ascii_alphanumeric() || c == '%') {
        return None;
    }
    format!("{sign}{numeric}")
        .parse::<f64>()
        .ok()
        .map(|value| (value, unit))
}

/// 由 `+`/`-` 连接的标识符是 Sass 的**字符串拼接**，而这两个算子打印起来不同：
/// `operators/plus.hrx::syntax/*` 把 `c+d`、`c + d`、`c +d`、`c+ d` 与 `c/**/+/**/d` 都折成 `cd`
/// （`+` 消失），而 `operators/minus.hrx::syntax/*` 把 `(c)-(d)`、`c - d`、`c -(d)` 与 `(c)- d` 都折成
/// `c-d` —— `-` 作为文本存活，因为 Sass 把不带引号字符串上的 `-` 解析为加上它的否定。
///
/// 有意做窄，好让今天能用的一切都不动：**每个**操作数都必须是标识符词素（整括号包装先被剥掉），至少
/// 要出现一个算子，且 run 里的任何数字都会把它送回算术路径 —— 正是这保住了间距律与双项列表
/// `10px -5px`，并让 `1px+2px` 一如既往地不被求值。空白对数字*是*数据、对这个形状不是：语料把全部
/// 四种间距折成同一个结果。
pub(crate) fn concatenation(toks: &[Tok]) -> Option<Expr> {
    let lexemes: Vec<&Tok> = toks
        .iter()
        .filter(|tok| !matches!(tok, Tok::Space(_)))
        .collect();
    let operands = lexemes
        .iter()
        .step_by(2)
        .map(operand_identifier)
        .collect::<Option<Vec<&str>>>()?;
    let operators = lexemes
        .iter()
        .skip(1)
        .step_by(2)
        .map(operator_sign)
        .collect::<Option<Vec<char>>>()?;
    if operands.len() < 2 || operators.len() + 1 != operands.len() {
        return None;
    }
    Some(Expr::Text(
        operands[1..]
            .iter()
            .zip(&operators)
            .fold(operands[0].to_string(), |joined, (operand, op)| {
                joined + if *op == '+' { "" } else { "-" } + operand
            }),
    ))
}

/// 一个非空白词素作为操作数名，若它是一个标识符（可能被括号包着）。命名字生命周期是因为这个 `&str`
/// 出自经双重引用到达的 `Tok` —— 本 crate 已经栽过两次的同一个省略陷阱。
fn operand_identifier<'tok>(tok: &'tok &Tok) -> Option<&'tok str> {
    match tok {
        Tok::Word(word) => identifier(unwrapped(word)),
        _ => None,
    }
}

/// 算子位置上的一个词素：这里只有独立的 `+`/`-` 才算。
fn operator_sign(tok: &&Tok) -> Option<char> {
    match tok {
        Tok::Op(op @ ('+' | '-')) => Some(*op),
        _ => None,
    }
}

/// 一个标识符词素：首字符是 ASCII 字母或 `_`，之后是字母、数字、`_` 与 `-`（`foo-bar` 里的连字符
/// 从不作为算子到达我们，因为 lexer 把它粘住了）。
fn identifier(text: &str) -> Option<&str> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    chars
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        .then_some(text)
}

/// 剥掉每一层整括号包装：`(c)` 与 `((c))` 给出 `c`，而 `(c)(d)` 什么都不给，因为它的第一个 `(` 在结束
/// 前就闭合了。一次 `successors` 折叠，无扫描循环。
fn unwrapped(text: &str) -> &str {
    std::iter::successors(Some(text), |open| whole_wrap(open))
        .last()
        .unwrap_or(text)
}

/// 当整个词素是一个配平的括号组时 `text` 的内部：剥掉最外一对之后，剩下的 run 其深度绝不提前回到零
/// （`()(c)` 是两个组，不是一个）。
fn whole_wrap(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    let balanced = inner.chars().try_fold(0i32, |depth, c| match c {
        '(' => Some(depth + 1),
        ')' => (depth > 0).then_some(depth - 1),
        _ => Some(depth),
    })?;
    (balanced == 0).then_some(inner)
}

/// 由 `+`/`-`/`*`/`/` 连接的操作数：先读操作数（一个数、一次调用、一次 calculation），再把扁平的
/// span 列表折叠两遍 —— 一遍在每个加法件内部处理 `*` 与 `/`，一遍在各件之间。孤立数字也算（它正是
/// `$var: 5px` 绑定所需，也正是 [`crate::exec::State::evaluated`] 稍后在整个值层面过滤掉的东西）。
pub(crate) fn arithmetic(toks: &[Tok], env: &Env) -> Option<Expr> {
    let (spans, operators) = split(toks)?;
    let operands = spans
        .iter()
        .filter_map(|(from, to)| expr::operand_expr(&toks[*from..*to], env))
        .collect::<Vec<Expr>>();
    // 每个 span 都必须产出一个操作数：被丢弃的那个会挪动交替，把错的算子与错的值配成对，
    // `rgb(18 52 none)` 曾因此变成数字 `52`。
    (operands.len() == operators.len() + 1).then(|| tree(operands, &operators))?
}

/// 一条 run 的加法件：每件带把它接到前一件的算子，以及它含有的乘法链。
type Pieces = Vec<(Option<BinOp>, Vec<(Option<BinOp>, Expr)>)>;

/// 一个操作数连同它前面的算子（列表首项为 `None`）。
type Chain = Vec<(Option<BinOp>, Expr)>;

/// 在已读操作数之上的两遍树：先加法件，再每块内部的乘积。全程左结合，所以 `1 - 2 - 3` 是
/// `(1 - 2) - 3`，`2 * 3 + 4` 是 `(2 * 3) + 4` —— 与语料对 `calc( … )` 陈述的次序相同
/// （`operator/precedence::simplified/*`）。
fn tree(operands: Vec<Expr>, operators: &[BinOp]) -> Option<Expr> {
    let paired = std::iter::once(None)
        .chain(operators.iter().copied().map(Some))
        .zip(operands);
    let pieces = paired.fold(Pieces::new(), |mut acc, (op, operand)| {
        match op {
            Some(loose @ (BinOp::Add | BinOp::Sub)) => {
                acc.push((Some(loose), vec![(None, operand)]));
            }
            tight => {
                if acc.is_empty() {
                    acc.push((None, Vec::new()));
                }
                if let Some((_, factors)) = acc.last_mut() {
                    factors.push((tight, operand));
                }
            }
        }
        acc
    });
    products(pieces)
}

/// 把每个加法件折成一个乘积，再把各乘积折成一个和。两者都是对该件自己列表的一次 `fold`，用每个
/// 操作数前面的算子相接。
fn products(pieces: Pieces) -> Option<Expr> {
    let folded = pieces
        .into_iter()
        .map(|(join, factors)| chain(factors).map(|product| (join, product)))
        .collect::<Option<Vec<(Option<BinOp>, Expr)>>>()?;
    folded
        .into_iter()
        .fold(None::<Expr>, |sum, (join, product)| {
            Some(match (sum, join) {
                (None, _) => product,
                (Some(left), Some(op)) => Expr::Binary(Box::new(left), op, Box::new(product)),
                (Some(_), None) => return None,
            })
        })
}

/// 对一条乘法链的同一次折叠：`2 * 3 / 5` 是 `((2 * 3) / 5)`。
fn chain(factors: Chain) -> Option<Expr> {
    factors
        .into_iter()
        .fold(None::<Expr>, |acc, (op, operand)| {
            Some(match (acc, op) {
                (None, _) => operand,
                (Some(left), Some(op)) => Expr::Binary(Box::new(left), op, Box::new(operand)),
                (Some(_), None) => return None,
            })
        })
}
