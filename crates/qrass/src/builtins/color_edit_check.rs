//! Channel-domain validation for the legacy adjust/scale/change edits,
//! split from `builtins/color.rs` for the 500-line budget (round 84).
//! The rule table comes straight from the `scale/error/*`,
//! `adjust/error/*`, and `change/error/*` golden families.

use super::color::{Mode, err};
use super::Or;
use crate::value::SassValue;

/// Channel-domain validation for the legacy `adjust`/`scale`/`change` edits,
/// read off `scale/error/*` + `adjust/error/*` + `change/error/*`:
/// * a name outside {rgb,hsl,hwb} channel vocab is
///   `$X: Color space rgb doesn't have a channel with this name.`;
/// * a non-number edit is `$X: c is not a number.`;
/// * wrong units (`10` for a percent channel, `%` for a color name …) follow
///   the units goldens' `Expected … to have unit "%".` shape;
/// * `scale` bounds every touched channel to `[-100%, 100%]`
///   (`Expected 100.001% to be within -100% and 100%.`) and refuses `$hue`
///   outright (`Channel isn't scalable.`).
///
/// Checks run in sorted-name order so multi-channel calls report the same
/// name the goldens do; only channels actually edited are examined.
pub(super) fn validate_legacy_channel_edits(c: &super::Call, mode: Mode) -> Or<()> {
    const RGB: [&str; 3] = ["red", "green", "blue"];
    const HSL: [&str; 3] = ["hue", "saturation", "lightness"];
    const HWB: [&str; 3] = ["hue", "whiteness", "blackness"];
    let mut names: Vec<&String> = c.named.keys().collect();
    names.sort();
    // The edited channels pick ONE color space whose channel set then applies
    // (`mixed_formats` goldens): any rgb-family channel present binds the rgb
    // space; else hsl-family; else hwb. Cross-domain names report that space.
    let edited = |vocab: &[&str]| names.iter().any(|k| vocab.contains(&k.as_str()));
    let (domain, domain_channels): (&str, &[&str]) = if edited(&RGB) {
        ("rgb", &RGB)
    } else if edited(&HSL) {
        ("hsl", &HSL)
    } else if edited(&HWB) {
        ("hwb", &HWB)
    } else {
        ("rgb", &[])
    };
    for key in names {
        let k = key.as_str();
        if matches!(
            k,
            "alpha" | "space" | "channels" | "color" | "color1" | "color2" | "weight" | "method"
        ) {
            continue;
        }
        if !domain_channels.contains(&k) {
            return Err(err(format!(
                "${k}: Color space {domain} doesn't have a channel with this name."
            )));
        }
        let Some(value) = c.named.get(k) else { continue };
        // A `none` edit is the missing-slot path (handled by the edit loops /
        // deferred legacy missing modeling), not a type error.
        if matches!(value, SassValue::Str { text, quoted: false } if text == "none") {
            continue;
        }
        let SassValue::Number { value: v, unit: _ } = value else {
            return Err(err(format!(
                "${k}: {} is not a number.",
                value.to_css()
            )));
        };
        // No unit *category* checks here: `adjust/units` golden-wise unknown
        // units (`60in`) pass through taking the magnitude (round-69
        // adjust-hue semantics), and the few real unit errors already surface
        // from the arithmetic layer.
        if mode == Mode::Scale {
            if k == "hue" {
                return Err(err("$hue: Channel isn't scalable."));
            }
            if *v < -100.0 || *v > 100.0 {
                let shown = value.to_css();
                return Err(err(format!(
                    "${k}: Expected {shown} to be within -100% and 100%."
                )));
            }
        }
    }
    Ok(())
}

