//! Selector-list pseudo handling for `selector.extend` (Round 23 Phase 4):
//! growing the argument list of `:is`/`:where`/`:has`/`:host`/`::slotted`/… in
//! place, and the special `:not` rule (`:not(X, Y) == :not(X):not(Y)`).
//!
//! Rules are derived purely from the `extend/simple/pseudo/**` `.hrx` goldens.

use crate::selector_ast::{Complex, Compound, PseudoArg, SelectorList, Simple};
use crate::selector_extend::{push_unique, top_level};

/// `sel` with every selector-list pseudo in it grown by `extendee`→`extender`.
pub(super) fn pseudo_replace(
    sel: &Complex,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> Option<Complex> {
    let mut new_comps = sel.components.clone();
    let mut changed = false;
    for comp in new_comps.iter_mut() {
        if let Some(m) = extend_pseudo_compound(&comp.compound, extendee, extender) {
            comp.compound = m;
            changed = true;
        }
    }
    if changed {
        Some(Complex::new(new_comps))
    } else {
        None
    }
}

/// Grow the selector-list pseudo arguments of `c`. Idempotent pseudos flatten a
/// same-named nesting; `:not` (single argument) appends `:not()` simples instead.
fn extend_pseudo_compound(
    c: &Compound,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> Option<Compound> {
    let mut simples = c.simples.clone();
    let mut extra_not: Vec<Simple> = Vec::new();
    let mut changed = false;
    for simple in simples.iter_mut() {
        let (name, list, anbn) = match simple {
            Simple::PseudoClass { name, arg }
            | Simple::PseudoElement { name, arg, .. } => match arg {
                PseudoArg::Selectors(list) => (name.clone(), list.clone(), None),
                PseudoArg::Nth {
                    anbn,
                    of: Some(list),
                } => (name.clone(), list.clone(), Some(anbn.clone())),
                _ => continue,
            },
            _ => continue,
        };
        if !is_selector_pseudo(&name) && anbn.is_none() {
            continue;
        }
        if name == "not" && list.complexes.len() == 1 {
            let variants = top_level(&list.complexes[0], extendee, extender);
            if variants.is_empty() {
                continue;
            }
            for v in &variants {
                expand_not(v, &mut extra_not);
            }
            changed = true;
            continue;
        }
        // In-list growth: keep each original and append its extensions.
        let mut grown: Vec<Complex> = Vec::new();
        for a in &list.complexes {
            push_unique(&mut grown, a.clone());
            for v in top_level(a, extendee, extender) {
                // A bare pseudo of a different family (mismatched name, or a
                // mismatched `<an+b>` for a `nth-*`) cannot be merged — refuse it.
                if name != "not" && is_bare_incompatible(&v, &name, anbn.as_deref()) {
                    continue;
                }
                push_unique(&mut grown, v);
            }
        }
        if grown.len() == list.complexes.len() {
            continue;
        }
        let grown = if is_idempotent_pseudo(&name) || anbn.is_some() {
            flatten_same_pseudo(&name, anbn.as_deref(), grown)
        } else {
            grown
        };
        match simple {
            Simple::PseudoClass { arg, .. } | Simple::PseudoElement { arg, .. } => {
                *arg = match anbn {
                    Some(anbn) => PseudoArg::Nth {
                        anbn,
                        of: Some(SelectorList::new(grown)),
                    },
                    None => PseudoArg::Selectors(SelectorList::new(grown)),
                };
            }
            _ => {}
        }
        changed = true;
    }
    if !changed {
        return None;
    }
    simples.extend(extra_not);
    Some(Compound::new(simples))
}

/// Turn one `:not` extension variant into the `:not()` simples it should add.
/// A bare idempotent `:is`/`:where`/`:matches` list fans out to one `:not` per
/// inner selector; a bare `:not` extender is ignored; anything else becomes a
/// single `:not(<variant>)`.
fn expand_not(v: &Complex, out: &mut Vec<Simple>) {
    if let [comp] = v.components.as_slice() {
        if comp.combinator.is_none() {
            if let [Simple::PseudoClass {
                name,
                arg: PseudoArg::Selectors(inner),
            }] = comp.compound.simples.as_slice()
            {
                if name == "not" {
                    return;
                }
                if is_idempotent_pseudo(name) {
                    for sel in &inner.complexes {
                        out.push(not_of(sel));
                    }
                    return;
                }
            }
        }
    }
    out.push(not_of(v));
}

fn not_of(sel: &Complex) -> Simple {
    Simple::PseudoClass {
        name: "not".to_string(),
        arg: PseudoArg::Selectors(SelectorList::new(vec![sel.clone()])),
    }
}

/// Is `v` a bare compound whose only simple is a selector-list / `nth-*` pseudo
/// that is *not* the same family as `target` (differing name, or, for `nth-*`,
/// a differing `<an+b>`)? Such a variant cannot be merged into the argument list.
fn is_bare_incompatible(v: &Complex, target: &str, target_anbn: Option<&str>) -> bool {
    let [comp] = v.components.as_slice() else {
        return false;
    };
    if comp.combinator.is_some() {
        return false;
    }
    let [simple] = comp.compound.simples.as_slice() else {
        return false;
    };
    match simple {
        Simple::PseudoClass {
            name,
            arg: PseudoArg::Selectors(_),
        } => name != target,
        Simple::PseudoClass {
            name,
            arg: PseudoArg::Nth { anbn, of: Some(_) },
        } => name != target || Some(anbn.as_str()) != target_anbn,
        _ => false,
    }
}

/// Splice `:F(:F(a, b), c)` argument lists flat for an idempotent pseudo `F`
/// (`anbn` pins a `nth-*`'s `<an+b>` so only equal ones collapse).
fn flatten_same_pseudo(name: &str, anbn: Option<&str>, args: Vec<Complex>) -> Vec<Complex> {
    let mut out: Vec<Complex> = Vec::new();
    for arg in args {
        let inner = match arg.components.as_slice() {
            [c] if c.combinator.is_none() => match c.compound.simples.as_slice() {
                [Simple::PseudoClass {
                    name: n,
                    arg: PseudoArg::Selectors(list),
                }] if n == name && anbn.is_none() => Some(list),
                [Simple::PseudoClass {
                    name: n,
                    arg: PseudoArg::Nth {
                        anbn: a,
                        of: Some(list),
                    },
                }] if n == name && Some(a.as_str()) == anbn => Some(list),
                _ => None,
            },
            _ => None,
        };
        if let Some(list) = inner {
            for inner_sel in &list.complexes {
                push_unique(&mut out, inner_sel.clone());
            }
        } else {
            push_unique(&mut out, arg);
        }
    }
    out
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
    let mut new_comps = sel.components.clone();
    let mut changed = false;
    for comp in new_comps.iter_mut() {
        if let Some(m) = sub_pseudo_compound(&comp.compound, extendee, extender) {
            comp.compound = m;
            changed = true;
        }
    }
    if changed {
        Some(Complex::new(new_comps))
    } else {
        None
    }
}

/// Rebuild a compound whose selector-list pseudo arguments have `extendee`
/// substituted (recursing through [`crate::selector_extend::replace_list`]); an
/// `nth-*`'s `<an+b>` wrapper shape is preserved.
fn sub_pseudo_compound(
    c: &Compound,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> Option<Compound> {
    let mut simples = c.simples.clone();
    let mut changed = false;
    for simple in simples.iter_mut() {
        let (name, anbn, list) = match simple {
            Simple::PseudoClass { name, arg }
            | Simple::PseudoElement { name, arg, .. } => match arg {
                PseudoArg::Selectors(l) => (name.clone(), None, l.clone()),
                PseudoArg::Nth {
                    anbn,
                    of: Some(l),
                } => (name.clone(), Some(anbn.clone()), l.clone()),
                _ => continue,
            },
            _ => continue,
        };
        if !is_selector_pseudo(&name) {
            continue;
        }
        let replaced = crate::selector_extend::replace_list(&list, extendee, extender);
        if replaced.complexes == list.complexes {
            continue;
        }
        let arg = match anbn {
            Some(anbn) => PseudoArg::Nth {
                anbn,
                of: Some(replaced),
            },
            None => PseudoArg::Selectors(replaced),
        };
        match simple {
            Simple::PseudoClass { arg: slot, .. } | Simple::PseudoElement { arg: slot, .. } => {
                *slot = arg
            }
            _ => {}
        }
        changed = true;
    }
    if changed { Some(Compound::new(simples)) } else { None }
}
