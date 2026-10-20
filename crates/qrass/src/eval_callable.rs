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

pub(crate) fn err(msg: impl Into<String>) -> SassError {
    SassError::new(ErrorKind::UndefinedFunction, msg)
}

/// Named-or-positional read on `RawArgs` (mirrors `builtins::Call::opt`).
pub(crate) fn opt<'a>(raw: &'a RawArgs, key: &str, pos: usize) -> Option<&'a SassValue> {
    raw.named().get(key).or_else(|| raw.positional().get(pos))
}

/// Does a mixin body mention `@content` anywhere (dead branches included —
/// the `accepts_content/if_false` golden reports true)?
fn body_takes_content(body: &[crate::ast::Stmt]) -> bool {
    use crate::ast::Stmt;
    body.iter().any(|s| match s {
        Stmt::Content(_) => true,
        Stmt::Ruleset(r) => body_takes_content(&r.body),
        Stmt::Each(e) => body_takes_content(&e.body),
        Stmt::For(f) => body_takes_content(&f.body),
        Stmt::While(w) => body_takes_content(&w.body),
        Stmt::If(i) => i.clauses.iter().any(|c| body_takes_content(&c.body)),
        Stmt::Media(m) => body_takes_content(&m.body),
        Stmt::Supports(su) => body_takes_content(&su.body),
        _ => false,
    })
}

impl EvalState {
    /// The definition-time closure snapshot recorded for user function `key`
    /// (shared by the by-name call path and `get-function` references).
    pub(crate) fn closure_snapshot(&self, key: &str) -> (crate::env::Env, String) {
        (
            self.function_env
                .get(key)
                .cloned()
                .unwrap_or_else(|| self.env.clone()),
            self.function_module
                .get(key)
                .cloned()
                .unwrap_or_else(|| self.env_owner.clone()),
        )
    }

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
            // `meta.call($function, $args...)`: the first argument is a
            // function reference (or, deprecatedly, its name as a string —
            // `call.hrx` string/* sections); everything else forwards to the
            // target's own arity/validation, whose errors surface verbatim
            // (`error/invalid_args` reads the rgb builtin's `$channels:` text).
            "call" => self.run_call(raw),
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
            "accepts-content" => {
                let given = raw.positional().len() + raw.named().len();
                if given == 0 {
                    return Err(err("Missing argument $mixin."));
                }
                if given > 1 {
                    return Err(err(
                        "Only 1 argument allowed, but {given} were passed."
                            .replace("{given}", &given.to_string())
                            .replace("were", if given == 1 { "was" } else { "were" }),
                    ));
                }
                let v = opt(raw, "mixin", 0).expect("given > 0");
                let accepts = match v {
                    SassValue::Mixin(m) => match &m.kind {
                        // `@content` anywhere in the body (even under a dead
                        // `@if false` branch — the `if_false` golden says true).
                        MixKind::User { decl, .. } => body_takes_content(&decl.body),
                        // `meta.apply` forwards a block; `load-css` does not.
                        MixKind::Builtin { .. } => m.name == "apply",
                    },
                    other => {
                        return Err(SassError::new(
                            ErrorKind::Builtins,
                            format!("$mixin: {} is not a mixin reference.", other.to_css()),
                        ));
                    }
                };
                Ok(Some(SassValue::Bool(accepts)))
            }
            // `meta.module-functions`/`module-mixins` (round-73 W4, the
            // module_functions/module_mixins goldens): the namespace's public
            // member names in *definition order* (not sorted like
            // module-variables), each value the callable reference. Built-in
            // modules carry no user members → empty map; a name never bound by
            // `@use … as ns` (including `as *`) is the namespace error.
            "module-functions" | "module-mixins" => {
                let mixins = name == "module-mixins";
                let given = raw.positional().len() + raw.named().len();
                if given == 0 {
                    return Err(err("Missing argument $module."));
                }
                if given > 1 {
                    return Err(err(format!(
                        "Only 1 argument allowed, but {given} {} passed.",
                        if given == 1 { "was" } else { "were" }
                    )));
                }
                let ns = match opt(raw, "module", 0) {
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
                let members: Vec<(String, String)> =
                    if let Some(module) = self.bound_modules.get(&ns) {
                        if mixins {
                            module.mixins.clone()
                        } else {
                            module.functions.clone()
                        }
                    } else if crate::builtins::has_module(&ns) {
                        Vec::new()
                    } else {
                        return Err(SassError::new(
                            ErrorKind::UndefinedVariable,
                            format!("There is no module with namespace \"{ns}\"."),
                        ));
                    };
                let mut entries = Vec::with_capacity(members.len());
                for (exported, defined) in members {
                    // `bind_members` mirrors the registry under `{ns}.{exported}`
                    // (forwarded renames included); the defining name keeps the
                    // snapshot lookups working for plain members too.
                    let key = format!("{ns}.{exported}");
                    let value = if mixins {
                        let Some(decl) =
                            self.mixins.get(&key).or_else(|| self.mixins.get(&defined)).cloned()
                        else
                        {
                            continue;
                        };
                        SassValue::Mixin(Rc::new(MixRef {
                            name: exported.clone(),
                            module: Some(ns.clone()),
                            kind: MixKind::User {
                                decl,
                                owner: self
                                    .mixin_module
                                    .get(&key)
                                    .or_else(|| self.mixin_module.get(&defined))
                                    .cloned()
                                    .unwrap_or_else(|| self.env_owner.clone()),
                            },
                        }))
                    } else {
                        let Some(decl) =
                            self.functions.get(&key).or_else(|| self.functions.get(&defined)).cloned()
                        else
                        {
                            continue;
                        };
                        let (env, owner) = self
                            .function_env
                            .get(&key)
                            .cloned()
                            .map(|e| (e, self.function_module.get(&key).cloned()))
                            .or_else(|| {
                                self.function_env.get(&defined).cloned().map(|e| {
                                    (e, self.function_module.get(&defined).cloned())
                                })
                            })
                            .map_or_else(
                                || self.closure_snapshot(&key),
                                |(e, o)| (e, o.unwrap_or_else(|| self.env_owner.clone())),
                            );
                        SassValue::Function(Rc::new(FnRef {
                            name: exported.clone(),
                            module: Some(ns.clone()),
                            kind: FnKind::User { decl, env, owner },
                        }))
                    };
                    entries.push((
                        SassValue::Str {
                            text: exported,
                            quoted: true,
                        },
                        value,
                    ));
                }
                Ok(Some(SassValue::Map { entries }))
            }
            _ => Ok(None),
        }
    }

    /// Run a user `@function` body against its definition-time snapshot — the
    /// shared core of the by-name call path and `meta.call` (round-73 W3).
    pub(crate) fn invoke_user_fn(
        &mut self,
        func: &std::rc::Rc<crate::ast::FunctionDecl>,
        def_env: crate::env::Env,
        def_owner: String,
        raw: RawArgs,
    ) -> Ev<SassValue> {
        let saved_env = std::mem::replace(&mut self.env, def_env);
        let saved_owner = std::mem::replace(&mut self.env_owner, def_owner);
        self.env.enter();
        let bound = self.bind_params(&func.params, raw);
        let value = match bound.and_then(|()| self.eval_fn_body(&func.body)) {
            Ok(Some(v)) => v,
            Ok(None) => SassValue::Null,
            Err(e) => {
                self.env.leave();
                self.env = saved_env;
                self.env_owner = saved_owner;
                return Err(e);
            }
        };
        self.env.leave();
        self.env = saved_env;
        self.env_owner = saved_owner;
        Ok(value)
    }

    /// `meta.get-function`/`get-mixin` per the error goldens: arity wording,
    /// `$name`/`$module` type wording, local → builtin lookup, and dart's
    /// `Function not found:` / `Mixin not found:` / namespace errors.
    pub(super) fn lookup_callable(&mut self, mixin: bool, raw: &RawArgs) -> Ev<SassValue> {
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

    pub(super) fn find_function(
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
            // the module's own user functions live mirrored under `{ns}.{name}`
            // by `bind_members` (get_function/different_module goldens);
            // forwarded renames ride the same key.
            if let Some(decl) = self.functions.get(&format!("{ns}.{canon}")) {
                let (env, owner) = self.closure_snapshot(&format!("{ns}.{canon}"));
                return Ok(Some(SassValue::Function(Rc::new(FnRef {
                    name: canon.to_string(),
                    module: Some(ns.to_string()),
                    kind: FnKind::User { decl: decl.clone(), env, owner },
                }))));
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
                let (env, owner) = self.closure_snapshot(key);
                return Ok(Some(SassValue::Function(Rc::new(FnRef {
                    name: key.to_string(),
                    module: None,
                    kind: FnKind::User {
                        decl: decl.clone(),
                        env,
                        owner,
                    },
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
            // The only built-in module mixins the goldens pin are
            // `meta.get-mixin(load-css, meta)` (`inspect/mixin.hrx`) and
            // `meta.apply` (`accepts_content/builtin`).
            if let Some(decl) = self.mixins.get(&format!("{ns}.{canon}")) {
                return Ok(Some(SassValue::Mixin(Rc::new(MixRef {
                    name: canon.to_string(),
                    module: Some(ns.to_string()),
                    kind: MixKind::User {
                        decl: decl.clone(),
                        owner: self
                            .mixin_module
                            .get(&format!("{ns}.{canon}"))
                            .cloned()
                            .unwrap_or_else(|| self.env_owner.clone()),
                    },
                }))));
            }
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
                    kind: MixKind::User {
                        decl: decl.clone(),
                        owner: self
                            .mixin_module
                            .get(key)
                            .cloned()
                            .unwrap_or_else(|| self.env_owner.clone()),
                    },
                }))));
            }
        }
        Ok(None)
    }
}
