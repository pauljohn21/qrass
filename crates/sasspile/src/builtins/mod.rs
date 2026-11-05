//! 内置函数登记表（tasks 1.7a）。每个函数一个 `Builtin`：它的调用名逐字照写、它的参数名、它的函数体。
//! 实参绑定与两条 arity 诊断都由签名给出而不是逐个函数各写一份，因为语料对它们是同一条说法：
//! `Missing argument $number2.` / `Only 2 arguments allowed, but 3 were passed.`
//! (`core_functions/math/div.hrx::error/*`)。
//!
//! 一个无法照实参兑现的函数体返回一个**非致命**错误，调用方退回该 run 的逐字文本。这是刻意的：
//! `math.div(6in, 3s)` 产出一个复合单位（`2in/s`），本编译器还不建模它，而猜一个标量就会在 Sass 照样
//! 能除下去的地方打出一个错数。
//!
//! ## 布局
//!
//! 本文件**只是脊柱**：契约、分组的登记表、分派。各个域的函数体住在它旁边（今天是 `math.rs`；`color.rs`
//! 随 1.5 到来），因为一个装下所有域的文件在色彩落地那一刻就会越过本项目 500 行的上限。分组是一张域的
//! 列表，所以多加一个域就是这里加一行、那里加一个文件——不重排任何东西、不改任何名字：对外只有
//! [`signature`] 与 [`evaluate`]，由 `calls`、`brackets` 与 `expr` 使用。

use crate::unit::Unit;
use crate::value::Value;
use crate::{ErrorKind, SassError};

mod color;
mod math;

/// 脊柱交给一个**认标签**的函数体的实参：它**绑定到哪个参数**（对一个不声明名字的签名的位置实参写
/// `""`）与那个值。
///
/// 名字留在这里，是因为一个要改动某通道的函数体必须被告知是*哪一个*：`color.change(black, $red: 500)` 与
/// `color.change(black, $green: -50)` 各传一个数，打出的却是 `hsl(0, 5000%, 98.0392156863%)` 和
/// `hsl(120, 100%, -9.8039215686%)`（`change/rgb.hrx::{red,green}/out_of_range`）。把名字丢掉，正是这两个
/// 调用在函数体那里无从区分的原因。
#[derive(Debug, Clone)]
pub struct Given {
    pub param: &'static str,
    pub value: Value,
    /// 这次调用是否为该参数**写了 `$label:`**，而不是按位置填进去的。`color.change`/`color.adjust`/`color.scale`
    /// 只接受恰好一个位置实参——那个颜色，其余一律要求按名字传，原话是
    /// `Only one positional argument is allowed. All other arguments must be passed by name.`
    /// （`change/error/args.hrx::too_many`）；而这个区别无法从绑定后的值倒推：`color.change(red, 1)` 与
    /// `color.change($color: red, $red: 1)` 会落到同样的槽位上。
    pub label: bool,
}

impl Given {
    /// 只要值本身，供那些“读实参时不依赖它填了哪个参数”的函数体使用。
    pub fn values(given: Vec<Self>) -> Vec<Value> {
        given.into_iter().map(|argument| argument.value).collect()
    }

    /// 有多少实参是**按位置**写的——颜色调用那条“只能有一个位置实参、其余必须按名字传”的规则判的就是这个数，
    /// 也是 [`Given::label`] 存在的原因（逐字拒语见上面的 `Only one positional argument …`）。
    pub fn positional(given: &[Self]) -> usize {
        given.iter().filter(|argument| !argument.label).count()
    }

    /// 这次调用点名给了某个参数的那个值，如果点了名的话。
    pub fn find<'a>(given: &'a [Self], param: &str) -> Option<&'a Value> {
        given
            .iter()
            .find(|argument| argument.param == param)
            .map(|argument| &argument.value)
    }

    /// 这次调用有没有点名这个参数——一个 `change` 函数体对每个通道都要问的问题，因为只有被点名的那些才动。
    pub fn names(given: &[Self], param: &str) -> bool {
        given.iter().any(|argument| argument.param == param)
    }
}

/// 一个**按参数**而不是按位置读实参的函数体。
///
/// 之所以给它命名，是因为 `Option<fn(Vec<Given>) -> Result<Value, SassError>>` 正是 clippy 有理由要求抽走的
/// 那类类型，也因为两个入口的差别不止形状：[`Given`] 才让 `color.change` 分得出 `$red:` 与 `$hue:`。
type LabelledBody = fn(Vec<Given>) -> Result<Value, SassError>;

/// 一个内置函数的契约。`params` 是一个 `$label:` 实参能绑定到的标签，按声明顺序。
///
/// `variadic` 函数（`math.min`/`math.max`）改为接受任意多个位置实参：`params` 为空，而语料自己的消息顶替
/// 那两条定长诊断——`At least one argument must be passed.`，以及类型不对时的 `c is not a number.`，后者
/// **不带** `$parameter:` 前缀，因为压根没有参数可点名（`core_functions/math/{min,max}.hrx::error/*`）。
#[derive(Debug)]
pub struct Builtin {
    pub name: &'static str,
    pub params: &'static [&'static str],
    /// 一次调用可以留下的参数，按声明顺序排在 `params` 之后。`rgb` 需要它，因为语料**两种**都收：
    /// `rgb(18, 52, 86)` 与 `rgb(0, 0, 0, 0.5)`（`rgb/four_args/alpha.hrx`）——定长 arity 会在第四个实参处
    /// 拒收，而 `variadic` 签名会丢掉 `$red:`/`$green:`/`$blue:` 这些标签（`rgb/three_args/unitless.hrx::named`）。
    pub optional: &'static [&'static str],
    pub variadic: bool,
    pub apply: fn(Vec<Value>) -> Result<Value, SassError>,
    /// 一个必须知道**每个实参绑到哪个参数**的函数体。目前写下的每个函数体这里都是 `None`，它们按位置读
    /// 实参；`color.change` 是第一个不认标签就答不出问题的调用（`$red: 500` 与 `$green: -50` 各是一个数，
    /// 打出的颜色却完全不同），它设置的是这个字段而不是 `apply`。
    ///
    /// 这个字段之所以是可选的、而不是把 `apply` 的实参类型改掉，是刻意的：为了放开一个函数而把所有既有函数体
    /// 改成穿过一层 newtype 去过，本会话已经因此走过两条弯路；而一个纯增字段让所有既有函数体——以及所有
    /// 既有 fixture 的读数——都不动，这正是 `ledger diff` 能证明的事。
    pub labelled: Option<LabelledBody>,
}

/// 每个域的登记表。按顺序搜索，而名字跨域唯一——重名会悄悄遮蔽，所以 [`signature`] 的第一个匹配就是唯一
/// 要紧的规矩。
static BUILTINS: &[&[&Builtin]] = &[math::MATHS, color::COLORS];

/// 给定被调名时的签名，如果我们登记过。不认识的名字（`color.contrast`、`var`、`map.merge`）是 `None`，
/// 于是 `calls::known_call` 让那个 run 保持它今天的样子。
///
/// 取名 `signature` 而不是 `resolve`：`fn resolve(` 是 `sasslint` 的一条禁令，记录的是被删掉的拉式引擎的
/// 递归下降 resolver，而这次查找根本不是那回事——为了迁就一个新名字去放松闸，是错的取舍。
pub fn signature(name: &str) -> Option<&'static Builtin> {
    BUILTINS
        .iter()
        .flat_map(|domain| domain.iter())
        .copied()
        .find(|builtin| builtin.name == name)
}

/// **一个内置模块的常数**，按 `module.$name` 寻址——`math.$pi`、`math.$epsilon`
/// （`core_functions/math/variables.hrx`）。只有拥有内置成员的命名空间会答：`utils.$empty-map` 与
/// `upstream.$a` 点名的是从文件加载的模块，那是 P5 的事，在这里解析等于对本编译器从不读的文件做猜测。
/// 调用方在 `None` 处 decline 并保留源文本。
pub fn module_value(module: &str, name: &str) -> Option<Value> {
    (module == "math")
        .then(|| math::constant(name))
        .flatten()
        .map(|value| Value::Number(value, Unit::unitless()))
}

/// 把给定的实参绑定到签名，然后应用。位置实参按顺序填参数；一个 `$label:` 实参填它命名的那个参数，
/// 不管写在哪儿（`math.div($number2: 3, $number1: 6)` → `2`）。
pub fn evaluate(name: &str, given: Vec<(Option<String>, Value)>) -> Result<Value, SassError> {
    let builtin =
        signature(name).ok_or_else(|| SassError::new(format!("unknown builtin {name}")))?;
    // 一个 variadic 签名收下所有位置实参。它唯一那条 arity 规则来自语料自己：一个实参都没有才是错误。
    // 当它同时声明了名字——`rgb` 就是这么声明的，好让 `$red:`/`$green:`/`$blue:` 能绑定
    // （`rgb/three_args/unitless.hrx::named`）——这个 run 会落到下面共享的绑定处，那里跳过必填参数检查；由函数体
    // 自己决定它接受几个实参，因为 `rgb` 是真重载（`rgb(#123, 1)` 是两个实参，`rgb(1, 2, 3)` 是三个）。
    if builtin.variadic {
        // 一个**不**声明名字的 variadic 把所有实参都按位置收，所以空调用没有参数能在消息里点名：
        // `At least one argument must be passed.`（`core_functions/math/{min,max}.hrx::error/too_few_args`，
        // 以及 `core_functions/math/hypot.hrx::error/zero_args`——`hypot` 给这个情形用的用例名不同，消息却是
        // 同一句 `At least one argument must be passed.`）。声明了名字的那些则让空调用进到函数体，因为函数体
        // 知道缺的是哪个参数——`color.change()` 报 `Missing argument $color.`
        // （`change/error/args.hrx::too_few`）、`saturate()` 报 `Missing argument $amount.`
        // （`saturate.hrx::error/too_few_args`），这两句都是这条通用消息说不出来的。
        if given.is_empty() && builtin.params.is_empty() && builtin.optional.is_empty() {
            return Err(at_least_one());
        }
        if builtin.params.is_empty() && builtin.optional.is_empty() {
            return given
                .into_iter()
                .map(|(label, value)| match label {
                    // 与下面那条检查同一个意思：有标签却无参数可绑就是拒收，而 kind 正是让调用方不去把
                    // `$number1` 当变量解析的东西。
                    Some(_) => Err(SassError::typed(
                        ErrorKind::UnknownParameter,
                        "named arguments have no parameter to bind to here",
                    )),
                    None => Ok(value),
                })
                .collect::<Result<Vec<Value>, SassError>>()
                .and_then(|bound| (builtin.apply)(bound));
        }
    }
    let positional = given
        .iter()
        .filter(|(label, _)| label.is_none())
        .map(|(_, value)| value.clone())
        .collect::<Vec<Value>>();
    let named = given
        .iter()
        .filter_map(|(label, value)| label.as_ref().map(|name| (name.as_str(), value)))
        .collect::<Vec<(&str, &Value)>>();
    let allowed = builtin.params.len() + builtin.optional.len();
    if !builtin.variadic && given.len() > allowed {
        return Err(too_many(allowed, given.len()));
    }
    // 一个 `$label:` 如果点的是**这个签名里没有的参数**，就不该由这个内置函数来答。
    // dart-sass 报 `No $x parameter.`；在 P7 造出那句话之前，这个 run decline 并保留自己的文本。把这件事
    // 放在这里而不是每个函数体里，就是要点所在：不这么做的话，一个带标签的实参会被下面的绑定直接丢掉，
    // 于是 `color.mix(#a, #b, $method: oklab)` 走到了 legacy sRGB 混合器上，答出一个对别的色彩空间的请求——
    // 而这是一个错值，pass 数看不见，因为那些行（`mix/explicit_method`、`named::polar_space`、
    // `hue_interpolation`）本来就没过。
    if named
        .iter()
        .any(|(label, _)| !builtin.params.contains(label) && !builtin.optional.contains(label))
    {
        return Err(SassError::typed(
            ErrorKind::UnknownParameter,
            "this signature has no parameter with that name",
        ));
    }
    // 每个名字一个槽位，先按标签填、再按位置填，所以一个顺序写错的可选参数（`rgb(1, 2, 3, $alpha: 0.5)`）
    // 仍然绑到 alpha。参数的名字跟着值一起走，因为认标签的函数体会问这次调用点名了哪个通道。
    let slots = builtin
        .params
        .iter()
        .chain(builtin.optional)
        .enumerate()
        .map(|(index, param)| {
            let label = named.iter().any(|(name, _)| name == param);
            let value = named
                .iter()
                .find(|(label, _)| label == param)
                .map(|(_, value)| (*value).clone())
                .or_else(|| positional.get(index).cloned());
            (*param, value, label)
        })
        .collect::<Vec<(&'static str, Option<Value>, bool)>>();
    // 到第一个缺席者为止的每个已声明参数都交给一个**定长**函数体，好让它永远看不见洞；而一个必填参数被留下
    // 缺席，用的就是语料自己的消息（`core_functions/math/div.hrx::error/too_few_args` → `Missing argument $number2.`）。
    let taken = slots
        .iter()
        .position(|(_, value, _)| value.is_none())
        .unwrap_or(slots.len());
    if taken < builtin.params.len() && !builtin.variadic {
        return Err(missing(builtin.params[taken].to_string()));
    }
    // 一个 **variadic** 签名让函数体自己判个数，所以函数体看见每一个拿到值的参数，按声明顺序。在第一个洞处
    // 裁掉（定长那条规矩）正是让绑到前部槽位之外的标签被悄悄丢掉的原因：`rgb($channels: 0 255 127)`
    // （`rgb/one_arg/no_alpha.hrx::unitless/named`，它要的是 `rgb(0, 255, 127)`）到达时变成了完全没有实参。
    // 位置调用不受影响——没有洞时两条规矩给出同一张列表。
    let kept = if builtin.variadic {
        slots
    } else {
        slots.into_iter().take(taken).collect()
    };
    let given = kept
        .into_iter()
        .filter_map(|(param, value, label)| {
            Some(Given {
                param,
                label,
                value: value?,
            })
        })
        .collect::<Vec<Given>>();
    match builtin.labelled {
        Some(labelled) => labelled(given),
        // 每个既有函数体读的那个位置视图。
        None => (builtin.apply)(Given::values(given)),
    }
}

/// `Error: At least one argument must be passed.` ——**不**声明名字的 variadic 的下限，也就是 [`evaluate`] 里
/// 那道守卫唯一能走到的形状；声明了名字的 variadic 会改报它缺的那个参数，它那几行写在那道守卫的注释里，
/// 而不是抄到这里。这条消息住在它的属主那里，好让函数体永远不必自己断言下限：读第一个实参的那两个
/// 过去用 `expect`，那会把一次未来的签名改动（给这样的 variadic 加 `optional: &["numbers"]`）变成 panic
/// 而不是这条消息。
pub(super) fn at_least_one() -> SassError {
    SassError::typed(
        ErrorKind::MissingArgument,
        "At least one argument must be passed.",
    )
}

/// `Error: Missing argument $number2.` ——第一个没有值的参数。
pub(super) fn missing(param: String) -> SassError {
    SassError::typed(
        ErrorKind::MissingArgument,
        format!("Missing argument ${param}."),
    )
}

/// `Error: Only 2 arguments allowed, but 3 were passed.` 两个数各自独立决定英文单复数，正如语料所示
/// （`Only 1 argument allowed, but 2 were passed.` 与 `Only 0 arguments allowed, but 1 was passed.`）。
pub(super) fn too_many(allowed: usize, passed: usize) -> SassError {
    SassError::typed(
        ErrorKind::TooManyArguments,
        format!(
            "Only {} argument{} allowed, but {} {} passed.",
            allowed,
            if allowed == 1 { "" } else { "s" },
            passed,
            if passed == 1 { "was" } else { "were" }
        ),
    )
}
