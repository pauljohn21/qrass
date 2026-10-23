//! Selector-list pseudo handling for `selector.extend` (Round 23 Phase 4):
//! growing the argument list of `:is`/`:where`/`:has`/`:host`/`::slotted`/… in
//! place, and the special `:not` rule (`:not(X, Y) == :not(X):not(Y)`).
//!
//! Rules are derived purely from the `extend/simple/pseudo/**` `.hrx` goldens.
//!
//! RFP contract (OpenSpec refactor-rfp-strict): every traversal is an rxrust
//! `Local` stream — component/simple walks are owned `reduce_initial` folds
//! (EvalFold shape), nested growth lists flow through `flat_map` streams, and
//! the fold state `step` methods dispatch through `match` expressions only.

use rxrust::prelude::*;

use crate::drain::{drain, drain_one};
use crate::selector_ast::{Complex, Component, Compound, PseudoArg, SelectorList, Simple};
use crate::selector_extend::{push_unique, top_level};

/// Walk one complex' components replacing each compound through `f`, keeping
/// `Some(complex)` only when at least one replacement fired (shared skeleton of
/// `pseudo_replace` / `pseudo_substitute`).
fn map_components(
    comps: Vec<Component>,
    extendee: SelectorList,
    extender: SelectorList,
    f: fn(&Compound, &SelectorList, &SelectorList) -> Option<Compound>,
) -> Option<Complex> {
    let (comps, changed) = drain_one(
        Local::from_iter(comps)
            .reduce_initial((Vec::new(), false), move |(mut acc, changed), comp| {
                match f(&comp.compound, &extendee, &extender) {
                    Some(m) => {
                        acc.push(Component {
                            compound: m,
                            ..comp
                        });
                        (acc, true)
                    }
                    None => {
                        acc.push(comp);
                        (acc, changed)
                    }
                }
            })
            .map(|(c, ch)| Ok((c, ch)))
            .box_it(),
    );
    changed.then(|| Complex::new(comps))
}

/// `sel` with every selector-list pseudo in it grown by `extendee`→`extender`.
pub(super) fn pseudo_replace(
    sel: &Complex,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> Option<Complex> {
    map_components(
        sel.components.clone(),
        extendee.clone(),
        extender.clone(),
        extend_pseudo_compound,
    )
}

/// The (name, argument list, `anbn`) triple of a selector-list-bearing pseudo
/// (`Selectors` or `Nth { of: Some }`), `None` for everything the walks skip.
fn pseudo_list(s: &Simple) -> Option<(String, SelectorList, Option<String>)> {
    match s {
        Simple::PseudoClass { name, arg } | Simple::PseudoElement { name, arg, .. } => match arg {
            PseudoArg::Selectors(l) => Some((name.clone(), l.clone(), None)),
            PseudoArg::Nth {
                anbn,
                of: Some(l),
            } => Some((name.clone(), l.clone(), Some(anbn.clone()))),
            _ => None,
        },
        _ => None,
    }
}

/// Grow the selector-list pseudo arguments of `c`. Idempotent pseudos flatten a
/// same-named nesting; `:not` (single argument) appends `:not()` simples instead.
fn extend_pseudo_compound(
    c: &Compound,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> Option<Compound> {
    #[derive(Default)]
    struct Ext {
        simples: Vec<Simple>,
        extra_not: Vec<Simple>,
        changed: bool,
    }
    impl Ext {
        fn step(mut self, s: Simple, ee: &SelectorList, ex: &SelectorList) -> Self {
            // owned snapshots feed the nested 'static streams below
            let (ee2, ex2) = (ee.clone(), ex.clone());
            match pseudo_list(&s) {
                // a simple carrying no selector-list pseudo passes through
                None => {
                    self.simples.push(s);
                    self
                }
                Some((name, list, anbn)) => {
                    let eligible = is_selector_pseudo(&name) || anbn.is_some();
                    let not_single = eligible && name == "not" && list.complexes.len() == 1;
                    match (eligible, not_single) {
                        // `:not(X)` grows by appending `:not()` simples instead
                        (_, true) => {
                            let variants = top_level(&list.complexes[0], &ee2, &ex2);
                            // an unextendable `:not(X)` stays untouched
                            let extras = match variants.is_empty() {
                                true => Vec::new(),
                                false => drain(
                                    Local::from_iter(variants.clone())
                                        .flat_map(|v| {
                                            Local::from_iter(expand_not(&v)).map(Ok).box_it()
                                        })
                                        .box_it(),
                                )
                                .unwrap_or_default(),
                            };
                            let fired = !extras.is_empty();
                            self.extra_not.extend(extras);
                            self.simples.push(s);
                            self.changed |= fired;
                            self
                        }
                        // a selector-list pseudo grows its argument list in place
                        (true, false) => {
                            let grown = drain(
                                Local::from_iter(list.complexes.clone())
                                    .reduce_initial(Vec::new(), {
                                        let (name, anbn, ee3, ex3) = (
                                            name.clone(),
                                            anbn.clone(),
                                            ee2.clone(),
                                            ex2.clone(),
                                        );
                                        move |mut acc, a| {
                                            push_unique(&mut acc, a.clone());
                                            let (name, anbn) = (name.clone(), anbn.clone());
                                            top_level(&a, &ee3, &ex3)
                                                .into_iter()
                                                .filter(move |v| {
                                                    name == "not"
                                                        || !is_bare_incompatible(
                                                            v,
                                                            &name,
                                                            anbn.as_deref(),
                                                        )
                                                })
                                                .fold(acc, |mut acc, v| {
                                                    push_unique(&mut acc, v);
                                                    acc
                                                })
                                        }
                                    })
                                    .map(Ok)
                                    .box_it(),
                            )
                            .ok()
                            .and_then(|mut v| v.pop())
                            .unwrap_or_default();
                            // nothing grew → the simple stays as it was
                            match grown.len() != list.complexes.len() {
                                true => {
                                    let flat = is_idempotent_pseudo(&name) || anbn.is_some();
                                    let grown = match flat {
                                        true => {
                                            flatten_same_pseudo(&name, anbn.as_deref(), grown)
                                        }
                                        false => grown,
                                    };
                                    self.simples
                                        .push(with_arg(s, anbn, SelectorList::new(grown)));
                                    self.changed = true;
                                }
                                false => self.simples.push(s),
                            }
                            self
                        }
                        // not a selector-list pseudo: pass through
                        (false, false) => {
                            self.simples.push(s);
                            self
                        }
                    }
                }
            }
        }
    }
    let (ee, ex) = (extendee.clone(), extender.clone());
    let ext = drain_one(
        Local::from_iter(c.simples.clone())
            .reduce_initial(Ext::default(), move |st, s| st.step(s, &ee, &ex))
            .map(Ok)
            .box_it(),
    );
    ext.changed
        .then(|| Compound::new(ext.simples.into_iter().chain(ext.extra_not).collect()))
}

/// Turn one `:not` extension variant into the `:not()` simples it should add.
/// A bare idempotent `:is`/`:where`/`:matches` list fans out to one `:not` per
/// inner selector; a bare `:not` extender contributes nothing; anything else
/// becomes a single `:not(<variant>)`.
fn expand_not(v: &Complex) -> Vec<Simple> {
    let bare = match v.components.as_slice() {
        [comp] if comp.combinator.is_none() => comp.compound.simples.as_slice(),
        _ => &[Simple::Class(String::new())][..] as &[Simple],
    };
    match bare {
        [Simple::PseudoClass {
            name,
            arg: PseudoArg::Selectors(_),
        }] if name == "not" => Vec::new(),
        [Simple::PseudoClass {
            name,
            arg: PseudoArg::Selectors(inner),
        }] if is_idempotent_pseudo(name) => drain(
            Local::from_iter(inner.complexes.clone())
                .map(|sel| Ok(not_of(&sel)))
                .box_it(),
        )
        .unwrap_or_default(),
        _ => vec![not_of(v)],
    }
}

fn not_of(sel: &Complex) -> Simple {
    Simple::PseudoClass {
        name: "not".to_string(),
        arg: PseudoArg::Selectors(SelectorList::new(vec![sel.clone()])),
    }
}

/// Rewrite the pseudo argument of `s`, preserving an `nth-*`'s `<an+b>` shape.
fn with_arg(mut s: Simple, anbn: Option<String>, list: SelectorList) -> Simple {
    match &mut s {
        Simple::PseudoClass { arg, .. } | Simple::PseudoElement { arg, .. } => {
            *arg = match anbn {
                Some(a) => PseudoArg::Nth {
                    anbn: a,
                    of: Some(list),
                },
                None => PseudoArg::Selectors(list),
            };
        }
        _ => {}
    }
    s
}

/// Is `v` a bare compound whose only simple is a selector-list / `nth-*` pseudo
/// that is *not* the same family as `target` (differing name, or, for `nth-*`,
/// a differing `<an+b>`)? Such a variant cannot be merged into the argument list.
fn is_bare_incompatible(v: &Complex, target: &str, target_anbn: Option<&str>) -> bool {
    match v.components.as_slice() {
        [comp] if comp.combinator.is_none() => match comp.compound.simples.as_slice() {
            [simple] => match simple {
                Simple::PseudoClass {
                    name,
                    arg: PseudoArg::Selectors(_),
                } => name != target,
                Simple::PseudoClass {
                    name,
                    arg: PseudoArg::Nth { anbn, of: Some(_) },
                } => name != target || Some(anbn.as_str()) != target_anbn,
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}

/// Splice `:F(:F(a, b), c)` argument lists flat for an idempotent pseudo `F`
/// (`anbn` pins a `nth-*`'s `<an+b>` so only equal ones collapse).
fn flatten_same_pseudo(name: &str, anbn: Option<&str>, args: Vec<Complex>) -> Vec<Complex> {
    let (name, anbn) = (name.to_string(), anbn.map(str::to_string));
    drain_one(
        Local::from_iter(args)
            .reduce_initial(Vec::new(), move |acc, arg| {
                let inner = match arg.components.as_slice() {
                    [c] if c.combinator.is_none() => match c.compound.simples.as_slice() {
                        [Simple::PseudoClass {
                            name: n,
                            arg: PseudoArg::Selectors(list),
                        }] if n == &name && anbn.is_none() => Some(list.complexes.clone()),
                        [Simple::PseudoClass {
                            name: n,
                            arg: PseudoArg::Nth {
                                anbn: a,
                                of: Some(list),
                            },
                        }] if n == &name && Some(a.as_str()) == anbn.as_deref() => {
                            Some(list.complexes.clone())
                        }
                        _ => None,
                    },
                    _ => None,
                };
                let pieces = inner.unwrap_or_else(move || vec![arg]);
                pieces.into_iter().fold(acc, |mut acc, c| {
                    push_unique(&mut acc, c);
                    acc
                })
            })
            .map(Ok)
            .box_it(),
    )
}

/// A selector-list pseudo (`:is`/`:where`/`:has`/`:host`/`::slotted`/`:not`/…),
/// prefix tolerant.
fn is_selector_pseudo(name: &str) -> bool {
    [
        "is",
        "where",
        "matches",
        "any",
        "current",
        "nth-match",
        "has",
        "host",
        "host-context",
        "slotted",
        "not",
    ]
    .iter()
    .any(|p| name == *p || name.ends_with(&format!("-{p}")))
}

/// Selector-list pseudos that collapse a same-named nesting when merged.
fn is_idempotent_pseudo(name: &str) -> bool {
    ["is", "where", "matches", "any", "current", "nth-match"]
        .iter()
        .any(|p| name == *p || name.ends_with(&format!("-{p}")))
}

/// `sel` with every selector-list pseudo argument *substituted* in place for
/// `selector.replace` — `extendee` replaced by `extender` inside the argument
/// list, NOT grown the way [`pseudo_replace`] does (`:is(c)` + `c`→`d` =
/// `:is(d)`). Returns `None` when no pseudo argument changes.
pub(super) fn pseudo_substitute(
    sel: &Complex,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> Option<Complex> {
    map_components(
        sel.components.clone(),
        extendee.clone(),
        extender.clone(),
        sub_pseudo_compound,
    )
}

/// Rebuild a compound whose selector-list pseudo arguments have `extendee`
/// substituted (recursing through [`crate::selector_extend::replace_list`]); an
/// `nth-*`'s `<an+b>` wrapper shape is preserved.
fn sub_pseudo_compound(
    c: &Compound,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> Option<Compound> {
    let (ee, ex) = (extendee.clone(), extender.clone());
    let (simples, changed) = drain_one(
        Local::from_iter(c.simples.clone())
            .reduce_initial((Vec::new(), false), move |(mut acc, changed), s| {
                match pseudo_list(&s) {
                    Some((name, list, anbn)) if is_selector_pseudo(&name) => {
                        let replaced = crate::selector_extend::replace_list(&list, &ee, &ex);
                        match replaced.complexes != list.complexes {
                            true => {
                                acc.push(with_arg(s, anbn, replaced));
                                (acc, true)
                            }
                            false => {
                                acc.push(s);
                                (acc, changed)
                            }
                        }
                    }
                    _ => {
                        acc.push(s);
                        (acc, changed)
                    }
                }
            })
            .map(|(s, ch)| Ok((s, ch)))
            .box_it(),
    );
    changed.then(|| Compound::new(simples))
}
