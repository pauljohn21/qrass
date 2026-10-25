//! V0-01 配套代码（纯代码，断言在同名 `.tests.rs` 里）。
//!
//! 主题：所有权、借用，以及"值必须被搬走"到底在解决什么问题——为后面每一课的
//! `fn next(&mut self, value: Item)` 与 `fn subscribe(self, …)` 铺地基。

/// 【拉 pull】调用方决定何时取：从借来的切片上算一遍。
///
/// `&[i32]` 是"借一段连续内存的视图"：数据仍归调用方的 `Vec` 所有，本函数只是看了一眼。
pub fn pull_sum(values: &[i32]) -> i32 {
    let mut total = 0;
    for v in values {
        // `v: &i32`（按引用遍历），所以要 `*v` 把值取出来再加。
        total += *v;
    }
    total
}

/// 【推 push】被调用方决定何时发：整份 `Vec` 的所有权交给它，逐个数塞进 `sink`。
///
/// 注意签名：`values: Vec<i32>` 不是 `&Vec<i32>`——数据"归属"随参数一起搬走了。
pub fn push_all(values: Vec<i32>, sink: &mut Vec<i32>) {
    for v in values {
        // `v: i32` 已经是自己的值，直接搬进 sink，没有任何一步在"借用源头"。
        sink.push(v);
    }
}

/// 推 + 早停：接收方表示"我不接了"，源头据此停止发射。
///
/// 这就是 rxrust `Observer::is_closed()`（`observer.rs:42`）在做的事：取消不是异常，
/// 是源头每发一个之前会问一次的**值**。
pub fn push_until(values: Vec<i32>, limit: usize, sink: &mut Vec<i32>) -> usize {
    let mut emitted = 0;
    for v in values {
        if emitted >= limit {
            break; // 收口：不再往下推
        }
        sink.push(v);
        emitted += 1;
    }
    emitted
}

/// 值必须被搬走的真正理由：`String` 拥有堆上的字符缓冲。
///
/// 若这里改成 `&[String]` + `sink: &mut Vec<&String>`，那些 `&String` 就必须一直有人
/// 养着它们指向的数据——"谁拥有、活多久、何时释放"又变回了一个需要小心维护的问题。
pub fn push_names(names: Vec<String>, sink: &mut Vec<String>) {
    for name in names {
        sink.push(name); // 每个 String 的堆缓冲转手交给 sink，全程只有一份所有者
    }
}

/// 消费者协议：`receive` 要可变借用（后面还要继续收），`finish` **消耗自身**
/// （交出结果之后这个消费者就不存在了，不可能再收第二个）。
///
/// 对应 `observer.rs:19-43`：`next(&mut self, …)` 与 `error(self)` / `complete(self)`。
pub trait Sink {
    fn receive(&mut self, value: i32);
    fn finish(self) -> i32;
    fn closed(&self) -> bool;
}

/// 一个最小实现：累加到上限，`closed()` 报"收够了"。
#[derive(Default)]
pub struct Counter {
    /// 累加的是 `i32` 的值，字段类型就必须是 `i32`（写 `usize` 会在 `+=` 处被编译器拦下）。
    total: i32,
    seen: usize,
    limit: usize,
}

impl Counter {
    /// `..Counter::default()` 是函数式更新语法：只写要改的字段，其余取默认值。
    pub fn new(limit: usize) -> Counter {
        Counter { limit, ..Counter::default() }
    }
}

impl Sink for Counter {
    fn receive(&mut self, value: i32) {
        self.total += value;
        self.seen += 1;
    }

    /// 消耗 `self`：协议上"终止之后不再发射"，于是这条规则由类型系统负责，不需要运行时断言。
    fn finish(self) -> i32 {
        self.total
    }

    fn closed(&self) -> bool {
        self.seen >= self.limit
    }
}

/// 推式管道 + 提前取消：源头每发一个先问一次 `closed()`；结束时把被消耗的 `sink` 交出结果。
pub fn run(values: Vec<i32>, limit: usize) -> i32 {
    let mut sink = Counter::new(limit);
    for v in values {
        if sink.closed() {
            break;
        }
        sink.receive(v);
    }
    sink.finish() // `sink` 在此被移动掉；这一行之后再写 `sink.receive(..)` 就编译不过
}
