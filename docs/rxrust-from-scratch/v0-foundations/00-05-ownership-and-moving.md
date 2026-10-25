# 第 5 课：值有主人 —— 所有权，以及"交出去就没了"

> 零基础第 5 课。已会：**程序**、**编译**、**变量**、**可变**、**函数**、**返回值**、
> **列表**、**循环**。这一课只加两个新词：**所有权**、**搬走**。代码真跑过，输出原样抄录。

这是 Rust 和大多数语言**最不一样**的一课。前面四课的内容你换个语言也能对上，
这一课开始不一样了——而后面所有让你头疼的报错，根子都在这里。

---

## 1. 今天只弄懂一件事

先看两条真实发生的事。

**第一条**：把一个变量交给函数之后，再用它——

```rust
fn show(text: String) {
    println!("我收到了：{}", text);
}

fn main() {
    let greeting = String::from("你好");
    show(greeting);
    println!("还能用吗？{}", greeting);
}
```

编译不过：

```
error[E0382]: borrow of moved value: `greeting`
 --> src/main.rs:8:25
  |
6 |     let greeting = String::from("你好");
  |         -------- move occurs because `greeting` has type `String`, which does not implement the `Copy` trait
7 |     show(greeting);
  |          -------- value moved here
8 |     println!("还能用吗？{}", greeting);
  |                              ^^^^^^^^ value borrowed here after move
```

**第二条**：同样叫"交出去"，数字却能反复用——

```rust
fn main() {
    let number = 5;
    println!("第一次用：{}", number);
    println!("第二次用：{}", number);
    println!("第三次用：{}", number);
}
```

跑得好好的，三行都打出来。

一个是"用过一次就废了"，一个是"想用几次用几次"。**为什么？**
这就是今天要弄懂的那件事。

## 2. 先跑起来看（把两件事放在一起）

```rust
fn show(text: String) {
    println!("我收到了：{}", text);
}

fn main() {
    let first = String::from("第一段话");
    show(first);

    let number = 5;
    println!("数字还能再用一次：{}", number);
    println!("第三次用它也没事：{}", number);

    let second = String::from("第二段话");
    let third = second;
    println!("换名字接着交出去：{}", third);
}
```

我的真实输出：

```
我收到了：第一段话
数字还能再用一次：5
第三次用它也没事：5
换名字接着交出去：第二段话
```

注意第 4 行：`second` 交给 `third` 之后，**`second` 就用不了了**，
但 `third` 完全可以继续用、继续交出去。

## 3. 你猜会发生什么

| 你把代码改成 | 你猜会怎样 |
|---|---|
| 在 `show(first);` 后面加一句 `show(first);`（交两次） | 第二次没事？还是报错？ |
| 把 `String::from("第一段话")` 改成 `"第一段话"` | 报错？报哪一行？ |
| 把 `let number = 5;` 后面再多打印两次 | 报错？ |

第一条会报错——这正是第 6 节第 ② 条。第三条永远没事。
两者的区别就是"要不要占地方存"。

## 4. 一行一行看

```rust
    let first = String::from("第一段话");
```
- 这段文字**要多长就有多长**，电脑得给它**占一块地方存起来**。
- 存起来就有个事实：**这块地方只有一个主人**。现在主人叫 `first`。
- "谁是主人"这件事，就叫这个值的**所有权**。

```rust
    show(first);
```
- 把 `first` 交给 `show`。注意：**交的是主人身份，不是复印件**（因为它占地方，
  电脑不做复印件，复印件会引出"两份都以为自己是主人"的麻烦）。
- 从这一行起，那段文字的主人变成了 `show` 里面的 `text`；
  `first` 这个名字**不再是任何值的主人**了。
- 这种"主人换人、旧名字作废"，就叫**搬走**。

```rust
    println!("还能用吗？{}", first);   // ← 这一行只要存在，就编译不过
```
- 因为 `first` 已经不是主人了。电脑不给你"也许还能用"的机会，直接拒绝编译。

```rust
    let number = 5;
```
- 一个小小的整数：它不用占地方存，用的时候**看一眼就能照抄**。
  所以交给谁都不影响你继续用原来的名字。
- 报错原话里那句 `which does not implement the Copy trait` 说的就是这件事：
  `String` 不能照抄，只能换主人；整数那种小东西可以照抄。
  （现在你只需要认得"能照抄 / 只能换主人"这两类，后面的课会正式讲。）

```rust
    let second = String::from("第二段话");
    let third = second;
```
- 这里没有调用函数，只是"换个名字"。但因为 `second` 占地方存，
  写 `let third = second;` 也是**换主人**：从这行起 `second` 作废，`third` 是主人。

```rust
    println!("换名字接着交出去：{}", third);
```
- 用 `third` 完全没问题——主人是它。

## 5. 两个新词的正式说法（本课仅此两个）

**所有权**：每个值都有一位主人（就是那个"负责它、它没了就一起释放"的名字）。
同一时刻只有一位主人，这是电脑帮你盯着的规则，不是你要小心维持的约定。

**搬走**：把值的**主人身份**交给别人（交给函数、或交给另一个名字）。
搬走之后，旧名字不能再用——电脑当场拦住你。

| 值的种类 | 交给别人时 | 旧名字还能用吗 |
|---|---|---|
| 要看一眼就能照抄的小东西（整数、`true`/`false`） | 照抄一份出去 | 能 |
| 要占地方存的东西（一段可长可短的文字） | 换主人（搬走） | 不能 |

## 6. 你会撞到的报错（三条原话，实测）

### ① 交出去之后又用旧名字

```
error[E0382]: borrow of moved value: `greeting`
 --> src/main.rs:8:25
  |
6 |     let greeting = String::from("你好");
  |         -------- move occurs because `greeting` has type `String`, which does not implement the `Copy` trait
7 |     show(greeting);
  |          -------- value moved here
8 |     println!("还能用吗？{}", greeting);
  |                              ^^^^^^^^ value borrowed here after move
note: consider changing this parameter type in function `show` to borrow instead if owning the value isn't necessary
```

注意最后一行 `note:`——它说的是"如果你其实不想交出主人身份，可以让它只看不拿"。
**这正是下一课要教的事**（今天先不加名字，你只需知道：电脑已经暗示存在这种写法）。

### ② 同一个东西交给两处

```rust
fn main() {
    let greeting = String::from("你好");
    show(greeting);
    show(greeting);
}
```

```
error[E0382]: use of moved value: `greeting`
 --> src/main.rs:8:10
  |
6 |     let greeting = String::from("你好");
  |         -------- move occurs because `greeting` has type `String`, which does not implement the `Copy` trait
7 |     show(greeting);
  |          -------- value moved here
8 |     show(greeting);
  |          ^^^^^^^^ value used here after move
```

第一次交出去之后，主人已经不是 `greeting` 了，第二次当然交不出东西。

### ③ 交的东西形状不对（这条会把新手绕晕）

把 `show(String::from("你好"))` 写成 `show("你好")`：

```
error[E0308]: mismatched types
 --> src/main.rs:6:10
  |
6 |     show("直接写一段话");
  |     ---- ^^^^^^^^^^^^^^ expected `String`, found `&str`
  |     |
  |     arguments to this function are incorrect
```

`String` 和 `&str` 是"两种长相的文字"，函数要前者，你给了后者。
现在你只需要照第一行改：它要 `String`，就给它 `String`（`String::from("你好")`）。
两者差在哪，等"占地方存"这件事讲透了再说。

## 7. 练习（做完再看答案）

1. 写一个函数 `announce(title: String)`，打印"标题是：X"。
   造一个 `String` 交给它，然后再打印一次那个变量——**你会撞到第 ① 条报错**，
   抄下来，然后改成"交出去之后不再用旧名字"。
2. 把同一句 `let name = String::from("阿明");` 后面写两次 `announce(name);`，
   撞第 ② 条；改成两次都用新建的值。
3. 用整数重复这件事（`let count = 7;` 交给函数两次）——为什么不报错？
   把你的解释写成一句中文。
4. 把 4 个名字放进列表，用循环逐个交给 `announce`。
   （提示：`for name in names` 里，`name` 每次都是"当前那个名字"。）

### 参考答案

```rust
fn announce(title: String) {
    println!("标题是：{}", title);
}

fn count_out(count: i32) {
    println!("数到 {}", count);
}

fn main() {
    // 练习 1：交出去之后就不用旧名字了
    let first_title = String::from("第一章");
    announce(first_title);

    let second_title = String::from("第二章");
    announce(second_title);

    // 练习 2：每次新建一个再交
    for label in [0, 1] {
        let fresh = String::from("新一份");
        announce(fresh);
        let _ = label;
    }

    // 3：整数看一眼就能照抄，所以交两次也不影响原来的名字
    let count = 7;
    count_out(count);
    count_out(count);
    println!("count 还是 {}", count);
}
```

第 3 题的参考解释：**小整数不用占地方存，交给谁只是照抄一份，所以原名字照样能用。**

## 8. 今天你真正学到的是什么

- 每个值有一位**主人**（**所有权**），同一时刻只有一位；
- 把占地方存的东西交给函数或别的名字 = **搬走**，旧名字立刻作废，电脑当场拦住；
- 小东西（整数这种）是"照抄"，所以能反复用——这解释了两条看似矛盾的现象；
- 报错里的 `note: … borrow instead …` 是电脑在提示你"还有另一种交法"。

**下一课**就讲那种交法：把东西给别人看，但不把主人身份交出去（那一课会给它起名字）。
这是"Rust 为什么能在不靠垃圾回收的情况下保证不乱用内存"的核心一课。
