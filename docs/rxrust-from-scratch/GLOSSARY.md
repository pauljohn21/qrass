# 术语表 — 中文释义 + 英文原词 + 首次出现的课号（逐卷追加，禁"自行查阅"）

| 术语 | 英文 | 释义 | 首现课 |
|---|---|---|---|
| 观察者/消费者 | Observer | 接收 next/error/complete 三信号的 sink；终止方法消耗自身以在类型层固化协议 | V1-01 |
| 元素 | Item | 流上被推动、被移动的那个 owned 值 | V1-01 |
| 取消 | unsubscribe / is_closed | 由值传播的控制信号，不是异常 | V1-01 |
| 所有权 | ownership | 每个值同一时刻只有一个主人；搬走（move）后旧名字不可再用 | V0-01 |
| 借用 | borrow / `&T`, `&mut T` | 只暂时访问而不拥有；可变借用同一时刻只能有一个 | V0-01 |
| 生命周期 | lifetime (`'a`) | 编译器用名字判定“谁必须活得比谁久”；推式按值传能避掉大量生命周期 | V0-01 |
| 消耗式方法 | method taking `self` | 调用后接收者被夺走，后续调用编译错 —— 用来把“终止后不再发”协议写进类型 | V0-01 |
| 拉 / 推 | pull / push | 拉：调用方决定何时取（`Iterator`）；推：源头决定何时发（`Observable`） | V0-01 |
| 早停 | early stop / `break` | 每发一个前先问接收方还要不要，要取消就走值而不是走异常 | V0-01 |
| 泛型参数 | trait 的 `<Item>` | 元素类型由使用点选定：同一能力要为每种元素类型各写一份签名 | V0-02 |
| 关联类型 | associated type `type Item;` | 元素类型由**实现者**选定；一个类型对同一 trait 只能实现一次（E0119） | V0-02 |
| GAT | generic associated type `type Item<'a>` | 带参数的关联类型：元素自己的寿命由**使用者**逐次选定 | V0-02 |
| `where Self: 'a` | outlives 约束 | “实现者至少活得和借用一样久”；GAT 里少了它直接被编译器拒（missing required bound） | V0-02 |
| HRTB | higher-ranked trait bound `for<'a>` | “对任意寿命都成立”的约束；rxrust `map` 用它接 `Self::Item<'a>` | V0-02 |
| 结束契约 | `Option::None` | `take()` 返回 `None` 即“没有更多了”；缺了它消费者循环永不退出 | V0-02 |
| 闭包三能力 | `Fn` / `FnMut` / `FnOnce` | 只读 / 可改环境 / 只能用一次；`next` 对应 `FnMut`，终止方法对应 `FnOnce` | V0-03 |
| 闭包吞错 | `FnMutObserver` 的空 `error`/`complete` | 用 `subscribe(closure)` 时，错误与完成信号没有出口；它只该用于 `Err = Infallible` 的流 | V0-03 |
| 镜像 trait | `DynObserver` / `box_error(self: Box<Self>)` | 把不可 object-safe 的消耗式方法改写成能通过 `dyn` 调用的形式，再 blanket impl 接回原协议 | V0-03 |
| `take()` 语义 | `Option::take` | 把内部消费者取走并结束它；取走后 `Option` 变 `None`，后续信号自动 no-op | V0-03 |
