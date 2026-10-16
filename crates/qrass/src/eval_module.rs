//! Module loading for [`EvalState`] (tasks 6.2 / 6.3): `@use` and `@forward`.
//!
//! Both canonicalize a URL against the importer's directory and instantiate each
//! canonical path exactly once in its own global scope, publishing the module's
//! CSS at that first load. `@use` binds the exports under a namespace; `@forward`
//! re-exports them (with `show`/`hide`/`as prefix*`) into the current module's
//! export set. `@use … with` / `@forward … with` configures the target's
//! `!default` variables; a supplied-but-unconsumed name is an error (design D3).

use std::path::{Path, PathBuf};

use crate::ast::{Forward, Use, UseAlias};
use crate::env::AssignFlags;
use crate::error::{ErrorKind, SassError};
use crate::loader::{SourceKind, canonicalize_with};
use crate::module::{Module, default_namespace, export_vars};
use crate::value::SassValue;

use super::{Ev, EvalState};

impl EvalState {
    /// Resolve and load one `@use`, emitting the module's CSS (once) and binding
    /// its namespace into the current scope. Its `with (…)` entries become offers
    /// that the target's `!default` declarations must claim, or the site reports
    /// them (`error/with/undefined.hrx`).
    pub(super) fn handle_use(&mut self, u: &Use, out: &mut Vec<String>) -> Ev<()> {
        let site = self.open_config_site();
        let view = self.offer_config(&u.config, Some(site))?;
        let (module, _ran) = self.load_module(&u.url, view, out, true)?;
        self.bind_alias(u, &module, &u.url);
        self.check_config_site(site)
    }

    /// Resolve and load one `@forward`, re-exporting the target's members into
    /// the current module's export buffer, and handing it the configuration view
    /// the forward's filters / `with` clause resolve to.
    pub(super) fn handle_forward(&mut self, f: &Forward, out: &mut Vec<String>) -> Ev<()> {
        let view = self.forward_config_view(f)?;
        // A forward never settles the configuration it passes on: an offer the
        // target cannot claim still has to be reported by (or claimed later in)
        // the `@use` site that created it — `with_unrelated_config` and
        // `error/with/missing_distributed_vars.hrx` both depend on that.
        let (module, _ran) = self.load_module(&f.url, view, out, false)?;
        let members = filter_forwarded(module.vars.clone(), &f.shown, &f.hidden, &f.prefix);
        if let Some(buf) = self.forward_buf.last_mut() {
            buf.extend(members);
        }
        Ok(())
    }

    /// Canonicalize, cycle-check, and load a module (cached ⇒ no re-emit).
    /// Returns `(module, did_the_body_run)`.
    ///
    /// `direct` says the load comes from a `@use`: its own `with (…)` entries are
    /// then settled when no body runs to claim them, because reporting them as
    /// "not declared with !default" would blame the wrong statement
    /// (`error/with/multi_configuration.hrx` wants a dedicated message that is
    /// not implemented yet).
    fn load_module(
        &mut self,
        url: &str,
        view: Vec<(String, usize)>,
        out: &mut Vec<String>,
        direct: bool,
    ) -> Ev<(Module, bool)> {
        // Built-in modules (`sass:math`, `sass:color`, …) have no source file: their
        // functions resolve through the builtin registry by namespace at call time
        // and their constants (`math.$pi`, …) through the `module_const` fallback, so
        // `@use`/`@forward` of a `sass:` URL is a no-op that binds nothing.
        if url.starts_with("sass:") {
            if direct {
                self.settle_offers(&view);
            }
            return Ok((Module { css: Vec::new(), vars: Vec::new(), ..Default::default() }, false));
        }
        let base = parent_dir(&self.name);
        let resolved = canonicalize_with(url, &base, &self.load_paths)?;
        let key = resolved.path.display().to_string();

        if self.stack.contains(&key) {
            let mut chain = self.stack.clone();
            chain.push(key);
            return Err(SassError::new(
                ErrorKind::Cycle,
                "It is a circular dependency: @use/@forward loops back on itself",
            )
            .note(chain.join(" -> ")));
        }

        if let Some(module) = self.modules.get(&key).cloned() {
            // Cached: the body already ran, so nothing here can claim the offers.
            if direct {
                self.settle_offers(&view);
            }
            return Ok((module, false)); // cached: no CSS, no re-run
        }

        let ran = resolved.kind != SourceKind::Css;
        let module = if !ran {
            let css = read_to_string(&resolved.path, &key)?;
            // Plain-CSS mode: the `.css` payload is passed through verbatim, but
            // SCSS-only constructs inside it must still be rejected the way
            // the goldens would. Parse for validation only; when the content does
            // not even parse (an exotic CSS feature the SCSS parser cannot read)
            // keep the prior passthrough instead of reporting a spurious error.
            if let Ok(program) = crate::lexer::lex(0, key.clone(), css.clone()).and_then(|tokens| {
                crate::parser::parse(0, key.clone(), css.clone(), tokens)
            }) {
                crate::plain_css::validate(&program)?;
            }
            if direct {
                self.settle_offers(&view);
            }
            Module {
                css: vec![css],
                vars: Vec::new(),
                ..Default::default()
            }
        } else {
            let content = read_to_string(&resolved.path, &key)?;
            self.instantiate(&key, &content, &view)?
        };

        self.modules.insert(key, module.clone());
        out.extend(module.css.iter().cloned());
        Ok((module, ran))
    }

    /// Bind a module's exports under the `@use` alias.
    fn bind_alias(&mut self, u: &Use, module: &Module, url: &str) {
        match &u.alias {
            UseAlias::Wildcard => {
                for (name, value) in &module.vars {
                    self.env.define(name, value.clone(), AssignFlags::default());
                }
            }
            UseAlias::Named(ns) => {
                self.env.bind_namespace(ns, module.vars.clone());
                self.bind_members(ns, module);
            }
            UseAlias::Default => {
                let ns = default_namespace(url);
                if !ns.is_empty() {
                    self.env.bind_namespace(&ns, module.vars.clone());
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

    /// Run a module body in a fresh global scope, returning its CSS + exports.
    /// Nested `@use`/`@forward` share `self.modules`/`self.stack`, so a diamond
    /// loads once. The incoming configuration is *not* pre-bound: the body sees it
    /// only through its own `!default` declarations (`with/variable_exists.hrx`
    /// wants `meta.variable-exists` false before the first one).
    fn instantiate(
        &mut self,
        key: &str,
        content: &str,
        view: &[(String, usize)],
    ) -> Ev<Module> {
        let program =
            crate::lexer::lex(0, key.to_string(), content.to_string()).and_then(|tokens| {
                crate::parser::parse(0, key.to_string(), content.to_string(), tokens)
            })?;

        let outer_env = std::mem::take(&mut self.env);
        let outer_owner = std::mem::replace(&mut self.env_owner, key.to_string());
        let outer_file = std::mem::replace(&mut self.file, 0);
        let outer_name = std::mem::replace(&mut self.name, key.to_string());
        let outer_content = std::mem::replace(&mut self.content, content.to_string());
        self.stack.push(key.to_string());
        self.forward_buf.push(Vec::new());
        self.config_stack.push(view.to_vec());

        let ran = (|st: &mut EvalState| -> Ev<Module> {
            let mut css = Vec::new();
            for stmt in &program {
                st.eval_top_level(stmt, &mut css)?;
            }
            let forwarded = st.forward_buf.last().cloned().unwrap_or_default();
            let mut vars = export_vars(st.env.global_bindings());
            vars.extend(forwarded);
            // Public mixin/function members: collect the top-level declarations so
            // `bind_alias` can register `{ns}.{name}` aliases over the live registry
            // entries (bare-name definitions + their lexical env snapshots remain).
            let mut mixins = Vec::new();
            let mut functions = Vec::new();
            for stmt in &program {
                match stmt {
                    crate::ast::Stmt::Mixin(m) => {
                        let n = st.render_interp(&m.name)?;
                        if !crate::module::is_private(&n) {
                            mixins.push(n);
                        }
                    }
                    crate::ast::Stmt::Function(f)
                        if !crate::module::is_private(&f.name) =>
                    {
                        functions.push(f.name.clone());
                    }
                    _ => {}
                }
            }
            Ok(Module { css, vars, mixins, functions })
        })(self);

        self.config_stack.pop();
        self.forward_buf.pop();
        self.stack.pop();
        // Persist this module's now-live global env so cross-module closures defined
        // in it share (and mutate) it on later calls, rather than a definition clone.
        self.module_envs.insert(key.to_string(), self.env.clone());
        self.env = outer_env;
        self.env_owner = outer_owner;
        self.file = outer_file;
        self.name = outer_name;
        self.content = outer_content;

        ran
    }
}

/// Apply `@forward` `show`/`hide`/`as prefix*` to a module's public members.
fn filter_forwarded(
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

/// Directory containing `path` (empty for a bare relative name).
fn parent_dir(path: &str) -> PathBuf {
    Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default()
}

/// Read a resolved module from disk, mapping IO failure to a load error.
fn read_to_string(path: &Path, name: &str) -> Result<String, SassError> {
    std::fs::read_to_string(path).map_err(|err| SassError::io(err, name))
}
