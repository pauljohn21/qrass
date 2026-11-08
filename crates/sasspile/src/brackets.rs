//! 把一条计算的词素 run 变成**带深度标签的 run**，让 [`crate::calc`] 里的归约永远不需要树或递归
//! 下降：一遍给每个词素一个 `level` 与拥有它的括号的 `owner` 索引，一个调用与它配平的实参列表成为
//! *一个*词素，而被一个组切成两段的 run 被合并回一条。全程保持文档序，所以后续的 fold 能按最深层
//! 优先走各个层级，看到每个组都已归约。
//!
//! 本文件拥有两个词素层事实，各由一个真实的 bug 换来：**调用是一个操作数**（把 `var(--c)` 切成
//! `var(`、`--c`、`)` 让周围不再有可读的链），以及**组括号不是调用**（把 `var(` 当作开启者吃掉了
//! 它的闭括号，把 `calc((1 + var(--c)))` 变成一条不可读的 run）。

use crate::env::Env;
use crate::expr;
use crate::lexer::{Tok, render};

/// 最外层 run 的拥有者；没有组能与它相撞，因为组的拥有者是词素索引。
pub(crate) const ROOT: usize = usize::MAX;

/// 一个深度上极大的一段文档序 run：`owner` 是创建它的开启括号的索引，最外层那个是 [`ROOT`]。
#[derive(Clone)]
pub(crate) struct Run {
    pub(crate) level: u32,
    pub(crate) owner: usize,
    pub(crate) entries: Vec<Entry>,
}

/// 一个词素，带着它的位置、它的深度，以及拥有它所属 run 的那个 `(`。
#[derive(Clone)]
pub(crate) struct Entry {
    pub(crate) index: usize,
    pub(crate) level: u32,
    pub(crate) owner: usize,
    pub(crate) tok: Tok,
}

/// 对词素的一遍扫描：深度与拥有关系变成数据，括号自身被丢弃（打印时再重新导出），而被一个组切成
/// 两段的 run 被合并回一条，仍是文档序 —— 所以归约按源码写下的顺序看到操作数与算子。
pub(crate) fn partition(flat: &[Tok]) -> Vec<Run> {
    let seeded = (Vec::<Run>::new(), 0u32, vec![ROOT; flat.len() + 2]);
    let (runs, _, _) = flat.iter().enumerate().fold(seeded, |acc, (index, tok)| {
        let (mut runs, level, mut owners) = acc;
        let entry = Entry {
            index,
            level,
            owner: owners[level as usize],
            tok: tok.clone(),
        };
        if opens(tok) {
            // 开启者留在**父** run 里，作为父 run 归约时穿过的占位符：它是把一个组与它自己 run
            // 产出的摘要链接起来的唯一把手。
            push(&mut runs, &entry);
            owners[level as usize + 1] = index;
            runs.push(Run {
                level: level + 1,
                owner: index,
                entries: Vec::new(),
            });
            return (runs, level + 1, owners);
        }
        if level > 0 && closes(tok) {
            return (runs, level - 1, owners);
        }
        push(&mut runs, &entry);
        (runs, level, owners)
    });
    runs.into_iter().fold(Vec::<Run>::new(), |mut acc, run| {
        match acc
            .iter_mut()
            .rev()
            .find(|held| held.owner == run.owner && held.level == run.level)
        {
            Some(held) => held.entries.extend(run.entries),
            None => acc.push(run),
        }
        acc
    })
}

/// 把一个条目追加到拥有它的 run，如果这是它的第一个词素就打开该 run。
fn push(runs: &mut Vec<Run>, entry: &Entry) {
    match runs
        .iter_mut()
        .rev()
        .find(|run| run.owner == entry.owner && run.level == entry.level)
    {
        Some(run) => run.entries.push(Entry {
            index: entry.index,
            level: entry.level,
            owner: entry.owner,
            tok: entry.tok.clone(),
        }),
        None => runs.push(Run {
            level: entry.level,
            owner: entry.owner,
            entries: vec![Entry {
                index: entry.index,
                level: entry.level,
                owner: entry.owner,
                tok: entry.tok.clone(),
            }],
        }),
    }
}

/// 一个开启组的词素：一个独立的 `(`，或一个嵌套计算的 `calc(`，其项拼接进父 run。其他每个调用 ——
/// `var(`、`min(`、`c()` —— 由 [`merge_calls`] 折成一个操作数。
pub(crate) fn opens(tok: &Tok) -> bool {
    match tok {
        Tok::Punct('(') => true,
        Tok::Word(word) => is_calculation_call(word),
        _ => false,
    }
}

/// 一个 `calc(` 词素：一个嵌套计算，其项拼接进父 run，而不是成为自己的操作数。
fn is_calculation_call(word: &str) -> bool {
    word.strip_suffix('(')
        .is_some_and(|name| name.eq_ignore_ascii_case("calc"))
}

/// 名字有已注册签名的调用词素：它们走内建表，而不是走 [`crate::calc`] 的单位代数。
pub(crate) fn builtin_call(word: &str) -> bool {
    word.strip_suffix('(')
        .is_some_and(|name| crate::builtins::signature(name).is_some())
}

/// 一遍扫描，把一个调用与它配平的实参列表折成单个 `Word` 词素，于是 `1 + var(--c)` 是两个操作数
/// 和一个算子。深度作为数据记在 [`Tok::net`] 上，而一个调用在本实参内始终不关闭的 run 保持来的
/// 样子 —— 链随后 decline，源文本成立。一条已完成的 run 是否*恰好是一个值得求值的内建调用*是另一个
/// 问题，由 [`crate::calls::known_call`] 回答，不是这里。
pub(crate) fn merge_calls(run: &[Tok]) -> Vec<Tok> {
    let head = |index: usize| {
        matches!(&run[index], Tok::Word(word)
            if word.ends_with('(') && !is_calculation_call(word) && !builtin_call(word))
    };
    run.iter()
        .enumerate()
        .fold(
            (Vec::<Tok>::new(), 0usize),
            |(mut merged, skip), (index, tok)| {
                if index < skip {
                    return (merged, skip);
                }
                if !head(index) {
                    merged.push(tok.clone());
                    return (merged, skip);
                }
                let closed = run[index..]
                    .iter()
                    .scan(0i32, |depth, inner| {
                        *depth += inner.net();
                        Some(*depth)
                    })
                    .position(|depth| depth == 0);
                match closed {
                    Some(offset) => {
                        let end = index + offset;
                        merged.push(Tok::Word(crate::lexer::render(&run[index..=end])));
                        (merged, end + 1)
                    }
                    None => {
                        merged.push(tok.clone());
                        (merged, skip)
                    }
                }
            },
        )
        .0
}

fn closes(tok: &Tok) -> bool {
    // 只认独立的 `)`：`calculation::cut_open` 把括号切成各自的词素，而 [`merge_calls`] 已把每个
    // 调用（以 `)` 结尾）折成单个操作数，所以这里以 `)` 结尾的 Word 是一个值，不是闭括号。
    matches!(tok, Tok::Punct(')'))
}

/// 这个词素是否不携带意义：布局，以及一个被丢弃的 silent comment。
pub(crate) fn is_layout(tok: &Tok) -> bool {
    matches!(tok, Tok::Space(_) | Tok::Silent(_))
}

/// 这个索引处的 `+`/`-` 两侧是否都有空白，在它自己的 run 内。
pub(crate) fn spaced(entries: &[Entry], index: usize) -> bool {
    let layout = |side: usize| side < entries.len() && is_layout(&entries[side].tok);
    index > 0 && layout(index - 1) && layout(index + 1)
}

/// 去掉首尾布局的 run —— 操作数*之间*的间距是 `+`/`-` 律读取的证据，所以只去边缘。
pub(crate) fn trim_layout(run: &[Tok]) -> Vec<Tok> {
    let first = run.iter().position(|tok| !is_layout(tok));
    let last = run.iter().rposition(|tok| !is_layout(tok));
    match (first, last) {
        (Some(from), Some(to)) => run[from..=to].to_vec(),
        _ => Vec::new(),
    }
}

/// 调用即计算的名字，按**裸名**大小写不敏感地匹配。有厂商前缀就不算：`-a-calc(` 是单个词素，
/// `css/functions/special/prefixed/lowercase::calc/script_like` 把 `-a-calc($c)` 保留为文本，
/// 所以 `expr::special_call` 做的前缀剥离不可复用在这里。
///
/// `min`/`max`/`clamp` 是 1.4a 的“不猜”三人组。四个反三角名字凭它们自己的 `simplification` 行加入，
/// 那些行精确陈述壳律 —— 保留被调者的拼写、折掉实参里可算的部分、把不可读的部分留在里面
/// （`values/calculation/{acos,asin,atan,atan2}.hrx::simplification`：`acos(3px - 1px + var(--c))` →
/// `acos(2px + var(--c))`、`atan2(3px - 1px + var(--c), -7px / 4em * 1em)` →
/// `atan2(2px + var(--c), -1.75px)`）。
///
/// 那四个文件里其余的行不被本清单认领，理由是逐族可查的：它们剩下的行**每个实参传一个字面量**
/// （`atan2(1cm, -10mm)` 在 `units/compatible`、`atan(infinity)` 在 `infinity`、`acos(-1)` 在
/// `negative_one`），所以没有实参内部算术给壳去折，文本按读到的原样回来。那些行因另一个原因失败 ——
/// 它们要的是函数的**值**（正是那三行上的 `135deg`、`90deg`、`180deg`），那需要内建注册、大小写
/// 不敏感的名字查找与用户函数优先级（`tasks.md` 1.7b 的前置 2）。
/// 行族在四个文件之间并不共享，所以不把它们合并列出：`acos`/`asin` 有
/// `one`/`zero`/`negative_one`/`greater_than_one`/`less_than_negative_one`，`atan` 有
/// `one`/`zero`/`infinity`/`negative_infinity`，`atan2` 有 `units/*` 族；它的拒绝前缀还是复数
/// `error/units/*`，另外三个用 `error/unit/*`。
///
/// 另有九个数字名凭同一证据加入，各占一条 `simplification` 行（`sqrt`、`exp`、`log`、`pow`、
/// `sin`、`cos`、`tan`、`hypot`、`rem` —— `values/calculation/<name>.hrx::simplification`）。
/// 其中两行标出折叠停在哪，各自出于不同的原因：`hypot(1px + 2px - var(--c), -7px + 4em)`
/// 把 `-7px + 4em` **未折地**印出，因为 `em` 是上下文相对的 —— px 与 em 都是长度，但 `em`
/// 被故意排除在 `unit::conversion` 之外，于是 `math.compatible(1px, 4em)` 为假
/// （`core_functions/math/comparable.hrx::unit/to_different`，这条律 `crate::unit` 已经拥有）——
/// 而 `log(3px - 1px + var(--c), var(--e))` 保留 `var(--e)`，因为那个实参根本读不成值。
///
/// `rem` 属于这里只因为 `calculation::one_argument_value` 说多实参计算不认领值：有那道守卫，
/// `values/calculation/rem::negative_zero` 让 `math.div(1, rem(-7, 7))` 保持所写（在 run
/// `number-shells` 里测得）；没有它同一行会印出捏造的 `-0.1428571429`。
///
/// `abs` 与 `sign` 被试过、测过、出了。`ledger diff shell-arity abs-sign-rem` 读 `regress=0 net=5`
/// —— 五条是 `abs::simplification`、`sign::simplification`、`rem::simplification`、`sign::zero`、
/// `sign::negative_zero` —— 但 `ledger changed` 显示两行的新值**与金样矛盾**：`abs::preserves_units`
/// 从逐字的 `abs(-7px / 4em) * 1em` 变成 `-1.75px`，而该行写 `1.75px`；`sign::preserves_units`
/// 到 `-1.75px`，而它写 `-1px`。成因是结构性的，不针对具体名字：**单实参**壳认领其实参的值，
/// 那是 `values/calculation/min.hrx::simplified/only` 要求的同一（`min(1px)` → `1px`），但对一个
/// 自己的行把函数作用于该值的名字是错的。`abs`/`sign` 等 1.7b 的前置 2（注册函数体、用户函数
/// 优先级），届时它们会计算而不是回显。
const CALCULATION_NAMES: [&str; 17] = [
    "calc", "min", "max", "clamp", "acos", "asin", "atan", "atan2", "sqrt", "exp", "log", "pow",
    "sin", "cos", "tan", "hypot", "rem",
];

/// 一个计算调用。`name` 是**按写下**的被调者，`lower` 是它的大小写折叠形，而律只读后者：1.4c
/// 禁止把大小写规范化变成一条输出规则，所以一个被保留的计算按源码拼出的原样印出名字。
#[derive(Clone)]
pub(crate) struct Shell {
    pub(crate) name: String,
    pub(crate) lower: String,
    pub(crate) inner: Vec<Tok>,
}

/// 本模块不能作为值读出的一段实参的文本：引用被**解析**，因为 `calc` 实参仍被当作 SassScript 解析
/// （`::variable/number/simple_unit`，以及 0′.3b-2 的教训：special 调用的实参解析变量而 `+`/`-`
/// 保持非算术语义）。一个不含任何动态之物的 run 与一个引用未绑定的 run 都回退到 [`text`] ——
/// `calc( … )` 里一个未绑定的 `$x` 保留自己的文本，永不成为致命错误
/// （`::variable/not_parsed_as_interpolation/*`）。
pub(crate) fn resolved(toks: &[Tok], env: &Env) -> String {
    let dynamic = meaningful(toks)
        .into_iter()
        .any(|tok| matches!(tok, Tok::Var(_) | Tok::MemberVar(..) | Tok::Interp(_)));
    let kept = dynamic
        .then(|| expr::interpolated(toks, env))
        .flatten()
        .and_then(|built| match built.eval(env) {
            Ok(value) => Some(value.to_css()),
            // 这里的失败意为“这个实参还读不出来”，不是“程序错了”：`calc( … )` 里一个未绑定的
            // 引用是定义上的透传，所以它停在 decline，调用方保留词素文本。哪些失败会终止留在
            // `exec::evaluated`。
            Err(_) => None,
        });
    match kept {
        Some(css) => css,
        None => text(toks),
    }
}

/// 一条作为计算调用的 run，如果它是一整个：一个结束在 `(` 的词素，其文本是一个计算名字，以及一个
/// 在把括号平衡带回零的 `)` 处关闭的实参列表。深度在**整个** run 上数，所以嵌套调用保留自己的闭
/// 括号 —— 只从被调者开始数会把 `var` 的 `)` 交给 `calc`，把 `calc(var(--c))` 变成 `calc(--c)`。
///
/// 两侧只允许布局：前导的 `1px + ` 是本切片不读的运算，尾部词素意味着这条 run 不是一个调用。
pub(crate) fn find(toks: &[Tok]) -> Option<Shell> {
    let cut = open_at(toks);
    let callee = cut.iter().enumerate().find_map(|(offset, tok)| match tok {
        Tok::Word(word) => word
            .strip_suffix('(')
            .filter(|name| name_exists(name))
            .map(|name| (offset, name.to_string(), name.to_lowercase())),
        _ => None,
    })?;
    let closing = close_index(&cut[callee.0..])?;
    let position = callee.0 + closing;
    let layout = |run: &[Tok]| {
        run.iter()
            .all(|tok| matches!(tok, Tok::Space(_) | Tok::Silent(_)))
    };
    layout(&cut[..callee.0])
        .then(|| Shell {
            name: callee.1,
            lower: callee.2,
            inner: cut[callee.0 + 1..position].to_vec(),
        })
        .filter(|_| layout(&cut[position + 1..]))
}

/// 关闭从本 run 前端开始的调用的 `Punct(')')` 的偏移，若它永不关闭则为 `None`。一个 `…(` 词素
/// 开一层，一个既开又闭的词（`url(x)`）净零，且只有被调者之后的位置才能关闭它。
pub(crate) fn close_index(run: &[Tok]) -> Option<usize> {
    run.iter()
        .enumerate()
        .skip(1)
        .scan(
            run.first().map_or(0, Tok::net),
            |depth, (offset, lexeme)| {
                *depth += lexeme.net();
                Some((*depth, offset))
            },
        )
        .find(|(depth, _)| *depth <= 0)
        .map(|(_, offset)| offset)
}

/// 本 run 已打开而未关闭的圆括号数：在它上面折叠 [`Tok::net`]，所以组内的分隔符永不会被读成顶层。
/// 折叠逐词素的平衡正是让这个读数在同一 run 上与 [`close_index`] 一致的东西；两种读法在一个关闭了
/// 自己从未打开的括号的词素上不同（`Word("3px)")` —— 这里 `-1`，而“以 `(` 结尾”的计数给 `0`），
/// 且没有语料行区分它们，所以与本模块自己的括号平衡一致就是选择的理由。
pub(crate) fn depth_of(toks: &[Tok]) -> i32 {
    toks.iter().fold(0, |depth, tok| depth + tok.net())
}

/// 重新切分一条 run，让每个括号拥有自己的位置；非词词素原样携带。
pub(crate) fn open_at(toks: &[Tok]) -> Vec<Tok> {
    toks.iter()
        .flat_map(|tok| match tok {
            Tok::Word(word) => cut_open(word),
            other => vec![other.clone()],
        })
        .collect()
}

/// 一个词在每个 `(` 与每个 `)` 处切开 —— trace 展示的三种形状：
/// * `Word("calc(1px)")` → `calc(`、`1px`、`)` —— 一条普通声明值，一个词素同时握着两个括号。
///   `(` 保持粘在它**前面**的文本上，因为以 `(` 结尾正是标记被调者的东西（把它粘到后面那块会
///   产生 `Word("calc")`，什么都不匹配，每个 case 都保持逐字）；
/// * `Word("calc(1px")` → `calc(`、`1px` —— 一条闭 `)` 在它之外的 run；
/// * `Word("calc( ( 1 ) )")` → `calc(`、`(`、`1`、`)`、`)` —— 一个周围有布局的括号被释放为
///   裸 `Punct('(')`，因为那些空白是布局，而这个组不是被调者。
///
/// 括号之间的文本保留其内部空格：Sass 的间距律正是让 `1px + 1%` 成为运算的东西，丢掉它们会把
/// 每个计算的内容变成 `1px+1%`。
fn cut_open(word: &str) -> Vec<Tok> {
    let (pieces, tail) = word.chars().fold(
        (Vec::<Tok>::new(), String::new()),
        |(mut pieces, text), c| match c {
            '(' => {
                let callee = !text.is_empty() && text.trim() == text;
                if callee {
                    pieces.push(Tok::Word(format!("{text}(")));
                } else {
                    let body = text.trim();
                    pieces.extend((!body.is_empty()).then(|| Tok::Word(body.to_string())));
                    pieces.push(Tok::Punct('('));
                }
                (pieces, String::new())
            }
            ')' => {
                let body = text.trim();
                pieces.extend((!body.is_empty()).then(|| Tok::Word(body.to_string())));
                pieces.push(Tok::Punct(')'));
                (pieces, String::new())
            }
            other => {
                let mut text = text;
                text.push(other);
                (pieces, text)
            }
        },
    );
    let body = tail.trim();
    pieces
        .into_iter()
        .chain((!body.is_empty()).then(|| Tok::Word(body.to_string())))
        .collect()
}

/// 一个实参列表的规范化文本：折叠空白 run，且只移除计算永不打印的那些空格 —— `(` **之后**
/// 一个、`)` 或 `,` **之前**一个（`calc( ( 1 ) )` → `(1)`、`min( 1% , 2px )` → `min(1%, 2px)`，
/// 而 `1px + (2px)` 保留它的空格）。
///
/// 结构已由词素一遍决定；这里只规范化空白。粘词内部（`Word("1px + 1%")`）的空格是语料期望
/// 的间距的唯一记录，丢掉它们会是 bug 而不是捷径。
pub(crate) fn text(toks: &[Tok]) -> String {
    render(toks)
        .chars()
        .fold((String::new(), false), |(mut out, pending), c| {
            if c == ' ' {
                let had_content = !out.is_empty();
                return (out, had_content);
            }
            let suppress = matches!(c, ')' | ',') || out.ends_with('(');
            if pending && !suppress {
                out.push(' ');
            }
            out.push(c);
            (out, false)
        })
        .0
}

/// 组的词素，**前导符号合并进它粘着的数字**：`Op('+')` 然后 `Word("1px")`（它们之间没有布局）
/// 变成 `Word("+1px")`，即 `number_lexeme` 已读出的形状。被空白隔开的符号、或任何第二个算子，
/// 都保持原样，让运算保持不可读 —— 那是 1.4d 的问题，不是本切片的。
pub(crate) fn without_leading_sign(content: &[Tok]) -> Vec<Tok> {
    match content {
        [Tok::Op(sign), Tok::Word(number), rest @ ..] if matches!(sign, '+' | '-') => {
            let mut merged = vec![Tok::Word(format!("{sign}{number}"))];
            merged.extend(rest.iter().cloned());
            merged
        }
        _ => content.to_vec(),
    }
}

/// 每个不是布局的词素 —— 结构据以读出的视图。
pub(crate) fn meaningful(toks: &[Tok]) -> Vec<&Tok> {
    toks.iter().filter(|tok| !is_layout(tok)).collect()
}

fn name_exists(name: &str) -> bool {
    CALCULATION_NAMES
        .iter()
        .any(|call| name.eq_ignore_ascii_case(call))
}

pub(crate) fn is_calc(name: &str) -> bool {
    name == "calc"
}
