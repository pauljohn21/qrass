//! Module loading for [`EvalState`] (tasks 6.2 / 6.3): `@use` and `@forward`.
//!
//! Both canonicalize a URL against the importer's directory and instantiate each
//! canonical path exactly once in its own global scope, publishing the module's
//! CSS at that first load. `@use` binds the exports under a namespace; `@forward`
//! re-exports them (with `show`/`hide`/`as prefix*`) into the current module's
//! export set. `@use … with` / `@forward … with` configures the target's
//! `!default` variables; a supplied-but-unconsumed name is an error (design D3).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{Level, span};

use super::branch_fanout::{Prepared, acquire, branch_key};
use super::eval_bind::filter_forwarded;
use crate::ast::{Forward, Use};
use crate::clock::StopWatch;
use crate::error::{ErrorKind, Frame, SassError};
use crate::loader::{SourceKind, canonicalize_with};
use crate::module::{Module, export_vars};
use crate::span::{BytePos, Span, line_col};

use super::{Ev, EvalState, LoadLink};

impl EvalState {
    /// Resolve and load one `@use`, emitting the module's CSS (once) and binding
    /// its namespace into the current scope. Its `with (…)` entries become offers
    /// that the target's `!default` declarations must claim, or the site reports
    /// them (`error/with/undefined.hrx`).
    pub(super) fn handle_use(&mut self, u: &Use, out: &mut Vec<String>) -> Ev<()> {
        let site = self.open_config_site();
        let view = self.offer_config(&u.config, Some(site))?;
        let link = self.link_to(&u.span, "@use");
        let (module, _ran) = self
            .load_module(&u.url, view, out, true, link, u.span)
            .map_err(|err| self.attach_frames(err))?;
        self.bind_alias(u, &module, &u.url);
        self.check_config_site(site)
    }

    /// Resolve and load one `@forward`, re-exporting the target's members into
    /// the current module's export buffer, and handing it the configuration view
    /// the forward's filters / `with` clause resolve to.
    ///
    /// A `with (…)` entry *without* `!default` is a hard value this statement
    /// owns, so it gets its own site: `directives/forward/error/with.hrx`
    /// `undefined`/`not_default`/`namespace`/`nested` demand that an entry the
    /// chain never declared be reported here, in the file that wrote it, while
    /// `through_forward/*` demand the same from the entry stylesheet (one frame).
    pub(super) fn handle_forward(&mut self, f: &Forward, out: &mut Vec<String>) -> Ev<()> {
        let site = self.open_config_site();
        let view = self.forward_config_view(f, site)?;
        let link = self.link_to(&f.span, "@forward");
        // A forward never settles the configuration it passes on: an offer the
        // target cannot claim still has to be reported by (or claimed later in)
        // the `@use` site that created it — `with_unrelated_config` and
        // `error/with/missing_distributed_vars.hrx` both depend on that.
        let (module, ran) = self
            .load_module(&f.url, view, out, false, link, f.span)
            .map_err(|err| self.attach_frames(err))?;
        let members = filter_forwarded(module.vars.clone(), &f.shown, &f.hidden, &f.prefix);
        if let Some(buf) = self.forward_buf.last_mut() {
            buf.extend(members);
        }
        // Only a body that actually ran could have claimed the entries; a cached
        // or plain-CSS target has no `!default` to claim them with, and the
        // golden gives that case its own message (`multi_configuration`).
        if ran {
            self.check_config_site(site)?;
        }
        Ok(())
    }

    /// The load-stack link a `@use`/`@forward` at `span` creates, resolved
    /// against the file that wrote it.
    fn link_to(&self, span: &Span, keyword: &'static str) -> LoadLink {
        let lc = line_col(&self.content, span.start);
        LoadLink {
            parent_file: self.name.clone(),
            parent_line: lc.line,
            parent_col: lc.col,
            keyword,
        }
    }

    /// Run `body` with `link` on the load stack, and — if it failed — caption the
    /// error with the frames the goldens print under the snippet. The innermost
    /// frame is the error's own location; each ancestor is the link above it.
    fn with_link<T>(&mut self, link: LoadLink, body: impl FnOnce(&mut Self) -> Ev<T>) -> Ev<T> {
        self.links.push(link);
        let traced = match body(self) {
            Ok(value) => Ok(value),
            Err(err) => Err(self.attach_frames(err)),
        };
        self.links.pop();
        traced
    }

    fn attach_frames(&self, err: SassError) -> SassError {
        let Some(loc) = err.located.as_ref() else {
            return err;
        };
        if err.has_trace() || self.links.is_empty() {
            return err;
        }
        let mut frames = Vec::with_capacity(self.links.len() + 1);
        frames.push(Frame {
            file: loc.file.clone(),
            line: loc.line,
            col: loc.col,
            label: self.links.last().expect("pushed link").keyword.to_string(),
        });
        for i in (0..self.links.len()).rev() {
            let link = &self.links[i];
            frames.push(Frame {
                file: link.parent_file.clone(),
                line: link.parent_line,
                col: link.parent_col,
                label: if i == 0 {
                    "root stylesheet"
                } else {
                    self.links[i - 1].keyword
                }
                .to_string(),
            });
        }
        err.with_trace(frames)
    }

    /// The golden's `already loaded` error: raised at the statement that tried
    /// to re-configure a settled module, underlining that statement (its
    /// `new load` run) in the file that wrote it.
    fn already_loaded(&self, stmt: Span) -> SassError {
        let (start, end) = underline(&self.content, stmt);
        let mut marker = crate::diag::marker_in(&self.content, '^', start, end);
        marker.label = Some("new load".to_string());
        SassError::new(
            ErrorKind::Config,
            "This module was already loaded, so it can't be configured using \"with\".",
        )
        .with_located(crate::diag::located_in(&self.name, &self.content, start))
        .with_markers(vec![marker])
    }

    /// Canonicalize, cycle-check, and load a module (cached ⇒ no re-emit).
    /// Returns `(module, did_the_body_run)`.
    ///
    /// `direct` says the load comes from a `@use`: its own `with (…)` entries are
    /// then settled when no body runs to claim them, because reporting them as
    /// "not declared with !default" would blame the wrong statement
    /// (`error/with/multi_configuration.hrx`). `stmt` is the loading statement's
    /// own span, recorded as the target's `original load` and underlined when a
    /// later load tries to re-open the same configuration.
    fn load_module(
        &mut self,
        url: &str,
        view: Vec<(String, usize)>,
        out: &mut Vec<String>,
        direct: bool,
        link: LoadLink,
        stmt: Span,
    ) -> Ev<(Arc<Module>, bool)> {
        // Cross-file pipeline boundary: one span per load so a module graph's
        // instantiation chain is readable as nested spans with timings.
        let span = span!(
            Level::DEBUG,
            "module_load",
            stage = "load_module",
            module = %self.name,
            url = %url,
            id = tracing::field::Empty,
            cached = tracing::field::Empty,
            hubs = tracing::field::Empty,
            elapsed_ms = tracing::field::Empty,
        );
        let _guard = span.enter();
        let started = StopWatch::start();
        // Built-in modules (`sass:math`, `sass:color`, …) have no source file: their
        // functions resolve through the builtin registry by namespace at call time
        // and their constants (`math.$pi`, …) through the `module_const` fallback, so
        // `@use`/`@forward` of a `sass:` URL is a no-op that binds nothing.
        if url.starts_with("sass:") {
            if direct {
                self.settle_offers(&view);
            }
            return Ok((Arc::new(Module::default()), false));
        }
        let base = parent_dir(&self.name);
        // The fan-out stage may already have acquired this branch. Reuse its
        // canonical path so the cycle and singleton checks below see exactly what
        // the sequential walk computed.
        let key = match self.prepared.get(&branch_key(&base, url)) {
            Some(Ok(prepared)) => prepared.key.clone(),
            // The fan-out's canonicalize failed: raise it here, which is where
            // `canonicalize_with` used to fail — before any check has a path to
            // test against.
            Some(Err(err)) => return Err(err.clone()),
            None => canonicalize_with(url, &base, &self.load_paths)?
                .path
                .display()
                .to_string(),
        };
        span.record("id", key.as_str());

        if self.stack.contains(&key) {
            let mut chain = self.stack.clone();
            chain.push(key);
            return Err(SassError::new(
                ErrorKind::Cycle,
                "It is a circular dependency: @use/@forward loops back on itself",
            )
            .note(chain.join(" -> ")));
        }

        if self.bus.is_open(&key) {
            // A carrier already exists: join its multicast — the body must not run
            // a second time for this parent.
            let module = self.bus.share(&key)?;
            // Re-opening a configuration the first load already settled: the body
            // will not run again, so no `!default` can claim the offer — the
            // golden gives that its own message rather than the "not declared
            // with !default" one. Only an offer still *unclaimed* re-opens
            // anything: a diamond that forwards the same module twice hands the
            // second branch the offers the first already settled, which
            // `with/distributed_vars.hrx` must let pass. A module with nothing
            // configurable cannot be re-opened, so passing values at it stays
            // silent.
            if view.iter().any(|(name, idx)| {
                !self.config_offers[*idx].consumed && module.configurable.iter().any(|c| c == name)
            }) {
                return Err(self.already_loaded(stmt));
            }
            // Cached: the body already ran, so nothing here can claim the offers.
            if direct {
                self.settle_offers(&view);
            }
            span.record("cached", true);
            span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));
            return Ok((module, false)); // settled: no CSS, no re-run
        }

        // Open the carrier *before* the body runs, so a request that arrives while
        // this one is still instantiating joins the same multicast (the "or still in
        // flight" half of the singleton contract).
        self.bus.open(&key);

        // Consume the acquisition. A fan-out hit arrives already read and parsed;
        // a miss (a branch the fan-out skipped, or a repeated URL) acquires it now
        // through the *same* unit, so a miss cannot differ from a hit.
        let prepared = match self.prepared.remove(&branch_key(&base, url)) {
            Some(Ok(prepared)) => prepared,
            Some(Err(err)) => return Err(err),
            None => acquire(url, &base, &self.load_paths)?,
        };
        let ran = prepared.resolved.kind != SourceKind::Css;
        let module = if !ran {
            // Plain-CSS mode: the `.css` payload is passed through verbatim, but
            // SCSS-only constructs inside it must still be rejected the way
            // the goldens would. When the content does not even parse (an exotic
            // CSS feature the SCSS parser cannot read) keep the prior passthrough
            // instead of reporting a spurious error.
            if let Ok(program) = &prepared.program {
                crate::plain_css::validate(program)?;
            }
            if direct {
                self.settle_offers(&view);
            }
            Module {
                css: vec![prepared.content],
                vars: Vec::new(),
                ..Default::default()
            }
        } else {
            let Prepared {
                content, program, ..
            } = prepared;
            self.with_link(link, |st| st.instantiate(&key, &content, program, &view))?
        };

        let module = Arc::new(module);
        // Broadcast the singleton and close the carrier. This parent already holds
        // the `Arc` it just produced, so it takes it directly; every later parent
        // lands on the `is_open` branch above and joins the same carrier — which is
        // why the payload keeps its `None` ("not settled yet") state: a request that
        // arrived mid-body would join this instantiation instead of starting another.
        self.bus.settle(&key, module.clone())?;
        // The singleton set's size: a diamond graph must grow it by exactly one.
        span.record("hubs", self.bus.len());
        out.extend(module.css.iter().cloned());
        span.record("cached", false);
        span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));
        Ok((module, ran))
    }

    /// Run a module body in a fresh global scope, returning its CSS + exports.
    /// Nested `@use`/`@forward` share `self.bus`/`self.stack`, so a diamond
    /// loads once. The incoming configuration is *not* pre-bound: the body sees it
    /// only through its own `!default` declarations (`with/variable_exists.hrx`
    /// wants `meta.variable-exists` false before the first one).
    ///
    /// `program` is the body's front-end output as acquired by the fan-out stage
    /// (or inline at the load site) — never parsed here.
    fn instantiate(
        &mut self,
        key: &str,
        content: &str,
        program: Result<Vec<crate::ast::Stmt>, SassError>,
        view: &[(String, usize)],
    ) -> Ev<Module> {
        // One span per module body, nested inside its `module_load` parent, so a
        // diamond graph reads as a tree of instantiation costs.
        let span = span!(
            Level::DEBUG,
            "module_instantiate",
            stage = "instantiate",
            module = %key,
            stmts = tracing::field::Empty,
            elapsed_ms = tracing::field::Empty,
        );
        let _guard = span.enter();
        let started = StopWatch::start();
        // A parse failure surfaces here — the same point as the old inline parse,
        // before any arena state is touched.
        let program = program?;
        span.record("stmts", program.len());

        let outer_env = std::mem::take(&mut self.env);
        let outer_owner = std::mem::replace(&mut self.env_owner, key.to_string());
        let outer_file = std::mem::replace(&mut self.file, 0);
        let outer_name = std::mem::replace(&mut self.name, key.to_string());
        let outer_content = std::mem::replace(&mut self.content, content.to_string());
        self.stack.push(key.to_string());
        self.forward_buf.push(Vec::new());
        self.config_stack.push(view.to_vec());

        // This body's own `@use`/`@forward` statements are independent branches:
        // fan their source acquisition out over the `Local` stage before any of
        // them mutates the arena (design D1, manual §2). It runs inside the frame
        // so the base directory is *this* module's and the singleton/cycle filter
        // sees the stack entry just pushed.
        self.prefetch_branches(&program);

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
            let mut configurable = Vec::new();
            for stmt in &program {
                match stmt {
                    crate::ast::Stmt::Variable(v) if v.default => {
                        if !crate::module::is_private(&v.name) {
                            let key = crate::parser::config_key(&v.name);
                            if !configurable.contains(&key) {
                                configurable.push(key);
                            }
                        }
                    }
                    crate::ast::Stmt::Mixin(m) => {
                        let n = st.render_interp(&m.name)?;
                        if !crate::module::is_private(&n) {
                            mixins.push(n);
                        }
                    }
                    crate::ast::Stmt::Function(f) if !crate::module::is_private(&f.name) => {
                        functions.push(f.name.clone());
                    }
                    _ => {}
                }
            }
            Ok(Module {
                css,
                vars,
                mixins,
                functions,
                configurable,
            })
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
        span.record("elapsed_ms", tracing::field::debug(started.elapsed_ms()));

        ran
    }
}

/// The underlineable text of a `@use`/`@forward` statement: from its keyword to
/// just before the terminating `;`, which the goldens never mark
/// (`multi_configuration/one_file` underlines 31 of the 32 characters of
/// `@use "other" as o2 with ($a: b);`).
fn underline(content: &str, span: Span) -> (BytePos, BytePos) {
    let start = span.start.0.min(content.len());
    let end = span.end.0.clamp(start, content.len());
    let text = &content[start..end];
    (
        span.start,
        BytePos(start + text.trim_end_matches([';', ' ', '\t', '\n', '\r']).len()),
    )
}

/// Directory containing `path` (empty for a bare relative name) — the base a
/// relative `@use`/`@forward` URL resolves against.
pub(crate) fn parent_dir(path: &str) -> PathBuf {
    Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default()
}
