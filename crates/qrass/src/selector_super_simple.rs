//! Simple-selector / pseudo-class / pseudo-element subsumption rules for the
//! `is-superselector` engine (split out of [`crate::selector_super`], which keeps
//! the list/complex/compound structural walk). The two halves are mutually
//! recursive (`:is(...)` folds a selector list back through `list_is_super`), so
//! the crossing entry points are `pub(crate)`.
//!
//! Rules derived purely from the `is_superselector/*` `.hrx` goldens.

use crate::selector_ast::{Complex, Compound, Ns, PseudoArg, SelectorList, Simple};
use crate::selector_super::{complex_is_super, list_is_super};

/// Every simple in `sup` must be satisfied by `sub` (the subject compound),
/// with `sub_complex` available for pseudo-classes that fold over it.
pub(crate) fn simples_are_super(
    sup: &[Simple],
    sub: &[Simple],
    sub_compound: &Compound,
    sub_complex: &Complex,
) -> bool {
    // A `nth-* of <list>` restricts the element to the list, so the list's
    // guaranteed subject simples join the sub's own for coverage checks.
    let mut sub_all: Vec<Simple> = sub.to_vec();
    sub_all.extend(nth_of_guaranteed(sub));
    let sub = &sub_all[..];

    // Type/universal constraint: a super's type-like simple (if any) must cover
    // the sub's type-like simple (namespace + name), evaluated once up front.
    if let Some(super_type) = sup.iter().find(|s| s.is_type_like()) {
        let sub_type = sub.iter().find(|s| s.is_type_like());
        if !type_covers(super_type, sub_type) {
            return false;
        }
    }

    sup.iter().all(|s| match s {
        Simple::Universal { .. } | Simple::Type { .. } => true, // handled above
        Simple::Class(name) => sub
            .iter()
            .any(|t| matches!(t, Simple::Class(n) if n == name)),
        Simple::Id(name) => sub.iter().any(|t| matches!(t, Simple::Id(n) if n == name)),
        Simple::Placeholder(name) => sub
            .iter()
            .any(|t| matches!(t, Simple::Placeholder(n) if n == name)),
        Simple::Attribute(a) => sub.iter().any(|t| {
            if let Simple::Attribute(b) = t {
                crate::selector_attr::covers(a, b)
            } else {
                false
            }
        }),
        Simple::PseudoClass { .. } | Simple::PseudoElement { .. } => {
            pseudo_is_super(s, sub, sub_compound, sub_complex)
        }
    })
}

/// Does a super's type-like simple (a `Type` or a `Universal`) cover the sub's
/// type-like simple? Rules derived from `simple/type` and `simple/universal`
/// `.hrx` goldens:
/// - a bare (`Ns::Bare`) or wildcard (`Ns::Wild`) *universal* (`*`, `*|*`) is a
///   super of everything, including compounds with no type simple at all;
/// - an explicit (`Ns::Named`) or empty (`Ns::None`) universal only covers a sub
///   whose own namespace matches exactly (so `c|*` ⊇ `c|d`, `|*` ⊇ `|d`, but
///   neither covers `d`, `*`, or a bare class);
/// - a concrete `Type` forces an equal sub `Type` name, and its namespace must
///   match (a wildcard `*|c` accepts any namespace; a bare `c` accepts only a
///   bare `c`; never a universal sub).
fn type_covers(super_type: &Simple, sub_type: Option<&Simple>) -> bool {
    let (super_ns, super_name) = match super_type {
        Simple::Type { ns, name } => (ns, Some(name.as_str())),
        Simple::Universal { ns } => (ns, None),
        _ => return true,
    };
    // A bare / wildcard universal has no name requirement and matches any element.
    if super_name.is_none() && matches!(super_ns, Ns::Bare | Ns::Wild) {
        return true;
    }
    // Every remaining case needs the sub to carry an explicit type-like simple.
    let Some(sub_type) = sub_type else { return false };
    let (sub_ns, sub_name) = match sub_type {
        Simple::Type { ns, name } => (ns, Some(name.as_str())),
        Simple::Universal { ns } => (ns, None),
        _ => return false,
    };
    let ns_ok = match (super_ns, super_name.is_some()) {
        (Ns::Bare, true) => matches!(sub_ns, Ns::Bare),
        (Ns::None, _) => matches!(sub_ns, Ns::None),
        (Ns::Named(a), _) => matches!(sub_ns, Ns::Named(b) if b == a),
        (Ns::Wild, _) => true,
        (Ns::Bare, false) => true, // unreachable: handled by the early return
    };
    if !ns_ok {
        return false;
    }
    match (super_name, sub_name) {
        (None, _) => true,
        (Some(n), Some(m)) => n == m,
        (Some(_), None) => false,
    }
}

fn pseudo_is_super(
    super_simple: &Simple,
    sub: &[Simple],
    _sub_compound: &Compound,
    sub_complex: &Complex,
) -> bool {
    let name = match super_simple {
        Simple::PseudoClass { name, .. } => name.clone(),
        Simple::PseudoElement { name, .. } => name.clone(),
        _ => return false,
    };
    let super_arg = arg_of(super_simple);

    // Same-named pseudo(s) in the sub: merge their selector-list arguments so a
    // repeated `:not(a):not(b)` behaves like `:not(a, b)`, then compare.
    let sub_matches: Vec<&Simple> = sub
        .iter()
        .filter(|t| pseudo_name(t) == Some(name.as_str()))
        .collect();
    if let Some(first) = sub_matches.first() {
        let merged = merge_pseudo_args(&sub_matches).or_else(|| arg_of(first).cloned());
        return args_are_super(&name, super_arg, merged.as_ref());
    }

    // Absent from the sub. Only the positive list-folding pseudo-classes can
    // still be a super over a bare sub: their argument list must cover the whole
    // sub complex. `:not` requires disjointness; everything else is a hard miss.
    if is_positive_bare(&name)
        && let Some(PseudoArg::Selectors(list)) = super_arg {
            // `:is(A)` is a super over the bare sub when some argument complex
            // is itself a super of the whole sub complex.
            return list.complexes.iter().any(|a| complex_is_super(a, sub_complex));
        }
    if is_not(&name)
        && let Some(PseudoArg::Selectors(list)) = super_arg {
            // `:not(A)` is a super of the sub only if none of A's complexes
            // overlaps the sub (their subject compounds cannot unify).
            return list.complexes.iter().all(|a| !complex_overlaps(a, sub_complex));
        }
    false
}

/// Compare two pseudo-class arguments of the same pseudo `name`.
fn args_are_super(name: &str, super_arg: Option<&PseudoArg>, sub_arg: Option<&PseudoArg>) -> bool {
    match (super_arg, sub_arg) {
        (None, _) => true,
        (Some(PseudoArg::None), Some(PseudoArg::None)) => true,
        (Some(PseudoArg::Text(a)), Some(PseudoArg::Text(b))) => a == b,
        (Some(PseudoArg::Selectors(a)), Some(PseudoArg::Selectors(b))) => {
            match (is_not(name), is_current(name)) {
                (true, _) => list_is_super(b, a), // `:not(A)` ⊇ `:not(B)` iff B ⊇ A
                (false, true) => a == b, // `:current` only subsumes an identical argument list
                (false, false) => list_is_super(a, b),
            }
        }
        (
            Some(PseudoArg::Nth {
                anbn: a_anbn,
                of: a_of,
            }),
            Some(PseudoArg::Nth {
                anbn: b_anbn,
                of: b_of,
            }),
        ) => {
            if a_anbn != b_anbn {
                return false;
            }
            match (a_of, b_of) {
                (None, None) => true,
                (Some(x), Some(y)) => list_is_super(x, y),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Whether two complexes can select a common element (subject-compound
/// compatibility). Conservative: enough for the `:not` disjointness goldens, which
/// test clear type/id conflicts.
fn complex_overlaps(a: &Complex, b: &Complex) -> bool {
    match (
        a.components.last().map(|c| &c.compound),
        b.components.last().map(|c| &c.compound),
    ) {
        (Some(x), Some(y)) => compounds_compatible(x, y),
        _ => true,
    }
}

fn compounds_compatible(a: &Compound, b: &Compound) -> bool {
    // Type conflict: two concrete, different types cannot co-occur.
    if let (Some(Simple::Type { name: n1, .. }), Some(Simple::Type { name: n2, .. })) = (
        a.simples.iter().find(|s| matches!(s, Simple::Type { .. })),
        b.simples.iter().find(|s| matches!(s, Simple::Type { .. })),
    )
        && n1 != n2 {
            return false;
        }
    // Id conflict.
    if let (Some(Simple::Id(i1)), Some(Simple::Id(i2))) = (
        a.simples.iter().find(|s| matches!(s, Simple::Id(_))),
        b.simples.iter().find(|s| matches!(s, Simple::Id(_))),
    )
        && i1 != i2 {
            return false;
        }
    true
}

fn arg_of(s: &Simple) -> Option<&PseudoArg> {
    match s {
        Simple::PseudoClass { arg, .. } | Simple::PseudoElement { arg, .. } => Some(arg),
        _ => None,
    }
}

fn pseudo_name(s: &Simple) -> Option<&str> {
    match s {
        Simple::PseudoClass { name, .. } | Simple::PseudoElement { name, .. } => Some(name),
        _ => None,
    }
}

fn is_not(name: &str) -> bool {
    name == "not" || name.ends_with("-not")
}

/// `:current(A)` (and a vendor-prefixed `-pfx-current`) subsumes another only
/// when the argument lists are *identical* (the `current.hrx` goldens).
fn is_current(name: &str) -> bool {
    name == "current" || name.ends_with("-current")
}

/// Concatenate the selector-list arguments of same-named pseudo-classes so a
/// compound like `:not(a):not(b)` is treated as the single `:not(a, b)`. When the
/// arguments are not all selector lists, returns `None` (the caller falls back to
/// the first argument).
fn merge_pseudo_args(simps: &[&Simple]) -> Option<PseudoArg> {
    let complexes = simps.iter().try_fold(Vec::new(), |mut acc, s| match arg_of(s) {
        Some(PseudoArg::Selectors(list)) => {
            acc.extend(list.complexes.iter().cloned());
            Some(acc)
        }
        _ => None,
    })?;
    Some(PseudoArg::Selectors(SelectorList::new(complexes)))
}

/// Is super pseudo-element `a` a superselector of sub pseudo-element `b`? Their
/// names and legacy-colon spellings must match; a selector-list argument (the
/// `::slotted(...)` case) compares by subsumption, a text argument by equality.
pub(crate) fn pseudo_element_is_super(a: &Simple, b: &Simple) -> bool {
    let (na, la, arga) = match a {
        Simple::PseudoElement {
            name,
            legacy_colon,
            arg,
        } => (name, *legacy_colon, arg),
        _ => return false,
    };
    let (nb, lb, argb) = match b {
        Simple::PseudoElement {
            name,
            legacy_colon,
            arg,
        } => (name, *legacy_colon, arg),
        _ => return false,
    };
    if na != nb || la != lb {
        return false;
    }
    match (arga, argb) {
        (PseudoArg::None, PseudoArg::None) => true,
        (PseudoArg::Text(x), PseudoArg::Text(y)) => x == y,
        (PseudoArg::Selectors(x), PseudoArg::Selectors(y)) => list_is_super(x, y),
        _ => false,
    }
}

/// Simples guaranteed by a `nth-* of <list>` pseudo: the element must match the
/// list, so any simple common to *every* subject compound of the list is a real
/// constraint the super can rely on (so `c` is a super of `:nth-child(n of c)`).
fn nth_of_guaranteed(sub: &[Simple]) -> Vec<Simple> {
    sub.iter()
        .filter_map(|s| match s {
            Simple::PseudoClass {
                arg: PseudoArg::Nth { of: Some(list), .. },
                ..
            } => Some(list),
            _ => None,
        })
        .flat_map(|list| list.complexes.iter())
        .filter_map(|complex| complex.components.last())
        .fold(None::<Vec<Simple>>, |acc, subject| {
            let simples = subject.compound.simples.clone();
            Some(match acc {
                None => simples,
                Some(prev) => prev
                    .into_iter()
                    .filter(|x| simples.iter().any(|y| y == x))
                    .collect(),
            })
        })
        .unwrap_or_default()
}

/// Vendor-agnostic positive list pseudo-classes that fold over a bare sub.
fn is_positive_bare(name: &str) -> bool {
    ["is", "where", "matches", "any"]
        .iter()
        .any(|suf| name == *suf || name.ends_with(&format!("-{suf}")))
}
