//! Lexer —— `Lexer` 阶段内核（`crate::phases`）背后的 owned 累加器**值**：对单个字符做前向 fold
//! 的一步。`#{}`/插值深度是累加器里的*数据*，从来不是调用栈；一个词素是后续任何阶段可以据以
//! 推理的最细粒度 —— `math.$pi` 是**一个** [`Tok::MemberVar`]，而不是消费者必须重新拼起来的两段。
//!
//! 内核安装的汇口 [`Emit`] 定义在这里，因为每个阶段都用它：内核以 `observer.next(..)` 驱动它，
//! 于是 `char → 0..N Tok` 的扇出逐项发出。故意不返回 `Vec`：把一个阶段的产出攒起来再到下游
//! 摊平一次，正是这次重构要去掉的非框架形状。下一阶段 `Tok → 0..N Event` 是 [`crate::structure`]。
//!
//! 词素律有一个缺口是开放的、被记录下来的，而不是被抹过去的：带引号的字符串仍然**不是**一个词素，
//! 所以坐在其中的 `}`/`;`/`,` 被当作结构 `Punct` 发出，下面的阶段把它当真的读
//! （`tasks.md` 9.8 记着实测代价与修法）。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// 一段逐字的源码文本（绝不含空白或算术符号）
    Word(String),
    /// 一个空白字符 —— 是**数据**：Sass 把 `10px -5px` 读成空格分隔的列表，
    /// 而把 `10px - 5px` 读成减法
    Space(char),
    /// `+`，或是在词素边界上独立出现的 `-`（一个运算符，由下游的间距律裁定）。属于标识符的
    /// `-` **不是** `Op`：它留在 `Word` 里，因为分类的粒度不会细过 lexer 的词素 —— 把
    /// `-a-calc(` 切成 `Op('-')+Word("a")+…` 之后，「这是不是厂商前缀的 CSS 调用？」就答不了了
    /// （`css/functions/special/prefixed/lowercase.hrx` 与 `uppercase.hrx` 的 `*/script_like`）。
    Op(char),
    Punct(char),
    /// 一个 `$name` 引用（名字不含开头的 `$`）
    Var(String),
    /// 一个**带命名空间**的引用 `module.$name` —— `math.$pi`、`math.$max-safe-integer`。
    /// 在这里、在 `$` 起头那个字符处就做成一个词素，因为下游没有任何东西能把它重新拼回来：
    /// `Word("math.")` 与 `Var("pi")` 分开时，消费者分不出 `math.$pi`（内置模块常量）与
    /// `math . $pi`（乘积）或 `5.$pi`；而在 `expr` 里重扫渲染出的文本，正是本项目已多次实测
    /// 的分类粒度错误（`-a-calc(`、`calls::split` 丢空格、`named_argument` 猜标签）。
    MemberVar(String, String),
    /// `#{ … }` 插值的内部词素
    Interp(Vec<Tok>),
    /// 一条 `// …` 静默注释（**含两条斜杠**），作为数据携带，好让每个消费者自己决定它是否
    /// 活进 CSS：选择器与普通声明值丢掉它，自定义属性（`--x:`）逐字保留它，因为它的值是
    /// 原始 CSS 文本。
    Silent(String),
}

impl Tok {
    /// 这个词素把括号深度拨动多少 —— 该律**唯一**的陈述，写死在词素上，消费者无从改口：
    /// 一个 word 携带自己的收支，所以 `Word("calc(")` 开一层，自闭的 `Word("url(x)")` 收支为零，
    /// 而独立站着的括号也恰好各计一次。组实参就是 `()` 那种独立括号：
    /// `core_functions/list/join/empty.hrx` 整件 27 条全被计量、全未求值（`both/*` 是其中最齐的
    /// 一簇 7 条，写的都是 `list.join((), …)`；callee 无签名 ⇒ run 保留文本）—— 换句话说，
    /// **没有任何读数能区分本律与被它替换的旧计数**。它与 [`crate::brackets`]、
    /// [`crate::operations`] 共用；`expr` 只经 `operations::protected` 取用同一条律，因为同一事实
    /// 若有两个答案，实参切分就会与同一模块早已算出的括号收支走偏。
    pub(crate) fn net(&self) -> i32 {
        match self {
            Tok::Word(word) => word.matches('(').count() as i32 - word.matches(')').count() as i32,
            Tok::Punct('(') => 1,
            Tok::Punct(')') => -1,
            _ => 0,
        }
    }
}

/// 内核为自己的下游 observer 安装的汇口：`FnMut(Item)`，把每项产出直接送进流。
pub type Emit<'a, T> = &'a mut dyn FnMut(T);

/// 这些字符总是收束正在攒的词素，并各自成为一个 `Punct`。
///
/// `&` 属于这里，因为父引用必须是一个**词素**：嵌套随后按片段决定（`crate::selector`），而不是
/// 在渲染出的字符串里搜 `'&'`，而任何分类器都不会比 lexer 切出的更细。`&:hover`、`&-suffix`
/// 与 `a & b` 都到达为邻旁的 `Punct('&')`，而不是粘成一个 `Word`。
const STRUCTURAL: [char; 6] = ['{', '}', ';', ':', ',', '&'];

fn is_var_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// 一个把标识符词素续下去的字符 —— 决定待定 `-` 角色的判据。
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

#[derive(Default)]
pub struct LexAcc {
    buf: String,
    hash: bool,
    var: Option<String>,
    /// 被 [`LexAcc::take_module_prefix`] 从待定 word 上切下的模块前缀，等那个补全 [`Tok::MemberVar`]
    /// 的 `$name`
    member: Option<String>,
    interp: Vec<Vec<Tok>>,
    slash: bool,
    block: bool,
    block_star: bool,
    /// 正处在一行 `// …` 到行尾的注释中，收集其文本
    line: bool,
    /// 正为 [`Tok::Silent`] 收集的段落（以 `//` 起头）
    comment: String,
    /// 注释之外消费的上一个源码字符 —— 这段回看让协议斜杠（`url(http://x)`、`url(//cdn/x)`）
    /// 不被读成静默注释
    prev: Option<char>,
    /// 在词素边界见过的 `-`，角色尚未定：Sass 的间距律需要下一个字符，与待定斜杠的折叠同理
    minus: bool,
    /// 我们正处其中的字符串字面量的引号字符（若有）。今天它唯一的作用是让 `$` 留作文本而不是
    /// 起头一个引用 —— Sass 不在带引号的字符串里解析变量
    /// （`core_functions/selector/extend/simple/pseudo/arg::class/equal` 传的
    /// `selector.extend(":c(@#$)", ":c(@#$)", "e")` 必须不失败）。
    quote: Option<char>,
    /// 字符串里见过的反斜杠：它转义下一个字符，所以 `\"` 不会闭合它
    escaped: bool,
}

impl LexAcc {
    /// 一个已完成的词素：插值还开着时，它继续把内部词素收进自己那一层；到了顶层，词素直接进流。
    fn push(&mut self, tok: Tok, emit: Emit<'_, Tok>) {
        match self.interp.last_mut() {
            Some(inner) => inner.push(tok),
            None => emit(tok),
        }
    }

    fn flush_word(&mut self, emit: Emit<'_, Tok>) {
        if !self.buf.is_empty() {
            let word = std::mem::take(&mut self.buf);
            self.push(Tok::Word(word), emit);
        }
    }

    /// 待定 word 以 `.` 结尾且前面还有东西时，那个尾巴是**模块前缀**：把它切下来，好让后面的
    /// `$name` 被切成一个 [`Tok::MemberVar`]。点之前的部分整个返回（`a.math.$pi` 得到 `a.math`），
    /// 于是不在模块表里的名字也能按源码拼写渲染回去。
    fn take_module_prefix(&mut self) -> Option<String> {
        let head = self.buf.strip_suffix('.')?;
        if head.is_empty() {
            return None;
        }
        let prefix = head.to_string();
        self.buf.clear();
        Some(prefix)
    }

    /// 把目前收集到的 `// …` 文本作为一个 [`Tok::Silent`] 发出，并重启段落。
    fn take_comment(&mut self, emit: Emit<'_, Tok>) {
        if !self.comment.is_empty() {
            let text = std::mem::take(&mut self.comment);
            self.push(Tok::Silent(text), emit);
        }
    }

    /// 喂进一个字符，把 0..N 个 owned 词素发进 `emit`。
    pub fn feed(&mut self, ch: char, emit: Emit<'_, Tok>) {
        if self.block {
            // 在 /* .. */ 响注释里：吃到 */ 为止
            if self.block_star && ch == '/' {
                self.block = false;
                self.block_star = false;
            } else {
                self.block_star = ch == '*';
            }
            return;
        }
        if self.line {
            // 注释吃掉整行，但绝不吃声明终止符：自定义属性保留它的 `// …` 文本，且仍在 `;` 收尾
            // （`--b: c // comment;`）。
            match ch {
                '\n' => {
                    self.line = false;
                    self.take_comment(emit);
                    // 结束注释的那个换行是**空白**，所以发成一个 `Space` 词素。它曾被推进 word 缓冲，
                    // 变成 `Word(" ")` —— 一个假标识符，把紧随其后绑定的 `$name` 从任何词素级判据
                    // 藏起来（`// c\n$ids: ();` 一度不再绑定 `$ids`）。
                    self.push(Tok::Space(' '), emit);
                }
                ';' => {
                    self.take_comment(emit);
                    self.push(Tok::Punct(';'), emit);
                }
                c => self.comment.push(c),
            }
            return;
        }
        if let Some(open) = self.quote {
            // 在带引号的字符串里，字符就是内容：反斜杠转义下一个，配对的引号结束字面量，
            // 而 `$` 不得被读成变量引用（`core_functions/selector/extend/simple/pseudo/arg::class/equal`）。
            //
            // 把字面量做成一个词素（好让它内部的 `}`/`;`/`,` 不再走到结构臂）在本分支尝试过两次，
            // 但**没有**做成：第二次尝试实测 `net=1 changed=70`、意外错误 `17 → 39`，并弄红
            // `tests/lexer.rs::dollar_inside_a_quoted_string_is_text` —— 因为 `#` 在字面量里是有歧义的
            // （孤立的 `#` 是文本，`#{` 开头 SassScript），而裁定它需要 pending-`#` 状态覆盖到 `{`
            // 之前的*整段*，而不是一个字符。见 `tasks.md` 9.8。
            if self.escaped {
                self.escaped = false;
            } else if ch == '\\' {
                self.escaped = true;
            } else if ch == open {
                self.quote = None;
                self.buf.push(ch);
                self.flush_word(emit);
                return;
            } else if ch == '$' {
                self.buf.push(ch);
                return;
            }
        }
        if self.slash {
            self.slash = false;
            if ch == '*' {
                self.block = true;
                self.block_star = false;
                return;
            }
            if ch == '/' {
                self.flush_word(emit);
                self.line = true;
                self.comment = String::from("//");
                return;
            }
            self.buf.push('/');
        }
        if self.hash {
            self.hash = false;
            if ch == '{' {
                self.flush_word(emit);
                self.interp.push(Vec::new());
                return;
            }
            self.buf.push('#');
        }
        if self.var.is_some() {
            if is_var_char(ch) {
                self.var.as_mut().unwrap().push(ch);
                return;
            }
            let name = std::mem::take(self.var.as_mut().unwrap());
            self.var = None;
            match self.member.take() {
                Some(namespace) => self.push(Tok::MemberVar(namespace, name), emit),
                None => self.push(Tok::Var(name), emit),
            }
        }
        // 待定的 `-` 据此字符了结：标识符字符把词素续下去（厂商前缀，或标识符里的连字符），
        // 其余一切让 `-` 成为独立的运算符/符号，并让 `ch` 在下面按自身处理。
        if self.minus {
            self.minus = false;
            if is_ident_char(ch) {
                self.buf.push('-');
            } else {
                self.flush_word(emit);
                self.push(Tok::Op('-'), emit);
            }
        }
        // 为下面的斜杠律做回看：先取上一个字符，再记下这个即将被消费的字符。
        let prev = self.prev;
        self.prev = Some(ch);
        match ch {
            // 只有当斜杠不可能是 URL authority 的一部分时（`http://`、`url(//cdn`）才起一个待定的
            // 注释/除法标志；第二根斜杠由下一步的待定标志折叠裁定。
            '/' if prev != Some(':') && prev != Some('(') => self.slash = true,
            '#' => {
                self.flush_word(emit);
                self.hash = true;
            }
            '$' => {
                let prefix = self.take_module_prefix();
                self.member = prefix;
                self.flush_word(emit);
                self.var = Some(String::new());
            }
            '}' if !self.interp.is_empty() => {
                self.flush_word(emit);
                let inner = self.interp.pop().unwrap();
                self.push(Tok::Interp(inner), emit);
            }
            c if STRUCTURAL.contains(&c) => {
                self.flush_word(emit);
                self.push(Tok::Punct(c), emit);
            }
            c if c.is_whitespace() => {
                self.flush_word(emit);
                self.push(Tok::Space(c), emit);
            }
            '+' => {
                self.flush_word(emit);
                self.push(Tok::Op(ch), emit);
            }
            // `*` 从不属于标识符或 URL，`/*` 注释又已被上面的块状态吃掉，所以它可以直接做自己的
            // 词素。把它留在 word 里曾让 `1px * 1em` 读不出运算（`Word("1px * 1em")`），而 1.4d 的
            // 单位代数正需要读出它。`/` 故意留在 word 里：在 `calc( … )` 之外它是列表分隔符
            // （`font: 12px/1.5`、`a {b: 1/2}`），`url(http://x)` 也靠这一点。
            '*' => {
                self.flush_word(emit);
                self.push(Tok::Op(ch), emit);
            }
            // 推迟：一个 `-` 是 Sass 的运算符（两侧都有空白）还是标识符的一部分（`-a-calc(`、
            // `10px -5px`），只有等下一个字符到了才知道。
            '-' => self.minus = true,
            // 引号开头一个字符串字面量；配对的引号在上面闭合它。词素保留引号字符，所以渲染仍逐字
            // 复现源码。
            quote @ ('"' | '\'') if self.quote.is_none() => {
                self.quote = Some(quote);
                self.buf.push(quote);
            }
            c => self.buf.push(c),
        }
    }
}

/// 把一段词素渲染回源码文本（P0：值/选择器逐字透传；完整的 `Value`/`Expr` 代数随 P1 到来）。
/// 插值拼合时不加多余空格。静默注释**丢掉** —— 保留它的场景见 [`render_raw`]。
pub fn render(toks: &[Tok]) -> String {
    canon(&text_of(toks, false))
}

/// 同 [`render`]，但保留 `// …` 注释文本 —— 给内容本身就是原始 CSS、必须原样存活的自定义属性值。
pub fn render_raw(toks: &[Tok]) -> String {
    canon(&text_of(toks, true))
}

fn text_of(toks: &[Tok], keep_silent: bool) -> String {
    toks.iter()
        .map(|t| match t {
            Tok::Word(w) => w.clone(),
            Tok::Space(c) => c.to_string(),
            Tok::Op(c) => c.to_string(),
            Tok::Punct(c) => c.to_string(),
            Tok::Var(n) => format!("${n}"),
            // 整个、按书写顺序：前缀是从待定 word 上切下来的，所以这里是它唯一存活的地方 ——
            // declined 的 run 必须把 `math.$pi` 逐字打回去。
            Tok::MemberVar(module, n) => format!("{module}.${n}"),
            Tok::Interp(inner) => text_of(inner, keep_silent),
            Tok::Silent(c) if keep_silent => c.clone(),
            Tok::Silent(_) => String::new(),
        })
        .collect()
}

/// 一个词素形式的源码文本，静默注释丢掉 —— 插值 run 拼装的件，无需重扫渲染出的字符串。
pub fn token_text(tok: &Tok) -> String {
    text_of(std::slice::from_ref(tok), false)
}

/// 绑定上的声明标志：`!` 粘在标识符上（`!default`、`!global`）。lexer 把它保留为一个 `Word`，
/// 所以判据从不重扫渲染文本。
fn is_flag(tok: &Tok) -> bool {
    matches!(tok, Tok::Word(word) if word.starts_with('!')
        && word.len() > 1
        && word[1..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
}

/// **绑定的**值去掉尾部声明标志：`!default` / `!global` 描述绑定，不是值的一部分，Sass 也从不
/// 序列化它们 —— `$a: b !default !default;` 之后 `c {d: $a}` 打出 `d: b`
/// （`variables/double_flag.hrx::default`；`!global` 的双子用例是 `::global`）。重复标志已废弃
/// 但仍被解析，那两个 fixture 说的正是这件事。
///
/// 只有绑定走这里。普通声明里的 `!important` 是 CSS 文本、必须存活（P7 7.1），所以
/// [`crate::exec`] 不得在那里调用本函数。`!default` 的*含义*（不要覆盖已有绑定）在这里同样
/// 未被触碰 —— 它唯一的 fixture 都是配置模块（`directives/use/with/*`），属于 P3.6/P5.1，
/// 不属于这次剥皮。
pub fn without_flags(toks: &[Tok]) -> &[Tok] {
    let meaningful: Vec<usize> = toks
        .iter()
        .enumerate()
        .filter(|(_, tok)| !matches!(tok, Tok::Space(_) | Tok::Silent(_)))
        .map(|(index, _)| index)
        .collect();
    let ends_with_flag = meaningful
        .last()
        .is_some_and(|index| is_flag(&toks[*index]));
    if !ends_with_flag {
        return toks;
    }
    // 在最后一个不是标志的有效词素之后切，让整段尾部一起走 —— `b !default !default` 得 `b`，
    // 不是 `b !default`。
    let cut = meaningful
        .iter()
        .rev()
        .find(|index| !is_flag(&toks[**index]))
        .map_or(0, |index| index + 1);
    &toks[..cut]
}

/// 把空白段落折成单个空格并去两端空白 —— 规范 CSS 文本形式。
pub fn canon(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
