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
        // Track loaded built-in namespaces by their *exact* bound spelling —
        // `meta.mixin-exists("c", "color")` answers false only when `sass:color`
        // was actually `@use`d; an unbound name raises dart's no-module error
        // (`meta/mixin_exists` `error/module/built_in_but_not_loaded`).
        if url.starts_with("sass:") {
            let bound = match &u.alias {
                UseAlias::Named(ns) => ns.clone(),
                UseAlias::Default => url.trim_start_matches("sass:").to_string(),
                UseAlias::Wildcard => String::new(),
            };
            if !bound.is_empty() && !self.used_builtin_ns.contains(&bound) {
                self.used_builtin_ns.push(bound);
            }
        }
        match &u.alias {
            UseAlias::Wildcard => {
                // Conflict identity = the origin file that *defines* the
                // member (Module::member_origins provenance), so two
                // midstreams forwarding one upstream stay conflict-free
                // (element-plus chains) while two independent definitions
                // of one name collide (`directives/use/error/member` goldens).
                let origin_of = |name: &str| -> String {
                    module
                        .member_origins
                        .iter()
                        .find(|(e, _, _)| e == name)
                        .map(|(_, o, _)| o.clone())
                        .unwrap_or_else(|| module.origin_base.clone())
                };
                for (name, value) in &module.vars {
                    let org = origin_of(name);
                    if let Some((_, prev)) =
                        self.wildcard_vars.iter().find(|(n, _)| n == name)
                        && prev != &org
                        && !self.wildcard_conflicts.0.iter().any(|c| c == name)
                    {
                        
                        self.wildcard_conflicts.0.push(name.clone());
                    }
                    if !self.wildcard_vars.iter().any(|(n, u)| n == name && u == &org) {
                        self.wildcard_vars.push((name.clone(), org));
                    }
                    self.env.define(name, value.clone(), AssignFlags::default());
                }
                // Record the *exported* mixin members (post show/hide) so
                // `meta.mixin-exists` can answer per the importer's view.
                for (exported, defined) in &module.mixins {
                    let org = origin_of(exported);
                    if let Some((_, prev)) =
                        self.wildcard_mixin_src.iter().find(|(e, _)| e == exported)
                        && prev != &org
                        && !self.wildcard_conflicts.2.iter().any(|c| c == exported)
                    {
                        self.wildcard_conflicts.2.push(exported.clone());
                    }
                    if !self.wildcard_mixin_src
                        .iter()
                        .any(|(e, u)| e == exported && u == &org)
                    {
                        self.wildcard_mixin_src.push((exported.clone(), org));
                    }
                    if !self.wildcard_mixins.iter().any(|(e, _)| e == exported) {
                        self.wildcard_mixins
                            .push((exported.clone(), defined.clone()));
                    }
                }
                for (exported, _defined) in &module.functions {
                    let org = origin_of(exported);
                    if let Some((_, prev)) =
                        self.wildcard_fns.iter().find(|(e, _)| e == exported)
                        && prev != &org
                        && !self.wildcard_conflicts.1.iter().any(|c| c == exported)
                    {
                        self.wildcard_conflicts.1.push(exported.clone());
                    }
                    if !self.wildcard_fns
                        .iter()
                        .any(|(e, u)| e == exported && u == &org)
                    {
                        self.wildcard_fns.push((exported.clone(), org));
                    }
                    // Bare-name mirror (mirrors `bind_members` without the
                    // namespace prefix): wildcard functions were never
                    // resolvable, the gap behind `use/member` fn cases.
                    if let Some(d) = self.functions.get(_defined) {
                        self.functions.insert(exported.clone(), d.clone());
                    }
                    if let Some(e) = self.function_env.get(_defined) {
                        self.function_env.insert(exported.clone(), e.clone());
                    }
                    if let Some(o) = self.function_module.get(_defined) {
                        self.function_module.insert(exported.clone(), o.clone());
                    }
                }
            }
            UseAlias::Named(ns) => {
                self.bind_ns_vars(ns, module);
                self.bound_modules.insert(ns.clone(), module.clone());
                self.bind_members(ns, module);
            }
            UseAlias::Default => {
                let ns = default_namespace(url);
                if !ns.is_empty() {
                    self.bind_ns_vars(&ns, module);
                    self.bound_modules.insert(ns.clone(), module.clone());
                    self.bind_members(&ns, module);
                }
            }
        }
    }

    /// Publish a module's vars under `ns` AND register the live-cell
    /// bookkeeping (OpenSpec M4 slice 3): `(ns, var) -> origin module` plus
    /// the initial `member_store` value, so `ns.$var: value` writes reach the
    /// defining module and every read path sees the update (dart module vars
    /// are shared-mutable, not snapshot copies).
    fn bind_ns_vars(&mut self, ns: &str, module: &std::rc::Rc<Module>) {
        self.env.bind_namespace(ns, module.vars.clone());
        for (name, value) in &module.vars {
            let origin = module
                .member_origins
                .iter()
                .find(|(e, _, _)| e == name)
                .map(|(_, o, d)| (o.clone(), d.clone()))
                .unwrap_or_else(|| (module.origin_base.clone(), name.clone()));
            self.ns_origins
                .insert((ns.to_string(), name.clone()), origin.clone());
            // `-`/`_` spellings of one member name are interchangeable for
            // reads and writes (`as` `different_separator`): route both
            // single-swap spellings through the origin map only — the ns view
            // itself keeps dart's clean export set (`meta.module-variables`).
            for alt in [name.replace('_', "-"), name.replace('-', "_")] {
                if alt != *name {
                    self.ns_origins
                        .insert((ns.to_string(), alt.clone()), origin.clone());
                }
            }
            self.member_store
                .entry(origin)
                .or_insert_with(|| value.clone());
        }
    }

    /// Mirror a module's public mixins/functions under `{ns}.{name}` keys, copying
    /// the lexical env/module snapshots registered at definition time so namespaced
    /// bodies still resolve against the defining module's environment.
    fn bind_members(&mut self, ns: &str, module: &Module) {
        for (exported, defined) in &module.mixins {
            let key = format!("{ns}.{exported}");
            if let Some(d) = self.mixins.get(defined) {
                self.mixins.insert(key.clone(), d.clone());
            }
            if let Some(e) = self.mixin_env.get(defined) {
                self.mixin_env.insert(key.clone(), e.clone());
            }
            if let Some(o) = self.mixin_module.get(defined) {
                self.mixin_module.insert(key, o.clone());
            }
        }
        for (exported, defined) in &module.functions {
            let key = format!("{ns}.{exported}");
            if let Some(d) = self.functions.get(defined) {
                self.functions.insert(key.clone(), d.clone());
            }
            if let Some(e) = self.function_env.get(defined) {
                self.function_env.insert(key.clone(), e.clone());
            }
            if let Some(o) = self.function_module.get(defined) {
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
    // dart applies `as prefix-*` FIRST and the `show`/`hide` lists name the
    // RENAMED members (`forward/member/as` `show/*`: `as b-* show b-a`
    // keeps; `as b-* hide a` hides nothing — `a` no longer names anything);
    // `-` and `_` are interchangeable in Sass member names (`d_* prefix +
    // `$d-c` access reads `different_separator`).
    let norm = |s: &str| s.trim_start_matches('$').replace('_', "-");
    let mut out = members;
    if !prefix.is_empty() {
        for (name, _) in &mut out {
            *name = format!("{prefix}{name}");
        }
    }
    if !shown.is_empty() {
        let keep: Vec<String> = shown.iter().map(|s| norm(s)).collect();
        out.retain(|(n, _)| keep.contains(&norm(n)));
    }
    if !hidden.is_empty() {
        let drop: Vec<String> = hidden.iter().map(|s| norm(s)).collect();
        out.retain(|(n, _)| !drop.contains(&norm(n)));
    }
    out
}

/// Apply the same `@forward` `show`/`hide`/`as prefix*` filter to mixin or
/// function members, keeping the (exported, defining) pairing intact.
pub(super) fn filter_forwarded_names(
    names: &[(String, String)],
    shown: &[String],
    hidden: &[String],
    prefix: &str,
) -> Vec<(String, String)> {
    // Rename first, then filter on the renamed names, with `-`/`_`
    // equivalence (see `filter_forwarded`).
    let norm = |s: &str| s.trim_start_matches(['$', '-']).replace('_', "-");
    let mut out: Vec<(String, String)> = names.to_vec();
    if !prefix.is_empty() {
        for (name, _) in &mut out {
            *name = format!("{prefix}{name}");
        }
    }
    if !shown.is_empty() {
        let keep: Vec<String> = shown.iter().map(|s| norm(s)).collect();
        out.retain(|(n, _)| keep.contains(&norm(n)));
    }
    if !hidden.is_empty() {
        let drop: Vec<String> = hidden.iter().map(|s| norm(s)).collect();
        out.retain(|(n, _)| !drop.contains(&norm(n)));
    }
    out
}
