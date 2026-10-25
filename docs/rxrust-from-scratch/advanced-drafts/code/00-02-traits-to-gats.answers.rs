//! V0-02 练习答案的可编译版本（与课文 §9 一致；放同目录以便与课程代码一起验证）。
//!
//! 这里刻意使用集成测试形态（`tests/`）：它需要一个真实的 crate 才能编译，
//! 因此验证流程是"把本文件 `cp` 到 workspace 外的验证 crate 的 `tests/` 下再 `cargo test`"。

use minirx_check::{Ints, Series, Source, Windowed, drain, window_total};

/// 练习 1：一个会结束的来源，加一个消耗自身的 `count()`。
pub struct Ticker {
    left: i32,
}

impl Source for Ticker {
    type Item = i32;

    fn take(&mut self) -> Option<i32> {
        match self.left > 0 {
            true => {
                self.left -= 1;
                Some(self.left)
            }
            false => None,
        }
    }
}

impl Ticker {
    /// `take()` 能为 `None` 才数得完：否则这是个永不返回的循环。
    fn count(mut self) -> usize {
        let mut n = 0;
        while self.take().is_some() {
            n += 1;
        }
        n
    }
}

/// 练习 2：矩阵取一行并按长度截断。
pub struct Matrix {
    rows: Vec<Vec<i32>>,
}

impl Windowed for Matrix {
    /// 截断后的迭代器是**另一种类型**：`Row` 必须写成 `Take<slice::Iter<..>>`，
    /// 不能继续写 `std::slice::Iter<'a, i32>`（那是练习里最容易踩错的点）。
    type Row<'a> = std::iter::Take<std::slice::Iter<'a, i32>> where Self: 'a;

    fn window(&self, at: usize, len: usize) -> Self::Row<'_> {
        self.rows[at].iter().take(len)
    }
}

/// 练习 3：返回子流本身而不是消费它。`'_` 绑到 `&self` 的那个匿名生命周期。
fn window_of<S: Windowed>(s: &S, at: usize, len: usize) -> S::Row<'_> {
    s.window(at, len)
}

#[test]
fn answers_compile_and_agree() {
    assert_eq!(Ticker { left: 5 }.count(), 5);

    let mut ints = Ints::new(3);
    let mut sink = Vec::new();
    drain(&mut ints, &mut sink);
    assert_eq!(sink, vec![1, 2, 3]);

    let matrix = Matrix { rows: vec![vec![7, 8, 9]] };
    let row: Vec<i32> = window_of(&matrix, 0, 2).copied().collect();
    assert_eq!(row, vec![7, 8]);

    let series = Series::new(vec![1, 2, 3]);
    assert_eq!(window_total(&series, 0, 3), 6);
}
