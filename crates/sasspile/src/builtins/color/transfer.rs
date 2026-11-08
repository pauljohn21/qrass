//! `color.to-space` —— 把一个颜色在 [`crate::color::Space`] 所建模的各个色彩空间之间移动。
//!
//! 从域根里拆出来，是因为它既不是构造子也不是轴操作：它读一个颜色、向换算表问一个问题、产出一个在另一个
//! 空间里的颜色。两个 legacy 目标（`rgb`、`hsl`）由那些已经在打印它们的表示来答，这正是语料区分的东西：同一个
//! `color(srgb 0.2 0.4 0.8)` 在 `rgb` 下变成 `#3366cc`，在 `hsl` 下变成 `hsl(220, 60%, 50%)`
//! （`to_space/srgb/{rgb,hsl}.hrx::middle`）。

use crate::SassError;
use crate::builtins::Builtin;
use crate::color::Space;
use crate::value::Value;

/// `color.to-space($color, $space)`。登记成一个**具名**的双参数内置函数，好让 `$color:` 与 `$space:` 能绑定；
/// 同时是 variadic，所以一次过长的调用报的是它自己的 arity，而不是被误读。
pub(super) static COLOR_TO_SPACE: Builtin = Builtin {
    name: "color.to-space",
    params: &["color", "space"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: to_space,
};

/// 调用所点名的目标空间。legacy 名字优先于空间表，因为两者产出的**形体**不同：`color.to-space($c, rgb)` 必须
/// 读成 `#3366cc`，不是 `color(srgb 0.2 0.4 0.8)`（`to_space/srgb/rgb.hrx::middle`）。
fn target(name: &str) -> Option<fn(&crate::color::Color) -> crate::color::Color> {
    match name.to_ascii_lowercase().replace('_', "-").as_str() {
        "rgb" => Some(|color| color.as_legacy_rgb()),
        "hsl" => Some(|color| color.as_legacy_hsl()),
        _ => None,
    }
}

fn to_space(args: Vec<Value>) -> Result<Value, SassError> {
    let decline = || SassError::new("not a colour space this slice can convert to");
    let [source, name] = args.as_slice() else {
        return Err(decline());
    };
    let (Value::Color(color), Value::Raw(text)) = (source, name) else {
        return Err(decline());
    };
    // `hwb` 是刻意缺席的：转到它的那些 fixture 是经一个模块 mixin 打印的（`to_space/srgb/hwb.hrx`），所以这里
    // 没有任何东西说出它的形体，而一个颜色不会被猜一个打印方式。
    if let Some(legacy) = target(text) {
        return Ok(Value::Color(legacy(&color)));
    }
    let space = Space::parse(text).ok_or_else(decline)?;
    Ok(Value::Color(color.converted(space)))
}
