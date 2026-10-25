# 第 2 课：让电脑记住一个数 —— 变量，以及能不能改它

> 零基础第 2 课。上一课你会了：**程序**、**编译**。这一课只加两个新词：
> **变量**、**可变**。看到别的陌生词，那一定是我写坏了，请指出来。
>
> 本课所有代码都真跑过，屏幕输出原样抄在下面。

---

## 1. 今天只弄懂一件事

上一课的程序每做一次都是全新的：数字 42 是当场算出来的，算完就没了。

可很多事需要电脑**先记着**：游戏里的分数、还剩几条命、玩家叫什么名字。
记下来，之后还能接着用、接着改。

"记下来"这件事，写法长这样：

```rust
let score = 3;
```

读作：**让电脑记住一个数，记住它就叫 `score`，它现在是 3。**

## 2. 先跑起来看

在上一课的 `hello` 项目里，把 `src/main.rs` 全部替换成这段，然后 `cargo run`：

```rust
fn main() {
    let score = 3;
    println!("一开始记住了：{}", score);

    let mut lives = 5;
    lives = lives + 1;
    println!("加一之后：{}", lives);

    let lives = "五条命";
    println!("同名再来一次，这次是：{}", lives);
}
```

这是我屏幕上的真实结果：

```
一开始记住了：3
加一之后：6
同名再来一次，这次是：五条命
```

## 3. 你猜会发生什么（先猜再改再跑）

| 你把代码改成 | 你猜会怎样 |
|---|---|
| 把 `let mut lives = 5;` 里的 `mut` 删掉，其余不动 | 照样跑？还是报错？ |
| 把 `lives = lives + 1;` 写成 `lives = livess + 1;` | 电脑会发现打错字吗？ |
| 把 `let score = 3;` 改成 `let score = "3";` | 报错？还是打印出 `3`？ |

第三行答案可能让你意外：**不报错**，屏幕上出现 `3`（不带引号）。
因为打印本来就要把东西变成文字，而 `"3"` 本来就是文字。
"什么时候电脑才真拦你"，看第 6 节第 ③ 条。

## 4. 一行一行看

```rust
    let score = 3;
```
- `let` 是"开始记一个东西"的记号。
- `score` 是你给它起的名字，随你取（英文字母、数字、下划线，别以数字开头）。
- `=` 在这里**不是**"等于"，而是"把右边的结果放进左边这个名字里"。
  这句要读成："score 拿来放 3"。
- 结尾 `;` 表示这句说完了。

这样一个"起名字让电脑记住值"的东西，就叫**变量**。

```rust
    println!("一开始记住了：{}", score);
```
- 括号里第二个位置写的是 `score`：把这个变量里记着的东西，填进前面句子的空位 `{}`。
- 注意：这里只是**看一眼**，并没有改它。

```rust
    let mut lives = 5;
```
- `mut` 是"以后还要改它"的声明。
- 不写 `mut`，这个名字就只许记一次；改第二次会被当场拦住（第 6 节第 ① 条）。
- 为什么电脑要这么麻烦？因为它想让"谁会改这个值"在你读代码时一眼可见：
  写了 `mut`，你才知道往后要盯着它的变化。

```rust
    lives = lives + 1;
```
- 把 `lives` 现在记着的数加 1，再放回同一个名字里。
- 这一句做完，`lives` 就是 6 了——所以屏幕打印出 6。

```rust
    let lives = "五条命";
```
- 又是 `let`，名字和上面那个一样。这叫"同名再来一次"：新建一个 `lives` 把旧的盖住。
  从这一行往后，`lives` 指的是"五条命"这几个字，不再是数字 6。
- 旧的 6 并没有被"改掉"，而是被挡住了，之后没法再拿回来。
  这个做法很容易看错，所以初学阶段**建议永远起不同的名字**，比如 `lives_count`、`lives_text`。

## 5. 现在给这两件事起名字（本课仅有的两个新词）

**变量**：`let 名字 = 值;` 造出来的东西，就是"一个有名字的值"，之后拿这个名字去用它。

**可变**：变量前写了 `mut`，才允许以后再给它换值；没写就只能记一次。

| 写法 | 能改吗 | 结果 |
|---|---|---|
| `let a = 3;` | 不能（改了报错） | 记住 3 |
| `let mut b = 3; b = 4;` | 能 | b 现在是 4 |

## 6. 你会撞到的报错（三条原话，我刚跑出来的）

### ① 没写 `mut` 就想改

我把 `let mut lives = 5;` 的 `mut` 删了：

```
error[E0384]: cannot assign twice to immutable variable `lives`
 --> src/main.rs:6:5
  |
5 |     let lives = 5;
  |         ----- first assignment to `lives`
6 |     lives = lives + 1;
  |     ^^^^^^^^^^^^^^^^^ cannot assign twice to immutable variable
  |
help: consider making this binding mutable
  |
5 |     let mut lives = 5;
  |         +++
```

怎么读：第一行告诉你是哪个名字；中间告诉你"它第一次被记在哪一行"；
最后 `help:` 直接把修法写给你（在名字前加 `mut`）。**看到 `help:` 就照抄。**

### ② 名字打错了一个字母

我把 `lives + 1` 写成 `livess + 1`：

```
error[E0425]: cannot find value `livess` in this scope
 --> src/main.rs:6:13
  |
6 |     lives = livess + 1;
  |             ^^^^^^
  |
help: a local variable with a similar name exists
  |
6 -     lives = livess + 1;
6 +     lives = lives + 1;
```

电脑不但说"找不到这个名字"，还猜到你本来想写什么，把改好的整行给你。
这种贴心报错以后很常见，先别怀疑自己，照 `help` 改就行。

### ③ 想给数字加文字

我把 `lives = lives + 1` 写成 `lives = lives + "1"`：

```
error[E0277]: cannot add `&str` to `{integer}`
 --> src/main.rs:6:19
  |
6 |     lives = lives + "1";
  |                   ^ no implementation for `{integer} + &str`
  |
  = help: the trait `Add<&str>` is not implemented for `{integer}`
  = help: the following other types implement trait `Add<Rhs>`:
            `&f128` implements `Add<f128>`
            ...（这里还列了一长串）
```

下面那一长串你现在**完全不用看懂**。只看两处：第一行说"数字加不了文字"，
以及 `^` 指在哪个符号上。报错像一本摊开的厚词典，你要的只是被翻到的那一页。

## 7. 练习（做完再看答案）

1. 造两个变量：一个记你的年龄，一个记今天要喝几杯水。打印成一句：
   `我 X 岁，今天要喝 Y 杯水。`
2. 把第 1 题的水杯数加 2，再打印一次。**故意少写一个 `mut`**，亲手撞一次第 ① 条报错。
3. 用变量算：原价 30 元，减 5 元折扣，买 2 件。打印结果（提示：注意括号）。
4. 跑这段：`let x = 1; let x = x + 1; let x = x + 1; println!("{}", x);`
   先猜屏幕上是几，再跑。（它**不会**报错——想想为什么。）

### 参考答案

```rust
fn main() {
    let age = 18;
    let mut cups = 3;
    println!("我 {} 岁，今天要喝 {} 杯水。", age, cups);

    cups = cups + 2;
    println!("现在要喝 {} 杯。", cups);

    let price = 30;
    let discount = 5;
    let pieces = 2;
    println!("一共 {} 元。", (price - discount) * pieces);

    // 练习 4：答案是 3。每一句 `let x = x + 1;` 都是"新建一个同名变量，
    // 用旧的 x 算出新的值"，不是"改"，所以不需要 mut。
    let x = 1;
    let x = x + 1;
    let x = x + 1;
    println!("{}", x);
}
```

第 2 题少写 `mut` 时，你会看到 `cannot assign twice to immutable variable`，
`help:` 叫你把 `mut` 加上——和第 6 节第 ① 条一模一样。

## 8. 今天你真正学到的是什么

- 变量 = 给一个值起名字；`mut` = 声明"以后我还要换掉它"；
- `=` 是"放进去"，不是"等于"；
- 报错里的 `help:` 就是标准答案；后面的长清单可以不看；
- "同名再来一次"能跑但容易看错，初学阶段起两个不同的名字更安全。

**下一课**：把"做一件事"起个名字，反复叫它的名字 —— 你会遇到**函数**，
以及它做完之后**交回给你的东西**（新词仍然只有两个）。
