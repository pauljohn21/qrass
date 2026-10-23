//! Compound-level weaving for `selector.extend` (Round 23 Phase 4): splicing the
//! extender into a selector at a matched slot, including the descendant "float"
//! interleaving of a multi-compound extender past a descendant grandparent run.
//!
//! Rules are derived purely from the `extend/complex/**` `.hrx` goldens.

use crate::selector_ast::{Combinator, Complex, Compound, Component};
use crate::selector_unify::compound_unify;

/// Splice `extender` (with `residual` unified onto its inner-most compound) in
/// place of `comps[start..=end]`, keeping the grandparent `comps[..start]` and the
/// tail `comps[end + 1..]`. A single-compound extender yields one reading; a
/// multi-compound extender whose head is a descendant of a descendant grandparent
/// run additionally floats its outer compounds outward past that chain. A
/// non-descendant slot with a multi-compound extender is a structural conflict.
pub(super) fn weave_all(
    comps: &[Component],
    start: usize,
    end: usize,
    residual: &Compound,
    extender: &Complex,
) -> Vec<Complex> {
    match extender.components.is_empty() {
        // A combinator-only extender (`>`): when the matched slot is fully consumed
        // the bare trailing combinator *is* the reading (`extend(".c", ".c", ">")` -> `>`).
        true => residual
            .simples
            .is_empty()
            .then(|| Complex {
                components: Vec::new(),
                trailing: extender.trailing.clone(),
            })
            .into_iter()
            .collect(),
        false => spliced_readings(comps, start, end, residual, extender),
    }
}

/// The common splice: classify the slot/extender pairing, then build the one
/// reading (or none) that pairing allows.
fn spliced_readings(
    comps: &[Component],
    start: usize,
    end: usize,
    residual: &Compound,
    extender: &Complex,
) -> Vec<Complex> {
    let xs = &extender.components;
    let n = xs.len();
    // a multi-compound extender only lands on a descendant edge
    let slot_ok =
        n == 1 || comps[start].combinator.is_none_or(|c| c == Combinator::Descendant);
    // when the extender lands at the very front, its leading and the selector's
    // own leading combinator must agree; a clash makes the reading invalid
    let leading = match start {
        0 => combine_leading(comps[0].combinator, xs[0].combinator),
        _ => Some(comps[0].combinator),
    };
    let merged = compound_unify(residual, &xs[n - 1].compound);
    match (slot_ok, leading, merged) {
        (true, Some(leading), Some(compound)) => {
            build_readings(comps, start, end, extender, compound, leading)
        }
        _ => Vec::new(),
    }
}

/// One reading for the non-floating case; the floating case yields one reading
/// per order-preserving merge of the grandparent run and the extender prefix.
fn build_readings(
    comps: &[Component],
    start: usize,
    end: usize,
    extender: &Complex,
    compound: Compound,
    leading: Option<Combinator>,
) -> Vec<Complex> {
    let xs = &extender.components;
    let tail = &comps[end + 1..];
    // A single trailing combinator on the extender becomes the edge joining the
    // spliced extender to the following tail (`.e >` before tail `.d` -> `.e > .d`).
    let tail_comb = (extender.trailing.len() == 1 && !tail.is_empty()).then(|| extender.trailing[0]);
    let prefix = &xs[..xs.len() - 1];
    let floatable = start > 0
        && !prefix.is_empty()
        && is_descendant_run(&comps[..start])
        && prefix
            .iter()
            .all(|c| c.combinator.is_none_or(|x| x == Combinator::Descendant));
    let pinned = |combinator: Option<Combinator>| Component {
        combinator,
        extras: Vec::new(),
        compound: compound.clone(),
    };
    match floatable {
        false => {
            let shifted = prefix.iter().map(|p| Component {
                combinator: Some(Combinator::Descendant),
                extras: Vec::new(),
                ..p.clone()
            });
            let mut pieces: Vec<Component> =
                comps[..start].iter().cloned().chain(shifted).collect();
            let comb = match pieces.is_empty() {
                true => leading,
                false => Some(Combinator::Descendant),
            };
            pieces.push(pinned(comb));
            splice_tail(&mut pieces, tail, tail_comb);
            pieces.first_mut().into_iter().for_each(|f| {
                f.combinator = leading;
                f.extras.clear();
            });
            vec![Complex::new(pieces)]
        }
        true => {
            // Float the extender's outer compounds through the descendant
            // grandparent run.
            let gp: Vec<Compound> = comps[..start].iter().map(|c| c.compound.clone()).collect();
            let pf: Vec<Compound> = prefix.iter().map(|c| c.compound.clone()).collect();
            interleave(&gp, &pf)
                .into_iter()
                .map(|merged| {
                    let mut pieces: Vec<Component> = merged
                        .into_iter()
                        .enumerate()
                        .map(|(i, compound)| Component {
                            combinator: match i {
                                0 => leading,
                                _ => Some(Combinator::Descendant),
                            },
                            extras: Vec::new(),
                            compound,
                        })
                        .collect();
                    pieces.push(pinned(Some(Combinator::Descendant)));
                    splice_tail(&mut pieces, tail, tail_comb);
                    Complex::new(pieces)
                })
                .collect()
        }
    }
}

/// Append the preserved `tail` after the spliced extender, overriding the first
/// tail edge's combinator with a single-token `tail_comb` when present.
fn splice_tail(pieces: &mut Vec<Component>, tail: &[Component], tail_comb: Option<Combinator>) {
    let tail_start = pieces.len();
    pieces.extend_from_slice(tail);
    if let Some(comb) = tail_comb
        && let Some(t) = pieces.get_mut(tail_start)
    {
        t.combinator = Some(comb);
        t.extras.clear();
    }
}

/// Merge two front-pinned combinators; `None` (outer) means they conflict.
fn combine_leading(a: Option<Combinator>, b: Option<Combinator>) -> Option<Option<Combinator>> {
    match (a, b) {
        (None, x) | (x, None) => Some(x),
        (Some(x), Some(y)) => (x == y).then_some(Some(x)),
    }
}

/// A run that floats freely: every (non-leading) edge is a descendant.
fn is_descendant_run(run: &[Component]) -> bool {
    run.iter()
        .all(|c| c.combinator.is_none_or(|x| x == Combinator::Descendant))
}

/// Every order-preserving merge of two compound sequences.
fn interleave(a: &[Compound], b: &[Compound]) -> Vec<Vec<Compound>> {
    // order-preserving merges: take a's head first, or b's head first
    let lead = |first: &Compound, rest: Vec<Vec<Compound>>| {
        rest.into_iter()
            .map(|r| {
                std::iter::once(first.clone())
                    .chain(r)
                    .collect::<Vec<Compound>>()
            })
            .collect::<Vec<Vec<Compound>>>()
    };
    match (a.split_first(), b.split_first()) {
        (None, _) => vec![b.to_vec()],
        (_, None) => vec![a.to_vec()],
        (Some((ha, ra)), Some((hb, rb))) => lead(ha, interleave(ra, b))
            .into_iter()
            .chain(lead(hb, interleave(a, rb)))
            .collect(),
    }
}
