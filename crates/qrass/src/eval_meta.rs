//! Environment-dependent `meta` predicates for [`EvalState`] — a
//! sibling `impl` split out of `eval_function` for the 500-line budget
//! (round 84 overall refactor). These forms read the live scope / registries
//! and cannot live in the pure builtin table; first-class callable lookups
//! delegate to `eval_callable`.

use crate::error::{ErrorKind, SassError};
use crate::value::SassValue;

use super::{Ev, EvalState};
use super::eval_mixin::RawArgs;

impl EvalState {
    /// Resolve the environment-dependent `meta` predicates that cannot live in
    /// the pure builtin registry: `variable-exists`, `global-variable-exists`,
    /// `mixin-exists`, `function-exists`, `content-exists`. Returns `Ok(None)` for
    /// any other name so the caller falls through to the pure table / undefined.
    pub(super) fn try_env_meta(
        &mut self,
        namespace: Option<&str>,
        name: &str,
        raw: &RawArgs,
    ) -> Ev<Option<SassValue>> {
        if !matches!(namespace, None | Some("meta")) {
            return Ok(None);
        }
        // First-class callable lookups live in `eval_callable` (round-73 W2).
        if let Some(v) = self.try_callable_meta(namespace, name, raw)? {
            return Ok(Some(v));
        }
        let first = || match raw.positional().first() {
            Some(SassValue::Str { text, .. }) => text.clone(),
            // a bare identifier argument (`function-exists(a-b)`) evaluates to
            // a symbolic string; `to_css` already spells it unquoted.
            Some(other) => other.to_css(),
            None => String::new(),
        };
        let found = match name {
            "variable-exists" => SassValue::Bool(self.env.get(&first()).is_some()),
            "global-variable-exists" => {
                // `$module:` accepts a module reference (`get_module.hrx`:
                // global-variable-exists("e", $module: meta.get-module(math))
                // asks the built-in module about its own constants).
                if let Some(SassValue::Module(m)) = raw.named().get("module")
                    && let Some(ns) = &m.builtin {
                    SassValue::Bool(crate::builtins::module_const(ns, &first()).is_some())
                } else {
                    SassValue::Bool(self.env.defined_global(&first()))
                }
            }
            "mixin-exists" => return self.run_mixin_exists(raw).map(Some),
            "function-exists" => {
                let f = first();
                // Sass treats `-` and `_` as interchangeable in identifiers
                // (function_exists `dash_insensitive` goldens: defining `a_b`
                // makes `function-exists(a-b)` true).
                let dashed = f.replace('_', "-");
                let underscored = f.replace('-', "_");
                // `meta.function-exists($name, $module)` searches that
                // module's own functions (`different_module` goldens:
                // `("rgb", "list")` is false, `("red", "color")` true).
                let module = raw
                    .positional()
                    .get(1)
                    .map(|v| match v {
                        SassValue::Str { text, .. } => text.clone(),
                        other => other.to_css(),
                    })
                    .or_else(|| {
                        raw.named().get("module").map(|v| match v {
                            SassValue::Str { text, .. } => text.clone(),
                            other => other.to_css(),
                        })
                    });
                if let Some(ns) = module {
                    let in_module = [dashed.as_str(), underscored.as_str()]
                        .iter()
                        .any(|k| crate::builtins::contains(Some(&ns), k));
                    return Ok(Some(SassValue::Bool(in_module)));
                }
                let user_defined = [f.as_str(), dashed.as_str(), underscored.as_str()]
                    .iter()
                    .any(|k| self.functions.contains_key(*k));
                let builtin = [dashed.as_str(), underscored.as_str()]
                    .iter()
                    .any(|k| crate::builtins::contains(None, k));
                SassValue::Bool(user_defined || builtin)
            }
            "content-exists" => {
                // Golden ladder (`meta/content_exists`): arity 0, mixin-scope
                // gate (also during an `@content` replay), then a per-include
                // "was a block passed" answer — `@include m {}` counts as
                // content, `@include m;` does not.
                let given = raw.positional().len();
                if given > 0 {
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        format!(
                            "Only 0 arguments allowed, but {given} {} passed.",
                            if given == 1 { "was" } else { "were" }
                        ),
                    ));
                }
                if self.in_mixin_body == 0 || self.content_replay > 0 {
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        "content-exists() may only be called within a mixin.",
                    ));
                }
                SassValue::Bool(self.content_stack.last().is_some_and(|c| c.has_content))
            }
            // `meta.module-variables($module)`: the namespace's user variables
            // as a name-sorted map (the module_variables goldens spell keys
            // without `$`, in declaration-consistent order). meta-namespace
            // only — there is no global spelling.
            "module-variables" if namespace == Some("meta") => {
                let given = raw.positional().len() + raw.named().len();
                if given == 0 {
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        "Missing argument $module.",
                    ));
                }
                if given > 1 {
                    return Err(SassError::new(
                        ErrorKind::Builtins,
                        format!(
                            "Only 1 argument allowed, but {given} {} passed.",
                            if given == 1 { "was" } else { "were" }
                        ),
                    ));
                }
                let ns = match raw.positional().first().or_else(|| raw.named().get("module")) {
                    Some(SassValue::Str { text, .. }) => text.clone(),
                    Some(other) => {
                        return Err(SassError::new(
                            ErrorKind::Builtins,
                            format!(
                                "$module: {} is neither a string nor a module reference.",
                                other.to_css()
                            ),
                        ));
                    }
                    None => String::new(),
                };
                let map = match self.env.namespace_vars(&ns) {
                    Some(v) => v.clone(),
                    None if crate::builtins::has_module(&ns) => Default::default(),
                    None => {
                        return Err(SassError::new(
                            ErrorKind::UndefinedVariable,
                            format!("There is no module with namespace \"{ns}\"."),
                        ));
                    }
                };
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let entries = keys
                    .into_iter()
                    .map(|k| {
                        (
                            SassValue::Str {
                                text: k.clone(),
                                quoted: true,
                            },
                            map[k].clone(),
                        )
                    })
                    .collect();
                SassValue::Map { entries }
            }
            _ => return Ok(None),
        };
        Ok(Some(found))
    }

    /// `meta.mixin-exists($name, $module)` (`core_functions/meta/mixin_exists`):
    /// arity/type validation plus per-module lookup. Rules read off the golden
    /// ladder only — `Missing argument $name.`, `Only 2 arguments allowed, …`,
    /// `$name: 12px is not a string.`, `$module: 1 is neither a string nor a
    /// module reference.`; `$module` accepts a bound namespace name or a
    /// `meta.get-module` reference, and the no-module form answers from the
    /// live registry *and* the `as *` exported members (show/hide aware).
    pub(crate) fn run_mixin_exists(&mut self, raw: &RawArgs) -> Ev<SassValue> {
        let pos = raw.positional();
        let named = raw.named();
        let name_val = pos
            .first()
            .or_else(|| named.get("name"))
            .ok_or_else(|| {
                SassError::new(ErrorKind::Builtins, "Missing argument $name.")
            })?;
        let given = pos.len() + named.len();
        if given > 2 {
            return Err(SassError::new(
                ErrorKind::Builtins,
                format!("Only 2 arguments allowed, but {given} were passed."),
            ));
        }
        let name = match name_val {
            SassValue::Str { text, .. } => text.clone(),
            other => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!("$name: {} is not a string.", other.to_css()),
                ));
            }
        };
        let module_arg = pos.get(1).or_else(|| named.get("module"));
        let found = match module_arg {
            None => {
                self.mixins.contains_key(&name)
                    || self.wildcard_mixins.iter().any(|(e, _)| e == &name)
            }
            Some(SassValue::Str { text, .. }) => {
                // The name must match a namespace actually in scope — exact
                // spelling, no `_`/`-` folding (`mixin_exists` `error/module/*`:
                // an unbound or wrongly-spelled name raises the goldens' no-module
                // error; a loaded built-in simply owns no mixins).
                let builtin = self.used_builtin_ns.iter().any(|u| u == text);
                match (self.bound_modules.get(text), builtin) {
                    (Some(m), _) => m.mixins.iter().any(|(exported, _)| exported == &name),
                    (None, true) => false,
                    (None, false) => {
                        return Err(SassError::new(
                            ErrorKind::Builtins,
                            format!("There is no module with namespace \"{text}\"."),
                        ));
                    }
                }
            }
            Some(SassValue::Module(m)) => match &m.module {
                Some(user) => user.mixins.iter().any(|(exported, _)| exported == &name),
                None => false,
            },
            Some(other) => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!(
                        "$module: {} is neither a string nor a module reference.",
                        other.to_css()
                    ),
                ));
            }
        };
        Ok(SassValue::Bool(found))
    }
}
