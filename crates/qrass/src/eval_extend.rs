//! `@extend` machinery (task 7.5, design D4).
//!
//! `@extend` is a *global* constraint: a placeholder's final selector set is only
//! known once the whole module is traversed (a placeholder may be defined after
//! its extend sites, and every extending selector inherits the placeholder's
//! declarations). So collection is split across the traversal and resolution:
//!
//! * [`EvalState::run_extend`] records an `@extend` site (target + the concrete
//!   selectors of the enclosing rule) while expanding;
//! * rules whose selector contains a placeholder are held back in
//!   [`EvalState::placeholder_rules`] by `run_ruleset` and never rendered alone;
//! * [`EvalState::extend_pass`] — invoked by the eval fold after every statement
//!   is collected — substitutes each placeholder with every concrete selector
//!   that extends it (the `collect → pass → flat_map` re-emit of D4) and drops
//!   the placeholders themselves.
//!
//! Scope (per D4): same-module placeholder merging, `!optional`, interpolated
//! placeholder names and empty placeholders that emit nothing. Cross-module
//! extends and complex combinators are left to the runner allowlist.

use std::collections::{HashMap, HashSet};

use crate::ast::Extend;
use crate::error::{ErrorKind, SassError};

use super::{Ev, EvalState};

impl EvalState {
    /// Record an `@extend <target> [!optional]` site. The extending selectors are
    /// the resolved selectors of the enclosing rule (`sel_ctx`); `@extend` at the
    /// sheet root (no enclosing rule) is an error.
    pub(super) fn run_extend(&mut self, e: &Extend, sel_ctx: &[String]) -> Ev<()> {
        let target = self.render_interp(&e.selector)?.trim().to_string();
        // one eligibility check: an extend needs both a target and an enclosing
        // rule's resolved selectors
        let recordable = !target.is_empty() && !sel_ctx.is_empty();
        recordable.then_some(()).ok_or_else(|| {
            SassError::new(
                ErrorKind::Eval,
                "You may only extend a selector within a rule",
            )
        })?;
        self.extends.extend(
            sel_ctx
                .iter()
                .map(|selector| (target.clone(), selector.clone(), e.optional)),
        );
        Ok(())
    }

    /// Global resolution pass: for every held-back placeholder rule, substitute
    /// its in-stream source-position marker with a *single grouped rule* whose
    /// selector list is every concrete selector that extends it — in reverse
    /// source order, matching the goldens' `@extend` output. Placeholders nothing
    /// extends (or with empty declarations) have their marker dropped. Errors on a
    /// non-`!optional` extend whose target was never defined.
    pub(crate) fn extend_pass(&mut self, chunks: &mut Vec<String>) -> Ev<()> {
        // Placeholder tokens actually defined in this module.
        let defined: HashSet<String> = self
            .placeholder_rules
            .iter()
            .flat_map(|pr| placeholder_tokens(&pr.selector))
            .collect();

        // Every non-optional extend must resolve to a defined placeholder.
        self.extends
            .iter()
            .filter(|(_, _, optional)| !*optional)
            .try_fold((), |(), (target, _, _)| {
                defined.contains(target).then_some(()).ok_or_else(|| {
                    SassError::new(
                        ErrorKind::Eval,
                        format!("\"{target}\" is not a known selector for @extend"),
                    )
                })
            })?;

        // Placeholder token -> concrete selectors that extend it, in reverse
        // source-of-extend order (dedup first, then reverse) to match the goldens.
        let by_target: HashMap<String, Vec<String>> = self.extends.iter().fold(
            HashMap::new(),
            |mut acc, (target, concrete, _optional)| {
                push_front_unique(acc.entry(target.clone()).or_default(), concrete);
                acc
            },
        );

        // Aggregate placeholder rules by selector. The same placeholder can be
        // defined more than once (e.g. a share-rule `@include`d inside a mixin that
        // iterates over a state list); the goldens emit a single grouped rule per
        // unique selector, at the *first* definition site, carrying the merged
        // declarations of every duplicate. Extra markers are dropped.
        struct Group {
            primary: usize,
            slots: Vec<usize>,
            decls: Vec<(String, String)>,
        }
        let (order, groups): (Vec<String>, HashMap<String, Group>) = self
            .placeholder_rules
            .iter()
            .fold((Vec::new(), HashMap::new()), |(mut order, mut groups), pr| {
                let g = groups.entry(pr.selector.clone()).or_insert_with(|| {
                    order.push(pr.selector.clone());
                    Group {
                        primary: pr.slot,
                        slots: Vec::new(),
                        decls: Vec::new(),
                    }
                });
                g.slots.push(pr.slot);
                let fresh: Vec<(String, String)> = pr
                    .decls
                    .iter()
                    .filter(|d| !g.decls.contains(*d))
                    .cloned()
                    .collect();
                g.decls.extend(fresh);
                (order, groups)
            });

        // Resolve each unique placeholder selector into its grouped replacement.
        order.iter().for_each(|selector| {
            let g = groups.get(selector).expect("group registered in order");
            // A selector that expands to nothing (a token no rule extends, or
            // empty declarations) contributes no CSS: every marker is dropped.
            let rendered = expand_selector(selector, &by_target)
                .filter(|expansions| !expansions.is_empty() && !g.decls.is_empty())
                .map(|expansions| {
                    let items: Vec<crate::css::Item> = g
                        .decls
                        .iter()
                        .map(|(prop, value)| crate::css::Item::Decl {
                            prop: prop.clone(),
                            value: value.clone(),
                        })
                        .collect();
                    crate::css::render_rule(&expansions, &items)
                });
            place_group(chunks, &g.slots, g.primary, rendered);
        });

        // Defensive: strip any stray marker (e.g. a placeholder whose slot was
        // never reached) so no sentinel text can leak into the CSS output.
        chunks.retain(|c| !c.starts_with('\u{f0000}'));
        self.placeholder_rules.clear();
        Ok(())
    }
}

/// The in-stream marker chunk reserved for placeholder slot `slot`.
fn extend_marker(slot: usize) -> String {
    format!("\u{f0000}qrass_extend_slot {slot}\u{f0001}")
}

/// Prepend `concrete` unless already present: the goldens dedup by first
/// occurrence and then read the list in reverse, so front insertion is exact.
fn push_front_unique(list: &mut Vec<String>, concrete: &str) {
    let present = list.iter().any(|s| s.as_str() == concrete);
    if !present {
        list.insert(0, concrete.to_string());
    }
}

/// Cross-product substitution of `selector`'s placeholder tokens with the
/// concrete selectors extending each; `None` when any token is unextended.
fn expand_selector(
    selector: &str,
    by_target: &HashMap<String, Vec<String>>,
) -> Option<Vec<String>> {
    placeholder_tokens(selector)
        .iter()
        .try_fold(vec![selector.to_string()], |acc, tok| {
            let concretes = by_target.get(tok)?;
            Some(
                acc.iter()
                    .flat_map(|sel| {
                        concretes
                            .iter()
                            .map(|c| sel.replace(tok.as_str(), c.as_str()))
                    })
                    .collect(),
            )
        })
}

/// Consume a group's slot markers: the rendering (when one exists) lands on the
/// primary marker — or the first surviving one — and every marker is dropped.
fn place_group(
    chunks: &mut Vec<String>,
    slots: &[usize],
    primary: usize,
    rendered: Option<String>,
) {
    let markers: Vec<String> = slots.iter().map(|slot| extend_marker(*slot)).collect();
    let pos = chunks
        .iter()
        .position(|c| *c == extend_marker(primary))
        .or_else(|| markers.iter().find_map(|m| chunks.iter().position(|c| c == m)));
    rendered
        .zip(pos)
        .into_iter()
        .for_each(|(rule, p)| chunks[p] = rule);
    chunks.retain(|c| !markers.contains(c));
}

/// Extract the `%name` placeholder tokens from a selector (a `%` followed by
/// identifier characters `[A-Za-z0-9_-]`). `%size:hover` yields `%size`;
/// `%shared-box` yields `%shared-box`.
fn placeholder_tokens(selector: &str) -> Vec<String> {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    selector
        .split('%')
        .skip(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| is_ident(*c))
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .map(|name| format!("%{name}"))
        .collect()
}
