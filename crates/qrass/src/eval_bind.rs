//! Export binding (task 6.3): how a loaded module's variables, mixins and
//! functions become names in the *importing* frame — the `@use` alias namespace
//! (`as ns`, `as *`, or the default basename) and the `@forward` `show`/`hide`/
//! `as prefix*` filters.
//!
//! Split from the load/instantiate walk in `eval_module` because binding never
//! touches the file system, the singleton bus, or the fan-out cache: it only
//! re-exports what a load already produced.

use crate::ast::{Use, UseAlias};
use crate::env::AssignFlags;
use crate::module::{Module, default_namespace};
use crate::value::SassValue;

use super::EvalState;

impl EvalState {
    /// Bind a module's exports under the `@use` alias.
    pub(super) fn bind_alias(&mut self, u: &Use, module: &std::rc::Rc<Module>, url: &str) {
        match &u.alias {
            UseAlias::Wildcard => {
                for (name, value) in &module.vars {
                    self.env.define(name, value.clone(), AssignFlags::default());
                }
            }
            UseAlias::Named(ns) => {
                self.env.bind_namespace(ns, module.vars.clone());
                self.bound_modules.insert(ns.clone(), module.clone());
                self.bind_members(ns, module);
            }
            UseAlias::Default => {
                let ns = default_namespace(url);
                if !ns.is_empty() {
                    self.env.bind_namespace(&ns, module.vars.clone());
                    self.bound_modules.insert(ns.clone(), module.clone());
                    self.bind_members(&ns, module);
                }
            }
        }
    }

    /// Mirror a module's public mixins/functions under `{ns}.{name}` keys, copying
    /// the lexical env/module snapshots registered at definition time so namespaced
    /// bodies still resolve against the defining module's environment.
    fn bind_members(&mut self, ns: &str, module: &Module) {
        for name in &module.mixins {
            let key = format!("{ns}.{name}");
            if let Some(d) = self.mixins.get(name) {
                self.mixins.insert(key.clone(), d.clone());
            }
            if let Some(e) = self.mixin_env.get(name) {
                self.mixin_env.insert(key.clone(), e.clone());
            }
            if let Some(o) = self.mixin_module.get(name) {
                self.mixin_module.insert(key, o.clone());
            }
        }
        for name in &module.functions {
            let key = format!("{ns}.{name}");
            if let Some(d) = self.functions.get(name) {
                self.functions.insert(key.clone(), d.clone());
            }
            if let Some(e) = self.function_env.get(name) {
                self.function_env.insert(key.clone(), e.clone());
            }
            if let Some(o) = self.function_module.get(name) {
                self.function_module.insert(key, o.clone());
            }
        }
    }
}

/// Apply `@forward` `show`/`hide`/`as prefix*` to a module's public members.
pub(super) fn filter_forwarded(
    members: Vec<(String, SassValue)>,
    shown: &[String],
    hidden: &[String],
    prefix: &str,
) -> Vec<(String, SassValue)> {
    let norm = |s: &str| s.trim_start_matches(['$', '-']).to_string();
    let mut out = members;
    if !shown.is_empty() {
        let keep: Vec<String> = shown.iter().map(|s| norm(s)).collect();
        out.retain(|(n, _)| keep.contains(n));
    }
    if !hidden.is_empty() {
        let drop: Vec<String> = hidden.iter().map(|s| norm(s)).collect();
        out.retain(|(n, _)| !drop.contains(n));
    }
    if !prefix.is_empty() {
        for (name, _) in &mut out {
            *name = format!("{prefix}{name}");
        }
    }
    out
}
