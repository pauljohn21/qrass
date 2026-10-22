//! `color.mix()` / `mix()` helpers split from `builtins/color_ops.rs`
//! for the 500-line budget (round 91): the `$method` validation matrix read
//! off `color/mix/error.hrx` golden input→error pairs only.

use super::Or;
use crate::ast_expr::ListSep;
use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

/// `$method` validation for the mix family (`color/mix/error.hrx`): the
/// spelling is `<space> [<interpolation>] [hue]` as unquoted words; a missing
/// method with a non-legacy operand is rejected per-operand. Rules read off
/// the golden input→error pairs only.
pub(crate) fn validate_mix_method(
    c: &super::Call,
    a: &crate::value::Color,
    b: &crate::value::Color,
) -> Or<()> {
    let err = |m: String| SassError::new(ErrorKind::Builtins, m);
    let method = c.opt("method", Some(3));
    let Some(m) = method else {
        // first non-legacy color without a $method blames, in order
        return [("color1", a), ("color2", b)]
            .iter()
            .find_map(|(v, col)| {
                col.c4.as_ref().map(|c4| {
                    err(format!(
                        "${v}: To use color.mix() with non-legacy color {}, you must provide a $method.",
                        c4.to_css()
                    ))
                })
            })
            .map_or(Ok(()), Err);
    };
    // Flatten the method value into words. Quoted strings blame the whole
    // text; Number items raise dart's not-a-string.
    let words: Vec<String> = match m {
        SassValue::Str { text, quoted: true } => {
            return Err(err(format!(
                "$method: Expected \"{text}\" to be an unquoted string."
            )));
        }
        SassValue::Str { text, .. } => text.split_whitespace().map(str::to_string).collect(),
        SassValue::List { items, sep, .. } => {
            // a Number item blames, everything else flattens to css text
            let parts: Vec<String> = items
                .iter()
                .map(|i| match i {
                    SassValue::Number { .. } => Err(err(format!(
                        "$method: {} is not a string.",
                        i.to_css()
                    ))),
                    other => Ok(other.to_css()),
                })
                .collect::<Or<Vec<String>>>()?;
            let joined = parts.join(match sep {
                ListSep::Slash => "/",
                ListSep::Comma => " ",
                ListSep::Space => " ",
            });
            joined.split_whitespace().map(str::to_string).collect()
        }
        SassValue::Number { .. } => {
            return Err(err(format!("$method: {} is not a string.", m.to_css())));
        }
        _ => return Ok(()),
    };
    if words.is_empty() {
        return Ok(());
    }
    // Slash-free word ladder:
    //   1 word  = space name (a slash-joined text stays one unknown space)
    //   2+ words = space, interpolation method, optional trailing `hue`
    let space = &words[0];
    if words.len() == 1 {
        if crate::color4::lookup(space).is_none() && !matches!(space.as_str(), "rgb" | "hsl" | "hwb") {
            return Err(err(format!("$method: Unknown color space \"{space}\".")));
        }
        return Ok(());
    }
    if words.len() >= 3 && words[2] != "hue" {
        return Err(err(format!(
            "$method: Expected unquoted string \"hue\" at the end of ({} {} {}), was {}.",
            words[0],
            words[1],
            words[2],
            words[2]
        )));
    }
    let interp = &words[1];
    let Some(sp) = crate::color4::lookup(space) else {
        return Err(err(format!("$method: Unknown color space \"{space}\".")));
    };
    if !matches!(
        interp.as_str(),
        "shorter" | "longer" | "increasing" | "decreasing"
    ) {
        return Err(err(format!(
            "$method: Unknown hue interpolation method {interp}."
        )));
    }
    // Rectangular spaces (everything but the polar/legacy-angle ones) reject
    // a hue interpolation method; the enum-name spelling is the golden's.
    if !matches!(sp.name, "hsl" | "hwb" | "lch" | "oklch") {
        let tail = if words.len() >= 3 { " hue" } else { "" };
        return Err(err(format!(
            "$method: Hue interpolation method \"HueInterpolationMethod.{interp}{tail}\" may not be set for rectangular color space {space}."
        )));
    }
    Ok(())
}
