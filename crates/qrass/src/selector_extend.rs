//! `selector.extend($selectors, $extendee, $extender)` (Round 23 Phase 4), built
//! on the owned AST from [`crate::selector_ast`] and the compound algebra of
//! [`crate::selector_unify`].
//!
//! The rules are derived purely from the `extend/**` `.hrx` goldens (never from a
//! reference implementation). Per output:
//!
//! * a top-level compound match removes the *union* of every (single-compound)
//!   extendee alternative that the slot covers, leaving a *residual* that is
//!   unified onto the extender's inner-most compound; each extender alternative
//!   then yields an additive sibling selector;
//! * a selector-list pseudo (`:is`, `:has`, `::slotted`, …) grows its argument list
//!   in place, and `:not` appends `:not()` simples instead — see
//!   [`crate::selector_extend_pseudo`];
//! * the extender is spliced in and, past a descendant grandparent, its outer
//!   compounds may float outward — see [`crate::selector_extend_weave`].

use crate::selector_ast::{Complex, Compound, SelectorList};
use crate::selector_extend_pseudo::{pseudo_replace, pseudo_substitute};
use crate::selector_extend_weave::weave_all;
use crate::selector_super::list_is_super;

/// Extend `selectors` by replacing every occurrence of `extendee` with
/// `extender`. Returns each selector's verbatim form plus its extensions, unless
/// a pseudo in-place growth replaces the selector (then only the grown form).
pub fn extend_list(
    selectors: &SelectorList,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> SelectorList {
    let mut out: Vec<Complex> = Vec::new();
    // Parallel flag: `true` marks a verbatim selector from the caller's input.
    // `extend` is additive, so those always survive the subsumption trim; only
    // extension / pseudo-growth results may be absorbed by a broader selector.
    let mut verbatim: Vec<bool> = Vec::new();
    for sel in &selectors.complexes {
        let adds = top_level(sel, extendee, extender);
        let repl = pseudo_replace(sel, extendee, extender);
        // A pure in-place pseudo growth supersedes the verbatim selector.
        if repl.is_none() || !adds.is_empty() {
            push_marked(&mut out, &mut verbatim, sel.clone(), true);
        }
        for add in adds {
            push_marked(&mut out, &mut verbatim, add, false);
        }
        if let Some(r) = repl {
            push_marked(&mut out, &mut verbatim, r, false);
        }
    }
    trim_subsumed(&mut out, &mut verbatim);
    SelectorList::new(out)
}

/// `selector.replace($selectors, $original, $replacement)` — the non-addive
/// sibling of [`extend_list`], derived from the `selector/replace.hrx` goldens:
///
/// * a selector whose `$original` matches at the top level is replaced by the
///   extension(s) `top_level` yields (a complex replacement still weaves into the
///   descendant position, e.g. `c d` + `d`→`e f` = `c e f, e c f`);
/// * a selector carrying `$original` inside a selector-list pseudo (`:is`/`:where`/
///   `:matches`/`:not`/…) substitutes it in place (`:is(c)` + `c`→`d` = `:is(d)`),
///   rather than growing the argument list the way `extend` does;
/// * a selector that matches nothing survives verbatim (`no_op`);
/// * unlike `extend`, results are NEVER subsumption-trimmed across inputs, so a
///   broad replacement cannot absorb a narrower one from a sibling selector
///   (`replace((c, d c), c, e)` = `e, d e`, not `e`).
pub fn replace_list(
    selectors: &SelectorList,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> SelectorList {
    let mut out: Vec<Complex> = Vec::new();
    for sel in &selectors.complexes {
        let adds = top_level(sel, extendee, extender);
        let sub = pseudo_substitute(sel, extendee, extender);
        if adds.is_empty() && sub.is_none() {
            push_unique(&mut out, sel.clone());
        } else {
            for add in adds {
                push_unique(&mut out, add);
            }
            if let Some(s) = sub {
                push_unique(&mut out, s);
            }
        }
    }
    SelectorList::new(out)
}

/// Append `c` unless an identical complex already precedes it, tracking whether
/// the entry is a verbatim input selector (a duplicate keeps the verbatim mark).
fn push_marked(out: &mut Vec<Complex>, verbatim: &mut Vec<bool>, c: Complex, is_verbatim: bool) {
    if let Some(pos) = out.iter().position(|x| x == &c) {
        if is_verbatim {
            verbatim[pos] = true;
        }
        return;
    }
    out.push(c);
    verbatim.push(is_verbatim);
}

/// Additive selectors from a compound (top-level) match of `extendee` in `sel`.
pub(super) fn top_level(
    sel: &Complex,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> Vec<Complex> {
    let mut results = Vec::new();
    let comps = &sel.components;
    let mut singles: Vec<&Compound> = Vec::new();
    let mut multis: Vec<&Complex> = Vec::new();
    for e in &extendee.complexes {
        if e.components.len() == 1 {
            singles.push(&e.components[0].compound);
        } else {
            multis.push(e);
        }
    }
    // Single-compound extendees: subtract the union of all alternatives a slot covers.
    for (i, comp) in comps.iter().enumerate() {
        let matched: Vec<&Compound> = singles
            .iter()
            .copied()
            .filter(|n| covers(&comp.compound, n))
            .collect();
        if matched.is_empty() {
            continue;
        }
        let residual = subtract_many(&comp.compound, &matched);
        for ext_with in &extender.complexes {
            results.extend(extend_slot(sel, i, i, &residual, ext_with));
        }
    }
    // Multi-compound extendees: the last compound anchors, the prefix must line up.
    for e in multis {
        let last = e.components.len() - 1;
        let tail_needle = &e.components[last].compound;
        for i in last..comps.len() {
            if !covers(&comps[i].compound, tail_needle) {
                continue;
            }
            let start = i - last;
            if !e.components[..last]
                .iter()
                .zip(&comps[start..i])
                .all(|(ee, c)| covers(&c.compound, &ee.compound))
            {
                continue;
            }
            let residual = subtract(&comps[i].compound, tail_needle);
            for ext_with in &extender.complexes {
                results.extend(extend_slot(sel, start, i, &residual, ext_with));
            }
        }
    }
    results
}

/// Splice `ext_with` at `sel.comps[start..=end]`, guarding the bogus-combinator
/// goldens: a chain of two or more combinators (leading, middle, or trailing) in
/// either operand drops the extension, and a trailing combinator on *both* the
/// selector and the extender is a conflict. A lone trailing combinator on the
/// selector is carried onto each produced reading.
fn extend_slot(
    sel: &Complex,
    start: usize,
    end: usize,
    residual: &Compound,
    ext_with: &Complex,
) -> Vec<Complex> {
    if is_bogus(sel) || is_bogus(ext_with) {
        return Vec::new();
    }
    if !sel.trailing.is_empty() && !ext_with.trailing.is_empty() {
        return Vec::new();
    }
    let mut results = weave_all(&sel.components, start, end, residual, ext_with);
    if sel.trailing.len() == 1 && ext_with.trailing.is_empty() {
        for r in &mut results {
            r.trailing = sel.trailing.clone();
        }
    }
    results
}

/// A combinator chain with a doubled edge, or two or more dangling combinators.
fn is_bogus(c: &Complex) -> bool {
    c.trailing.len() >= 2 || c.components.iter().any(|x| !x.extras.is_empty())
}

/// Drop *generated* selectors (extensions / pseudo growth) strictly subsumed by
/// another result — e.g. `c` absorbs the more specific `d c` variant a
/// self-referential extender would have produced. Verbatim input selectors are
/// never dropped (`extend` is additive), and bogus-combinator results are exempt
/// (their edge structure is not comparable).
fn trim_subsumed(out: &mut Vec<Complex>, verbatim: &mut Vec<bool>) {
    let snapshot = out.clone();
    let dropped: Vec<usize> = (0..out.len())
        .filter(|&i| {
            if verbatim[i] {
                return false;
            }
            let c = &out[i];
            snapshot.iter().enumerate().any(|(j, d)| {
                i != j
                    && !has_dangling(d)
                    && !has_dangling(c)
                    && covers_complex(d, c)
                    && !covers_complex(c, d)
            })
        })
        .collect();
    for i in dropped.into_iter().rev() {
        out.remove(i);
        verbatim.remove(i);
    }
}

/// Any dangling or doubled combinator structure.
fn has_dangling(c: &Complex) -> bool {
    !c.trailing.is_empty() || c.components.iter().any(|x| !x.extras.is_empty())
}

fn covers_complex(super_c: &Complex, sub_c: &Complex) -> bool {
    list_is_super(
        &SelectorList::new(vec![super_c.clone()]),
        &SelectorList::new(vec![sub_c.clone()]),
    )
}

/// Does `c` require everything `needle` requires (a compound-level superselector)?
fn covers(c: &Compound, needle: &Compound) -> bool {
    needle.simples.iter().all(|s| c.simples.contains(s))
}

/// The simples of `c` that are not part of `needle`.
fn subtract(c: &Compound, needle: &Compound) -> Compound {
    Compound::new(
        c.simples
            .iter()
            .filter(|s| !needle.simples.contains(s))
            .cloned()
            .collect(),
    )
}

/// The simples of `c` covered by none of `needles`.
fn subtract_many(c: &Compound, needles: &[&Compound]) -> Compound {
    Compound::new(
        c.simples
            .iter()
            .filter(|s| !needles.iter().any(|n| n.simples.contains(s)))
            .cloned()
            .collect(),
    )
}

/// Append `c` unless an identical complex is already present.
pub(super) fn push_unique(out: &mut Vec<Complex>, c: Complex) {
    if !out.contains(&c) {
        out.push(c);
    }
}
