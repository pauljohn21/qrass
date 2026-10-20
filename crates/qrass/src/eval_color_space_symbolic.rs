//! Bare-`invert()` dispatch and the symbolic-slot comma serialization for the
//! space-separated `hsl`/`hsla` spelling — sibling `impl` methods split from
//! `eval_color_space` for the 500-line budget (round 83).

use crate::ast_expr::{Expr, FunctionCall};
use crate::error::{ErrorKind, SassError};
use super::{Ev, EvalState};
use crate::value::SassValue;

impl EvalState {
        /// The bare global `invert()`: dart disambiguates by argument kind — a
        /// plain-CSS filter call (`invert(10%)`, `invert(var(--c))`) passes through
        /// verbatim (two or more CSS-value arguments are dart's plain-CSS arity
        /// note), while any Sass-side spelling (`invert(c)`, `invert(red)`, a named
        /// `$weight`/`$space`) routes to the `color.invert` builtin verbatim.
        pub(super) fn eval_plain_invert(&mut self, call: &FunctionCall) -> Ev<SassValue> {
            let raw = self.collect_args(&call.args)?;
            let cssish = |v: &SassValue| {
                matches!(v, SassValue::Number { .. }) || super::eval_color4_check::is_symbolic(v)
            };
            if raw.named().is_empty() && raw.positional().iter().all(cssish) {
                if raw.positional().len() == 1 {
                    return Ok(SassValue::Str {
                        text: self.render_css_call(call)?,
                        quoted: false,
                    });
                }
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    "Only one argument may be passed to the plain-CSS invert() function.",
                ));
            }
            let Some(result) = crate::builtins::call(
                Some("color"),
                "invert",
                raw.positional(),
                raw.named(),
            ) else {
                return Ok(SassValue::Str {
                    text: self.render_css_call(call)?,
                    quoted: false,
                });
            };
            result.map_err(|e| e.note(format!("during builtin `invert` in module {}", self.name)))
        }

        /// Comma serialization for a symbolic `hsl`/`hsla` call (see
        /// `eval_space_color`). Slots are recognized by their evaluated value:
        /// a number, `none`, or an unquoted symbolic text (`calc(…)`, `var(…)`,
        /// `env(…)`, `attr(…)` — which covers `string.unquote("calc(2%)")`, the
        /// `calc/string/*` goldens). Anything else (`from`, products, concrete
        /// junk) keeps the verbatim path.
        pub(super) fn try_comma_symbolic_color(
            &mut self,
            call: &FunctionCall,
            channels: &[&Expr],
            alpha: Option<&Expr>,
        ) -> Ev<Option<String>> {
            if !matches!(call.name.as_str(), "hsl" | "hsla") {
                return Ok(None);
            }
            let symbolic = |s: &str| {
                let t = s.to_ascii_lowercase();
                t == "none"
                    || ["calc(", "var(", "env(", "attr("]
                        .iter()
                        .any(|p| t.starts_with(p))
            };
            let mut parts = Vec::with_capacity(4);
            for e in channels.iter().copied().chain(alpha) {
                match self.eval_expr(e)? {
                    v @ SassValue::Number { .. } => parts.push(v.to_css()),
                    SassValue::Str { text, quoted: false } if symbolic(&text) => {
                        parts.push(text);
                    }
                    _ => return Ok(None),
                }
            }
            Ok(Some(format!("{}({})", call.name, parts.join(", "))))
        }
}
