# 术语表 — 中文释义 + 英文原词 + 首次出现的课号（逐卷追加，禁"自行查阅"）

> 这张表是**可读性闸的数据源**：`docsizer terms` 用它检查"正文有没有用还没教过的行话"。
> 一行 = 一个概念（同一行的中英文名算一个，不重复计入预算）。每课新概念 ≤ 2。
> V1 起的编号在真正写那一卷时会按同一预算复核并可能微调；前置卷（V0）已按"新概念 ≤ 2"排到 16 课。

| 术语 | 英文 | 释义 | 首现课 |
|---|---|---|---|
| 程序 | program | 写给电脑的一串指令，存在文件里；它自己不会动，要有人让它跑起来 | V0-01 |
| 编译 | compile | 把你写给人看的文字，翻译成电脑真能执行的动作；`cargo run` 就是先编译再运行 | V0-01 |
| 变量 | variable (`let`) | 给一个值起名字，让电脑记住它 | V0-02 |
| 可变 | `mut` | 在名字前写 `mut`，才允许以后改掉它 | V0-02 |
| 函数 | function (`fn`) | 把一件事起个名字，之后随时叫这个名字让它再做一遍 | V0-03 |
| 返回值 | return value | 这件事做完之后交给你的那个东西 | V0-03 |
| 列表 | array `[T; N]` | 一排放在一起的值，写作 `[10, 20, 30]`，位置从 0 开始数 | V0-04 |
| 循环 | `for` | 把同一件事对列表里每个东西做一遍 | V0-04 |
| 所有权 | ownership | 每个值同一时刻只有一个主人；主人没了值就没了 | V0-05 |
| 搬走 | move | 把值的归属交给别人之后，原来的名字就不能再用了 | V0-05 |
| 借用 | borrow / `&T`, `&mut T` | 只暂时访问而不拥有；可变借用同一时刻只能有一个 | V0-06 |
| 生命周期 | lifetime (`'a`) | 编译器用名字判定"谁必须活得比谁久" | V0-06 |
| 闭包三能力 | `Fn` / `FnMut` / `FnOnce` | 只读 / 可改环境 / 只能用一次 | V0-07 |
| 消耗式方法 | method taking `self` | 调用后接收者被夺走，后续调用编译错——把"终止后不再发"写进类型 | V0-07 |
| trait | trait | 一份"能做到什么"的约定：谁实现它，谁就得提供这些做法 | V0-08 |
| 泛型参数 | generic parameter `<T>` | 把"什么类型"留成一个洞，用的时候再填 | V0-08 |
| 关联类型 | associated type `type Item;` | 由实现者来填的那个洞：一个实现只填一种 | V0-09 |
| GAT | generic associated type `type Item<'a>` | 带参数的关联类型：填进去的类型自己也带寿命 | V0-09 |
| `where Self: 'a` | outlives 约束 | "实现者至少活得和借用一样久"，少了编译器直接拒 | V0-10 |
| HRTB | higher-ranked trait bound `for<'a>` | "对任意寿命都成立"的约束 | V0-10 |
| 共享所有权 | `Rc` / `Arc` | 引用计数容器；`Rc` 单线程，`Arc` 跨线程（靠 `Send`/`Sync` 筛） | V0-11 |
| 内部可变性 | interior mutability | 只拿 `&self` 也能改内部；代价是把"能不能改"从编译期推到运行期 | V0-11 |
| 守卫 | guard / `Ref` / `RefMut` / `MutexGuard` | 借出来的凭据，离开作用域即归还；两种线程模型的守卫类型不同 | V0-12 |
| 重入探测 | `try_rc_deref_mut` | 拿不到就返回 `None`，用来发现同步重入而不是当场炸 | V0-12 |
| 锁中毒 | mutex poisoning | 持锁线程 panic 后的两种策略：`.unwrap()` 一起炸，`into_inner()` 继续用 | V0-13 |
| 无守卫共享 | `SharedCell` / `CellRc` / `CellArc` | 给 `Copy` 小值的 `get`/`set`/`compare_exchange`，不 panic 也不死锁 | V0-13 |
| 宏规则集 | `macro_rules!` | "matcher `=>` 模板"的有序集合，匹配的是令牌而非值 | V0-14 |
| 片段分类符 | fragment specifier（`ty`/`expr`/`ident`/`lifetime`/`tt`） | 声明"这里吃什么"；选错就报 `no rules expected …` | V0-14 |
| `tt` 流 | `$($x:tt)*` | 把"泛型参数列表"连同逗号当令牌搬走，展开时原样吐出 | V0-15 |
| 内部规则 | `@impl` 首令牌 | 惯例性的"私有入口"：骨架只写一份，公开规则负责打包参数 | V0-15 |
| 卫生性 | hygiene | 宏里的名字不会捕获调用方同名变量 | V0-16 |
| 一致性泄漏检查 | `coherence_leak_check` | 两条规则在类型上可能重叠时产生的警告，需显式 `allow` | V0-16 |
| 观察者/消费者 | Observer | 接收 next/error/complete 三信号的 sink；终止方法消耗自身 | V1-01 |
| 元素 | Item | 流上被推动、被移动的那个值 | V1-01 |
| 取消 | unsubscribe / is_closed | 由值传播的控制信号，不是异常 | V1-02 |
| 结束契约 | `Option::None` | 返回 `None` 即"没有更多了"，缺了它消费者循环永不退出 | V1-02 |
| 早停 | early stop / `break` | 每发一个前先问接收方还要不要 | V1-03 |
| 拉 / 推 | pull / push | 拉：调用方决定何时取；推：源头决定何时发 | V1-03 |
| `take()` 语义 | `Option::take` | 把内部对象取走并结束它，之后自动 no-op | V1-04 |
| 闭包吞错 | 空的 `error`/`complete` | `subscribe(closure)` 时错误与完成信号没有出口，只该用于不可能失败的流 | V1-04 |
| 镜像 trait | `DynObserver` / `box_error(self: Box<Self>)` | 把不能通过 `dyn` 调的方法改写成能调的形式，再自动接回原约定 | V1-05 |
