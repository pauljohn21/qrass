//! `selector.is-superselector` engine (Round 23 Phase 2), operating purely on the
//! owned AST from [`crate::selector_ast`].
//!
//! The rules are derived from the `is_superselector/*` `.hrx` goldens (never from
//! any reference implementation):
//! - **list**: `S` is a superselector of `T` when *every* complex in `T` is
//!   covered by *some* complex in `S` (`is-superselector("c","c, d")` → false).
//! - **complex**: align the subject (right-most) compound, then walk left.
//!   `Child`/`NextSibling` admit no skipped compound; `Descendant`/`Sibling`
//!   do (so `a ~ b ~ c` covers `a ~ x ~ b ~ c`, but `a + b + c` does not cover
//!   `a + x b + c`).
//! - **compound**: every simple requirement in the super must be satisfied by the
//!   sub's subject — a concrete type forces an equal type; classes/ids/
//!   placeholders must be present; attributes use [`selector_attr::covers`].
//! - **pseudo-classes**: `:is`/`:where`/`:matches`/`:any` fold over their argument
//!   lists (and, when absent from the sub, their list must cover the sub complex);
//!   `:has` matches only same-named; `:not` reverses and needs the argument to be
//!   disjoint from the sub (a unify-style emptiness test).
//! - **pseudo-elements**: `::x` must match and its base/after segments compare in
//!   order (the fake-pseudo-element-order goldens).

use crate::selector_ast::{
    Combinator, Complex, Compound, Component, Ns, PseudoArg, SelectorList, Simple,
};

/// Public entry: is `super_list` a superselector of `sub_list`?
pub fn list_is_super(super_list: &SelectorList, sub_list: &SelectorList) -> bool {
    sub_list
        .complexes
        .iter()
        .all(|sub| super_list.complexes.iter().any(|sup| complex_is_super(sup, sub)))
}

/// A complex with a leading combinator, a doubled combinator edge (`extras`), or
/// a dangling trailing combinator — all invalid CSS that cannot subsume or be
/// subsumed.
fn has_bogus_chain(c: &Complex) -> bool {
    !c.trailing.is_empty()
        || c.components.iter().any(|x| !x.extras.is_empty())
        || c.components.first().and_then(|x| x.combinator).is_some()
}

fn complex_is_super(super_complex: &Complex, sub_complex: &Complex) -> bool {
    let sup = &super_complex.components;
    let sub = &sub_complex.components;
    // A bogus chain (a leading combinator, a doubled combinator edge, or a
    // dangling trailing combinator) is invalid CSS that can never take part in a
    // subsumption relation (`bogus.hrx`).
    if has_bogus_chain(super_complex) || has_bogus_chain(sub_complex) {
        return false;
    }
    if sup.is_empty() {
        return sub.is_empty();
    }
    if sup.len() == 1 {
        // A single-compound super covers the whole sub chain when it covers the
        // subject (last compound).
        let subject = &sub[sub.len() - 1];
        return compound_is_super(&sup[0].compound, subject, sub_complex);
    }
    // The subject compounds must relate, then the leading super components must be
    // matched against a prefix of the sub chain, respecting the last combinator.
    if !compound_is_super(&sup[sup.len() - 1].compound, &sub[sub.len() - 1], sub_complex) {
        return false;
    }
    components_are_super(&sup[..sup.len() - 1], &sub[..sub.len() - 1], sup[sup.len() - 1].combinator, sub[sub.len() - 1].combinator)
}

/// Match the leading super components against the leading sub components, given
/// the combinator that links the super's last two components (`super_comb`) and
/// the one that links the sub's last two (`sub_comb`).
fn components_are_super(
    sup: &[Component],
    sub: &[Component],
    super_comb: Option<Combinator>,
    sub_comb: Option<Combinator>,
) -> bool {
    let need = match super_comb {
        Some(c) => c,
        // No combinator left in the super means the super had only the subject.
        None => return true,
    };
    match need {
        Combinator::Child | Combinator::NextSibling => {
            // Adjacency / parenthood cannot tolerate an intervening sub compound.
            if !combinator_implied_by(need, sub_comb) {
                return false;
            }
            complex_prefix_is_super(sup, sub)
        }
        Combinator::Descendant => {
            // Any sub combinator implies descendant; allow skipping sub compounds.
            for k in (0..sub.len()).rev() {
                if complex_prefix_is_super(sup, &sub[..k + 1]) {
                    return true;
                }
            }
            false
        }
        Combinator::Sibling => {
            if !matches!(sub_comb, Some(Combinator::Sibling) | Some(Combinator::NextSibling))
                && sub_comb.is_some()
            {
                // The immediate step into the subject must itself be a sibling
                // relation for `~` to hold across it.
                return false;
            }
            for k in (0..sub.len()).rev() {
                if complex_prefix_is_super(sup, &sub[..k + 1]) {
                    return true;
                }
            }
            false
        }
    }
}

/// `sup` chain is a superselector of `sub` chain (used when recursion has fully
/// aligned the subjects).
fn complex_prefix_is_super(sup: &[Component], sub: &[Component]) -> bool {
    if sup.is_empty() {
        // Nothing left in the super: it is an ancestor-only match, always ok.
        return true;
    }
    if sub.is_empty() {
        return false;
    }
    if sup.len() == 1 {
        let subject = &sub[sub.len() - 1];
        let chain = Complex::new(sub.to_vec());
        return compound_is_super(&sup[0].compound, subject, &chain);
    }
    if !compound_is_super(&sup[sup.len() - 1].compound, &sub[sub.len() - 1], &Complex::new(sub.to_vec()))
    {
        return false;
    }
    components_are_super(
        &sup[..sup.len() - 1],
        &sub[..sub.len() - 1],
        sup[sup.len() - 1].combinator,
        sub[sub.len() - 1].combinator,
    )
}

/// Does a sub combinator imply the super's required combinator?
fn combinator_implied_by(super_comb: Combinator, sub_comb: Option<Combinator>) -> bool {
    let sub_comb = match sub_comb {
        Some(c) => c,
        None => return matches!(super_comb, Combinator::Descendant),
    };
    match super_comb {
        Combinator::Descendant => true,
        Combinator::Child => sub_comb == Combinator::Child,
        Combinator::NextSibling => sub_comb == Combinator::NextSibling,
        Combinator::Sibling => matches!(sub_comb, Combinator::Sibling | Combinator::NextSibling),
    }
}

/// Is `super_compound` a superselector of the subject compound, knowing the whole
/// `sub_complex` (needed so `:is(...)` can fold over the entire sub chain)?
fn compound_is_super(super_compound: &Compound, subject: &Component, sub_complex: &Complex) -> bool {
    let sup = &super_compound.simples;
    let sub = &subject.compound.simples;

    // Pseudo-elements must match; split into base / element / after and compare
    // each part in order (the fake-pseudo-element-order goldens).
    let (sup_before, sup_el, sup_after) = super_compound.split_at_element();
    let sub_compound = &subject.compound;
    let (sub_before, sub_el, sub_after) = sub_compound.split_at_element();
    if sup_el.is_some() || sub_el.is_some() {
        match (sup_el, sub_el) {
            (Some(a), Some(b)) if pseudo_element_is_super(a, b) => {}
            _ => return false,
        }
        let before = simples_are_super(
            sup_before,
            sub_before,
            &Compound::new(sub_before.to_vec()),
            sub_complex,
        );
        let after = simples_are_super(
            sup_after,
            sub_after,
            &Compound::new(sub_after.to_vec()),
            sub_complex,
        );
        return before && after;
    }

    simples_are_super(sup, sub, sub_compound, sub_complex)
}

/// Every simple in `sup` must be satisfied by `sub` (the subject compound),
/// with `sub_complex` available for pseudo-classes that fold over it.
fn simples_are_super(
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
    if is_positive_bare(&name) {
        if let Some(PseudoArg::Selectors(list)) = super_arg {
            // `:is(A)` is a super over the bare sub when some argument complex
            // is itself a super of the whole sub complex.
            return list.complexes.iter().any(|a| complex_is_super(a, sub_complex));
        }
    }
    if is_not(&name) {
        if let Some(PseudoArg::Selectors(list)) = super_arg {
            // `:not(A)` is a super of the sub only if none of A's complexes
            // overlaps the sub (their subject compounds cannot unify).
            return list.complexes.iter().all(|a| !complex_overlaps(a, sub_complex));
        }
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
            if is_not(name) {
                list_is_super(b, a) // `:not(A)` ⊇ `:not(B)` iff B ⊇ A
            } else if is_current(name) {
                a == b // `:current` only subsumes an identical argument list
            } else {
                list_is_super(a, b)
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
    ) {
        if n1 != n2 {
            return false;
        }
    }
    // Id conflict.
    if let (Some(Simple::Id(i1)), Some(Simple::Id(i2))) = (
        a.simples.iter().find(|s| matches!(s, Simple::Id(_))),
        b.simples.iter().find(|s| matches!(s, Simple::Id(_))),
    ) {
        if i1 != i2 {
            return false;
        }
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
    let mut complexes = Vec::new();
    for s in simps {
        match arg_of(s) {
            Some(PseudoArg::Selectors(list)) => complexes.extend(list.complexes.iter().cloned()),
            _ => return None,
        }
    }
    Some(PseudoArg::Selectors(SelectorList::new(complexes)))
}

/// Is super pseudo-element `a` a superselector of sub pseudo-element `b`? Their
/// names and legacy-colon spellings must match; a selector-list argument (the
/// `::slotted(...)` case) compares by subsumption, a text argument by equality.
fn pseudo_element_is_super(a: &Simple, b: &Simple) -> bool {
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
    let mut acc: Option<Vec<Simple>> = None;
    for s in sub {
        let list = match s {
            Simple::PseudoClass {
                arg: PseudoArg::Nth { of: Some(list), .. },
                ..
            } => list,
            _ => continue,
        };
        for complex in &list.complexes {
            let Some(subject) = complex.components.last() else {
                continue;
            };
            let simples = subject.compound.simples.clone();
            acc = Some(match acc {
                None => simples,
                Some(prev) => prev
                    .into_iter()
                    .filter(|x| simples.iter().any(|y| y == x))
                    .collect(),
            });
        }
    }
    acc.unwrap_or_default()
}

/// Vendor-agnostic positive list pseudo-classes that fold over a bare sub.
fn is_positive_bare(name: &str) -> bool {
    ["is", "where", "matches", "any"]
        .iter()
        .any(|suf| name == *suf || name.ends_with(&format!("-{suf}")))
}
