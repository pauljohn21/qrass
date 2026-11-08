//! State —— 运行中的程序状态，由 `Exec` 相内核的 **observer**（`crate::phases`）持有。
//!
//! 这是框架自己为有状态前向计算采用的形状：状态住在 `ExecObserver` 里，每个事件经 `&mut self` 移动
//! 它，而流只携带这一相真正产出的东西（落定的规则）。此前的形式 —— `scan(seed, step)` —— 让累加子
//! **成为**被发射的项，于是每个事件都克隆整个 `State`（开着的块、env、rules），`filter_map` 随后
//! 把除规则以外的一切丢掉：分解正确，载体错误。也没有任何东西搭在声明旁边偷渡（没有逐声明的
//! `Arc<Env>` 快照），所以声明仍在它出现的位置求值 —— Sass 的文档序语义。
//!
//! 嵌套深度是这个值内部的数据（`scopes`），绝不是调用栈。唯一的递归是 `step` 在一个
//! [`crate::structure::MsgAcc`] 从花括号里切出的 `@for` body 上重入自身：它深到源码嵌套循环的
//! 深度，而绝不深到循环的计数。这里不出现任何可观察类型，因为 `compile`（crate 根）拥有唯一的
//! 算子链。

use crate::ErrorKind;
use crate::brackets;
use crate::env::Env;
use crate::expr::{self, Expr};
use crate::lexer::{Tok, render, render_raw, token_text, without_flags};
use crate::loops::{ForLoop, for_loop};
use crate::scope::{Scope, StyleRule};
use crate::selector::{Piece, PropName, Selector};
use crate::structure::{Event, as_block};
use crate::trace;
use crate::unit::Unit;
use crate::value::Value;

use crate::SassError;

/// `Exec` 内核随事件流改动的程序状态：开着的块与词法环境。
#[derive(Debug, Clone, Default)]
pub struct State {
    scopes: Vec<Scope>,
    env: Env,
}

impl State {
    /// 把一个事件移入状态，发射它落定的一切规则 —— 零条、一条、或每轮循环迭代一条。
    ///
    /// **块的文本何时落定**，正是让输出保持文档序而无需排序键、无需重排的东西。按
    /// `sass-spec/css/style_rule` 的 `declaration/interleaved/**`：
    /// - 打开一个嵌套块时，先落定外层块**待决的声明 run**（`.a {b:c; .d{e:f}}` → `.a{b:c}` 在
    ///   `.a .d{e:f}` 之前）；
    /// - 关闭一个块时，落定它仍持有的东西（`.a {.b{c:d}; e:f}` → `.a{e:f}` 在 `.a .b{c:d}` **之后**）；
    /// - 于是一个被孩子切开的块变成几条规则（`b:c`，然后 `g:h`）。
    ///
    /// 错误以 `Err` 到达调用方，好让内核把它们放上流的真 `Err` 通道；某一类错误是致命的还是留作
    /// 逐字兜底，由 `SassError::is_fatal` 决定（见 crate 根的 `FATAL` 集与 P0′.3b）。
    ///
    /// **0..N 条/事件**正是这里收下一个 emit 汇口而不只返回一条规则的原因：`@for` 的 body 持有块时，
    /// 每迭代落定一条规则（`directives/for/for.hrx::to_scope` 要求一个事件产出四条 `a { … }` 规则）。
    /// 这个汇口与 `Lexer`/`Structurize` 两相用的是同一载体（[`crate::lexer::Emit`]，以
    /// `observer.next(..)` 驱动）。
    ///
    /// 这一相不再做的事是*查明循环在哪里结束* —— [`crate::structure::MsgAcc`] 拥有花括号并把 body 作为
    /// 一个 owned [`Event::For`] 交上来，所以这里没有缓冲、没有“还欠几个块要关”的计数、也没有循环
    /// 形状的 scope。
    ///
    /// 它**仍**在做的事被说出来，而不是被藏起来：把一个 `For` 展开成对 body 的 N 遍，在这里写成对
    /// 生成值的一个迭代加一次重灌（`unroll` → `pass` → `step`），每遍一次 `Vec` 克隆。design.md 第
    /// 88 行要求 `内核内 flat_map(一项 → body 子流)`，而**那个动词在 rc.6 里存在**：`flat_map` 是
    /// `Map` + `MergeAll`，内层 observable 自带 `Context`（`docs/rxrust-source-analysis/05-operators.md`，
    /// 其中也警告 `merge_all` 在一个并发槽之上会丢弃排队内层的发射 —— 所以保序要用 `concat_map`）。
    /// 因此这是一个**未做的选型，不是框架限制**：采用它意味着 `State` 不再是这个 observer 的字段，
    /// 而变成流携带的累加子 —— 长期挂着的 0′.5（`C::RcMut`/`C::RcCell`），或 `scan` 的 owned 值。那是
    /// 一次相形状层面的决策，按实情上报在 `tasks.md` 9.8，而不是悄悄留着或粉饰成限制。
    pub fn step(
        &mut self,
        ev: Event,
        emit: crate::lexer::Emit<'_, StyleRule>,
    ) -> Result<(), SassError> {
        match ev {
            Event::Open(head) => {
                let parent = self
                    .scopes
                    .last()
                    .map(|s| s.selector.clone())
                    .unwrap_or_default();
                let child = self.head_selector(&head, &parent.to_css())?;
                let text = child.to_css();
                // 嵌套块继承包围它的块的上下文：它仍在那个 body 之内。
                let plain_css = self.in_plain_css() || plain_css_body(&text);
                let inert = self.in_inert() || (!plain_css && callable_declaration(&head));
                // 可调用声明不是 CSS 块：打开它不得冲刷外层规则待决的声明。`@for` 也不是，而且它根本
                // 不再走到这个分支 —— 分界相把它作为 `Event::For` 交上来，在下面处理。
                if !inert {
                    self.settle_top().map(emit);
                }
                self.scopes.push(Scope {
                    selector: child.nest(&parent),
                    decls: Vec::new(),
                    plain_css,
                    inert,
                });
                Ok(())
            }
            Event::Decl { prop, value } => {
                // 可调用声明的 body 之内，事件是 inert 的：求值它会把可调用自己的参数（`$v`）对着
                // 一个只有调用才提供的环境解析，而绑定它会把一个参数泄进 mixin 被声明的那个作用域。
                if self.in_inert() {
                    return Ok(());
                }
                self.declare(&prop, &value)
            }
            Event::Close => {
                Self::settled(self.scopes.pop()).map(emit);
                Ok(())
            }
            Event::For { head, body } => self.unroll(head, body, emit),
        }
    }

    /// 一个 `@for` 进去，每个生成的值出来一遍对 body 的遍历。
    ///
    /// 范围在上游决定：[`crate::structure::MsgAcc`] 在花括号处把 body 切出并作为一个 owned 值送来，
    /// 所以这里只做绑定与重灌。每一遍都走源码自己的事件所走的**同一条** [`State::step`]，正因如此
    /// 一个由声明组成的 body 落进循环所坐的块的那个待决 run（`directives/for/for.hrx::inclusive_forward/scss`
    /// → 一条 `a` 规则装着 `b: 1 … b: 5`），而一个持有块的 body 每迭代落定一条规则（`…::to_scope` →
    /// 四条 `a { limit: 2; i: <n> }` 规则，文档序）。嵌套的 `@for` 只是 body 里的一个 `For` 事件，
    /// 所以递归跟随源码的嵌套，而绝不跟随迭代计数。
    ///
    /// 一个读取器 decline 的头 —— 一个它不读的边界、一个必须保持 inert CSS 的 body —— 取普通块的
    /// 形状（`structure::as_block`）：以它的头作为块的文本打开、喂入、关闭。它的任何部分都不丢弃，
    /// 而这个选择**不是**语料的回答：头被读成选择器，正是 `directives/for/for.hrx::unit/same` 与
    /// `…::error/from_float` 保持红的原因（1.3/P3.2 与 P7 拥有那些答案）。
    ///
    /// 一个后果被说出来而不是藏起来：body 里做出的绑定写进 [`Env`] 唯一的那一层扁平结构，所以循环
    /// 结束后它仍然可见。没有任何读数从任何一侧衡量它：scoping fixture（`sass-spec/spec/non_conformant/
    /// scope/for.hrx`、`spec/libsass/variable-scoping/`）都坐在 `tests/hrx.rs` 按策略排除的目录里，所以
    /// 它们不在 11,533 行之中，在这里不构成任何读数 —— 正因如此这是**本编译器当前的形状，不是 Sass
    /// 律**。块作用域是任务 3.6。
    fn unroll(
        &mut self,
        head: Vec<Tok>,
        body: Vec<Event>,
        emit: crate::lexer::Emit<'_, StyleRule>,
    ) -> Result<(), SassError> {
        let walk = (!self.in_inert() && !self.in_plain_css())
            .then(|| for_loop(&head, &self.env))
            .transpose()?
            .flatten();
        let Some(ForLoop { var, walk }) = walk else {
            return as_block(head, body)
                .into_iter()
                .try_for_each(|ev| self.step(ev, emit));
        };
        // 循环变量每个值绑定一次，body 走同一个 step；这个绑定是流动环境里的一次声明，而不是压上
        // 遍历的一个帧。
        walk.values().try_for_each(|value| {
            self.env
                .bind(&var, Value::Number(value as f64, Unit::unitless()));
            self.pass(body.clone(), emit)
        })
    }

    /// 循环 body 的一遍：它的事件按源码写下的顺序移入 `step`，它们落定的每条规则一旦决定就经汇口
    /// 出去。这是内核自己的 0..N 发射，[`crate::lexer::LexAcc::feed`] 在一个字符变成多个词素时用的
    /// 也是它 —— 不是被收集成 `Vec` 规则留待下游展平。
    fn pass(
        &mut self,
        body: Vec<Event>,
        emit: crate::lexer::Emit<'_, StyleRule>,
    ) -> Result<(), SassError> {
        body.into_iter().try_for_each(|ev| self.step(ev, emit))
    }

    /// 声明就地在它出现的位置：绑定更新流动的环境，属性加入最内层块的待决声明 run。
    fn declare(&mut self, prop: &[Tok], value: &[Tok]) -> Result<(), SassError> {
        // `$x: …` 的左侧是**绑定目标**，由变量自己命名；普通属性名是一个结构化的 [`PropName`]。
        // 没有让后续 `strip_prefix('$')` 必须撤销的 `"${var}"` 往返。
        let target = match prop.iter().find(|tok| !skipped_in_name(tok)) {
            Some(Tok::Var(var)) => Target::Variable(var.clone()),
            // `math.$pi: 0;` 不绑定任何东西：内建模块的常数是只读的，而 oracle 按名这样说
            // （`core_functions/math/variables.hrx::error/assignment/*`）。测试问的是模块表而不是匹配
            // 命名空间文本，所以 `upstream.$a:` —— 本编译器没有的成员 —— 保持旧的读数，而不是得到
            // 一条没人陈述过的错误。
            Some(Tok::MemberVar(module, name))
                if crate::builtins::module_value(module, name).is_some() =>
            {
                Target::ReadOnly
            }
            _ => Target::Property(PropName::from_pieces(&self.head_pieces(prop, "")?)),
        };
        match target {
            // 绑定更新流动状态内部的环境；后一条声明读到这个位置上有的值，所以文档序成立，无需携带
            // 任何快照。
            Target::Variable(var) => {
                // `!default` / `!global` 描述绑定，不是它的值，且永不进入 CSS（`lexer::without_flags`）；
                // 声明的 `!important` 保留它的文本。
                let bound = self.value_of(&var, without_flags(value))?;
                self.env.bind(&var, bound);
            }
            Target::ReadOnly => {
                return Err(SassError::typed(
                    ErrorKind::ReadOnlyVariable,
                    "Cannot modify built-in variable.",
                ));
            }
            Target::Property(name) => {
                let owned = self.decl_value(&name, value, self.in_plain_css())?;
                if let Some(top) = self.scopes.last_mut() {
                    top.decls.push((name, owned));
                }
            }
        }
        Ok(())
    }

    /// 最内层开着的块是不是 plain-CSS at-rule 的 body？
    fn in_plain_css(&self) -> bool {
        self.scopes.last().is_some_and(|scope| scope.plain_css)
    }

    /// 最内层开着的块是不是坐在可调用声明的 body 里？
    fn in_inert(&self) -> bool {
        self.scopes.last().is_some_and(|scope| scope.inert)
    }

    /// 最内层开着块的待决声明 run，**不**弹出它：块为后来的声明与孩子保持打开，但它迄今为止的
    /// 文本立刻成为一条规则。
    fn settle_top(&mut self) -> Option<StyleRule> {
        let top = self.scopes.last_mut()?;
        if top.decls.is_empty() {
            return None;
        }
        Self::settled(Some(Scope {
            selector: top.selector.clone(),
            decls: std::mem::take(&mut top.decls),
            plain_css: top.plain_css,
            inert: top.inert,
        }))
    }

    /// 把一个结束或已冲刷的 scope 变成规则；没有声明的 run，或根上的（空选择器），落定为空。
    fn settled(scope: Option<Scope>) -> Option<StyleRule> {
        scope
            .filter(|s| !s.decls.is_empty() && !s.selector.is_empty())
            .map(|s| StyleRule {
                selector: s.selector,
                decls: s.decls,
            })
    }

    /// 声明的 owned 值。自定义属性（[`PropName::is_custom`]，由名字的 pieces 一次性决定）与携带 CSS
    /// 原生函数（`var()`、`calc()`、`url()`、…）的值都持有 Sass 绝不可解释的**原始 CSS 文本**；前者
    /// 逐字存下，后者在 [`State::evaluated`] 内部 bail。任何其他值当 —— 且仅当 —— run 无歧义时变成
    /// 带类型的 [`Value`]；解析不出的 run，或失败并不致命的 run，保留它的逐字文本。`plain` 是外围
    /// 的 plain-CSS at-rule body，它把未绑定的 `$x` 保留为文本。
    fn decl_value(&self, prop: &PropName, value: &[Tok], plain: bool) -> Result<Value, SassError> {
        if prop.is_custom() {
            // 自定义属性的值按定义就是原始 CSS，所以没有可追踪的读取。
            return Ok(Value::from_text(render_raw(value)));
        }
        // 每次值读取一个 span：源码说了什么、它变成了什么（或它 decline 了）、读取花了多久。这是
        // 一致性轮次对一行逐字输出的东西总要问的问题，而回答它曾经意味着加一个一次性探针并重建。
        let read = trace::ValueRead::begin(prop.as_str(), &self.scope_id(), &render(value));
        let outcome = self.evaluated(value, plain);
        read.record(&outcome);
        Ok(outcome?.unwrap_or_else(|| Value::from_text(render(value))))
    }

    /// 绑定的值：按定义是 SassScript，绝不是 plain-CSS 文本。`var` 命名目标，因为词素 run 只携带值，
    /// 而一条说 `module: "1px"` 的诊断不是诊断。
    fn value_of(&self, var: &str, toks: &[Tok]) -> Result<Value, SassError> {
        let read = trace::ValueRead::begin(var, &self.scope_id(), &render(toks));
        let outcome = self.evaluated(toks, false);
        read.record(&outcome);
        Ok(outcome?.unwrap_or_else(|| Value::from_text(render(toks))))
    }

    /// 最内层开着的块，作为诊断按它称呼的文本。根层没有 scope，空字符串诚实地说出这一点，而不是
    /// 发明一个选择器。
    fn scope_id(&self) -> String {
        self.scopes
            .last()
            .map_or_else(String::new, |scope| scope.selector.to_css())
    }

    /// 把一条词素 run 折叠成一个带类型的值。孤零零的数字**还**不是表达式 —— 只有运算（或插值）才
    /// 赢得求值，所以 `w: 10px` 保留它精确的源码数字，而不是被从 float 归一化成 `10px`。
    ///
    /// `Ok(None)` 意为“保持逐字”：run 没解析成 Sass 表达式，或它在 plain CSS 内部失败 ——
    /// **special** 调用的实参（`expr::special_call`）或 **plain-CSS at-rule body** 里的声明（`plain`），
    /// 其中未绑定的 `$x` 只是文本（`css/functions/special/prefixed/lowercase::calc/script_like`、
    /// `css/function.hrx::lowercase/result/sass_script`）。其他任何地方，致命的失败走真正的 `Err`
    /// 通道 —— 判据按形状与按类别显式给出，绝不是既吃掉真错误的一张 `.ok()` 大网。
    ///
    /// 一条**传具名实参**的 run 被整个排除：它的 `$name:` 是实参标签，所以求值它根本就会解析一个
    /// 不存在的引用（`expr::named_argument`；实测为给一条无效 `Undefined variable` 的 1,611 例）。
    fn evaluated(&self, toks: &[Tok], plain: bool) -> Result<Option<Value>, SassError> {
        let arithmetic = expr::from_tokens(toks, &self.env).and_then(|expr| match expr {
            Expr::Num(_, _) => None,
            other => Some(other),
        });
        let Some(expr) = arithmetic.or_else(|| expr::interpolated(toks, &self.env)) else {
            return Ok(None);
        };
        // 一条**传具名实参**的 run 只有解析成我们有签名的调用时才被求值。它的 `$name:` 是实参标签，
        // 所以求值其他任何东西都会解析一个不存在的引用（`expr::named_argument`；实测为给一条无效
        // `Undefined variable` 的 1,611 例）。因此 `color.hwb($hue: 180)` 这样的未知被调者保持其文本。
        if expr::named_argument(toks) && !matches!(expr, Expr::Call { .. }) {
            return Ok(None);
        }
        match expr.eval(&self.env) {
            // 携带**复合单位**的数字是 calculation 专用的（`Unit::is_plain`），所以 run 保留它的文本，
            // 而不是裸打 `2in/s`。语料从不单独展示一个：`math.div(6in, 3s)` 只以
            // `math.div(6in, 3s) * 1s` → `2in` 出现（`core_functions/math/div.hrx::unit/incompatible`），
            // 乘积约掉了单位；唯一把这次拒绝写明出来的文件（`libsass-closed-issues/issue_783.hrx`）
            // 被注释掉且在基线之外排除，所以这里不为它发明任何错误消息 —— 那属于 P7。
            Ok(value) if !value.is_plain() => Ok(None),
            Ok(value) => Ok(Some(value)),
            Err(err) if err.is_fatal() && !(plain || expr::special_call(toks)) => Err(err),
            // 一个不命名签名任何参数的 `$label:` 是拒绝回答，不是重渲染的对象：那些标签命名参数而不
            // 是引用变量，所以 run 保留自己的文本（`color.mix(#a, #b, $method: oklab)` 绝不能拿一个
            // legacy mix 回答）。
            Err(err) if err.kind == ErrorKind::UnknownParameter => Ok(None),
            // 一次内建 **decline** 的调用按未知被调者文本的渲染方式渲染 —— 经 [`expr::interpolated`]，
            // 它保持词素 run 逐字、只解析其中的变量读取。那正是语料要求的形状，且它的每一半都被实测：
            // 用求值后的实参重建调用丢了 40 行（`rgb(18 52 none)` 需要实参内部的空格，`attr(c, %)`
            // 需要逗号前的空格，`math.max($number1: 1, 2)` 需要一个未绑定的标签），而保持原始 run
            // 不做替换又会让 `saturate($c)` 留在那里，而 `saturate.hrx::css_overload/with_sass_var`
            // 要求 `saturate(1)`。
            Err(_) => Ok(decline(toks, &self.env)),
        }
    }

    /// 一个头部位置（选择器或属性名）作为结构化的 pieces：**只解析 `#{ … }`**，其他每个词素都逐字
    /// 保持所写的样子，而一个字面的 `&` 词素变成 `Piece::Parent`。
    ///
    /// Sass 在头部没有裸变量引用，而头部也不总是选择器 —— `@for` 头*绑定*它的 `$i`，所以在这里查
    /// 一个裸 `$var` 曾把 `sass-spec/directives/for/whitespace.hrx` 的 `@for $i from … {}` 变成一条
    /// "Undefined variable"（实测：`UndefinedVariable` 被定为致命时，那 60 条回退里约 47 条正是
    /// 这一处越界造成的）。词素也绝不可被读成数字（`.2xl` 是标识符）。
    ///
    /// `parent` 是外围 scope 的解析后文本，只有 `#{&}` 需要它。
    fn head_selector(&self, toks: &[Tok], parent: &str) -> Result<Selector, SassError> {
        self.head_pieces(toks, parent).map(Selector::from_pieces)
    }

    /// 头部的 pieces，按源码顺序：把每个词素映射成 0..N 个 piece 再展平，顺序就保持，而无需构建
    /// 一个字符串再在里面搜 `&`。边缘在 piece 层修剪，因为头部周围的新行是布局，不是选择器的一部分 ——
    /// 是这个头部被嵌套时 `a.b` 与 `a .b` 的差别。
    fn head_pieces(&self, toks: &[Tok], parent: &str) -> Result<Vec<Piece>, SassError> {
        toks.iter()
            .map(|tok| self.pieces_of(tok, parent))
            .collect::<Result<Vec<Vec<Piece>>, SassError>>()
            .map(|runs| Selector::from_pieces(runs.concat()).trimmed().into_pieces())
    }

    fn pieces_of(&self, tok: &Tok, parent: &str) -> Result<Vec<Piece>, SassError> {
        match tok {
            Tok::Punct('&') => Ok(vec![Piece::Parent]),
            // `#{&}` **不是**父引用位置：语料把父作为文本代入，然后正常嵌套，所以
            // `.test { .selector#{&} {} }` 打印 `.test .selector.test`，不是 `.selector.test`
            // （`libsass-closed-issues/issue_1043.hrx`）。
            Tok::Interp(inner) if matches!(inner.as_slice(), [Tok::Punct('&')]) => {
                Ok(vec![Piece::Text(parent.to_string())])
            }
            other => self.piece_text(other).map(|text| vec![Piece::Text(text)]),
        }
    }

    /// 一个头部词素作为文本：只有插值被求值，其他每个词素都逐字保持所写的样子。
    fn piece_text(&self, tok: &Tok) -> Result<String, SassError> {
        let Tok::Interp(inner) = tok else {
            return Ok(token_text(tok));
        };
        let Some(expr) = expr::interpolated(inner, &self.env) else {
            return Ok(render(inner));
        };
        match expr.eval(&self.env) {
            Ok(value) => Ok(value.to_css()),
            Err(err) if err.is_fatal() => Err(err),
            Err(_) => Ok(render(inner)),
        }
    }
}

/// 一次被内建 decline 的调用变成的东西：run 按未知被调者文本的方式渲染，即经
/// [`expr::interpolated`]，它保持每个词素逐字、只解析它的**变量读取** —— 那正是 `saturate($c)`
/// 需要的（`saturate.hrx::css_overload/with_sass_var`）。
///
/// 一次 decline 的调用绝不是要报告的错误：语料对本编译器产不出的 run 的回答，就是 run 自己的文本。
/// 从重渲染传播失败曾被试过并实测 —— 它把无效错误读数从 53 抬到 74，并把 `math.max($number1: 1, 2)`
/// 变成 `Undefined variable.`，因为那个错误来自**我们的**第二遍，而不是来自 Sass 源码。绝不可重渲染
/// 的东西 —— 一个未知参数的**标签**，它命名参数而不是变量 —— 在调用方由它自己的 kind 匹配，所以
/// 这条路只到达那些标签（如果有）成功绑定的 run。
fn decline(toks: &[Tok], env: &Env) -> Option<Value> {
    expr::interpolated(toks, env)
        .map(|run| run.eval(env))
        .and_then(Result::ok)
        .filter(Value::is_plain)
}

/// 什么不把意义带进属性名：布局空白与一个被丢弃的 silent comment。两者对旧的渲染文本形式都不可见
/// （`canon` 修剪前者，[`render`] 丢弃后者），所以绑定测试越过它们，看第一个真正属于名字的词素。
fn skipped_in_name(tok: &Tok) -> bool {
    matches!(tok, Tok::Space(_) | Tok::Silent(_))
}

/// 声明左侧命名什么：环境的变量，或一个 CSS 属性。
enum Target {
    Variable(String),
    Property(PropName),
    /// 声明的左侧命名了一个内建模块的常数，它没有可更新的绑定
    ReadOnly,
}

/// 一个落定的头文本是否打开一个 **plain-CSS at-rule** 的 body：`@function`（任意大小写）命名一个
/// 以 `--` 开头的自定义 ident —— `@function --a()`、`@function --a(--b <color>)`、
/// `@function --a() returns <ident>`、`@#{function} --a()`（`css/function.hrx`、
/// `css/plain/function.hrx`）。
///
/// 它读**解析后的**头，因为头自己可能携带 `#{…}`（上面最后一例），所以插值必须先折叠才能知道形状。
/// `@mixin --a` 有意不包含：语料把它做成错误（`css/mixin.hrx::error/css/mixin`），不是透传的 body。
fn plain_css_body(text: &str) -> bool {
    let resolved = text.to_lowercase();
    let mut words = resolved.strip_prefix('@').unwrap_or("").split_whitespace();
    matches!(words.next(), Some("function"))
        && words.next().is_some_and(|name| name.starts_with("--"))
}

/// 一个块头是否**声明一个可调用**：`@mixin name(…)` 或 `@function name(…)`（关键字的大小写无关，
/// 与每个 Sass at-rule 一样）。
///
/// 像这里每个分类器一样在词素上判定：第一个有意义的词素必须是粘合成一个 `Word` 的关键字，而跟在
/// 后面的名字**不得**以 `--` 开头 —— `@function --a()` 是 plain-CSS at-rule（`css/function.hrx`），
/// 它的 body *是*被发射的 CSS，所以把它误当 Sass 声明会删掉正常工作的规则。
fn callable_declaration(toks: &[Tok]) -> bool {
    let words = brackets::meaningful(toks);
    let [Tok::Word(keyword), Tok::Word(name), ..] = words.as_slice() else {
        return false;
    };
    let declares =
        keyword.eq_ignore_ascii_case("@mixin") || keyword.eq_ignore_ascii_case("@function");
    declares && !name.starts_with("--")
}
