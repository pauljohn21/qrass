//! **结构相** —— `Tok → Event`，只按分隔符判定，没有树，也没有执行器状态。
//!
//! 这是读花括号的那一相，所以也是能说出块在哪里结束的那一相。因此 `@for` 在**这里**被切出并作为
//! 一个 owned 的 [`Event::For`] 交下去，而不是让 [`crate::exec`] 去发现自己的范围 —— 后者正是反例
//! 索引禁止的形状（一个缓冲加上一个“还欠几个块要关”的计数，住在一个根本不该知道这两件事的相里）。
//! 深度是这个相自己的累加子里的一个数字，与 [`crate::brackets::Run`] 为圆括号携带的是同一形状，
//! 而驱动它的内核是 [`crate::phases`] 里的 `Structurize`。
//!
//! 与 [`crate::lexer`] 一样，这是一个带自己累加子的前向 fold；这里不出现任何可观察类型。

use crate::brackets;
use crate::lexer::{Emit, Tok};

/// 分隔符 fold 产出的一个结构事件。
#[derive(Debug, Clone)]
pub enum Event {
    Open(Vec<Tok>),
    Decl {
        prop: Vec<Tok>,
        value: Vec<Tok>,
    },
    Close,
    /// 一个被这一相切出的 `@for` **块**：按原样写下的头，以及它的 `{` 与自己的 `}` 之间的每个事件。
    /// 嵌套住在值里（body 事件自己可以是一个 `For`），绝不放在一个后续相必须去遍历的 `children`
    /// 字段里 —— 而且决定性地，没有任何后续相必须*数花括号*才能查明循环在哪里结束。读头仍然是
    /// [`crate::loops`] 的工作，所以本编译器不执行的 `@for` 原封不动地经 [`Event::Open`] 回来。
    For {
        head: Vec<Tok>,
        body: Vec<Event>,
    },
}

/// 一个**未被执行的** `@for` 块变成的形状：以它的头作文本打开，喂入它的 body，关闭。被 decline 的头
/// （`exec::State::unroll`）与一个从未关掉花括号的输入结尾（[`MsgAcc::finish`]）都走这同一个构造，
/// 所以“一个未执行的循环就是一个普通块”只陈述一次，而不是分两相各说一遍。
///
/// 这是一个**选型，不是语料的回答**，并照此说出来：头届时被读成一个选择器，这正是
/// `directives/for/for.hrx::unit/same`（要五条 `b: Npx` 规则）与 `…::error/from_float`（要
/// `Error: 1.5 is not an int.`）保持红的原因。本编译器两个都还没答 —— 单位区间是 1.3/P3.2，
/// 消息是 P7。
pub(crate) fn as_block(head: Vec<Tok>, body: Vec<Event>) -> Vec<Event> {
    let mut events = vec![Event::Open(head)];
    events.extend(body);
    events.push(Event::Close);
    events
}

/// 第一遍结构：`Tok → Event`，只按分隔符（没有树）。自带累加子。
#[derive(Default)]
pub struct MsgAcc {
    run: Vec<Tok>,
    /// 还开着几个 `{`。这一相是读花括号的那一相，所以也是能说出块在哪里结束的那一相 —— 而它
    /// **在这里**说出来，作为自己累加子里的一个数字（与 [`crate::brackets::Run`] 为圆括号携带的
    /// 是同一类事实），而不是把它留给后续相。
    depth: u32,
    /// 当前正被切出的 `@for` 块，由外到内。它们在 `depth` 回到其 `{` 所在层级时，按遇见的顺序
    /// 各自作为一个 [`Event::For`] 离开。
    cut: Vec<ForRun>,
}

/// 一个关闭花括号尚未到达的 `@for` 块：它的头与迄今找到的 body 事件。
struct ForRun {
    head: Vec<Tok>,
    body: Vec<Event>,
    /// 在它的 `{` 被吃掉**之前**的深度，所以它自己的 `}` 是第一个把 `depth` 带回这个数字的括号 ——
    /// 内层块的 `}` 回到一个更深的层级，因此不是它
    depth: u32,
}

impl MsgAcc {
    /// 喂入一个词素，向 `emit` 发射 0..N 个结构事件。
    pub fn feed(&mut self, tok: Tok, emit: Emit<'_, Event>) {
        match tok {
            Tok::Punct('{') => {
                let head = std::mem::take(&mut self.run);
                if crate::loops::is_for(&head) {
                    self.cut.push(ForRun {
                        head,
                        body: Vec::new(),
                        depth: self.depth,
                    });
                } else {
                    self.route(Event::Open(head), emit);
                }
                self.depth += 1;
            }
            Tok::Punct(';') => self.push_decl(emit),
            Tok::Punct('}') => {
                self.push_decl(emit);
                // **饱和减，是故意的。** 一个没有开着的花括号的 `}` 是普通输入，不是坏掉的程序：
                // 自定义属性持有原始 CSS 文本，所以 `--prop: };` 给这一相一个它从未打开过的右花括号
                // （`css/custom_properties/error.hrx::brackets/curly`，它自己的回答是
                // `Error: unmatched "}".`）。在那里回绕的计数器会在 debug 构建里 panic，而一个在
                // depth 0 切出的 `@for` 随后会在错误的括号上关闭。
                self.depth = self.depth.saturating_sub(1);
                if self.cut.last().is_some_and(|run| run.depth == self.depth) {
                    if let Some(run) = self.cut.pop() {
                        self.route(
                            Event::For {
                                head: run.head,
                                body: run.body,
                            },
                            emit,
                        );
                    }
                } else {
                    self.route(Event::Close, emit);
                }
            }
            other => self.run.push(other),
        }
    }

    /// 把输入结尾留下的未切之物交下去。在 [`Event::For`] 存在之前，这一相一看到 `{` 就发出一个
    /// [`Event::Open`]，所以在循环体里被截断的程序仍产出它已到达的块；在这里留一份 run 反而会丢掉
    /// 一整个块和它里面的规则 —— 一条缺失的规则在通过计数里不留下任何痕迹。每个未关闭的 run 都经
    /// [`as_block`] 交下去，即被 decline 的头也走的那个构造：不读区间，不执行 body。
    ///
    /// 写成一个构建一条 owned 序列的 fold，由内到外，因为这些 run 是嵌套的：内层的重建必须落在
    /// 外层的 body 之内。
    pub fn finish(&mut self, emit: Emit<'_, Event>) {
        std::mem::take(&mut self.cut)
            .into_iter()
            .rev()
            .fold(Vec::new(), |trailing, run| {
                // 这个 run 里仍开着的每一段都排在它的 `}` 之前。
                let mut body = run.body;
                body.extend(trailing);
                as_block(run.head, body)
            })
            .into_iter()
            .for_each(|ev| emit(ev));
    }

    /// 把一个事件送到它在流上的归属处：送进仍被切出的最内层 `@for`，或者 —— 没有循环打开时 ——
    /// 直接往下游。
    fn route(&mut self, ev: Event, emit: Emit<'_, Event>) {
        match self.cut.last_mut() {
            Some(run) => run.body.push(ev),
            None => emit(ev),
        }
    }

    fn push_decl(&mut self, emit: Emit<'_, Event>) {
        let run = std::mem::take(&mut self.run);
        if let Some(colon) = run.iter().position(|t| matches!(t, Tok::Punct(':'))) {
            let prop = run[..colon].to_vec();
            let value = run[colon + 1..].to_vec();
            // 名字在不在，是一个词素问题，按 crate 其余部分都读的同一条律回答 —— 而不是把 run
            // 渲染回文本再扫它的字符。
            if !brackets::meaningful(&prop).is_empty() {
                self.route(Event::Decl { prop, value }, emit);
            }
        }
    }
}
