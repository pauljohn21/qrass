//! `with (…)` configuration for [`EvalState`]: how an `@use … with` offer
//! travels down a `@forward` chain, how a `!default` declaration claims it, and
//! what an unclaimed offer reports. Split out of `eval_module.rs` (which owns
//! loading) as a second `impl EvalState` block.
//!
//! The rules are read off the `directives/use/with` goldens: a configuration is
//! never pre-bound as a variable (`with/variable_exists.hrx` wants
//! `meta.variable-exists` false before the declaration), a `!default`
//! declaration that sees an offer does not evaluate its own right-hand side
//! (`with/doesnt_run_default.hrx`), a `null` offer does not satisfy a
//! `!default` (`with/null.hrx`), names match across `-`/`_`
//! (`with/dash_insensitive.hrx`), and a forward hands its unresolved offers on
//! to its target (`with/through_forward.hrx`, `with/distributed_vars.hrx`).

use crate::ast::{ConfigEntry, Forward};
use crate::error::{ErrorKind, SassError};
use crate::parser::config_key;
use crate::value::SassValue;
use tracing::debug_span;

use super::{ConfigOffer, EvalState, Ev};

impl EvalState {
    /// Allocate a `@use … with` site id: the offers it creates are checked for
    /// being claimed once the target module has run.
    pub(crate) fn open_config_site(&mut self) -> usize {
        self.config_sites.push(Vec::new());
        self.config_sites.len() - 1
    }

    /// Evaluate a `with (…)` clause in the *importer's* environment and register
    /// its values under `site`. Returns the frame view (`normalized name`, offer
    /// index) the target module is instantiated with.
    pub(crate) fn offer_config(
        &mut self,
        entries: &[ConfigEntry],
        site: Option<usize>,
    ) -> Ev<Vec<(String, usize)>> {
        let span = debug_span!("with_config", stage = "offer", module = %self.name, id = site.map_or(-1, |s| s as i64), entries = entries.len());
        let _entered = span.entered();
        let mut view: Vec<(String, usize)> = Vec::new();
        for e in entries {
            let value = self.eval_config_value(e)?;
            view.push(self.register_offer(config_key(&e.name), value, site, e));
        }
        Ok(view)
    }

    /// Claim every offer of `view` without running a module: the target was
    /// already loaded, is plain CSS, or is a built-in, so no `!default`
    /// declaration can report it. `error/with/multi_configuration.hrx` wants a
    /// dedicated message for that case, which is not implemented yet; staying
    /// silent keeps those cases at their previous outcome.
    pub(crate) fn settle_offers(&mut self, view: &[(String, usize)]) {
        for (_, idx) in view {
            self.config_offers[*idx].consumed = true;
        }
    }

    /// The view a `@forward` hands its target: the incoming offers that survive
    /// `show`/`hide`/`as` (renamed by the `as` prefix), overridden by whatever
    /// the forward's own `with (…)` clause resolves to. Entries the statement
    /// hard-coded (no `!default`) are recorded under `site`, which the forwarder
    /// checks once the target has run.
    pub(crate) fn forward_config_view(
        &mut self,
        f: &Forward,
        site: usize,
    ) -> Ev<Vec<(String, usize)>> {
        let span = debug_span!("with_config", stage = "forward", module = %self.name, url = %f.url, clauses = f.config.len());
        let _entered = span.entered();
        let incoming = self.config_stack.last().cloned().unwrap_or_default();
        let mut view: Vec<(String, usize)> = Vec::new();
        for (name, idx) in incoming {
            let shown = f.shown.is_empty()
                || f.shown.iter().any(|s| config_key(s) == name)
                || f.shown.iter().any(|s| config_key(s) == stripped(&name, &f.prefix));
            let hidden = f.hidden.iter().any(|s| config_key(s) == name);
            if !hidden && shown {
                view.push((stripped(&name, &f.prefix), idx));
            }
        }
        for e in &f.config {
            let key = config_key(&e.name);
            // A `!default` clause is pass-through: an incoming entry of the same
            // name is claimed here whichever value wins — `with/null.hrx` still
            // resolves a `null` offer, it just does not travel down — while a
            // clause without `!default` is a hard value that leaves the
            // importer's entry unclaimed, which
            // `error/with/through_forward.hrx` `with` reports.
            let upstream = if e.default {
                view.iter().find(|(n, _)| *n == key).map(|(_, i)| *i)
            } else {
                None
            };
            let offered = match upstream {
                Some(idx) => {
                    let offer = &mut self.config_offers[idx];
                    offer.consumed = true;
                    Some(offer.value.clone())
                }
                None => None,
            };
            let value = match offered {
                None | Some(SassValue::Null) => self.eval_config_value(e)?,
                Some(value) => value,
            };
            let idx = self
                .register_offer(key.clone(), value, if e.default { None } else { Some(site) }, e)
                .1;
            if let Some(slot) = view.iter().position(|(n, _)| *n == key) {
                view[slot].1 = idx;
            } else {
                view.push((key, idx));
            }
        }
        Ok(view)
    }

    /// Claim the offer a `!default` declaration matches, if any. Returning
    /// `Some` means the declaration's own value must not be evaluated — except
    /// when the offered value is `null`, which the golden lets the default win.
    pub(crate) fn take_config(&mut self, name: &str) -> Option<SassValue> {
        let key = config_key(name);
        let idx = self.config_stack.last()?.iter().find(|(n, _)| *n == key)?.1;
        self.config_offers[idx].consumed = true;
        match self.config_offers[idx].value.clone() {
            SassValue::Null => None,
            value => Some(value),
        }
    }

    /// Report the first offer of `site` no `!default` declaration claimed,
    /// underlining the entry in the importing file exactly as the golden does.
    pub(crate) fn check_config_site(&self, site: usize, load_css: bool) -> Ev<()> {
        let Some(idx) = self.config_sites[site]
            .iter()
            .copied()
            .find(|i| !self.config_offers[*i].consumed)
        else {
            return Ok(());
        };
        let span = self.config_offers[idx].entry;
        let source = self.content.clone();
        let file = self.name.clone();
        // The two families spell the same failure differently in the goldens:
        // `@use … with` blames "This variable" while `meta.load-css($with:)`
        // quotes the name (`$a was not declared with !default in the @used
        // module.`).
        let message = if load_css {
            format!(
                "${} was not declared with !default in the @used module.",
                self.config_offers[idx].name
            )
        } else {
            "This variable was not declared with !default in the @used module.".to_string()
        };
        Err(SassError::new(ErrorKind::Config, message)
            .with_located(crate::diag::located_in(&file, &source, span.start))
            .with_markers(vec![crate::diag::marker_in(&source, '^', span.start, span.end)]))
    }

    /// Evaluate one clause value, anchoring a failure on the value text the way
    /// `error/with/invalid_expression.hrx` does when the error carries no place.
    fn eval_config_value(&mut self, e: &ConfigEntry) -> Ev<SassValue> {
        match self.eval_expr(&e.value) {
            Err(err) if err.located.is_none() => {
                let source = self.content.clone();
                let file = self.name.clone();
                let mut err = err;
                err.located = Some(crate::diag::located_in(
                    &file,
                    &source,
                    e.value_span.start,
                ));
                Err(err.with_markers(vec![crate::diag::marker_in(
                    &source,
                    '^',
                    e.value_span.start,
                    e.value_span.end,
                )]))
            }
            other => other,
        }
    }

    /// Add one offer to the shared arena and index it under `site`; returns the
    /// `(normalized name, arena index)` pair a frame view is made of.
    fn register_offer(
        &mut self,
        key: String,
        value: SassValue,
        site: Option<usize>,
        e: &ConfigEntry,
    ) -> (String, usize) {
        let idx = self.config_offers.len();
        self.config_offers.push(ConfigOffer {
            name: e.name.clone(),
            value,
            consumed: false,
            entry: e.entry,
        });
        if let Some(site) = site {
            self.config_sites[site].push(idx);
        }
        (key, idx)
    }

    /// Register an offer from an already-evaluated value (the `meta.load-css`
    /// `$with:` map — the values are literals in the calling expression, no
    /// `ConfigEntry` text exists). Returns the frame view entry.
    pub(crate) fn offer_value(
        &mut self,
        name: String,
        value: SassValue,
        site: usize,
        entry: crate::span::Span,
    ) -> (String, usize) {
        let idx = self.config_offers.len();
        self.config_offers.push(ConfigOffer {
            name,
            value,
            consumed: false,
            entry,
        });
        self.config_sites[site].push(idx);
        (crate::parser::config_key(&self.config_offers[idx].name.clone()), idx)
    }
}

/// Drop a forward's `as` prefix from a configuration name, so `as b-*` makes the
/// importer's `b-a` reach the target's `$a` (`with/through_forward.hrx` `as`).
fn stripped(name: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return name.to_string();
    }
    name.strip_prefix(prefix)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{prefix}{name}"))
}
