//! The `color` domain (`core_functions/color/**`): the accessors and the axis operations, with the value
//! law in [`crate::color`] and the constructors (`rgb`/`rgba`/`hsl`/`hsla`) in [`constructors`].
//!
//! A body that cannot honour its arguments returns a **non-fatal** error, so the run keeps its verbatim
//! text — which is what several fixtures in this domain ask for. `rgb(var(--foo), 3, 0.4)` is the clearest:
//! `var()` is substituted before parsing, so the argument list can hold anything, and the corpus prints the
//! call as written (`rgb/three_args::1_of_3`). `rgb(1px, 2px, 3px)` is another: channels are numbers or
//! percentages, never a `px` quantity, and the answer is an error message this project has not built (P7),
//! not a guess.

use crate::SassError;
use crate::builtins::Builtin;
use crate::color::Color;
use crate::value::Value;

mod amount;
mod constructors;
mod modify;
mod transfer;

use constructors::{COLOR_HWB, COLOR_SPACE, HSL, HSLA, HWB, RGB, RGBA};
use modify::{COLOR_ADJUST, COLOR_CHANGE, COLOR_SCALE, SCALE_COLOR};
use transfer::COLOR_TO_SPACE;

/// This domain's registrations.
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
];

/// `color.mix(#91e16f, #0144bf)` → `rgb(28.6274509804%, 57.4509803922%, 59.2156862745%)`.
///
/// Variadic with names so `$color1:`/`$color2:`/`$weight:` bind (`mix/named.hrx`) while the body judges the
/// count, and the **weight is read as a percentage whatever its unit** — `mix/units.hrx` prints the same
/// result for `50`, `50%` and `50px`. Rows whose call carries `$space:`/`$method:` (`mix/predefined.hrx`,
/// `named::{polar_space,rectangular_space}`, `mixed_spaces`, `hue_interpolation`, `explicit_method`) ask for
/// colour spaces this compiler has not implemented, and they decline to text rather than mix in sRGB and
/// claim the row.
static COLOR_MIX: Builtin = Builtin {
    name: "color.mix",
    params: &["color1", "color2"],
    optional: &["weight"],
    variadic: true,
    labelled: None,
    apply: mixed,
};

/// The legacy hsl manipulators, each `($color, $amount)`. All are **variadic with two names**, for the two
/// reasons the corpus gives: the labels have to bind (`darken.hrx::named` is `darken($color: red, $amount:
/// 14%)`), and a call whose arguments are not a colour plus an amount is not a colour call at all —
/// `saturate.hrx::css_overload/*` keeps `saturate(50%)`, `saturate(1)` and even `saturate($amount: 50%)`
/// exactly as written, because CSS defines `saturate()` as a filter function.
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

/// `lighten(red, 14%)` → `rgb(100%, 28%, 28%)` (`lighten.hrx::middle`).
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

/// `saturate(plum, 100%)` → `#ff7eff` (`saturate.hrx::max`; `max_remaining` with 53% lands on the same
/// colour because saturation clamps at 100). Registered as `($color, $amount)` variadic so the CSS filter
/// spellings survive: `saturate(50%)`, `saturate(1)`, `saturate($c)` and `saturate($amount: 50%)` are all
/// `css_overload/*` rows of that file, and they print as written — with variables substituted and labels
/// dropped, which is the decline path `expr.rs` now rebuilds from evaluated arguments instead of copying
/// the source run (that copy is what made `saturate($c)` measure as a PASS → FAIL regression).
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

/// `desaturate(plum, 100%)` → `rgb(74.7058823529%, …)` (`desaturate.hrx::max`).
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

/// `adjust-hue(…)` moves only the hue; its amount is degrees.
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

/// `grayscale(#633736)` → `rgb(30%, 30%, 30%)`, `grayscale(#494949)` → `#494949`
/// (`core_functions/color/grayscale.hrx`): the saturation goes to zero and the lightness survives the
/// hsl round trip — and the two spellings differ only because one round trip lands on a whole byte.
///
/// Registered variadic like the accessors, because CSS also defines `grayscale(…)` as a filter function:
/// `grayscale(10%)` is not a colour call and must print as written.
static GRAYSCALE: Builtin = Builtin {
    name: "grayscale",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: grayscale_of,
};

/// `color.grayscale(…)`, the namespaced spelling of the same law.
static COLOR_GRAYSCALE: Builtin = Builtin {
    name: "color.grayscale",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: grayscale_of,
};

/// `color.red(…)`, the first channel; likewise green and blue.
///
/// Declared **variadic with one name**, deliberately: `core_functions/color/alpha.hrx::css_overloads/alpha/multi_arg`
/// writes `color.alpha(c=d, e=f, g=h)` — a Microsoft filter that must print verbatim — and a fixed arity
/// raises its `Only 1 argument allowed…` **before** any body runs, which turned that row into an unexpected
/// error. The `ledger failures <run> "unexpected error" --count` reading is what caught it: `net=26` was
/// real, but the invalid-error count had moved 53 → 55. The body now judges the count, so an unmodellable
/// call declines instead of erroring; the arity-message rows stay red and are P7's, not lost to a guess.
static COLOR_RED: Builtin = Builtin {
    name: "color.red",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| channel(args, 0),
};

/// `color.green(…)`, the second channel.
static COLOR_GREEN: Builtin = Builtin {
    name: "color.green",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| channel(args, 1),
};

/// `color.blue(…)`, the third channel.
static COLOR_BLUE: Builtin = Builtin {
    name: "color.blue",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: |args| channel(args, 2),
};

/// `color.alpha(red)` → `1`, `color.alpha(rgba(red, 0.42))` → `0.42`, and `color.alpha($color: …)` binds
/// by name (`core_functions/color/alpha.hrx::color/{max,middle,min,named}`).
static COLOR_ALPHA: Builtin = Builtin {
    name: "color.alpha",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: alpha_of,
};

/// `color.hue(hsl(359, 50%, 50%))` → `359deg`; `hsl(540, …)` → `180deg` — the fold already happened when
/// the colour was built, so the accessor reads it back (`core_functions/color/hue.hrx`). Same variadic
/// shape as the channel accessors, for the same reason.
static COLOR_HUE: Builtin = Builtin {
    name: "color.hue",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: hue_of,
};

/// `color.saturation(hsl(0, 100%, 42%))` → `100%` (`core_functions/color/saturation.hrx`).
static COLOR_SATURATION: Builtin = Builtin {
    name: "color.saturation",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: saturation_of,
};

/// `color.lightness(hsl(0, 100%, 42%))` → `42%` (`core_functions/color/lightness.hrx::named`).
static COLOR_LIGHTNESS: Builtin = Builtin {
    name: "color.lightness",
    params: &["color"],
    optional: &[],
    variadic: true,
    labelled: None,
    apply: lightness_of,
};

/// One channel of a colour argument, as a unitless number.
fn channel(args: Vec<Value>, index: usize) -> Result<Value, SassError> {
    accessor(args, None, |color| color.byte(index))
}

/// `color.channel(#abcdef, "red")` → `171`, `color.channel(hsl(10 40% 60%), "hue")` → `10deg`, and
/// `color.channel(pink, "hue", $space: hsl)` → `349.5238095238deg`
/// (`core_functions/color/channel/{rgb,hsl}.hrx::local/*` and `…::foreign/*`). Declared **variadic with
/// names** like every other colour call here: the arity message is not built yet (P7), and a call this
/// slice cannot read — `color.channel(color(srgb 0.2 0.5 0.8), "red")`, `hwb(…)`, `oklch(…)`, an unknown
/// channel name — must decline to text instead of answering with a number from the wrong axes.
static COLOR_CHANNEL: Builtin = Builtin {
    name: "color.channel",
    params: &["color", "channel"],
    optional: &["space"],
    variadic: true,
    labelled: None,
    apply: channel_of,
};

/// One `color.channel` call: a colour, a **quoted channel name**, and an optional `$space:` written as a bare
/// keyword. The name arrives as text with its quotes — the same shape `math.unit` produces
/// (`Value::Raw("\"px\"")`) — so it is unquoted structurally here rather than re-parsed out of a run.
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
    // An unreadable `$space:` label is refused rather than ignored: defaulting to the colour's own space
    // would answer `color.channel(pink, "hue", $space: hwb)` with the hsl hue.
    let space = match space {
        Some(given) => Some(word(given)?),
        None => None,
    };
    colour.channel(&name, space.as_deref())
}

/// A keyword or quoted string as the word it spells.
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

/// `color.alpha`: the colour's own alpha, which every representation carries.
fn alpha_of(args: Vec<Value>) -> Result<Value, SassError> {
    accessor(args, None, |color| Some(color.alpha()))
}

/// `color.hue(hsl(359, 50%, 50%))` → `359deg` (`hue.hrx`: `359deg`, `180deg`, `0deg`, `123deg`,
/// `0.5deg`, `234deg` — the last two through `$color:` and the `above_max`/`negative` rows, which are
/// folded at creation because `Color::hsl` wraps the hue into `0–360`).
fn hue_of(args: Vec<Value>) -> Result<Value, SassError> {
    accessor(args, Some("deg"), Color::hue)
}

/// `color.saturation(hsl(0, 100%, 42%))` → `100%` (`saturation.hrx`: `100%`, `0%`, `50%`, `0.5%`, `42%`).
fn saturation_of(args: Vec<Value>) -> Result<Value, SassError> {
    accessor(args, Some("%"), Color::saturation)
}

/// `color.lightness(hsl(0, 100%, 42%))` → `42%` (`lightness.hrx`: the same readings along the L axis).
fn lightness_of(args: Vec<Value>) -> Result<Value, SassError> {
    accessor(args, Some("%"), Color::lightness)
}

/// `color.grayscale`: saturation to zero, everything else kept.
fn grayscale_of(args: Vec<Value>) -> Result<Value, SassError> {
    transform(args, |color| {
        color.transform_axes(|[hue, _, lightness]| [hue, 0.0, lightness])
    })
}

/// One `($color, $amount)` call: judge the arguments, then move the axes.
fn adjusted(args: Vec<Value>, change: fn([f64; 3], f64) -> [f64; 3]) -> Result<Value, SassError> {
    // `saturate()` with nothing at all names the parameter it lacks — `Error: Missing argument $amount.`
    // (`core_functions/color/saturate.hrx::error/too_few_args`, and `darken.hrx` says the same), which the
    // registry's generic variadic message could not say for a signature that declares names.
    if args.is_empty() {
        return Err(super::missing("amount".to_string()));
    }
    match colour_and_amount(&args) {
        Some((color, amount)) => Ok(Value::Color(
            color.transform_axes(|axes| change(axes, amount)),
        )),
        None => Err(SassError::new("not a colour-and-amount call")),
    }
}

/// The `(colour, amount)` shape, or nothing. An `amount` is a plain number, a percentage (the lightness
/// and saturation axes) or degrees (the hue axis) — the three units the corpus writes for these calls.
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

/// One colour in, one colour out, with the same argument judgement the accessors make: arguments that all
/// read as colours are a colour call (so a wrong count is reported), and anything else — `grayscale(10%)`,
/// the CSS filter — declines and prints as written.
fn transform(args: Vec<Value>, change: impl Fn(&Color) -> Color) -> Result<Value, SassError> {
    match access(&args) {
        Access::Colour(color) => Ok(Value::Color(change(&color))),
        Access::NotAColour => Err(SassError::new("not a colour this slice can transform")),
        Access::TooFew => Err(super::missing("color".to_string())),
        Access::TooMany(passed) => Err(super::too_many(1, passed)),
    }
}

/// The shape every accessor shares: read one number out of the colour argument and print it with the unit
/// the fixtures give that axis — `red.hrx` a bare `255`, `hue.hrx` a `359deg`, `saturation.hrx` a `42%`.
///
/// An axis the colour does not carry (an `rgb` colour asked for a `hue`, which needs the rgb→hsl conversion
/// that still has no golden) declines through the same non-fatal error, so the run keeps its text rather
/// than printing a number read off the wrong axes.
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

/// What an accessor's argument list is: a colour, a call this compiler must leave alone, or a colour call
/// with the wrong number of arguments.
///
/// The third case cannot be left to the signature: `alpha.hrx::css_overloads/alpha/multi_arg` writes
/// `color.alpha(c=d, e=f, g=h)` (a Microsoft filter, to be printed verbatim) with the **same** shape as
/// `color.red(red, green)` (a real misuse, to be reported). What tells them apart is the arguments
/// themselves — every argument reading as a colour means a colour call — so that is the test, and an
/// unmodellable run declines to text instead of erroring.
///
/// **What this does not claim**: the two arity rows (`red::error/too_few_args`, `…/too_many_args`) are
/// still red — 0 arguments hits the spine's own variadic message before any body runs, and the golden text
/// differs. So these branches exist to keep the filter overload as text, not because the arity messages are
/// satisfied; those rows, like `error/non_legacy` and `error/type`, are P7's message work.
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

/// `color.mix`: two colours and an optional weight, which defaults to 50%.
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

/// A weight is a number **whatever its unit** (`mix/units.hrx`: `50`, `50%` and `50px` all mix evenly), so
/// no unit is rejected here — only a non-number, which is what the `$method:`/`$space:` keyword rows and
/// the CSS-overload spellings fall to.
fn given_weight(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number, _) => Some(*number),
        _ => None,
    }
}
