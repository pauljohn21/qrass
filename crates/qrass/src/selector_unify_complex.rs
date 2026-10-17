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
    let mut acc: Vec<Complex> = lists.first().map(|l| l.complexes.clone()).unwrap_or_default();
    for list in lists.iter().skip(1) {
        let mut next: Vec<Complex> = Vec::new();
        for a in &acc {
            for b in &list.complexes {
                next.extend(complex_unify(a, b));
            }
        }
        acc = next;
    }
    if acc.is_empty() {
        return None;
    }
    Some(SelectorList::new(acc))
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
            if subject_only {
                if let Some(first) = c.components.first_mut() {
                    first.combinator =
                        combine_leading(as_[0].combinator, bs[0].combinator);
                }
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
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let idx = n - 1 - k;
        let comb = if k == 0 { subject_comb } else { lead[idx + 1].combinator };
        out.push((lead[idx].compound.clone(), comb));
    }
    out
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
    let mut results: Vec<Vec<Node>> = Vec::new();

    // Emit the split orderings first (matching the goldens' left-to-right comma
    // order), then the merged reading last.
    if !suppress_split {
        // `k2` inner-most against the subject, `k1` floating outward.
        if can_float(*c1, *c2) {
            for rest in build(m1, &m2[1..]) {
                let mut seq = vec![(k2.clone(), *c2)];
                seq.extend(rest);
                results.push(seq);
            }
        }
        // `k1` inner-most, `k2` floating outward.
        if can_float(*c2, *c1) {
            for rest in build(&m1[1..], m2) {
                let mut seq = vec![(k1.clone(), *c1)];
                seq.extend(rest);
                results.push(seq);
            }
        }
    }

    if merge_ok {
        // The merged compound lists the *outer* partner's simples first: whichever
        // side floats outward in the accompanying split sits left-most. A forced
        // tight pair keeps `a` first; a shared-identity (`#id`/pseudo/root) merge
        // takes `b` first (the goldens' canonical order).
        let (mo, mi) = if !suppress_split {
            if can_float(*c1, *c2) { (k1, k2) } else { (k2, k1) }
        } else if id_or_pseudo_identity {
            (k2, k1)
        } else {
            (k1, k2)
        };
        match compound_unify(mo, mi) {
            Some(pm) => {
                let mc = meet.flatten().or(Some(Combinator::Descendant));
                for rest in build(&m1[1..], &m2[1..]) {
                    let mut seq = vec![(pm.clone(), mc)];
                    seq.extend(rest);
                    results.push(seq);
                }
            }
            // Both pinned to one slot but the compounds cannot coexist: impossible.
            None if tight_both => return Vec::new(),
            None => {}
        }
    }
    dedup(results)
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
    let mut dp = vec![vec![0usize; mm + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..mm).rev() {
            let take =
                if slot_anchor(&m1[i], &m2[j]) { dp[i + 1][j + 1] + 1 } else { 0 };
            dp[i][j] = take.max(dp[i + 1][j]).max(dp[i][j + 1]);
        }
    }
    let full = dp[0][0];
    if full == 0 {
        return (n, mm);
    }
    for i in 0..n {
        for j in 0..mm {
            if dp[i][j] == full && slot_anchor(&m1[i], &m2[j]) {
                return (i, j);
            }
        }
    }
    (n, mm)
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
        let mut split = run.len();
        while split > 0 && has_root(&run[split - 1].0) {
            split -= 1;
        }
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
    let mut variants: Vec<Vec<Node>> = Vec::new();
    if a_core.is_empty() {
        variants.push(b_core.to_vec());
    } else if b_core.is_empty() {
        variants.push(a_core.to_vec());
    } else {
        let mut v = b_core.to_vec();
        v.extend(relink(&a_core));
        variants.push(v);
        let mut v2 = a_core.to_vec();
        v2.extend(relink(&b_core));
        variants.push(v2);
    }
    // The *outer* (tail) arrangements vary fastest and this inner block slowest,
    // mirroring the multi-block comma order in `lcs/non_contiguous`.
    let mut out: Vec<Vec<Node>> = Vec::new();
    for var in variants {
        for rest in tail {
            let mut s = var.clone();
            s.extend(rest.iter().cloned());
            // Re-pin the rooted suffixes at the global outermost.
            s.extend(a_root.iter().cloned());
            s.extend(b_root.iter().cloned());
            out.push(s);
        }
    }
    out
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
fn dedup(mut seqs: Vec<Vec<Node>>) -> Vec<Vec<Node>> {
    let mut out: Vec<Vec<Node>> = Vec::new();
    for s in seqs.drain(..) {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
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
    if let (Some(x), Some(y)) = (find_id(a), find_id(b)) {
        if x == y {
            return true;
        }
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
fn finalize(mut lead: Vec<Node>, subject: &Compound) -> Complex {
    lead.reverse(); // outer → inner (matches left→right output)
    let mut comps: Vec<Component> = Vec::new();
    for i in 0..lead.len() {
        // The AST combinator of an element records its relation to the LEFT
        // (outer) neighbour; that relation is stored as the outer neighbour's
        // combinator-to-inner, i.e. `lead[i - 1].1`. The outermost element has
        // none.
        let combinator = if i == 0 { None } else { lead[i - 1].1 };
        comps.push(Component { combinator, extras: Vec::new(), compound: lead[i].0.clone() });
    }
    // The subject's combinator binds it to its parent (the inner-most lead).
    let subject_comb = lead.last().and_then(|n| n.1);
    comps.push(Component {
        combinator: subject_comb.filter(|_| !comps.is_empty()),
        extras: Vec::new(),
        compound: subject.clone(),
    });
    if let Some(first) = comps.first_mut() {
        first.combinator = None;
    }
    Complex::new(comps)
}
