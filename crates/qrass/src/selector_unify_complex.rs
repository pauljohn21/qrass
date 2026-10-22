//! Complex-selector unification for `selector.unify` (Round 23 Phase 3).
//!
//! The compound-level algebra lives in [`crate::selector_unify`]; this module
//! glues compounds into complex selectors while honouring combinators. Rules are
//! derived purely from the `unify/complex/**` `.hrx` goldens.
//!
//! The two subject (right-most) compounds always merge. The leading compounds are
//! then unified inner-most-first: at every boundary the two subject-adjacent
//! compounds either merge onto one element, or one is placed directly against the
//! subject while the other floats outward (and vice versa). A `>` or `+` *pins*
//! its compound tight against the subject — nothing may be inserted between them
//! — which collapses the otherwise free two-way interleave and is why unify can
//! yield one, two or three comma-separated selectors depending on the
//! combinators involved.

use rxrust::prelude::*;

use crate::selector_ast::{Complex, Combinator, Component, Compound, SelectorList, Simple};
use crate::selector_unify::{compound_unify, has_root, shares_pseudo_element};

/// A leading compound together with the combinator binding it to its *inner*
/// neighbour (toward the subject). For the compound adjacent to the subject this
/// is the subject's own combinator.
type Node = (Compound, Option<Combinator>);

/// Cross-product unify of several selector lists. Returns `None` when no pair of
/// complexes can unify (the builtin renders that as `null`).
pub fn unify_lists(lists: &[SelectorList]) -> Option<SelectorList> {
    // A bogus combinator chain (two or more combinators in a row, e.g. `.c + ~ > .d`,
    // or a dangling trailing combinator) has no well-defined unification: the
    // `unify/complex/combinators/{initial,multiple}` goldens demand `null` for any
    // such input, while a *single* combinator still unifies normally.
    if lists.iter().flat_map(|l| l.complexes.iter()).any(has_bogus_chain) {
        return None;
    }
    let acc: Vec<Complex> = lists.first().map(|l| l.complexes.clone()).unwrap_or_default();
    let rest: Vec<SelectorList> = lists.iter().skip(1).cloned().collect();
    // The cross-product fold: each owned list drives one `reduce_initial`
    // step, its inner pairwise `flat_map` staying a pure closure chain.
    let merged = crate::drain::drain_one(
        Local::from_iter(rest)
            .reduce_initial(acc, |acc, list| {
                acc.into_iter().flat_map(move |a| {
                    let right = list.complexes.clone();
                    right.into_iter().flat_map(move |b| complex_unify(&a, &b))
                })
                .collect()
            })
            .map(Ok)
            .box_it(),
    );
    (!merged.is_empty()).then(|| SelectorList::new(merged))
}

/// A complex carrying a doubled combinator edge (`Component.extras`) or a
/// dangling trailing combinator.
fn has_bogus_chain(c: &Complex) -> bool {
    !c.trailing.is_empty() || c.components.iter().any(|x| !x.extras.is_empty())
}

/// Merge the two relations a shared compound bears to its inner neighbour. The
/// combinators split into two mutually exclusive *families*: the ancestor family
/// (`> / descendant`) and the sibling family (`+ / ~`). A compound cannot be both
/// the subject's parent and its preceding sibling, so a cross-family pair is
/// contradictory (`None`). Within a family the more specific relation wins: `>`
/// beats a bare descendant, `+` beats `~`.
fn comb_meet(a: Option<Combinator>, b: Option<Combinator>) -> Option<Option<Combinator>> {
    let key = |c: Option<Combinator>| match c {
        None | Some(Combinator::Descendant) => (0, 0),
        Some(Combinator::Child) => (0, 1),
        Some(Combinator::NextSibling) => (1, 1),
        Some(Combinator::Sibling) => (1, 0),
    };
    let (fa, sa) = key(a);
    let (fb, sb) = key(b);
    if fa != fb {
        return None;
    }
    let second = sa.max(sb);
    let merged = match (fa, second) {
        (0, 1) => Some(Combinator::Child),
        (1, 1) => Some(Combinator::NextSibling),
        (1, 0) => Some(Combinator::Sibling),
        _ => {
            if a.is_none() && b.is_none() {
                None
            } else {
                Some(Combinator::Descendant)
            }
        }
    };
    Some(merged)
}

/// Whether a combinator pins its compound immediately against the inner
/// neighbour (`>` strict parent, `+` adjacent sibling) with no gap allowed.
fn is_tight(c: Option<Combinator>) -> bool {
    matches!(c, Some(Combinator::Child) | Some(Combinator::NextSibling))
}

/// Unify two complex selectors into zero or more complex selectors.
pub fn complex_unify(a: &Complex, b: &Complex) -> Vec<Complex> {
    let as_ = &a.components;
    let bs = &b.components;
    if as_.is_empty() || bs.is_empty() {
        return Vec::new();
    }
    // Subjects (right-most compounds) always merge.
    let Some(subject) = compound_unify(&as_.last().unwrap().compound, &bs.last().unwrap().compound)
    else {
        return Vec::new();
    };

    let m1 = lead_nodes(as_);
    let m2 = lead_nodes(bs);
    let subject_only = as_.len() == 1 && bs.len() == 1;
    build(&m1, &m2)
        .into_iter()
        .map(|lead| {
            let mut c = finalize(lead, &subject);
            // A subject-only pair (`> .c` + `.d`) has no lead chain to carry a
            // front-pinned combinator, so re-attach the merged leading combinator
            // the parser retained on the input's first component.
            if subject_only
                && let Some(first) = c.components.first_mut() {
                    first.combinator =
                        combine_leading(as_[0].combinator, bs[0].combinator);
                }
            c
        })
        .collect()
}

/// Merge two leading (front-pinned) combinators. A missing one yields to the
/// other; two present ones meet within their family, and a cross-family clash
/// (`>` against `+`/`~`) drops the marker entirely.
fn combine_leading(a: Option<Combinator>, b: Option<Combinator>) -> Option<Combinator> {
    match (a, b) {
        (None, other) | (other, None) => other,
        (Some(x), Some(y)) => comb_meet(Some(x), Some(y)).flatten(),
    }
}

/// Leading compounds subject-first, each tagged with the combinator that binds it
/// to its inner neighbour (the subject's combinator for the first).
fn lead_nodes(cs: &[Component]) -> Vec<Node> {
    let lead = &cs[..cs.len() - 1];
    let subject_comb = cs.last().unwrap().combinator;
    let n = lead.len();
    // subject-first walk: `k` counts inward-out, the binding combinator is the
    // neighbour one step further in (the subject's own for the first node)
    (0..n)
        .map(|k| {
            let idx = n - 1 - k;
            let comb = (k == 0)
                .then_some(subject_comb)
                .or_else(|| Some(lead[idx + 1].combinator))
                .flatten();
            (lead[idx].compound.clone(), comb)
        })
        .collect()
}

/// Enumerate the merged leading sequences (subject-first) for the two streams.
fn build(m1: &[Node], m2: &[Node]) -> Vec<Vec<Node>> {
    if m1.is_empty() && m2.is_empty() {
        return vec![Vec::new()];
    }
    if m1.is_empty() {
        return vec![m2.to_vec()];
    }
    if m2.is_empty() {
        return vec![m1.to_vec()];
    }
    let (k1, c1) = &m1[0];
    let (k2, c2) = &m2[0];
    let meet = comb_meet(*c1, *c2);
    let tight_both = is_tight(*c1) && is_tight(*c2);
    let aligned = should_align(k1, k2);
    // Two compounds on the *same* (subject-adjacent) slot may only be kept apart
    // when neither relation pins it; a shared sibling family always admits the
    // merged reading in addition to the split ones.
    let sibling_family =
        matches!(meet.flatten(), Some(Combinator::Sibling) | Some(Combinator::NextSibling));
    // Merge the two subject-adjacent compounds onto one element. Forced when both
    // are tight onto the same slot; otherwise offered when they already denote one
    // element (`aligned`) or share a sibling family.
    let merge_ok = meet.is_some() && (tight_both || aligned || sibling_family);

    // Pure ancestor-chain blocks. When neither front can share a slot *and* both
    // relations are loose ancestors (a bare descendant, never `>` / `+` / `~`), the
    // two remaining runs are strict ancestor chains that dart-sass only ever
    // interleaves as whole contiguous blocks — `[B then A]` and `[A then B]`, never
    // a per-compound shuffle. Commit both blocks out to the next shared slot.
    if !merge_ok
        && matches!(*c1, None | Some(Combinator::Descendant))
        && matches!(*c2, None | Some(Combinator::Descendant))
    {
        let (pa, pb) = next_anchor(m1, m2);
        let a_run = &m1[..pa];
        let b_run = &m2[..pb];
        let tail = build(&m1[pa..], &m2[pb..]);
        return dedup(block_orders(a_run, b_run, &tail));
    }

    // Suppress the split orderings when the merged reading is the only valid one:
    // an ancestor-family merge, a forced tight pair, or a sibling pair whose
    // redundant (subsumed) compound is loose — a *tight* (`+`) subsumed compound
    // must stay pinned as its own element, so that pair still splits.
    let ancestor_family = meet.is_some() && !sibling_family;
    let same_identity = matches!(find_id(k1).zip(find_id(k2)), Some((x, y)) if x == y)
        || shares_pseudo_element(k1, k2)
        || (has_root(k1) && has_root(k2));
    let id_or_pseudo_identity = matches!(find_id(k1).zip(find_id(k2)), Some((x, y)) if x == y)
        || shares_pseudo_element(k1, k2);
    let merged_probe = compound_unify(k1, k2);
    let sub1 = matches!(merged_probe, Some(ref u) if u == k1);
    let sub2 = matches!(merged_probe, Some(ref u) if u == k2);
    let subsumed_comb = match (sub1, sub2) {
        (true, true) => None,          // equal → always one element
        (true, false) => Some(*c2),    // k2 redundant
        (false, true) => Some(*c1),    // k1 redundant
        _ => None,                     // distinct union, no subsumption
    };
    let sibling_suppress = match subsumed_comb {
        // A tight (`+`/`>`) redundant compound stays pinned as its own element,
        // so the pair still splits; a loose one merges away.
        Some(rc) => !is_tight(rc),
        None if sub1 && sub2 => true,
        None => same_identity,
    };
    let suppress_split =
        merge_ok && (ancestor_family || tight_both || (sibling_family && sibling_suppress));

    // Emit the split orderings first (matching the goldens' left-to-right comma
    // order), then the merged reading last.
    let prepend = |head: Node, rest: Vec<Vec<Node>>| {
        rest.into_iter()
            .map(|r| {
                let mut s = vec![head.clone()];
                s.extend(r);
                s
            })
            .collect::<Vec<Vec<Node>>>()
    };
    let split_reads = (!suppress_split).then(|| {
        // single binary value branches (not an if/else-if chain): the two float
        // readings, each empty when its relation cannot float.
        let k1_out = if can_float(*c1, *c2) {
            prepend((k2.clone(), *c2), build(m1, &m2[1..]))
        } else {
            Vec::new()
        };
        let k2_out = if can_float(*c2, *c1) {
            prepend((k1.clone(), *c1), build(&m1[1..], m2))
        } else {
            Vec::new()
        };
        k1_out.into_iter().chain(k2_out).collect::<Vec<Vec<Node>>>()
    });
    let split_reads = split_reads.unwrap_or_default();

    let (mo, mi) = match (!suppress_split, can_float(*c1, *c2), id_or_pseudo_identity) {
        (true, true, _) | (false, _, false) => (k1, k2),
        (true, false, _) | (false, _, true) => (k2, k1),
    };
    // The merged compound lists the *outer* partner's simples first: whichever
    // side floats outward in the accompanying split sits left-most. A forced
    // tight pair keeps `a` first; a shared-identity (`#id`/pseudo/root) merge
    // takes `b` first (the goldens' canonical order).
    let merged_probe2 = merge_ok.then(|| compound_unify(mo, mi)).flatten();
    // Both pinned to one slot but the compounds cannot coexist: impossible.
    if merge_ok && merged_probe2.is_none() && tight_both {
        return Vec::new();
    }
    let merged_reads = merged_probe2
        .map(|pm| {
            let mc = meet.flatten().or(Some(Combinator::Descendant));
            prepend((pm, mc), build(&m1[1..], &m2[1..]))
        })
        .unwrap_or_default();
    dedup(split_reads.into_iter().chain(merged_reads).collect())
}

/// Can the two nodes denote one element (share a slot)? Mirrors the merge
/// decision in [`build`]: a compatible (same-family) relation plus a forced or
/// genuinely co-locatable compound pair.
fn slot_anchor(a: &Node, b: &Node) -> bool {
    let (k1, c1) = a;
    let (k2, c2) = b;
    let meet = comb_meet(*c1, *c2);
    if meet.is_none() {
        return false;
    }
    let tight_both = is_tight(*c1) && is_tight(*c2);
    let sibling_family =
        matches!(meet.flatten(), Some(Combinator::Sibling) | Some(Combinator::NextSibling));
    tight_both || sibling_family || should_align(k1, k2)
}

/// Indices of the first subject-adjacent slot (scanning inward-out) where the two
/// runs can anchor, chosen so the anchor lies on a *longest* chain of shared
/// compounds. A greedy "first aligned pair" scan can pick a short-lived match and
/// leave a longer common subsequence un-unified; instead compute the suffix-LCS
/// length `dp[i][j]` over the [`slot_anchor`] alignment predicate and take the
/// inner-most aligned pair `(i, j)` whose `dp[i][j]` still equals the global LCS
/// `dp[0][0]` (anchoring there forfeits no matches). Falls back to the far ends
/// when nothing aligns.
fn next_anchor(m1: &[Node], m2: &[Node]) -> (usize, usize) {
    let n = m1.len();
    let mm = m2.len();
    // Suffix-LCS table built row-by-row inward-out: each row is a fold over the
    // reversed columns seeded with the row below.
    let dp: Vec<Vec<usize>> = (0..n).rev().fold(
        vec![vec![0usize; mm + 1]; n + 1],
        |mut acc, i| {
            acc[i] = (0..mm).rev().fold(acc[i + 1].clone(), |mut row, j| {
                let take =
                    if slot_anchor(&m1[i], &m2[j]) { acc[i + 1][j + 1] + 1 } else { 0 };
                row[j] = take.max(acc[i + 1][j]).max(row[j + 1]);
                row
            });
            acc
        },
    );
    let full = dp[0][0];
    if full == 0 {
        return (n, mm);
    }
    // the inner-most pair still on a longest chain
    (0..n)
        .flat_map(|i| (0..mm).map(move |j| (i, j)))
        .find(|(i, j)| dp[*i][*j] == full && slot_anchor(&m1[*i], &m2[*j]))
        .unwrap_or((n, mm))
}

/// Whole-block interleavings of two ancestor runs feeding a shared `tail`.
/// `inner-first` runs sit closest to the subject; the outer run's inner-most
/// compound is relinked with a bare descendant so the two chains stay contiguous.
/// The `[B inner, A inner]` order matches the goldens' comma ordering.
fn block_orders(a_run: &[Node], b_run: &[Node], tail: &[Vec<Node>]) -> Vec<Vec<Node>> {
    let relink = |run: &[Node]| -> Vec<Node> {
        let mut v = run.to_vec();
        if let Some(first) = v.first_mut() {
            first.1 = Some(Combinator::Descendant);
        }
        v
    };
    // A root-ish compound (`:root`/`:scope`/`:host`) names one unique element and
    // can only sit outermost. Peel any such compounds from each run's OUTER end and
    // pin them back at the global outermost *after* the interleave — the remaining
    // unrooted compounds are then free to swap as a whole block (`:root .c .e`,
    // `:root .e .c`). This is stricter than the old whole-run suppression, which
    // wrongly froze an unrooted sibling of the root as well.
    fn peel_outer_rooted(run: &[Node]) -> (Vec<Node>, Vec<Node>) {
        // the outermost non-root position + 1 is the peel point (0 when all rooted)
        let split = run.iter().rposition(|n| !has_root(&n.0)).map_or(0, |p| p + 1);
        (run[..split].to_vec(), run[split..].to_vec())
    }
    let (a_core, a_root) = peel_outer_rooted(a_run);
    let (b_core, b_root) = peel_outer_rooted(b_run);
    // Two runs each rooted to a *distinct* element (they failed to anchor/merge in
    // [`build`]) cannot coexist: a selector has at most one root element. Such a
    // pair unifies to nothing.
    if !a_root.is_empty() && !b_root.is_empty() {
        return Vec::new();
    }

    // Candidate inner arrangements of the two cores. `B` floated inward (so `A`
    // sits outer-most) comes first, matching the goldens' left-to-right comma
    // order; each run's inner-most compound is relinked with a bare descendant so
    // the two chains stay contiguous when the other floats outward.
    let variants: Vec<Vec<Node>> = match (a_core.is_empty(), b_core.is_empty()) {
        (true, _) => vec![b_core.to_vec()],
        (false, true) => vec![a_core.to_vec()],
        (false, false) => {
            let mut v = b_core.to_vec();
            v.extend(relink(&a_core));
            let mut v2 = a_core.to_vec();
            v2.extend(relink(&b_core));
            vec![v, v2]
        }
    };
    // The *outer* (tail) arrangements vary fastest and this inner block slowest,
    // mirroring the multi-block comma order in `lcs/non_contiguous`.
    variants
        .into_iter()
        .flat_map(move |var| {
            let (ar, br) = (a_root.clone(), b_root.clone());
            tail.iter().cloned().map(move |rest| {
                let mut s = var.clone();
                s.extend(rest);
                // Re-pin the rooted suffixes at the global outermost.
                s.extend(ar.iter().cloned());
                s.extend(br.iter().cloned());
                s
            })
        })
        .collect()
}

/// May the compound with relation `floated` to the subject be pushed outward so
/// that `inner` ends up between it and the subject? The relation must still hold
/// after the insertion:
/// - a descendant (or bare) compound may float past anything;
/// - a `~` sibling stays a preceding sibling of the subject only if `inner` is
///   likewise in the sibling family (siblings of siblings share the parent);
/// - a `+` adjacent sibling never floats (any intervening compound breaks
///   adjacency);
/// - a `>` child stays the subject's parent when `inner` is a sibling of the
///   subject (parent-of-a-sibling is the shared parent), but never past another
///   ancestor compound.
fn can_float(floated: Option<Combinator>, inner: Option<Combinator>) -> bool {
    let inner_sibling =
        matches!(inner, Some(Combinator::Sibling) | Some(Combinator::NextSibling));
    match floated {
        None | Some(Combinator::Descendant) => true,
        Some(Combinator::Sibling) => inner_sibling,
        Some(Combinator::Child) => inner_sibling,
        Some(Combinator::NextSibling) => false,
    }
}

/// Remove duplicate lead orderings that arise from identical compounds.
fn dedup(seqs: Vec<Vec<Node>>) -> Vec<Vec<Node>> {
    seqs.into_iter().fold(Vec::new(), |mut acc, s| {
        acc.extend((!acc.contains(&s)).then_some(s));
        acc
    })
}

/// Do two leading compounds align (denote one element)? They align when one
/// subsumes the other or they force-merge on a shared identity (`#id`,
/// pseudo-element, or a root pseudo-class on both).
fn should_align(a: &Compound, b: &Compound) -> bool {
    if let Some(u) = compound_unify(a, b) {
        if u == *a || u == *b {
            return true;
        }
        return shares_identity(a, b);
    }
    false
}

fn shares_identity(a: &Compound, b: &Compound) -> bool {
    if let (Some(x), Some(y)) = (find_id(a), find_id(b))
        && x == y {
            return true;
        }
    if shares_pseudo_element(a, b) {
        return true;
    }
    has_root(a) && has_root(b)
}

fn find_id(c: &Compound) -> Option<&str> {
    c.simples.iter().find_map(|s| match s {
        Simple::Id(v) => Some(v.as_str()),
        _ => None,
    })
}

/// Convert a subject-first merged lead into a final left→right complex, splicing
/// the shared subject onto the end.
fn finalize(lead: Vec<Node>, subject: &Compound) -> Complex {
    // outer → inner (matches left→right output)
    let lead: Vec<Node> = lead.into_iter().rev().collect();
    let mut comps: Vec<Component> = lead
        .iter()
        .enumerate()
        // The AST combinator of an element records its relation to the LEFT
        // (outer) neighbour; that relation is stored as the outer neighbour's
        // combinator-to-inner, i.e. `lead[i - 1].1`. The outermost element has
        // none.
        .map(|(i, node)| Component {
            combinator: (i > 0).then(|| lead[i - 1].1).flatten(),
            extras: Vec::new(),
            compound: node.0.clone(),
        })
        .collect();
    // The subject's combinator binds it to its parent (the inner-most lead).
    let subject_comb = lead.last().and_then(|n| n.1).filter(|_| !comps.is_empty());
    comps.push(Component {
        combinator: subject_comb,
        extras: Vec::new(),
        compound: subject.clone(),
    });
    if let Some(first) = comps.first_mut() {
        first.combinator = None;
    }
    Complex::new(comps)
}
