//! `color` 域（`core_functions/color/**`）：访问器与轴操作，值上的定律在 [`crate::color`]，构造子
//! （`rgb`/`rgba`/`hsl`/`hsla`）在 [`constructors`]。
//!
//! 一个无法照实参兑现的函数体返回一个**非致命**错误，于是那个 run 保住自己的逐字文本——这个域里好几个
//! fixture 要的就是这个。`rgb(var(--foo), 3, 0.4)` 是最清楚的一例：`var()` 在解析之前就被替换，所以实参
//! 列表里能装任何东西，而语料把那次调用原样打出（`rgb/multi_argument_var.hrx::1_of_3`）。`rgb(1px, 2px, 3px)`
//! 是另一例：
//! 通道是数字或百分比，从来不是一个 `px` 量，而答案是本项目还没造出来的错误消息（P7），不是猜一个。

use crate::SassError;
use crate::builtins::Builtin;
use crate::color::Color;
use crate::value::Value;

mod amount;
mod constructors;
mod modify;
mod transfer;
/// 同一批三族的写入侧：一次调用写进哪个空间，以及每种表示各自拿到的算术。在 [`modify`] 越过行数预算时拆出。
pub(super) mod write;

use constructors::{COLOR_HWB, COLOR_SPACE, HSL, HSLA, HWB, LAB, LCH, OKLAB, OKLCH, RGB, RGBA};
use modify::{COLOR_ADJUST, COLOR_CHANGE, COLOR_SCALE, SCALE_COLOR};
use transfer::COLOR_TO_SPACE;

/// 本域的登记表。
pub(super) static COLORS: &[&Builtin] = &[
    &RGB,
    &RGBA,
    &HSL,
    &HSLA,
    &HWB,
    &COLOR_HWB,
    &COLOR_RED,
    &COLOR_GREEN,
    &COLOR_BLUE,
    &COLOR_ALPHA,
    &COLOR_HUE,
    &COLOR_SATURATION,
    &COLOR_LIGHTNESS,
    &COLOR_CHANNEL,
    &COLOR_CHANGE,
    &COLOR_ADJUST,
    &COLOR_SCALE,
    &SCALE_COLOR,
    &GRAYSCALE,
    &COLOR_GRAYSCALE,
    &DARKEN,
    &LIGHTEN,
    &SATURATE,
    &DESATURATE,
    &ADJUST_HUE,
    &COLOR_MIX,
    &COLOR_SPACE,
    &COLOR_TO_SPACE,
    &LAB,
    &LCH,
    &OKLAB,
    &OKLCH,
];

/// `color.mix(#91e16f, #0144bf)` → `rgb(28.6274509804%, 57.4509803922%, 59.2156862745%)`.
///
/// 是带名字的 variadic，所以 `$color1:`/`$color2:`/`$weight:` 能绑定（`mix/named.hrx`），而个数由函数体判；
/// **权重无论带什么单位都按百分比读**——`mix/units.hrx::weight/{unitless,unknown}` 对 `50` 与 `50px` 打出同一个
/// 结果（各带一条 `DEPRECATION WARNING [function-units]`），而 `50%` 那一行在 `mix/explicit_weight.hrx`。那些实参里带
/// `$space:`/`$method:` 的行（`mix/predefined.hrx`、`named::{polar_space,rectangular_space}`、`mixed_spaces`、
/// `hue_interpolation`、`explicit_method`）要的是本编译器还没实现的空间，它们 decline 成文本，而不是在 sRGB
/// 里混一下然后认领那一行。
static COLOR_MIX: Builtin = Builtin {
    name: "color.mix",
    params: &["color1", "color2"],
    optional: &["weight"],
    variadic: true,
    labelled: None,
    apply: mixed,
};

/// legacy 的 hsl 操纵子，每个都是 `($color, $amount)`。全部是**带两个名字的 variadic**，理由语料给了两条：标签
/// 必须能绑定（`darken.hrx::named` 是 `darken($color: red, $amount: 14%)`），而一次实参不是“一个颜色加一个量”
/// 的调用根本不是颜色调用——`saturate.hrx::css_overload/*` 把 `saturate(50%)`、`saturate(1)` 乃至
/// `saturate($amount: 50%)` 原样留着，因为 CSS 把 `saturate()` 定义成滤镜函数。
static DARKEN: Builtin = Builtin {
    name: "darken",
    params: &["color", "amount"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| {
        adjusted(args, |[hue, saturation, lightness], amount| {
            [hue, saturation, lightness - amount]
        })
    },
};

/// `lighten(red, 14%)` → `rgb(100%, 28%, 28%)`（`lighten.hrx::middle`）。
static LIGHTEN: Builtin = Builtin {
    name: "lighten",
    params: &["color", "amount"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| {
        adjusted(args, |[hue, saturation, lightness], amount| {
            [hue, saturation, lightness + amount]
        })
    },
};

/// `saturate(plum, 100%)` → `#ff7eff`（`saturate.hrx::two_args/max`；`max_remaining` 用 53% 落在同一个颜色上，因为
/// 饱和度在 100 处夹紧）。登记成 `($color, $amount)` 的 variadic，好让那些 CSS 滤镜写法活下来：
/// `saturate(50%)`、`saturate(1)`、`saturate($c)` 与 `saturate($amount: 50%)` 全是那个文件的 `css_overload/*`
/// 行，而它们原样打出——变量被替换、标签被丢掉，这正是 `expr.rs` 现在从求值后的实参重建、而不是把源 run 抄
/// 一份的那条 decline 路径（正因为抄那一份，`saturate($c)` 曾经量成一个 PASS → FAIL 回归）。
static SATURATE: Builtin = Builtin {
    name: "saturate",
    params: &["color", "amount"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| {
        adjusted(args, |[hue, saturation, lightness], amount| {
            [hue, saturation + amount, lightness]
        })
    },
};

/// `desaturate(plum, 100%)` → `rgb(74.7058823529%, …)`（`desaturate.hrx::max`）。
static DESATURATE: Builtin = Builtin {
    name: "desaturate",
    params: &["color", "amount"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| {
        adjusted(args, |[hue, saturation, lightness], amount| {
            [hue, saturation - amount, lightness]
        })
    },
};

/// `adjust-hue(…)` 只移动色相；它的量以度计。
static ADJUST_HUE: Builtin = Builtin {
    name: "adjust-hue",
    params: &["color", "amount"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| {
        adjusted(args, |[hue, saturation, lightness], amount| {
            [hue + amount, saturation, lightness]
        })
    },
};

/// `grayscale(#633736)` → `rgb(30%, 30%, 30%)`、`grayscale(#494949)` → `#494949`
/// （`core_functions/color/grayscale.hrx`）：饱和度归零，而亮度活过了 hsl 的往返——两种写法之所以不同，
/// 只因为其中一次往返落在一个整字节上。
///
/// 像访问器那样声明成 variadic，因为 CSS 也把 `grayscale(…)` 定义成滤镜函数：`grayscale(10%)` 不是颜色调用，
/// 必须原样打出。
static GRAYSCALE: Builtin = Builtin {
    name: "grayscale",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: grayscale_of,
};

/// `color.grayscale(…)`，同一条律的带命名空间写法。
static COLOR_GRAYSCALE: Builtin = Builtin {
    name: "color.grayscale",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: grayscale_of,
};

/// `color.red(…)`，第一个通道；green 与 blue 同理。
///
/// 刻意声明成**带一个名字的 variadic**：`core_functions/color/alpha.hrx::css_overloads/alpha/multi_arg` 写的是
/// `color.alpha(c=d, e=f, g=h)`——一个 Microsoft 滤镜，必须逐字打出——而定长 arity 会在任何函数体**跑之前**就抛
/// 它的 `Only 1 argument allowed…`，那一行就变成了一个意外错误。抓住它的是
/// `ledger failures <run> "unexpected error" --count` 那个读数：`net=26` 是真的，但无效错误数已经从
/// 53 → 55。现在由函数体判个数，所以一个建模不了的调用是 decline 而不是报错；那些 arity 消息行仍是红的、
/// 属于 P7，而不是被猜掉。
static COLOR_RED: Builtin = Builtin {
    name: "color.red",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| channel(args, 0),
};

/// `color.green(…)`，第二个通道。
static COLOR_GREEN: Builtin = Builtin {
    name: "color.green",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| channel(args, 1),
};

/// `color.blue(…)`，第三个通道。
static COLOR_BLUE: Builtin = Builtin {
    name: "color.blue",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| channel(args, 2),
};

/// `color.alpha(red)` → `1`、`color.alpha(rgba(red, 0.42))` → `0.42`，而 `color.alpha($color: …)` 按名字绑定
/// （`core_functions/color/alpha.hrx::color/{max,middle,min,named}`）。
static COLOR_ALPHA: Builtin = Builtin {
    name: "color.alpha",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: alpha_of,
};

/// `color.hue(hsl(359, 50%, 50%))` → `359deg`；`hsl(540, …)` → `180deg`——折叠在颜色造出来时已经做完了，
/// 所以访问器只是把它读回来（`core_functions/color/hue.hrx`）。与通道访问器同样的 variadic 形状，同一个理由。
static COLOR_HUE: Builtin = Builtin {
    name: "color.hue",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: hue_of,
};

/// `color.saturation(hsl(0, 100%, 42%))` → `100%`（`core_functions/color/saturation.hrx`）。
static COLOR_SATURATION: Builtin = Builtin {
    name: "color.saturation",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: saturation_of,
};

/// `color.lightness(hsl(0, 100%, 42%))` → `42%`（`core_functions/color/lightness.hrx::named`）。
static COLOR_LIGHTNESS: Builtin = Builtin {
    name: "color.lightness",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: lightness_of,
};

/// 颜色实参的一个通道，作为一个无单位数字。
fn channel(args: Vec<Value>, index: usize) -> Result<Value, SassError> {
    accessor(args, None, |color| color.byte(index))
}

/// `color.channel(#abcdef, "red")` → `171`、`color.channel(hsl(10 40% 60%), "hue")` → `10deg`，以及
/// `color.channel(pink, "hue", $space: hsl)` → `349.5238095238deg`
/// （`core_functions/color/channel/{rgb,hsl}.hrx::local/*` 与 `…::foreign/*`）。像这里每个颜色调用一样声明成
/// **带名字的 variadic**：那条 arity 消息还没造（P7），而一次这一刀读不了的调用——
/// `color.channel(color(srgb 0.2 0.5 0.8), "red")`、`hwb(…)`、`oklch(…)`、一个不认识的通道名——必须 decline
/// 成文本，而不是用一个从错轴上读出的数字作答。
static COLOR_CHANNEL: Builtin = Builtin {
    name: "color.channel",
    params: &["color", "channel"],
    optional: &["space"],
    variadic: true,
    labelled: None,
    apply: channel_of,
};

/// 一次 `color.channel` 调用：一个颜色、一个**带引号的通道名**，以及一个写成裸关键词的可选 `$space:`。那个名字
/// 到达时带着它的引号——与 `math.unit` 产出的是同一个形状（`Value::Raw("\"px\"")`）——所以这里是从结构上剥引号，
/// 而不是把一个 run 重新解析一遍。
fn channel_of(args: Vec<Value>) -> Result<Value, SassError> {
    let read = match args.as_slice() {
        [colour, name] => read_channel(colour, name, None),
        [colour, name, space] => read_channel(colour, name, Some(space)),
        _ => None,
    };
    match read {
        Some(value) => Ok(value),
        None => Err(SassError::new(
            "not a channel this slice can read in that space",
        )),
    }
}

fn read_channel(colour: &Value, name: &Value, space: Option<&Value>) -> Option<Value> {
    let colour = Color::of(colour)?;
    let name = word(name)?;
    // 一个读不了的 `$space:` 标签是被拒掉而不是被忽略：默认成颜色自己的空间，就会用 hsl 的色相去答
    // `color.channel(pink, "hue", $space: hwb)`。
    let space = match space {
        Some(given) => Some(word(given)?),
        None => None,
    };
    colour.channel(&name, space.as_deref())
}

/// 一个关键词或带引号的字符串，按它拼出的那个词。
fn word(value: &Value) -> Option<String> {
    let Value::Raw(text) = value else {
        return None;
    };
    let text = text.trim();
    let quoted = text
        .strip_prefix('\"')
        .and_then(|rest| rest.strip_suffix('\"'))
        .unwrap_or(text);
    (!quoted.is_empty()).then(|| quoted.to_string())
}

/// `color.alpha`：颜色自己的 alpha，每种表示都带着它。
fn alpha_of(args: Vec<Value>) -> Result<Value, SassError> {
    accessor(args, None, |color| Some(color.alpha()))
}

/// `color.hue(hsl(359, 50%, 50%))` → `359deg`（`hue.hrx`：`359deg`、`180deg`、`0deg`、`123deg`、`0.5deg`、
/// `234deg`——最后两个走 `$color:` 与 `above_max`/`negative` 那两行，它们在创建时就被折叠，因为 `Color::hsl`
/// 把色相 wrap 进 `0–360`）。
fn hue_of(args: Vec<Value>) -> Result<Value, SassError> {
    accessor(args, Some("deg"), Color::hue)
}

/// `color.saturation(hsl(0, 100%, 42%))` → `100%`（`saturation.hrx`：`100%`、`0%`、`50%`、`0.5%`、`42%`）。
fn saturation_of(args: Vec<Value>) -> Result<Value, SassError> {
    accessor(args, Some("%"), Color::saturation)
}

/// `color.lightness(hsl(0, 100%, 42%))` → `42%`（`lightness.hrx`：沿 L 轴是同一批读数）。
fn lightness_of(args: Vec<Value>) -> Result<Value, SassError> {
    accessor(args, Some("%"), Color::lightness)
}

/// `color.grayscale`：饱和度归零，其余全部保住。
fn grayscale_of(args: Vec<Value>) -> Result<Value, SassError> {
    transform(args, |color| {
        color.transform_axes(|[hue, _, lightness]| [hue, 0.0, lightness])
    })
}

/// 一次 `($color, $amount)` 调用：先判实参，再移动轴。
fn adjusted(args: Vec<Value>, change: fn([f64; 3], f64) -> [f64; 3]) -> Result<Value, SassError> {
    // 什么都不带的 `saturate()` 会点名它缺的那个参数——`Error: Missing argument $amount.`
    // （`core_functions/color/saturate.hrx::error/too_few_args`，`darken.hrx` 也是同一句），而登记表那条通用
    // variadic 消息对一个声明了名字的签名说不出来。
    if args.is_empty() {
        return Err(super::missing("amount".to_string()));
    }
    match colour_and_amount(&args) {
        Some((color, amount)) => color
            .transform_axes(|axes| change(axes, amount))
            .map(Value::Color)
            .ok_or_else(no_legacy_axes),
        None => Err(SassError::new("not a colour-and-amount call")),
    }
}

/// `(colour, amount)` 那个形状，或者什么都没有。一个 `amount` 是普通数字、百分比（亮度与饱和度轴）或度
/// （色相轴）——语料为这些调用写下的就是这三个单位。
fn colour_and_amount(args: &[Value]) -> Option<(Color, f64)> {
    let [colour, amount] = args else {
        return None;
    };
    let amount = match amount {
        Value::Number(value, unit)
            if unit.is_unitless() || matches!(unit.single(), Some("%") | Some("deg")) =>
        {
            *value
        }
        _ => return None,
    };
    Some((Color::of(colour)?, amount))
}

/// 一个颜色进、一个颜色出，用的就是访问器那套实参判定：实参全部能读成颜色的就是一个颜色调用（所以一个错的
/// 个数会被报出来），其余任何东西——`grayscale(10%)`，那个 CSS 滤镜——decline 并原样打出。
fn transform(
    args: Vec<Value>,
    change: impl Fn(&Color) -> Option<Color>,
) -> Result<Value, SassError> {
    match access(&args) {
        Access::Colour(color) => change(&color).map(Value::Color).ok_or_else(no_legacy_axes),
        Access::NotAColour => Err(SassError::new("not a colour this slice can transform")),
        Access::TooFew => Err(super::missing("color".to_string())),
        Access::TooMany(passed) => Err(super::too_many(1, passed)),
    }
}

/// [`transform`] 与 [`adjusted`] 共用的那一个 decline 理由：调用点名的是一个 legacy 轴操作，而那个颜色带着
/// 它自己的色彩空间，它的 hsl 轴这一刀还没推导出来（`tasks.md` 9.9 把它记为未闭缺陷）。这条消息再来一份
/// 副本就是一个事实两个答案，而那正是单一真源那条规矩要防的事。
pub(super) fn no_legacy_axes() -> SassError {
    SassError::new("no legacy axes for a colour in its own colour space")
}

/// 每个访问器共用的形状：从颜色实参里读出一个数字，并用 fixture 给那个轴的单位打它——`red.hrx` 是一个裸
/// `255`、`hue.hrx` 是一个 `359deg`、`saturation.hrx` 是一个 `42%`。
///
/// 一个颜色答不了的轴经由同一个非致命错误 decline，于是那个 run 保住自己的文本，而不是打一个从错轴上读出的数字。
/// 今天还属于这一类的是**自带空间**的颜色（[`no_legacy_axes`]）；legacy `rgb` 颜色被问 hsl 轴早已能答（
/// `crate::axes::rgb_to_axes`，金样 `core_functions/modules/color/hue.hrx` 现量 pass=1，守卫
/// `tests/color.rs::rgb_colours_answer_hsl_axis_questions`）。
fn accessor(
    args: Vec<Value>,
    unit: Option<&str>,
    read: impl Fn(&Color) -> Option<f64>,
) -> Result<Value, SassError> {
    let read = match access(&args) {
        Access::Colour(color) => read(&color),
        Access::NotAColour => None,
        Access::TooFew => return Err(super::missing("color".to_string())),
        Access::TooMany(passed) => return Err(super::too_many(1, passed)),
    };
    match read {
        Some(number) => Ok(Value::Number(
            number,
            unit.map_or_else(crate::unit::Unit::unitless, crate::unit::Unit::named),
        )),
        None => Err(SassError::new(
            "not a colour whose axis this slice can read",
        )),
    }
}

/// 一个访问器的实参列表到底是什么：一个颜色、一个本编译器必须放过的调用、还是一个颜色但个数不对。
///
/// 第三种情形不能交给签名：`alpha.hrx::css_overloads/alpha/multi_arg` 写的是
/// `color.alpha(c=d, e=f, g=h)`（一个 Microsoft 滤镜，要逐字打出），而它的形状与 `color.red(red, green)`
/// （一个真的要报出来的误用）**一样**。区分它们的是实参本身——每个实参都读成颜色就是一个颜色调用——
/// 所以判据就是它，而一个建模不了的 run 是 decline 成文本而不是报错。
///
/// **这里没有声称的东西**：两条 arity 行（`red::error/too_few_args`、`…/too_many_args`）仍是红的——0 个实参
/// 在任何函数体跑之前先撞上脊柱自己的那条 variadic 消息，而金样文本不同。所以这些分支的存在是为了把那个
/// 滤镜重载写法留在文本里，不是因为 arity 消息已经满足；那些行和 `error/non_legacy`、`error/type` 一样，
/// 是 P7 的消息工作。
enum Access {
    Colour(Color),
    NotAColour,
    TooFew,
    TooMany(usize),
}

fn access(args: &[Value]) -> Access {
    if args.is_empty() {
        return Access::TooFew;
    }
    match args.iter().map(Color::of).collect::<Option<Vec<Color>>>() {
        Some(colours) if colours.len() == 1 => Access::Colour(colours[0].clone()),
        Some(colours) => Access::TooMany(colours.len()),
        None => Access::NotAColour,
    }
}

/// `color.mix`：两个颜色加一个可选的权重，权重默认 50%。
fn mixed(args: Vec<Value>) -> Result<Value, SassError> {
    let built = match args.as_slice() {
        [one, two] => mix(one, two, 50.0),
        [one, two, weight] => given_weight(weight).and_then(|weight| mix(one, two, weight)),
        _ => None,
    };
    match built {
        Some(color) => Ok(Value::Color(color)),
        None => Err(SassError::new("not a pair this slice can mix")),
    }
}

fn mix(one: &Value, two: &Value, weight: f64) -> Option<Color> {
    Color::mixed(&Color::of(one)?, &Color::of(two)?, weight)
}

/// 权重是**无论单位**的数字（`mix/units.hrx::weight/{unitless,unknown}` 与 `mix/explicit_weight.hrx`：`50`、`50%`、
/// `50px` 都是均匀相混），所以这里不拒任何单位——
/// 只拒非数字，而那正是 `$method:`/`$space:` 那些关键词行与 CSS 重载写法落入的那一类。
fn given_weight(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number, _) => Some(*number),
        _ => None,
    }
}
