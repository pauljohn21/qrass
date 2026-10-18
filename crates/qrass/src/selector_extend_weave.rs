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
    let xs = &extender.components;
    let n = xs.len();
    // A combinator-only extender (`>`): when the matched slot is fully consumed the
    // bare trailing combinator *is* the reading (`extend(".c", ".c", ">")` -> `>`).
    if n == 0 {
        return if residual.simples.is_empty() {
            vec![Complex {
                components: Vec::new(),
                trailing: extender.trailing.clone(),
            }]
        } else {
            Vec::new()
        };
    }
    let slot_comb = comps[start].combinator;
    if n > 1 && matches!(slot_comb, Some(c) if c != Combinator::Descendant) {
        return Vec::new();
    }
    let mut pinned = xs[n - 1].clone();
    pinned.compound = match compound_unify(residual, &pinned.compound) {
        Some(m) => m,
        None => return Vec::new(),
    };
    pinned.extras.clear();
    // When the extender lands at the very front its leading and the selector's own
    // leading combinator must agree; a clash makes the reading invalid.
    let leading = if start == 0 {
        match combine_leading(comps[0].combinator, xs[0].combinator) {
            Some(l) => l,
            None => return Vec::new(),
        }
    } else {
        comps[0].combinator
    };

    let tail = &comps[end + 1..];
    // A single trailing combinator on the extender becomes the edge joining the
    // spliced extender to the following tail (`.e >` before tail `.d` -> `.e > .d`).
    let tail_comb = if extender.trailing.len() == 1 && !tail.is_empty() {
        Some(extender.trailing[0])
    } else {
        None
    };
    let prefix = &xs[..n - 1];
    let floatable = start > 0
        && !prefix.is_empty()
        && is_descendant_run(&comps[..start])
        && prefix
            .iter()
            .all(|c| c.combinator.is_none_or(|x| x == Combinator::Descendant));

    if !floatable {
        let mut pieces: Vec<Component> = comps[..start].to_vec();
        for p in prefix {
            let mut pc = p.clone();
            pc.combinator = Some(Combinator::Descendant);
            pc.extras.clear();
            pieces.push(pc);
        }
        pinned.combinator = if pieces.is_empty() {
            leading
        } else {
            Some(Combinator::Descendant)
        };
        pieces.push(pinned);
        splice_tail(&mut pieces, tail, tail_comb);
        if let Some(f) = pieces.first_mut() {
            f.combinator = leading;
            f.extras.clear();
        }
        return vec![Complex::new(pieces)];
    }

    // Float the extender's outer compounds through the descendant grandparent run.
    let gp: Vec<Compound> = comps[..start].iter().map(|c| c.compound.clone()).collect();
    let pf: Vec<Compound> = prefix.iter().map(|c| c.compound.clone()).collect();
    interleave(&gp, &pf)
        .into_iter()
        .map(|merged| {
            let mut pieces: Vec<Component> = merged
                .into_iter()
                .enumerate()
                .map(|(i, compound)| Component {
                    combinator: if i == 0 { leading } else { Some(Combinator::Descendant) },
                    extras: Vec::new(),
                    compound,
                })
                .collect();
            pieces.push(Component {
                combinator: Some(Combinator::Descendant),
                extras: Vec::new(),
                compound: pinned.compound.clone(),
            });
            splice_tail(&mut pieces, tail, tail_comb);
            Complex::new(pieces)
        })
        .collect()
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
        (Some(x), Some(y)) => {
            if x == y {
                Some(Some(x))
            } else {
                None
            }
        }
    }
}

/// A run that floats freely: every (non-leading) edge is a descendant.
fn is_descendant_run(run: &[Component]) -> bool {
    run.iter()
        .all(|c| c.combinator.is_none_or(|x| x == Combinator::Descendant))
}

/// Every order-preserving merge of two compound sequences.
fn interleave(a: &[Compound], b: &[Compound]) -> Vec<Vec<Compound>> {
    if a.is_empty() {
        return vec![b.to_vec()];
    }
    if b.is_empty() {
        return vec![a.to_vec()];
    }
    let mut out: Vec<Vec<Compound>> = Vec::new();
    for rest in interleave(&a[1..], b) {
        let mut merged = Vec::with_capacity(1 + rest.len());
        merged.push(a[0].clone());
        merged.extend(rest);
        out.push(merged);
    }
    for rest in interleave(a, &b[1..]) {
        let mut merged = Vec::with_capacity(1 + rest.len());
        merged.push(b[0].clone());
        merged.extend(rest);
        out.push(merged);
    }
    out
}
