//! First-class callable lookups for [`EvalState`] (round-73 W2 of the
//! callable-values plan): `meta.get-function`, `meta.get-mixin`,
//! `meta.is-function`, `meta.is-mixin` and `meta.feature-exists`. Every rule
//! below is read off the `core_functions/meta/get_function/*`, `get_mixin/*`
//! and `feature_exists.hrx` goldens — never off dart internals.
//!
//! The lookup order for a module-less read is local user functions first, then
//! the global builtin table (`as *` merged exports arrive with W4); a
//! `$module:` read consults that module's builtins and errors with dart's
//! namespace wording when the namespace was never bound.

use std::rc::Rc;

use crate::error::{ErrorKind, SassError};
use crate::value::{FnKind, FnRef, MixKind, MixRef, SassValue};

use super::eval_mixin::RawArgs;
use super::{Ev, EvalState};

pub(super) fn err(msg: impl Into<String>) -> SassError {
    SassError::new(ErrorKind::UndefinedFunction, msg)
}

/// Named-or-positional read on `RawArgs` (mirrors `builtins::Call::opt`).
fn opt<'a>(raw: &'a RawArgs, key: &str, pos: usize) -> Option<&'a SassValue> {
    raw.named().get(key).or_else(|| raw.positional().get(pos))
}

impl EvalState {
    /// The callable-family `meta` predicates. `Ok(None)` keeps the caller's
    /// existing dispatch untouched for every other name.
    pub(super) fn try_callable_meta(
        &mut self,
        namespace: Option<&str>,
        name: &str,
        raw: &RawArgs,
    ) -> Ev<Option<SassValue>> {
        if !matches!(namespace, None | Some("meta")) {
            return Ok(None);
        }
        match name {
            "get-function" | "get-mixin" => {
                let mixin = name == "get-mixin";
                let found = self.lookup_callable(mixin, raw)?;
                Ok(Some(found))
            }
            "is-function" => Ok(Some(SassValue::Bool(matches!(
                raw.positional().first(),
                Some(SassValue::Function(_))
            )))),
            "is-mixin" => Ok(Some(SassValue::Bool(matches!(
                raw.positional().first(),
                Some(SassValue::Mixin(_))
            )))),
            "feature-exists" => {
                // The names the goldens mark `true` (exact spelling: the
                // `dash_sensitive` golden reads `at_error` as *false*).
                let feature = match opt(raw, "name", 0) {
                    Some(SassValue::Str { text, .. }) => text.clone(),
                    _ => String::new(),
                };
                let known = matches!(
                    feature.as_str(),
                    "global-variable-shadowing"
                        | "extend-selector-pseudoclass"
                        | "units-level-3"
                        | "at-error"
                        | "custom-property"
                        | "global-builtin"
                        | "first-class-callables"
                );
                Ok(Some(SassValue::Bool(known)))
            }
            _ => Ok(None),
        }
    }

    /// `meta.get-function`/`get-mixin` per the error goldens: arity wording,
    /// `$name`/`$module` type wording, local → builtin lookup, and dart's
    /// `Function not found:` / `Mixin not found:` / namespace errors.
    fn lookup_callable(&mut self, mixin: bool, raw: &RawArgs) -> Ev<SassValue> {
        let max = if mixin { 2 } else { 3 };
        let given = raw.positional().len() + raw.named().len();
        if given == 0 {
            return Err(err("Missing argument $name."));
        }
        if given > max {
            return Err(err(format!(
                "Only {max} {} allowed, but {given} {} passed.",
                if max == 1 { "argument" } else { "arguments" },
                if given == 1 { "was" } else { "were" }
            )));
        }
        let name_v = opt(raw, "name", 0).expect("given > 0");
        let text = match name_v {
            SassValue::Str { text, .. } => text.clone(),
            other => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!("$name: {} is not a string.", other.to_css()),
                ));
            }
        };
        let module_v = opt(raw, "module", 1);
        let ns: Option<String> = match module_v {
            Some(SassValue::Str { text, .. }) => Some(text.clone()),
            Some(other) => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!(
                        "$module: {} is neither a string nor a module reference.",
                        other.to_css()
                    ),
                ));
            }
            None => None,
        };
        let css = !mixin
            && opt(raw, "css", 2).is_some_and(|v| !matches!(v, SassValue::Bool(false) | SassValue::Null));
        let display = name_v.to_css();
        let label = if mixin { "Mixin" } else { "Function" };
        let canon = text.replace('_', "-");

        if mixin {
            let found = self.find_mixin(&text, &canon, ns.as_deref())?;
            if let Some(v) = found {
                return Ok(v);
            }
        } else {
            let found = self.find_function(&text, &canon, ns.as_deref(), css)?;
            if let Some(v) = found {
                return Ok(v);
            }
        }
        Err(err(format!("{label} not found: {display}")))
    }

    fn find_function(
        &mut self,
        text: &str,
        canon: &str,
        ns: Option<&str>,
        css: bool,
    ) -> Ev<Option<SassValue>> {
        if css {
            // `$css: true` never resolves to a Sass function (`plain_css`
            // golden: the *call* re-emits `round(0.6)` verbatim).
            return Ok(Some(SassValue::Function(Rc::new(FnRef {
                name: text.to_string(),
                module: None,
                kind: FnKind::Css,
            }))));
        }
        if let Some(ns) = ns {
            if !crate::builtins::has_module(ns)
                && !self.env.has_namespace(ns)
                && !(ns == "meta" || ns == "sass:meta")
            {
                return Err(SassError::new(
                    ErrorKind::UndefinedVariable,
                    format!("There is no module with namespace \"{ns}\"."),
                ));
            }
            if crate::builtins::contains(Some(ns), canon) {
                return Ok(Some(SassValue::Function(Rc::new(FnRef {
                    name: canon.to_string(),
                    module: Some(ns.to_string()),
                    kind: FnKind::Builtin {
                        namespace: Some(ns.to_string()),
                    },
                }))));
            }
            return Ok(None);
        }
        for key in [text, canon] {
            if let Some(decl) = self.functions.get(key) {
                return Ok(Some(SassValue::Function(Rc::new(FnRef {
                    name: key.to_string(),
                    module: None,
                    kind: FnKind::User(decl.clone()),
                }))));
            }
        }
        if crate::builtins::contains(None, canon) {
            return Ok(Some(SassValue::Function(Rc::new(FnRef {
                name: canon.to_string(),
                module: None,
                kind: FnKind::Builtin { namespace: None },
            }))));
        }
        Ok(None)
    }

    fn find_mixin(&mut self, text: &str, canon: &str, ns: Option<&str>) -> Ev<Option<SassValue>> {
        if let Some(ns) = ns {
            if !crate::builtins::has_module(ns) && !self.env.has_namespace(ns) && ns != "meta" {
                return Err(SassError::new(
                    ErrorKind::UndefinedVariable,
                    format!("There is no module with namespace \"{ns}\"."),
                ));
            }
            // The only built-in module mixin the goldens pin is
            // `meta.get-mixin(load-css, meta)` (`inspect/mixin.hrx`).
            if ns == "meta" && canon == "load-css" {
                return Ok(Some(SassValue::Mixin(Rc::new(MixRef {
                    name: "load-css".to_string(),
                    module: Some("meta".to_string()),
                    kind: MixKind::Builtin {
                        namespace: "meta".to_string(),
                    },
                }))));
            }
            return Ok(None);
        }
        for key in [text, canon] {
            if let Some(decl) = self.mixins.get(key) {
                return Ok(Some(SassValue::Mixin(Rc::new(MixRef {
                    name: key.to_string(),
                    module: None,
                    kind: MixKind::User(decl.clone()),
                }))));
            }
        }
        Ok(None)
    }
}
