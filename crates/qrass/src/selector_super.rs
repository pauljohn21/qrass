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

use crate::selector_ast::{Combinator, Complex, Compound, Component, SelectorList};
use crate::selector_super_simple::{pseudo_element_is_super, simples_are_super};

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

pub(crate) fn complex_is_super(super_complex: &Complex, sub_complex: &Complex) -> bool {
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

// Simple / pseudo-class / pseudo-element subsumption rules live in the sibling
// `crate::selector_super_simple` module (`simples_are_super` and
// `pseudo_element_is_super` are reached through the import at the top).

