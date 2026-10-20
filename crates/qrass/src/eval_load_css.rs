//! `meta.load-css($url, $with: …)` for [`EvalState`] — a sibling `impl` split
//! of `eval_module` for the 500-line budget (round 80). Loads the target
//! module, splices its CSS into the output, and hands it the `$with:`
//! configuration; only the CSS joins — no namespace binds (`load_css/error/
//! member.hrx`: `$c` and `other.$c` both stay Undefined in the caller).
//!
//! The load-css error family spells several failures differently from the
//! `@use … with` goldens, and every wording below is read off the golden
//! tables: `$a was not declared with !default…` (check_config_site's
//! load_css branch), `_other.scss was already loaded, so it can't be
//! configured using "with".` (direct target; forwarded distribution keeps
//! `This module was…`), `Built-in module sass:color can't be configured.`,
//! and `The variable $a-b was configured twice.`.

use crate::ast::Include;
use crate::clock::StopWatch;
use crate::error::{ErrorKind, SassError};
use crate::value::{ModuleRef, SassValue};

use std::rc::Rc;

use super::{Ev, EvalState};
use tracing::{Level, span};

impl EvalState {
    /// `meta.get-module($module)` (`get_module.hrx`): exactly one argument —
    /// a namespace string (a bare identifier evaluates to an unquoted
    /// string), a module reference (passes through), or the type-error
    /// spelling for anything else. Built-ins resolve by registry/constant
    /// membership; user modules ride their bound singleton (`bound_modules`).
    pub(crate) fn run_get_module(&mut self, raw: &super::eval_mixin::RawArgs) -> Ev<Option<SassValue>> {
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
                format!("Only 1 argument allowed, but {given} were passed."),
            ));
        }
        let v = raw
            .positional()
            .first()
            .or_else(|| raw.named().get("module"))
            .cloned()
            .expect("given checked");
        let text = match &v {
            SassValue::Module(m) => return Ok(Some(SassValue::Module(m.clone()))),
            SassValue::Str { text, .. } => text.clone(),
            other => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!(
                        "$module: {} is neither a string nor a module reference.",
                        other.to_css()
                    ),
                ));
            }
        };
        let ns = text.replace("sass:", "").replace('_', "-");
        if ns == "meta" || crate::builtins::has_module(&ns) || crate::builtins::is_const_ns(&ns) {
            return Ok(Some(SassValue::Module(Rc::new(ModuleRef {
                builtin: Some(ns),
                module: None,
            }))));
        }
        if let Some(module) = self.bound_modules.get(&ns) {
            return Ok(Some(SassValue::Module(Rc::new(ModuleRef {
                builtin: None,
                module: Some(module.clone()),
            }))));
        }
        Err(SassError::new(
            ErrorKind::UndefinedVariable,
            format!("There is no module with namespace \"{ns}\"."),
        ))
    }

    /// `meta.load($url, $with:)` (`meta/load/*`): like `meta.load-css` but the
    /// module's CSS is *not* emitted and the result is the module reference;
    /// all configuration spellings are shared with the load-css family.
    pub(crate) fn run_load_function(&mut self, raw: &super::eval_mixin::RawArgs) -> Ev<Option<SassValue>> {
        // Same cross-file pipeline convention as `run_load_css`.
        let span = span!(
            Level::DEBUG,
            "load_fn",
            stage = "load",
            module = %self.name,
            url = tracing::field::Empty,
            elapsed_ms = tracing::field::Empty,
        );
        let _guard = span.enter();
        let started = StopWatch::start();
        let result = self.run_load_function_inner(raw);
        span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));
        result
    }

    fn run_load_function_inner(&mut self, raw: &super::eval_mixin::RawArgs) -> Ev<Option<SassValue>> {
        let url = match raw.positional().first().or_else(|| raw.named().get("url")) {
            Some(SassValue::Str { text, .. }) => text.clone(),
            None => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    "Missing argument $url.",
                ));
            }
            Some(other) => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!("$url: {} is not a string.", other.to_css()),
                ));
            }
        };
        let entry = crate::span::Span::default();
        tracing::Span::current().record("url", url.as_str());
        let with_v = raw
            .named()
            .get("with")
            .cloned()
            .or_else(|| raw.positional().get(1).cloned());
        let site = self.open_config_site();
        let view = self.prepare_with(with_v, site, entry)?;
        if url.starts_with("sass:") {
            if !view.is_empty() {
                return Err(SassError::new(
                    ErrorKind::Config,
                    format!("Built-in module {url} can't be configured."),
                ));
            }
            let ns = url.trim_start_matches("sass:").replace('_', "-");
            self.settle_offers(&view);
            return Ok(Some(SassValue::Module(Rc::new(ModuleRef {
                builtin: Some(ns),
                module: None,
            }))));
        }
        self.css_config_precheck(&url, &view, entry)?;
        let mut scratch = Vec::new();
        let link = self.link_to(&entry, "@use");
        let (module, _ran) = self
            .load_module(&url, view, &mut scratch, true, link, entry)
            .map_err(|err| self.attach_frames(err))?;
        self.check_config_site(site, true)?;
        Ok(Some(SassValue::Module(Rc::new(ModuleRef {
            builtin: None,
            module: Some(module),
        }))))
    }

    pub(super) fn run_load_css(&mut self, inc: &Include, out: &mut Vec<String>) -> Ev<()> {
        // Cross-file pipeline boundary (rule: every stage gets a span): this
        // entry drives a full module load + configuration settlement, so it
        // records the same stage/url/elapsed fields as `module_load`.
        let span = span!(
            Level::DEBUG,
            "load_css",
            stage = "load",
            module = %self.name,
            url = tracing::field::Empty,
            elapsed_ms = tracing::field::Empty,
        );
        let _guard = span.enter();
        let started = StopWatch::start();
        let result = self.run_load_css_inner(inc, out);
        span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));
        result
    }

    fn run_load_css_inner(&mut self, inc: &Include, out: &mut Vec<String>) -> Ev<()> {
        let raw = self.collect_args(&inc.args)?;
        let url = match raw.positional().first() {
            Some(SassValue::Str { text, .. }) => text.clone(),
            None => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    "Missing argument $url.",
                ));
            }
            Some(other) => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!("$url: {} is not a string.", other.to_css()),
                ));
            }
        };
        let entry = inc.span;
        tracing::Span::current().record("url", url.as_str());

        // `$with:` — named first, then the second positional (dart's order).
        let with_v = raw
            .named()
            .get("with")
            .cloned()
            .or_else(|| raw.positional().get(1).cloned());
        let site = self.open_config_site();
        let view = self.prepare_with(with_v, site, entry)?;

        // A built-in module has no `!default` to claim: configuring it fails
        // before anything loads (`load_css/error/with` `core_module`).
        if url.starts_with("sass:") && !view.is_empty() {
            return Err(SassError::new(
                ErrorKind::Config,
                format!("Built-in module {url} can't be configured."),
            ));
        }

        // Already-loaded with a live configuration: the direct target is named
        // by file (`_other.scss was already loaded…`); forwarded distribution
        // keeps the generic spelling, which `load_module` raises for the
        // nested frames.
        self.css_config_precheck(&url, &view, entry)?;

        let link = self.link_to(&inc.span, "@use");
        let (_module, _ran) = self
            .load_module(&url, view, out, true, link, entry)
            .map_err(|err| self.attach_frames(err))?;
        // No binding: `@use`s stay private to the loaded module.
        self.check_config_site(site, true)
    }

    /// Shared already-loaded pre-check for the load-css/load families (the
    /// direct frame spells the target by file name; see `run_load_css`).
    pub(crate) fn css_config_precheck(
        &mut self,
        url: &str,
        view: &[(String, usize)],
        entry: crate::span::Span,
    ) -> Ev<()> {
        if view.is_empty() || url.starts_with("sass:") {
            return Ok(());
        }
        let base = crate::eval::eval_module::parent_dir(&self.name);
        if let Ok(resolved) = crate::loader::canonicalize_with(url, &base, &self.load_paths) {
            let key = resolved.path.display().to_string();
            if self.bus.is_open(&key) {
                let module = self.bus.share(&key)?;
                if view.iter().any(|(name, idx)| {
                    !self.config_offers[*idx].consumed
                        && module.configurable.iter().any(|c| c == name)
                }) {
                    let file = key.rsplit('/').next().unwrap_or(&key).to_string();
                    return Err(SassError::new(
                        ErrorKind::Config,
                        format!(
                            "{file} was already loaded, so it can't be configured using \"with\"."
                        ),
                    ))
                ;
                }
            }
        }
        let _ = entry;
        Ok(())
    }

    /// Turn a `$with:` map into offers on `site` — shared by `meta.load-css`
    /// and `meta.load` (identical golden spellings: `$with: … is not a map.`,
    /// `$with key: … is not a string.`, `The variable $x was configured
    /// twice.`).
    pub(crate) fn prepare_with(
        &mut self,
        with_v: Option<SassValue>,
        site: usize,
        entry: crate::span::Span,
    ) -> Ev<Vec<(String, usize)>> {
        let mut view: Vec<(String, usize)> = Vec::new();
        match &with_v {
            None | Some(SassValue::Null) => {}
            Some(SassValue::Map { entries }) => {
                let mut seen: Vec<String> = Vec::new();
                for (k, v) in entries {
                    let name = match k {
                        SassValue::Str { text, .. } => text.clone(),
                        other => {
                            return Err(SassError::new(
                                ErrorKind::Builtins,
                                format!("$with key: {} is not a string.", other.to_css()),
                            ));
                        }
                    };
                    let key = crate::parser::config_key(&name);
                    if seen.contains(&key) {
                        return Err(SassError::new(
                            ErrorKind::Config,
                            format!("The variable ${name} was configured twice."),
                        ));
                    }
                    seen.push(key.clone());
                    let _ = key;
                    view.push(self.offer_value(name, v.clone(), site, entry));
                }
            }
            Some(other) => {
                return Err(SassError::new(
                    ErrorKind::Builtins,
                    format!("$with: {} is not a map.", other.to_css()),
                ));
            }
        }
        Ok(view)
    }
}
