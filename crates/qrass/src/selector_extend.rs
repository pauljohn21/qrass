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
    // each input selector contributes its verbatim form — unless a pure in-place
    // pseudo growth supersedes it — plus the additive results and that growth
    let (mut out, mut verbatim) = selectors.complexes.iter().fold(
        (Vec::new(), Vec::new()),
        |(mut out, mut verbatim), sel| {
            let adds = top_level(sel, extendee, extender);
            let repl = pseudo_replace(sel, extendee, extender);
            (repl.is_none() || !adds.is_empty())
                .then_some(sel)
                .into_iter()
                .for_each(|sel| push_marked(&mut out, &mut verbatim, sel.clone(), true));
            adds.into_iter()
                .for_each(|add| push_marked(&mut out, &mut verbatim, add, false));
            repl.into_iter()
                .for_each(|r| push_marked(&mut out, &mut verbatim, r, false));
            (out, verbatim)
        },
    );
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
    let out: Vec<Complex> = selectors.complexes.iter().fold(Vec::new(), |mut out, sel| {
        let adds = top_level(sel, extendee, extender);
        let sub = pseudo_substitute(sel, extendee, extender);
        (adds.is_empty() && sub.is_none())
            .then_some(sel.clone())
            .into_iter()
            .chain(adds)
            .chain(sub)
            .for_each(|c| push_unique(&mut out, c));
        out
    });
    SelectorList::new(out)
}

/// Append `c` unless an identical complex already precedes it, tracking whether
/// the entry is a verbatim input selector (a duplicate keeps the verbatim mark).
fn push_marked(out: &mut Vec<Complex>, verbatim: &mut Vec<bool>, c: Complex, is_verbatim: bool) {
    match out.iter().position(|x| x == &c) {
        // a duplicate keeps (and may upgrade) the verbatim mark
        Some(pos) => verbatim[pos] |= is_verbatim,
        None => {
            out.push(c);
            verbatim.push(is_verbatim);
        }
    }
}

/// Additive selectors from a compound (top-level) match of `extendee` in `sel`.
pub(super) fn top_level(
    sel: &Complex,
    extendee: &SelectorList,
    extender: &SelectorList,
) -> Vec<Complex> {
    let comps = &sel.components;
    // split the alternatives: single-compound needles vs multi-compound
    // complexes (the latter anchor on their last compound)
    let (singles, multis): (Vec<&Compound>, Vec<&Complex>) = extendee.complexes.iter().fold(
        (Vec::new(), Vec::new()),
        |(mut singles, mut multis), e| {
            match e.components.split_first() {
                Some((first, [])) => singles.push(&first.compound),
                _ => multis.push(e),
            }
            (singles, multis)
        },
    );
    let mut results = Vec::new();
    // Single-compound extendees: subtract the union of all alternatives a slot covers.
    comps.iter().enumerate().for_each(|(i, comp)| {
        let matched: Vec<&Compound> = singles
            .iter()
            .copied()
            .filter(|n| covers(&comp.compound, n))
            .collect();
        (!matched.is_empty())
            .then_some(matched)
            .into_iter()
            .for_each(|matched| {
                let residual = subtract_many(&comp.compound, &matched);
                results.extend(
                    extender
                        .complexes
                        .iter()
                        .flat_map(|ext_with| extend_slot(sel, i, i, &residual, ext_with)),
                );
            });
    });
    // Multi-compound extendees: the last compound anchors, the prefix must line up.
    multis.iter().for_each(|e| {
        let last = e.components.len() - 1;
        let tail_needle = &e.components[last].compound;
        comps
            .iter()
            .enumerate()
            .skip(last)
            .filter(|(i, comp)| {
                covers(&comp.compound, tail_needle)
                    && e.components[..last]
                        .iter()
                        .zip(&comps[*i - last..*i])
                        .all(|(ee, c)| covers(&c.compound, &ee.compound))
            })
            .for_each(|(i, _)| {
                let start = i - last;
                let residual = subtract(&comps[i].compound, tail_needle);
                results.extend(
                    extender
                        .complexes
                        .iter()
                        .flat_map(|ext_with| extend_slot(sel, start, i, &residual, ext_with)),
                );
            });
    });
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
    let bogus = is_bogus(sel) || is_bogus(ext_with);
    let tail_conflict = !sel.trailing.is_empty() && !ext_with.trailing.is_empty();
    (!bogus && !tail_conflict)
        .then(|| weave_all(&sel.components, start, end, residual, ext_with))
        .map(|results| {
            // a lone trailing combinator on the selector rides onto each reading
            results
                .into_iter()
                .map(|mut r| {
                    if sel.trailing.len() == 1 && ext_with.trailing.is_empty() {
                        r.trailing = sel.trailing.clone();
                    }
                    r
                })
                .collect()
        })
        .unwrap_or_default()
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
            // verbatim inputs survive; only generated selectors may be absorbed
            !verbatim[i]
                && snapshot.iter().enumerate().any(|(j, d)| {
                    let c = &out[i];
                    i != j
                        && !has_dangling(d)
                        && !has_dangling(c)
                        && covers_complex(d, c)
                        && !covers_complex(c, d)
                })
        })
        .collect();
    dropped.into_iter().rev().for_each(|i| {
        out.remove(i);
        verbatim.remove(i);
    });
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
