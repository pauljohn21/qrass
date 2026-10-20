//! Pre-passthrough shape checks for the legacy comma-separated color
//! constructors (`hsl`/`hsla`/`rgb`/`rgba` bare, `color.hwb`). Without these
//! the calls fall into the plain-CSS passthrough and emit verbatim text while
//! the goldens demand builtins-style errors —
//! `core_functions/color/{hsl,rgb,hwb}/error/{four_args,five_args}/*`.
//!
//! The rule set is read off the golden input→error pairs only:
//! - more than four positional arguments: `Only 4 arguments allowed, but N
//!   were passed.` (`five_args`);
//! - a quoted-string channel slot: `$saturation: "foo" is not a number.`
//!   (`hsl/rgb four_args` `type/error`);
//! - alpha with a foreign unit: `$alpha: Expected 0.5px to have unit "%" or
//!   no units.` (`four_args` `alpha/unit/error`);
//! - `color.hwb` diverges in both spellings: `Expected whiteness channel to
//!   be a number, was "foo".` plus percent-forced w/b units
//!   (`$whiteness: Expected 30 to have unit "%".`).

use crate::ast_expr::{Arg, Expr, FunctionCall, Literal};
use crate::error::{ErrorKind, SassError};

fn err(msg: impl Into<String>) -> SassError {
    SassError::new(ErrorKind::Builtins, msg)
}

/// True for argument ASTs the builtins-style check must not judge (symbolic
/// `calc()`/`var()` calls, variables, `none`, bare identifiers all keep the
/// historical verbatim passthrough).
fn skip_slot(e: &Expr) -> bool {
    match e {
        Expr::Literal(Literal::Ident(_)) => true,
        Expr::Literal(Literal::Number { .. }) => false,
        Expr::FunctionCall(_) | Expr::Variable(..) => true,
        Expr::Binary { .. } | Expr::Unary { .. } | Expr::Paren(..) => true,
        _ => true,
    }
}

fn shown(e: &Expr) -> String {
    match e {
        Expr::Literal(Literal::Number { value, unit }) => {
            let u = unit.as_deref().unwrap_or("");
            let v = if *value == value.round() && value.abs() < 1e15 {
                format!("{}", *value as i64)
            } else {
                format!("{}", value)
            };
            format!("{v}{u}")
        }
        Expr::Literal(Literal::Str { text, quote }) => match quote {
            crate::token::Quote::Double => format!("\"{text}\""),
            crate::token::Quote::Single => format!("'{text}'"),
        },
        _ => String::new(),
    }
}

/// Check a bare `hsl/hsla/rgb/rgba` comma call (3+ positional args).
pub(crate) fn check_legacy_comma_color(call: &FunctionCall) -> Option<SassError> {
    let n = call.args.len();
    if n > 4 {
        return Some(err(format!(
            "Only 4 arguments allowed, but {n} were passed."
        )));
    }
    let names: &[&str; 4] = match call.name.as_str() {
        "rgb" | "rgba" => &["red", "green", "blue", "alpha"],
        _ => &["hue", "saturation", "lightness", "alpha"],
    };
    for (i, arg) in call.args.iter().enumerate() {
        let Arg::Positional(e) = arg else {
            continue;
        };
        // A quoted string is concrete in any slot: the type blame precedes the
        // skip filter (`four_args` `type/error`).
        if let Expr::Literal(Literal::Str { text, .. }) = e {
            return Some(err(format!(
                "${}: \"{text}\" is not a number.",
                names[i.min(3)]
            )));
        }
        if skip_slot(e) {
            continue;
        }
        // Slot 4 is alpha: `%` or unitless only.
        if i == 3
            && let Expr::Literal(Literal::Number { unit, .. }) = e
            && !matches!(unit.as_deref(), None | Some("%")) {
            return Some(err(format!(
                "$alpha: Expected {} to have unit \"%\" or no units.",
                shown(e)
            )));
        }
    }
    None
}

/// Check a four-positional `color.hwb` comma call.
pub(crate) fn check_color_hwb_comma(call: &FunctionCall) -> Option<SassError> {
    if call.args.len() != 4 {
        return None;
    }
    for (i, key) in [(1usize, "whiteness"), (2, "blackness")] {
        let Arg::Positional(e) = &call.args[i] else {
            continue;
        };
        if let Expr::Literal(Literal::Str { text, .. }) = e {
            return Some(err(format!(
                "Expected {key} channel to be a number, was \"{text}\"."
            )));
        }
        if !skip_slot(e) {
            continue;
        }
    }
    // w/b units: percent-forced (`$whiteness: Expected 30 to have unit "%".`).
    for (i, key) in [(1usize, "whiteness"), (2, "blackness")] {
        if let Arg::Positional(e @ Expr::Literal(Literal::Number { unit, .. })) = &call.args[i]
            && unit.as_deref() != Some("%") {
            return Some(err(format!(
                "${key}: Expected {} to have unit \"%\".",
                shown(e)
            )));
        }
    }
    None
}
