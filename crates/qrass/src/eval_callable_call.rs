//! `meta.call` dispatch for [`EvalState`] — a sibling `impl` split out
//! of `eval_callable` for the 500-line budget (round 74). Invoking a
//! first-class function value (or its name string, the deprecated
//! `call-string` spelling) through the builtin table or the user-function
//! closure; the target's own errors surface verbatim.


use crate::error::{ErrorKind, SassError};
use crate::value::{FnKind, SassValue};

use super::eval_callable::{err, opt};
use super::eval_mixin::RawArgs;
use super::{Ev, EvalState};

impl EvalState {
    /// The `"call"` arm body; `Ok(None)` is never produced here.
    pub(super) fn run_call(&mut self, raw: &RawArgs) -> Ev<Option<SassValue>> {
        let Some(target) = opt(raw, "function", 0).cloned() else {
            return Err(err("Missing argument $function."));
        };
        let resolved = match &target {
            SassValue::Function(f) => Some(f.clone()),
            SassValue::Str { .. } => match self.find_function(
                &match target {
                    SassValue::Str { ref text, .. } => text.clone(),
                    _ => unreachable!(),
                },
                &match target {
                    SassValue::Str { ref text, .. } => text.replace('_', "-"),
                    _ => unreachable!(),
                },
                None,
                false,
            )? {
                Some(SassValue::Function(f)) => Some(f),
                _ => None,
            },
            _ => None,
        };
        let Some(f) = resolved else {
            return Err(if matches!(target, SassValue::Str { .. }) {
                err(format!("Function not found: {}", target.to_css()))
            } else {
                SassError::new(
                    ErrorKind::Builtins,
                    format!("$function: {} is not a function reference.", target.to_css()),
                )
            });
        };
        // Peel the head argument off the positional list; when the
        // function came from the named pool instead, every positional
        // is a sub-argument.
        let skip_head = raw
            .positional()
            .first()
            .is_some_and(|v| v == &target);
        let positional: Vec<SassValue> = if skip_head {
            raw.positional()[1..].to_vec()
        } else {
            raw.positional().to_vec()
        };
        let named: std::collections::HashMap<String, SassValue> = raw
            .named()
            .iter()
            .filter(|(k, _)| k.as_str() != "function")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let value = match &f.kind {
            FnKind::Css => {
                let mut parts: Vec<String> =
                    positional.iter().map(|v| v.to_css()).collect();
                for (k, v) in &named {
                    parts.push(format!("{k}: {}", v.to_css()));
                }
                return Ok(Some(SassValue::Str {
                    text: format!("{}({})", f.name, parts.join(", ")),
                    quoted: false,
                }));
            }
            FnKind::Builtin { namespace } => crate::builtins::call(
                namespace.as_deref(),
                &f.name,
                &positional,
                &named,
            )
            .ok_or_else(|| err(format!("Function not found: {}", f.name)))?
            .map_err(|e| {
                e.note(format!("during call of `{}` in module {}", f.name, self.name))
            })?,
            FnKind::User { decl, env, owner } => {
                let sub = RawArgs::from_parts(positional, named);
                self.invoke_user_fn(decl, env.clone(), owner.clone(), sub)?
            }
        };
        Ok(Some(value))
    }
}
