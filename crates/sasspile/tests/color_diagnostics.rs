//! `color.change` / `color.adjust` / `color.scale` 在参数**不可用**时给出的消息，钉在
//! `core_functions/color/{change,adjust,scale}/error/{args,type,units,bounds}.hrx` 与
//! `core_functions/color/saturate.hrx::error/too_few_args` 上。下面每条输入与每条消息都抄自 fixture
//! 自己的 `input.scss` 与 `error` 段。
//!
//! 本文件的主旨是：三个族**不**共用一种措辞，发明一条通用消息会答出语料没有陈述的行：
//! * `change` 还接受通道的未引号 `none`——它的意思是*缺*通道——故它的类型消息说
//!   `is not a number or unquoted "none".`，而未引号的 `none` 本身被安静拒绝
//!   （`change/hwb.hrx::whiteness/none` 期望输出 `hwb(180deg none 50%)` 而非错误；那处拒答钉在
//!   `tests/color_change.rs`）；
//! * `adjust` 与 `scale` 即使对 `none` 也说 `is not a number.`；
//! * 只有 **scale** 对每个量都要求 `%`，并把 share 夹在 `-100%…100%`。

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// 一次调用预期失败时的消息——harness 比较语料 `error` 段的首行（`tests/hrx.rs::oracle_header`）。
/// 在此，**编译成功**的调用就是失败。
async fn failure(input: &str) -> String {
    match compile(input).await {
        Ok(css) => panic!("{input} was expected to fail, but it compiled to:{css}"),
        Err(error) => error.message,
    }
}

/// `$color: 1 is not a color.` 是颜色槽里放**数字**时的消息，三个族措辞相同
/// （`{change,adjust,scale}/error/type.hrx::color`）。本编译器读不了的空间里的颜色完全得不到
/// 消息——它保留文本，因为拿 sRGB 答 `color(srgb …)` 或 `lab(…)` 会是一个错值。
#[tokio::test]
async fn a_number_in_the_colour_slot_is_not_a_colour() {
    for input in [
        "a {b: color.change(1)}",
        "a {b: color.adjust(1)}",
        "a {b: color.scale(1)}",
    ] {
        assert_eq!(
            failure(input).await,
            "$color: 1 is not a color.",
            "for {input}"
        );
    }
}

/// 类型消息，逐族不同：`change` 点出它也收的未引号 `none`
/// （`change/error/type.hrx::{red,alpha,quoted_none}`），而 `adjust`/`scale` 不点，连那个词也拒
/// （`adjust/error/type.hrx::none` 是 `color.adjust(red, $alpha: none)` →
/// `Error: $alpha: none is not a number.`，`scale/error/type.hrx::none` 同理）。
#[tokio::test]
async fn an_amount_that_is_not_a_number_uses_its_family_wording() {
    for (input, want) in [
        (
            "a {b: color.change(red, $red: c)}",
            "$red: c is not a number or unquoted \"none\".",
        ),
        (
            "a {b: color.change(red, $alpha: \"none\")}",
            "$alpha: \"none\" is not a number or unquoted \"none\".",
        ),
        (
            "a {b: color.change(red, $saturation: c)}",
            "$saturation: c is not a number or unquoted \"none\".",
        ),
        (
            "a {b: color.adjust(red, $red: c)}",
            "$red: c is not a number.",
        ),
        (
            "a {b: color.adjust(red, $alpha: none)}",
            "$alpha: none is not a number.",
        ),
        (
            "a {b: color.scale(red, $red: c)}",
            "$red: c is not a number.",
        ),
        (
            "a {b: color.scale(red, $alpha: none)}",
            "$alpha: none is not a number.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// scale 的量是**份额**，故只有百分比能承载：`color.scale(white, $red: 1)` 与
/// `color.scale(white, $red: 1px)` 分别以 `Expected 1 to have unit "%".` 与
/// `Expected 1px to have unit "%".` 拒答（`scale/error/units/rgb.hrx::{none,wrong}/*`，
/// `…/units/hsl.hrx` 与 `…/units/hwb.hrx` 里的轴同理）。
#[tokio::test]
async fn a_scale_amount_has_to_be_a_percentage() {
    for (input, want) in [
        (
            "a {b: color.scale(white, $red: 1)}",
            "$red: Expected 1 to have unit \"%\".",
        ),
        (
            "a {b: color.scale(white, $red: 1px)}",
            "$red: Expected 1px to have unit \"%\".",
        ),
        (
            "a {b: color.scale(white, $saturation: 1)}",
            "$saturation: Expected 1 to have unit \"%\".",
        ),
        (
            "a {b: color.scale(white, $whiteness: 1px)}",
            "$whiteness: Expected 1px to have unit \"%\".",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// `adjust` 与 `change` 对 rgb 通道与轴收无单位数——但**不**对白度和黑度，后者按陈述就是百分比：
/// `color.adjust(white, $whiteness: 1)` 与 `color.change(black, $blackness: 1)` →
/// `Expected 1 to have unit "%".`（`adjust/error/units/hwb.hrx::none/*`，`change` 同文件同理）。
#[tokio::test]
async fn whiteness_and_blackness_are_percentages_for_the_other_two_as_well() {
    for (input, want) in [
        (
            "a {b: color.adjust(white, $whiteness: 1)}",
            "$whiteness: Expected 1 to have unit \"%\".",
        ),
        (
            "a {b: color.adjust(black, $blackness: 1px)}",
            "$blackness: Expected 1px to have unit \"%\".",
        ),
        (
            "a {b: color.change(black, $blackness: 1)}",
            "$blackness: Expected 1 to have unit \"%\".",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// share 本身在 `-100%` 与 `100%` 之间：`color.scale(red, $saturation: -100.001%)` →
/// `Error: $saturation: Expected -100.001% to be within -100% and 100%.`，
/// `…/legacy/too_high` 是 `$whiteness: 100.001%`（`scale/error/bounds.hrx`）。
#[tokio::test]
async fn a_scale_share_stays_between_minus_a_hundred_and_a_hundred() {
    for (input, want) in [
        (
            "a {b: color.scale(red, $saturation: -100.001%)}",
            "$saturation: Expected -100.001% to be within -100% and 100%.",
        ),
        (
            "a {b: color.scale(red, $whiteness: 100.001%)}",
            "$whiteness: Expected 100.001% to be within -100% and 100%.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// hue 没有可朝其上移的界：`color.scale(white, $hue: 10%)` →
/// `Error: $hue: Channel isn't scalable.`（`scale/error/args.hrx::hue`，`scale/error/polar.hrx`
/// 的三行说同一件事）。`adjust` 会回绕 hue（`adjust/hsl.hrx::hue/arg_above_max` 把 540 读作
/// 180），这就是为什么这条拒答是逐族的、而不是关于角度的一般律。
#[tokio::test]
async fn a_hue_is_not_scalable() {
    assert_eq!(
        failure("a {b: color.scale(white, $hue: 10%)}").await,
        "$hue: Channel isn't scalable."
    );
}

/// 颜色占唯一的位置参数，每个通道都按名点名：`color.change(red, 1)` →
/// `Error: Only one positional argument is allowed. All other arguments must be passed by name.`
/// （`{change,adjust,scale}/error/args.hrx::too_many`）。
#[tokio::test]
async fn the_colour_is_the_only_positional_argument() {
    let want =
        "Only one positional argument is allowed. All other arguments must be passed by name.";
    for input in [
        "a {b: color.change(red, 1)}",
        "a {b: color.adjust(red, 1)}",
        "a {b: color.scale(red, 1)}",
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// 空调用点名它缺的参数，这是 registry 的通用 variadic 消息做不到的：`color.change()` →
/// `Missing argument $color.`（`{change,adjust,scale}/error/args.hrx::too_few`），而 `saturate()`——
/// 其颜色可选、其量不可选——→ `Missing argument $amount.`
/// （`core_functions/color/saturate.hrx::error/too_few_args`）。
#[tokio::test]
async fn a_missing_argument_is_named_by_the_function_that_lacks_it() {
    for input in [
        "a {b: color.change()}",
        "a {b: color.adjust()}",
        "a {b: color.scale()}",
    ] {
        assert_eq!(
            failure(input).await,
            "Missing argument $color.",
            "for input {input}"
        );
    }
    assert_eq!(
        failure("a {b: saturate()}").await,
        "Missing argument $amount.",
        "saturate's colour is optional, its amount is not"
    );
}

/// 点名**两个空间**通道的调用按名拒答，报在决定组所属的空间上：
/// `color.scale(red, $red: 1%, $saturation: 1%)` →
/// `Error: $saturation: Color space rgb doesn't have a channel with this name.`，而
/// `color.change(red, $saturation: 1%, $blackness: 1%)` → `Error: $blackness: Color space hsl …`
/// （`{change,adjust,scale}/error/mixed_formats.hrx`；三个族有同样的七行）。
#[tokio::test]
async fn naming_channels_of_two_spaces_is_refused_by_name() {
    for (input, want) in [
        (
            "a {b: color.scale(red, $red: 1%, $saturation: 1%)}",
            "$saturation: Color space rgb doesn't have a channel with this name.",
        ),
        (
            "a {b: color.scale(red, $green: 1%, $saturation: 1%)}",
            "$saturation: Color space rgb doesn't have a channel with this name.",
        ),
        (
            "a {b: color.scale(red, $blue: 1%, $lightness: 1%)}",
            "$lightness: Color space rgb doesn't have a channel with this name.",
        ),
        (
            "a {b: color.scale(red, $red: 1%, $blackness: 1%)}",
            "$blackness: Color space rgb doesn't have a channel with this name.",
        ),
        (
            "a {b: color.scale(red, $green: 1%, $whiteness: 1%)}",
            "$whiteness: Color space rgb doesn't have a channel with this name.",
        ),
        (
            "a {b: color.scale(red, $saturation: 1%, $blackness: 1%)}",
            "$blackness: Color space hsl doesn't have a channel with this name.",
        ),
        (
            "a {b: color.scale(red, $lightness: 1%, $whiteness: 1%)}",
            "$whiteness: Color space hsl doesn't have a channel with this name.",
        ),
        (
            "a {b: color.change(red, $red: 1, $hue: 1)}",
            "$hue: Color space rgb doesn't have a channel with this name.",
        ),
        (
            "a {b: color.change(red, $saturation: 1%, $blackness: 1%)}",
            "$blackness: Color space hsl doesn't have a channel with this name.",
        ),
        (
            "a {b: color.adjust(red, $green: 1, $saturation: 1%)}",
            "$saturation: Color space rgb doesn't have a channel with this name.",
        ),
    ] {
        assert_eq!(failure(input).await, want, "for input {input}");
    }
}

/// hue 被**两个**轴空间共有，故它谁也决定不了：把它与白度、黑度并列点明是一次普通的 hwb 写入，
/// 不是拒答——`color.change(blue, $hue: 150, $whiteness: 20%, $blackness: 40%)` → `#339966`
/// 且颜色标名时同一调用同（`change/hwb.hrx::{all,named}`，都在本律不得破坏的行里）。
#[tokio::test]
async fn the_hue_belongs_to_both_axis_spaces_and_decides_neither() {
    for (input, want) in [
        (
            "a {b: color.change(blue, $hue: 150, $whiteness: 20%, $blackness: 40%)}",
            "#339966",
        ),
        (
            "a {b: color.change($color: blue, $hue: 150, $whiteness: 20%, $blackness: 40%)}",
            "#339966",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `$space:` 必须是字符串：`color.scale(red, $space: 1)` → `Error: $space: 1 is not a string.`
/// （`scale/error/type.hrx::space`，`adjust`/`change` 有同一行）。而*是*字符串的 `$space:` 是另一
/// 种情形——本编译器无法转换的空间——保留调用文本（`adjust/error/units/*.hrx` 与 `…/space.hrx`
/// 正因这个理由而红，不是因这条）。
#[tokio::test]
async fn a_space_that_is_not_a_string_is_said() {
    for input in [
        "a {b: color.scale(red, $space: 1)}",
        "a {b: color.adjust(red, $space: 1)}",
        "a {b: color.change(red, $space: 1)}",
    ] {
        assert_eq!(
            failure(input).await,
            "$space: 1 is not a string.",
            "for input {input}"
        );
    }
}
