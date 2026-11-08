//! 管线相位内核 —— 每个编译相位都是一个**真正的 rxrust 算子**，用框架自己的形状：
//!
//! * `struct Phase<S> { source }` —— 惰性数据，不持订阅级状态，`Clone`；
//! * `PhaseObserver<O> { observer, state }` —— 状态住在这里，随订阅出生；`next` 做相位的活，
//!   用自己的 `observer.next(..)` 转发**每一项**产出；`error`/`complete` 是一次性消费的
//!   透传，`is_closed` 委派给下游，让 source 可以提前停止；
//! * `impl ObservableType` + `impl CoreObservable<C: Context>` —— `subscribe` 用 `C::transform`
//!   包装 context 的内层 observer（保留 scheduler），再把包装过的 context 交给 source。
//!
//! `Err` 保持泛型：相位从不决定链的错误通道，所以同一个内核在 `Local`（`!Send`）或 `Shared`
//! context 上、配任何 `Err` 都成立。

use std::convert::Infallible;

use rxrust::context::Context;
use rxrust::observable::{CoreObservable, ObservableType};
use rxrust::observer::Observer;

use crate::SassError;
use crate::exec::State;
use crate::lexer::{LexAcc, Tok};
use crate::scope::StyleRule;
use crate::structure::{Event, MsgAcc};
use crate::trace::Phase;

// 每个 observer 携带的相位句柄是 `crate::trace::Phase`：一个与订阅同寿的 span，所以
// `elapsed_ms` 覆盖流式传输本身，而不只是链的接线。

/// `char → Tok`：每完成一个词素发射一次，由 [`LexAcc`] 驱动。
#[derive(Clone)]
pub struct Lexer<S> {
    pub source: S,
}

pub struct LexerObserver<O> {
    observer: O,
    acc: LexAcc,
    timing: Phase,
    items: usize,
}

impl<O, Err> Observer<char, Err> for LexerObserver<O>
where
    O: Observer<Tok, Err>,
{
    fn next(&mut self, ch: char) {
        // 解构让累加器、下游 observer、span 与计数各自独立借用，于是 sink 能直接往流里发射，
        // 同时计自己的产出数。
        let LexerObserver {
            observer,
            acc,
            timing,
            items,
        } = self;
        let _current = timing.enter();
        acc.feed(ch, &mut |tok| {
            observer.next(tok);
            *items += 1;
        });
    }

    fn error(self, err: Err) {
        let LexerObserver {
            observer,
            mut timing,
            items,
            ..
        } = self;
        timing.close(items);
        observer.error(err);
    }

    fn complete(self) {
        let LexerObserver {
            observer,
            mut timing,
            items,
            ..
        } = self;
        timing.close(items);
        observer.complete();
    }

    fn is_closed(&self) -> bool {
        self.observer.is_closed()
    }
}

impl<S> ObservableType for Lexer<S>
where
    S: ObservableType,
{
    type Item<'a>
        = Tok
    where
        Self: 'a;
    type Err = S::Err;
}

impl<S, C> CoreObservable<C> for Lexer<S>
where
    C: Context,
    S: CoreObservable<C::With<LexerObserver<C::Inner>>>,
{
    type Unsub = S::Unsub;

    fn subscribe(self, context: C) -> Self::Unsub {
        let Lexer { source } = self;
        let wrapped = context.transform(|observer| LexerObserver {
            observer,
            acc: LexAcc::default(),
            timing: Phase::new("lex", "lexer"),
            items: 0,
        });
        source.subscribe(wrapped)
    }
}

/// `Tok → Event`：按分隔符发出的结构事件，由 [`MsgAcc`] 驱动。
#[derive(Clone)]
pub struct Structurize<S> {
    pub source: S,
}

pub struct StructurizeObserver<O> {
    observer: O,
    acc: MsgAcc,
    timing: Phase,
    items: usize,
}

impl<O, Err> Observer<Tok, Err> for StructurizeObserver<O>
where
    O: Observer<Event, Err>,
{
    fn next(&mut self, tok: Tok) {
        let StructurizeObserver {
            observer,
            acc,
            timing,
            items,
        } = self;
        let _current = timing.enter();
        acc.feed(tok, &mut |event| {
            observer.next(event);
            *items += 1;
        });
    }

    fn error(self, err: Err) {
        let StructurizeObserver {
            observer,
            mut timing,
            items,
            ..
        } = self;
        timing.close(items);
        observer.error(err);
    }

    fn complete(self) {
        let StructurizeObserver {
            observer,
            mut acc,
            mut timing,
            mut items,
            ..
        } = self;
        // 输入从未收束的 `@for` 仍必须把它到达的块送进 executor：本相位曾对每个 `{` 发一个
        // `Open`，所以丢掉一条未收束的 run 会静默丢失没有哪遍计数能看见的 CSS（`MsgAcc::finish`）。
        // 事件通过与 `feed` 相同的 sink 离开，并计入本相位的 items。
        let mut downstream = observer;
        acc.finish(&mut |event| {
            downstream.next(event);
            items += 1;
        });
        timing.close(items);
        downstream.complete();
    }

    fn is_closed(&self) -> bool {
        self.observer.is_closed()
    }
}

impl<S> ObservableType for Structurize<S>
where
    S: ObservableType,
{
    type Item<'a>
        = Event
    where
        Self: 'a;
    type Err = S::Err;
}

impl<S, C> CoreObservable<C> for Structurize<S>
where
    C: Context,
    S: CoreObservable<C::With<StructurizeObserver<C::Inner>>>,
{
    type Unsub = S::Unsub;

    fn subscribe(self, context: C) -> Self::Unsub {
        let Structurize { source } = self;
        let wrapped = context.transform(|observer| StructurizeObserver {
            observer,
            acc: MsgAcc::default(),
            timing: Phase::new("structure", "lexer"),
            items: 0,
        });
        source.subscribe(wrapped)
    }
}

/// `Event → StyleRule`：程序状态（`scopes`、`env`）住在本 observer 里，按事件以 `&mut self` 移动；
/// 每事件 **0..N 条规则**经 `State::step` 拿到的 sink 出去 —— 收束一个 `@for` 每次迭代发一条规则。
/// 致命失败走真正的 `Err` 通道（`Observer::error`）结束流，这正是下游 observer 持为 `Option`
/// 并用 `take()` 消费的原因 —— 恰好终止一次，与框架自己多源 observer 的做法相同。今天是哪类
/// 失败致命，看 crate root 的 `FATAL` 集合。
#[derive(Clone)]
pub struct Exec<S> {
    pub source: S,
}

pub struct ExecObserver<O> {
    observer: Option<O>,
    state: State,
    timing: Phase,
    items: usize,
}

impl<O> Observer<Event, Infallible> for ExecObserver<O>
where
    O: Observer<StyleRule, SassError>,
{
    fn next(&mut self, ev: Event) {
        if self.observer.is_none() {
            return; // 已在 `Err` 通道终止
        }
        // 相位 span 包住整步进入，于是它打开的 declaration span 与它发出的规则都嵌套在里面；
        // guard 在 `close` 取走 timing 之前释放。
        let ExecObserver {
            observer,
            state,
            timing,
            items,
            ..
        } = self;
        let outcome = {
            let _current = timing.enter();
            // 每事件 0..N 条规则：收束一个 `@for` 每次迭代结算一条规则，所以 `step` 写进这个 sink，
            // 而不是返回单条规则（见 `crate::exec::State::step`）。
            let mut emit = |rule: StyleRule| {
                if let Some(downstream) = observer {
                    downstream.next(rule);
                    *items += 1;
                }
            };
            state.step(ev, &mut emit)
        };
        match outcome {
            Ok(()) => {}
            Err(err) => {
                if let Some(terminated) = observer.take() {
                    timing.close(*items);
                    terminated.error(err);
                }
            }
        }
    }

    // 上游不会失败；错误只能源自本相位。
    fn error(self, never: Infallible) {
        match never {}
    }

    fn complete(self) {
        let ExecObserver {
            observer,
            mut timing,
            items,
            ..
        } = self;
        timing.close(items);
        if let Some(observer) = observer {
            observer.complete();
        }
    }

    fn is_closed(&self) -> bool {
        self.observer.as_ref().is_none_or(|o| o.is_closed())
    }
}

impl<S> ObservableType for Exec<S>
where
    S: ObservableType<Err = Infallible>,
{
    type Item<'a>
        = StyleRule
    where
        Self: 'a;
    type Err = SassError;
}

impl<S, C> CoreObservable<C> for Exec<S>
where
    C: Context,
    S: ObservableType<Err = Infallible> + CoreObservable<C::With<ExecObserver<C::Inner>>>,
{
    type Unsub = S::Unsub;

    fn subscribe(self, context: C) -> Self::Unsub {
        let Exec { source } = self;
        let wrapped = context.transform(|observer| ExecObserver {
            observer: Some(observer),
            state: State::default(),
            timing: Phase::new("exec", "exec"),
            items: 0,
        });
        source.subscribe(wrapped)
    }
}

/// `StyleRule → String`：把一条结算好的规则序列化成展开的 CSS 文本，1:1，按规则到达的顺序。
/// 无任务交接、无重排序：链在这里是串行的，所以流自身的顺序**就是**文档顺序。它取代的尾部
/// （每规则 `from_stream` + `collect` + `sort_by_key`）是纯 CPU 工作上的并发表演，而且更糟：
/// `merge_all` 里排队的内层会**丢失**等待期间做出的发射，任何排序都找不回来。
#[derive(Clone)]
pub struct Render<S> {
    pub source: S,
}

pub struct RenderObserver<O> {
    observer: O,
    timing: Phase,
    items: usize,
}

impl<O, Err> Observer<StyleRule, Err> for RenderObserver<O>
where
    O: Observer<String, Err>,
{
    fn next(&mut self, rule: StyleRule) {
        let RenderObserver {
            observer,
            timing,
            items,
        } = self;
        let _current = timing.enter();
        observer.next(rule.to_css());
        *items += 1;
    }

    fn error(self, err: Err) {
        let RenderObserver {
            observer,
            mut timing,
            items,
        } = self;
        timing.close(items);
        observer.error(err);
    }

    fn complete(self) {
        let RenderObserver {
            observer,
            mut timing,
            items,
        } = self;
        timing.close(items);
        observer.complete();
    }

    fn is_closed(&self) -> bool {
        self.observer.is_closed()
    }
}

impl<S> ObservableType for Render<S>
where
    S: ObservableType,
{
    type Item<'a>
        = String
    where
        Self: 'a;
    type Err = S::Err;
}

impl<S, C> CoreObservable<C> for Render<S>
where
    C: Context,
    S: CoreObservable<C::With<RenderObserver<C::Inner>>>,
{
    type Unsub = S::Unsub;

    fn subscribe(self, context: C) -> Self::Unsub {
        let Render { source } = self;
        let wrapped = context.transform(|observer| RenderObserver {
            observer,
            timing: Phase::new("render", "exec"),
            items: 0,
        });
        source.subscribe(wrapped)
    }
}
