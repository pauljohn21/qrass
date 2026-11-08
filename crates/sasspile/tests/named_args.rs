//! 命名实参是标签，不是引用（1,611 例无效 `Undefined variable` 报错背后的形状）。下面每条输入
//! 都照抄其注明的夹具，所以边界是语料的，不是发明的期望。
//!
//! 这里钉住的契约是「`$label:` 命名一个参数，它绝不成为变量引用」—— 对有签名的被调者，该调用
//! 求值到夹具自己的输出；对没有的，该次运行的文本存活。两种无论哪种，编译都必须不失败：为一个
//! 标签做环境查找正是曾经产生约 1,611 个伪 `Undefined variable` 报错的错误，这些断言的存在
//! 就是让它不能回来。

use sasspile::ErrorKind;
use sasspile::compile;

/// `core_functions/color/hwb/four_args::named` —— 每个实参都带名字。Sass 对照函数的参数解析它们；
/// 为 `$hue` 做环境查找本身就是错的。
///
/// 这条断言曾经更弱 —— 该调用未注册，所以测试只能要求它的文本存活。`hwb` 现在有签名，于是标签
/// 对照夹具自己的 `output.css` 校验，这是同一契约更强的表述：`$hue:`/`$whiteness:`/`$blackness:`/
/// `$alpha:` 标签按任意顺序绑定到参数，无一被当变量查找，且调用被求值。
#[tokio::test]
async fn named_arguments_are_not_variable_references() {
    let css = compile("a {b: color.hwb($hue: 180, $whiteness: 30%, $blackness: 40%, $alpha: 0.4)}")
        .await
        .expect("named arguments must not fail the compile");
    assert_eq!(css, "a {\n  b: hsla(180, 33.3333333333%, 45%, 0.4);\n}\n");
}

/// `core_functions/map/has_key.hrx:69` —— 一个命名实参，其值是**map 字面量**，所以 `$map:` 的
/// 两种读法出现在同一次运行里，而只有外层那个是标签。
#[tokio::test]
async fn a_map_literal_inside_named_arguments_still_parses() {
    let css = compile("a {b: map.has-key($map: (c: d), $key: c)}")
        .await
        .expect("a call with a map argument must not fail");
    assert!(css.contains("map.has-key("), "got {css}");
}

/// `core_functions/color/is_powerless/named` —— 被调者是命名空间的（`color.is-powerless`），
/// 所以实参起始的判定不能依赖名字是普通标识符。
#[tokio::test]
async fn a_namespaced_callee_still_names_its_arguments() {
    compile("a {b: color.is-powerless($color: red, $channel: \"a\", $space: lab)}")
        .await
        .expect("namespaced call with named arguments");
}

/// 使本改动不至于沦为铁律所禁的全面兜底的边界：一次**没有**命名实参的运行仍解析它的引用，
/// 而未绑定的引用仍中止 stream。
#[tokio::test]
async fn a_real_reference_still_fails() {
    let err = compile("a { b: hwb($missing) }")
        .await
        .expect_err("an unbound reference in a call's argument position is still a reference");
    assert_eq!(err.kind, ErrorKind::UndefinedVariable);
}
