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
        if target.is_empty() || sel_ctx.is_empty() {
            return Err(SassError::new(
                ErrorKind::Eval,
                "You may only extend a selector within a rule",
            ));
        }
        for selector in sel_ctx {
            self.extends
                .push((target.clone(), selector.clone(), e.optional));
        }
        Ok(())
    }

    /// Global resolution pass: for every held-back placeholder rule, substitute
    /// its in-stream source-position marker with a *single grouped rule* whose
    /// selector list is every concrete selector that extends it — in reverse
    /// source order, matching dart-sass's `@extend` output. Placeholders nothing
    /// extends (or with empty declarations) have their marker dropped. Errors on a
    /// non-`!optional` extend whose target was never defined.
    pub(crate) fn extend_pass(&mut self, chunks: &mut Vec<String>) -> Ev<()> {
        // Placeholder tokens actually defined in this module.
        let mut defined: HashSet<String> = HashSet::new();
        for pr in &self.placeholder_rules {
            for tok in placeholder_tokens(&pr.selector) {
                defined.insert(tok);
            }
        }

        // Every non-optional extend must resolve to a defined placeholder.
        for (target, _sel, optional) in &self.extends {
            if !*optional && !defined.contains(target) {
                return Err(SassError::new(
                    ErrorKind::Eval,
                    format!("\"{target}\" is not a known selector for @extend"),
                ));
            }
        }

        // Placeholder token -> concrete selectors that extend it, in reverse
        // source-of-extend order (dedup first, then reverse) to match dart-sass.
        let mut by_target: HashMap<String, Vec<String>> = HashMap::new();
        for (target, concrete, _optional) in &self.extends {
            let list = by_target.entry(target.clone()).or_default();
            if !list.iter().any(|s| s == concrete) {
                list.push(concrete.clone());
            }
        }
        for list in by_target.values_mut() {
            list.reverse();
        }

        // Aggregate placeholder rules by selector. The same placeholder can be
        // defined more than once (e.g. a share-rule `@include`d inside a mixin that
        // iterates over a state list); dart-sass emits a single grouped rule per
        // unique selector, at the *first* definition site, carrying the merged
        // declarations of every duplicate. Extra markers are dropped.
        struct Group {
            primary: usize,
            slots: Vec<usize>,
            decls: Vec<(String, String)>,
        }
        let mut order: Vec<String> = Vec::new();
        let mut groups: HashMap<String, Group> = HashMap::new();
        for pr in &self.placeholder_rules {
            let g = groups.entry(pr.selector.clone()).or_insert_with(|| {
                order.push(pr.selector.clone());
                Group {
                    primary: pr.slot,
                    slots: Vec::new(),
                    decls: Vec::new(),
                }
            });
            g.slots.push(pr.slot);
            for d in &pr.decls {
                if !g.decls.iter().any(|e| e == d) {
                    g.decls.push(d.clone());
                }
            }
        }

        // Resolve each unique placeholder selector into its grouped replacement.
        for selector in &order {
            let g = groups.get(selector).expect("group registered in order");
            // Expand the selector by substituting each placeholder token with the
            // concrete selectors that extend it (a cross product when several).
            let mut expansions: Vec<String> = vec![selector.clone()];
            let mut drop = false;
            for tok in placeholder_tokens(selector) {
                let Some(concretes) = by_target.get(&tok) else {
                    drop = true;
                    break;
                };
                let mut next: Vec<String> = Vec::new();
                for sel in &expansions {
                    for c in concretes {
                        next.push(sel.replace(tok.as_str(), c.as_str()));
                    }
                }
                expansions = next;
            }

            // A selector that expands to nothing (empty declarations, or a token no
            // rule extends) contributes no CSS: strip all of its markers.
            if drop || expansions.is_empty() || g.decls.is_empty() {
                for slot in &g.slots {
                    let m = extend_marker(*slot);
                    if let Some(p) = chunks.iter().position(|c| *c == m) {
                        chunks.remove(p);
                    }
                }
                continue;
            }

            let items: Vec<crate::css::Item> = g
                .decls
                .iter()
                .map(|(prop, value)| crate::css::Item::Decl {
                    prop: prop.clone(),
                    value: value.clone(),
                })
                .collect();
            let primary_marker = extend_marker(g.primary);
            match chunks.iter().position(|c| *c == primary_marker) {
                Some(pos) => chunks[pos] = crate::css::render_rule(&expansions, &items),
                None => {
                    // Primary marker missing: fall back to the first surviving one.
                    let mut emitted = false;
                    for slot in &g.slots {
                        let m = extend_marker(*slot);
                        if let Some(p) = chunks.iter().position(|c| *c == m) {
                            if emitted {
                                chunks.remove(p);
                            } else {
                                chunks[p] = crate::css::render_rule(&expansions, &items);
                                emitted = true;
                            }
                        }
                    }
                }
            }
            // Drop duplicate markers beyond the primary group emission.
            for slot in &g.slots {
                if *slot == g.primary {
                    continue;
                }
                let m = extend_marker(*slot);
                if let Some(p) = chunks.iter().position(|c| *c == m) {
                    chunks.remove(p);
                }
            }
        }

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

/// Extract the `%name` placeholder tokens from a selector (a `%` followed by
/// identifier characters `[A-Za-z0-9_-]`). `%size:hover` yields `%size`;
/// `%shared-box` yields `%shared-box`.
fn placeholder_tokens(selector: &str) -> Vec<String> {
    let chars: Vec<char> = selector.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '%' {
            let mut j = i + 1;
            while j < chars.len()
                && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '-')
            {
                j += 1;
            }
            if j > i + 1 {
                tokens.push(chars[i..j].iter().collect());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    tokens
}
