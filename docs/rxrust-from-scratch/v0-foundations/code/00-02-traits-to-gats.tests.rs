//! V0-02 的断言：关联类型够用、GAT 能表达"元素自己带生命周期"，
//! 以及泛型参数版与关联类型版在使用点的成本差异。

use minirx_check::{Ints, Series, SourceG, Windowed, Words, drain, map_window, window_total};

/// 只为对照方案 A：同一个能力用"泛型参数化 Item"再写一遍，看签名要付什么。
struct Tens {
    taken: u8,
}

impl SourceG<i32> for Tens {
    fn take(&mut self) -> Option<i32> {
        match self.taken < 3 {
            true => {
                self.taken += 1;
                Some(i32::from(self.taken) * 10)
            }
            false => None,
        }
    }
}

/// 方案 A 的泛型函数：签名必须把元素类型 `i32` 说死。
fn first_of_three<S: SourceG<i32>>(src: &mut S) -> Vec<i32> {
    let mut out = Vec::new();
    while let Some(v) = src.take() {
        out.push(v);
    }
    out
}

#[test]
fn one_generic_function_serves_different_associated_items() {
    let mut ints = Ints::new(3);
    let mut numbers = Vec::new();
    drain(&mut ints, &mut numbers);
    assert_eq!(numbers, vec![1, 2, 3]);

    let mut words = Words::new(vec!["a".to_string(), "b".to_string()]);
    let mut sink = Vec::new();
    drain(&mut words, &mut sink);
    assert_eq!(sink, vec!["b".to_string(), "a".to_string()]);
}

#[test]
fn the_generic_parameter_form_burns_the_item_into_every_signature() {
    assert_eq!(first_of_three(&mut Tens { taken: 0 }), vec![10, 20, 30]);
    // 换成方案 B 就不需要 `<i32>`：`drain` 对 `Ints`（i32）与 `Words`（String）同样成立。
}

#[test]
fn a_gat_window_borrows_the_data_it_iterates() {
    let series = Series::new(vec![1, 2, 3, 4, 5]);

    let mut seen = Vec::new();
    map_window(&series, 1, 3, |v| seen.push(*v));
    assert_eq!(seen, vec![2, 3, 4]);

    // 元素类型是 `&i32`：带生命周期的元素，正是 `type Item<'a>` 要表达的东西。
    let borrowed: Vec<&i32> = series.window(0, 2).collect();
    assert_eq!(borrowed, vec![&1, &2]);
}

#[test]
fn the_caller_picks_the_lifetime_not_the_impl() {
    let series = Series::new(vec![10, 20, 30]);
    assert_eq!(window_total(&series, 0, 3), 60);
    assert_eq!(window_total(&series, 1, 2), 50);
}
