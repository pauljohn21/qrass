//! The hsl axes arithmetic, kept apart from [`crate::color`] because it is pure `f64` in, `f64` out.
//!
//! Two reasons this lives in its own file rather than inside the colour type: the round trip is the only
//! place a colour leaves its own space, and separating it is what lets the *conversion* be tested against
//! goldens without the printing rules mixed in — the printing rule (whole bytes print as numbers or hex,
//! anything between prints as percentages) is a separate law and belongs to [`crate::color`].
//!
//! The formulas are CSS Color 4's; the readings below are what the local corpus pins, and every constant
//! in the doc comments is a fixture row rather than a derivation.

/// `[hue in degrees, saturation in %, lightness in %]` for a colour given as three `0–255` channel values.
///
/// * `red` = `(255, 0, 0)` → `[0, 100, 50]` — the row behind `color.grayscale(red)` → `rgb(50%, 50%, 50%)`
///   (`core_functions/color/grayscale.hrx`: lightness 50 survives, saturation goes to 0).
/// * `#633736` = `(99, 55, 54)` → lightness `30%` of 255, i.e. 76.5, which is what makes
///   `color.grayscale(#633736)` print `rgb(30%, 30%, 30%)` (`grayscale.hrx`).
/// * `purple` = `(128, 0, 128)` → `[300, 100, 25.098039215686]`, the equality
///   `purple == hsl(300, 100%, 25.098039215686%)` in `values/colors/equality.hrx`.
/// * a grey (`span == 0`) has hue `0` and saturation `0` — `color.grayscale(#494949)` stays `#494949`.
pub(crate) fn rgb_to_axes(red: f64, green: f64, blue: f64) -> [f64; 3] {
    let [red, green, blue] = [red / 255.0, green / 255.0, blue / 255.0];
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) / 2.0;
    let span = max - min;
    if span == 0.0 {
        return [0.0, 0.0, lightness * 100.0];
    }
    let saturation = span / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = match max {
        v if v == red => ((green - blue) / span).rem_euclid(6.0) * 60.0,
        v if v == green => ((blue - red) / span + 2.0) * 60.0,
        _ => ((red - green) / span + 4.0) * 60.0,
    };
    [hue, saturation * 100.0, lightness * 100.0]
}

/// The three `0–255` channel values of `[hue in degrees, saturation in %, lightness in %]`.
///
/// Deliberately un-rounded: `(128, 0, 128)` → saturation 0 → `(127.5, 127.5, 127.5)`, and *that* is the
/// reading `color.grayscale(red)` publishes as `rgb(50%, 50%, 50%)` — rounding here would turn a
/// percentage row into a number row and silently change the output.
pub(crate) fn axes_to_rgb(hue: f64, saturation: f64, lightness: f64) -> [f64; 3] {
    let saturation = saturation / 100.0;
    let lightness = lightness / 100.0;
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    // The hue is periodic, so the sector is chosen from its position *inside the turn*: an angle that
    // reaches this conversion past 360 belongs to the sector of `hue mod 360`, and matching on the raw
    // quotient would send 450 to the last arm instead of 90's. The corpus states that wrap in three rows that
    // all answer 540 with `aqua` (`adjust/hsl.hrx::hue/arg_above_max`, `change/hsl.hrx::hue/above_max`,
    // `adjust_hue/above_max.hrx`) and in two that state it with other numbers (`change/lch.hrx::hue/above_max`
    // 400deg → 40deg, `…::hue/negative` -20deg → 340deg; `change/oklch.hrx::hue/above_max` likewise). Every
    // one of them folds the angle on the way in, at `color::angle_of`, so none hands **this** function the
    // past-a-turn value: the caller that does is the whiteness/blackness branch of
    // [`crate::builtins::color::modify`], which adds two already-folded hues and reaches 720.
    let sector = (hue / 60.0).rem_euclid(6.0);
    // Six sectors, two per `second`: folding the turn first changes nothing here, because 6 is a
    // multiple of 2 — the reading stays identical for every hue the callers used to hand over.
    let second = sector.rem_euclid(2.0) - 1.0;
    let intermediate = chroma * (1.0 - second.abs());
    let matched = match sector {
        h if h < 1.0 => [chroma, intermediate, 0.0],
        h if h < 2.0 => [intermediate, chroma, 0.0],
        h if h < 3.0 => [0.0, chroma, intermediate],
        h if h < 4.0 => [0.0, intermediate, chroma],
        h if h < 5.0 => [intermediate, 0.0, chroma],
        _ => [chroma, 0.0, intermediate],
    };
    let offset = lightness - chroma / 2.0;
    matched.map(|component| (component + offset) * 255.0)
}

/// The whiteness and blackness a colour **stores** after a pair that overflows it: they are scaled by their
/// own sum, not clamped — `hwb(0, 101%, 40%)` is `1.01 / 1.41` of the range, and a clamp to 100% would
/// print `71.4285714286%` where the corpus prints `71.6312056863%` (`hwb/four_args::whiteness/above_max`).
/// One source for that rule, shared by [`hwb_to_hsl`], [`Color::hwb`](crate::color::Color::hwb) and
/// [`Color::with_hwb`](crate::color::Color::with_hwb) — a pair of copies is how the two drifted apart last
/// time a law like this was written down twice.
pub(crate) fn normalise_hwb(whiteness: f64, blackness: f64) -> (f64, f64) {
    if whiteness + blackness > 100.0 {
        let span = whiteness + blackness;
        (whiteness / span * 100.0, blackness / span * 100.0)
    } else {
        (whiteness, blackness)
    }
}

/// The hsl axes of a **whiteness/blackness** colour, as `[hue in degrees, saturation in %, lightness in %]`.
///
/// Three readings, each a row of `core_functions/color/hwb/**`. The middle one is why this is not a clamp:
/// * `hwb(0deg, 30%, 40%)` → `[0, 33.3333333333, 45]` (`three_args/units::hue/deg`) — the whiteness *is*
///   the lowest channel and `1 - blackness` the highest, so lightness is their mean and the saturation
///   follows the same rule as [`rgb_to_axes`];
/// * `hwb(0, 101%, 40%)` → `[0, 0, 71.6312056738]` (`four_args::whiteness/above_max`) — together the two
///   exceed the colour, and they are **normalised by their sum**: `1.01 / 1.41 = 0.7163120567…`. Clamping
///   the whiteness to 100% first would print `71.4285714286%` instead, so the fixture distinguishes the
///   two readings and picks this one;
/// * `hwb(0, -1%, 40%)` → `[0, 103.3898305085, 29.5]` (`four_args::whiteness/below_min`) — a negative
///   whiteness is **not** clamped either; it pushes the lightness to `29.5%` and the saturation past 100%.
pub(crate) fn hwb_to_hsl(hue: f64, whiteness: f64, blackness: f64) -> [f64; 3] {
    let (low, high) = normalise_hwb(whiteness, blackness);
    let (low, high) = (low / 100.0, high / 100.0);
    let (min, max) = (low, 1.0 - high);
    let lightness = (min + max) / 2.0;
    let span = max - min;
    // Whiteness and blackness filling the range leave no span: a grey whose saturation is 0 while its hue
    // is kept, which is what `hwb(0, 101%, 40%)` above prints as `hsl(0, 0%, …)`.
    let saturation = if span == 0.0 {
        0.0
    } else {
        span / (1.0 - (2.0 * lightness - 1.0).abs())
    };
    [hue, saturation * 100.0, lightness * 100.0]
}

/// The whiteness and blackness (`0–100`) of an `rgb`-space colour, for a `hwb` read of it.
///
/// `color.channel(pink, "whiteness", $space: hwb)` → `75.2941176471%` and `…"blackness"…` → `0%`
/// (`channel/hwb.hrx::foreign/*`): `pink` is `(255, 192, 203)`, so the whiteness is the lowest channel over
/// 255 and the blackness is what the highest channel leaves below full.
pub(crate) fn rgb_to_hwb(red: f64, green: f64, blue: f64) -> [f64; 2] {
    let (red, green, blue) = (red / 255.0, green / 255.0, blue / 255.0);
    [
        red.min(green).min(blue) * 100.0,
        (1.0 - red.max(green).max(blue)) * 100.0,
    ]
}
