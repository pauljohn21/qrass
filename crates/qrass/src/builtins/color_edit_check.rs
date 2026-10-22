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
    let (domain, domain_channels): (&str, &[&str]) =
        match (edited(&RGB), edited(&HSL), edited(&HWB)) {
            (true, _, _) => ("rgb", &RGB),
            (false, true, _) => ("hsl", &HSL),
            (false, false, true) => ("hwb", &HWB),
            _ => ("rgb", &[]),
        };
    // one validator per named channel, collected through the error union
    names.iter().map(|key| -> Or<()> {
        let k = key.as_str();
        let value = c.named.get(k);
        match k {
            "alpha" => match value {
                // `$alpha` is not exempt from typing: anything but a number —
                // including `none` — raises `$alpha: … is not a number.`
                // (`scale/error/type` `none/error`).
                Some(a) if !matches!(a, SassValue::Number { .. }) => Err(err(format!(
                    "$alpha: {} is not a number.",
                    a.to_css()
                ))),
                _ => Ok(()),
            },
            "space" | "channels" | "color" | "color1" | "color2" | "weight" | "method" => {
                // `$space:` on a legacy color must still be an unquoted string
                // (`scale/error/space` `quoted/error`).
                match value {
                    Some(SassValue::Str { text, quoted: true }) if k == "space" => Err(err(
                        format!("$space: Expected \"{text}\" to be an unquoted string."),
                    )),
                    _ => Ok(()),
                }
            }
            _ if !domain_channels.contains(&k) => Err(err(format!(
                "${k}: Color space {domain} doesn't have a channel with this name."
            ))),
            _ => {
                let Some(value) = value else { return Ok(()) };
                // Legacy hwb-family channels are percent-only in every mode
                // (`adjust/error/units/hwb`: `$blackness: Expected 1 to have unit "%".`).
                let hwb_bad = matches!(k, "whiteness" | "blackness")
                    && matches!(value, SassValue::Number { unit, .. } if unit.as_deref() != Some("%"));
                if hwb_bad {
                    return Err(err(format!(
                        "${k}: Expected {} to have unit \"%\".",
                        value.to_css()
                    )));
                }
                // A `none` edit is the missing-slot path (handled by the edit loops /
                // deferred legacy missing modeling), not a type error.
                if matches!(value, SassValue::Str { text, quoted: false } if text == "none") {
                    return Ok(());
                }
                let SassValue::Number { value: v, unit: _ } = value else {
                    return Err(err(format!("${k}: {} is not a number.", value.to_css())));
                };
                // No unit *category* checks here: `adjust/units` golden-wise unknown
                // units (`60in`) pass through taking the magnitude (round-69
                // adjust-hue semantics), and the few real unit errors already surface
                // from the arithmetic layer.
                let scale_err = match mode {
                    Mode::Scale if k == "hue" => {
                        Some("$hue: Channel isn't scalable.".to_string())
                    }
                    Mode::Scale if *v < -100.0 || *v > 100.0 => Some(format!(
                        "${k}: Expected {} to be within -100% and 100%.",
                        value.to_css()
                    )),
                    _ => None,
                };
                scale_err.map_or(Ok(()), |m| Err(err(m)))
            }
        }
    })
    .collect::<Or<Vec<()>>>()?;
    Ok(())
}

