//! Compound-level unification for `selector.unify` (Round 23 Phase 3), operating
//! purely on the owned AST from [`crate::selector_ast`]. The complex-selector
//! glue (combinators, subject alignment) lives in [`crate::selector_unify_complex`].
//!
//! Rules are derived from the `unify/*` `.hrx` goldens (never from any reference
//! implementation). `selector.unify($s1, $s2)` returns a selector matching only
//! elements matched by *both* inputs, or null when no element can match. Within a
//! single compound:
//! - simples are merged (classes/attributes/pseudo-classes are a set union,
//!   deduped); a type/universal merges by namespace + name; a second, different
//!   `#id` (or two conflicting types) makes it impossible; a pseudo-element splits
//!   the compound into base / element / after so a trailing pseudo-class never
//!   crosses over the element (`do_not_cross`).
//! - a `:host`-family pseudo tolerates only selector-list pseudos and other host
//!   pseudos beside it, never a class/id/type/attribute/ordinary pseudo-class.

use crate::selector_ast::{Compound, Ns, PseudoArg, Simple};

/// Unify two compounds into one, or `None` when no element can match both.
pub fn compound_unify(a: &Compound, b: &Compound) -> Option<Compound> {
    let (a_before, a_el, a_after) = a.split_at_element();
    let (b_before, b_el, b_after) = b.split_at_element();

    let el = match (a_el, b_el) {
        (Some(x), Some(y)) => {
            if !element_compatible(x, y) {
                return None;
            }
            Some(x.clone())
        }
        (Some(x), None) | (None, Some(x)) => Some(x.clone()),
        (None, None) => None,
    };

    let mut before = merge_group(a_before, b_before)?;
    before.extend(el);
    let after = merge_group(a_after, b_after)?;
    before.extend(after);
    let merged = Compound::new(before);
    if !host_compatible(&merged) {
        return None;
    }
    Some(merged)
}

/// A `:host` / `:host-context` pseudo scopes to the shadow host and can only
/// coexist, within one compound, with selector-list pseudos (`:is`, `:where`,
/// …) and other host pseudos. The goldens reject it beside a class, id, type,
/// universal, attribute, or an ordinary pseudo-class (`:hover`) — such a pair
/// unifies to nothing.
fn host_compatible(c: &Compound) -> bool {
    let has_host = c.simples.iter().any(is_host_pseudo);
    if !has_host {
        return true;
    }
    !c.simples.iter().any(|s| !is_host_pseudo(s) && !is_selector_pseudo(s))
}

fn is_host_pseudo(s: &Simple) -> bool {
    match s {
        Simple::PseudoClass { name, .. } => {
            name == "host" || name == "host-context" || name.ends_with("-host")
        }
        _ => false,
    }
}

fn is_selector_pseudo(s: &Simple) -> bool {
    let name = match s {
        Simple::PseudoClass { name, .. } => name,
        _ => return false,
    };
    ["is", "where", "matches", "any"].iter().any(|p| name == *p || name.ends_with(&format!("-{p}")))
}

fn element_compatible(a: &Simple, b: &Simple) -> bool {
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
    let _ = (la, lb);
    na == nb && compatible_arg(arga, argb)
}

fn compatible_arg(a: &PseudoArg, b: &PseudoArg) -> bool {
    match (a, b) {
        (PseudoArg::None, _) | (_, PseudoArg::None) => true,
        (PseudoArg::Text(x), PseudoArg::Text(y)) => x == y,
        (PseudoArg::Selectors(x), PseudoArg::Selectors(y)) => {
            for cx in &x.complexes {
                for cy in &y.complexes {
                    if !crate::selector_unify_complex::complex_unify(cx, cy).is_empty() {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Merge two simple lists that share no pseudo-element: unify any type, enforce a
/// single `#id`, dedupe, and emit in canonical order (type, class/id/attr,
/// pseudo-class).
fn merge_group(a: &[Simple], b: &[Simple]) -> Option<Vec<Simple>> {
    let mut all: Vec<Simple> = Vec::new();
    all.extend(a.iter().cloned());
    all.extend(b.iter().cloned());

    // Type/universal: at most one, unified across the two sources.
    let mut ty: Option<Simple> = None;
    for s in all.iter().filter(|s| s.is_type_like()) {
        ty = Some(match ty {
            None => s.clone(),
            Some(prev) => unify_type(&prev, s)?,
        });
    }

    // Id: two different ids cannot co-occur.
    let mut id: Option<String> = None;
    for s in all.iter() {
        if let Simple::Id(v) = s {
            match &id {
                None => id = Some(v.clone()),
                Some(prev) if prev == v => {}
                Some(_) => return None,
            }
        }
    }

    // Rank everything else.
    let mut mid: Vec<Simple> = Vec::new();
    let mut pseudo_classes: Vec<Simple> = Vec::new();
    let mut seen_mid: Vec<Simple> = Vec::new();
    let mut seen_pc: Vec<Simple> = Vec::new();
    for s in all.iter() {
        match s {
            Simple::Universal { .. } | Simple::Type { .. } => {}
            Simple::Id(_) => {}
            Simple::PseudoClass { .. } => {
                if !seen_pc.contains(s) {
                    seen_pc.push(s.clone());
                    pseudo_classes.push(s.clone());
                }
            }
            Simple::PseudoElement { .. } => {}
            _ => {
                if !seen_mid.contains(s) {
                    seen_mid.push(s.clone());
                    mid.push(s.clone());
                }
            }
        }
    }

    let mut out: Vec<Simple> = Vec::new();
    if let Some(t) = ty {
        out.push(t);
    }
    if let Some(v) = id {
        out.push(Simple::Id(v));
    }
    out.extend(mid);
    // Pseudo-classes keep their merged source order, with one nudge: a `:host`
    // (family) that *led* the compound is shifted to sit just after the first
    // selector-list pseudo it merged with — `:host` + `:is(.c):is(.d)` renders
    // `:is(.c):host:is(.d)`, yet a host that already trailed stays put
    // (`:is(.c):is(.d)` + `:host` → `:is(.c):is(.d):host`).
    let lead_host = pseudo_classes.iter().take_while(|s| is_host_pseudo(s)).count();
    if lead_host > 0 && lead_host < pseudo_classes.len() {
        // `pseudo_classes[lead_host]` is the first non-host; slot the leading host
        // block in right after it.
        let mut hosts: Vec<Simple> = pseudo_classes.drain(..lead_host).collect();
        out.extend(pseudo_classes.drain(..1));
        out.append(&mut hosts);
        out.extend(pseudo_classes);
    } else {
        out.extend(pseudo_classes);
    }
    Some(out)
}

/// Unify two type-like simples (both `Type` or `Universal`).
fn unify_type(a: &Simple, b: &Simple) -> Option<Simple> {
    let (an, a_name) = match a {
        Simple::Type { ns, name } => (ns, Some(name.as_str())),
        Simple::Universal { ns } => (ns, None),
        _ => return None,
    };
    let (bn, b_name) = match b {
        Simple::Type { ns, name } => (ns, Some(name.as_str())),
        Simple::Universal { ns } => (ns, None),
        _ => return None,
    };
    let ns = ns_unify(an, bn)?;
    match (a_name, b_name) {
        (Some(x), Some(y)) if x == y => Some(Simple::Type { ns, name: x.to_string() }),
        (Some(x), None) => Some(Simple::Type { ns, name: x.to_string() }),
        (None, Some(y)) => Some(Simple::Type { ns, name: y.to_string() }),
        (None, None) => Some(Simple::Universal { ns }),
        _ => None,
    }
}

/// Namespace intersection: a wildcard (`*|`) yields to the other side; equal
/// namespaces unify; a bare/empty/explicit mismatch is impossible.
fn ns_unify(a: &Ns, b: &Ns) -> Option<Ns> {
    if *a == Ns::Wild {
        return Some(b.clone());
    }
    if *b == Ns::Wild {
        return Some(a.clone());
    }
    match (a, b) {
        (Ns::Bare, Ns::Bare) => Some(Ns::Bare),
        (Ns::None, Ns::None) => Some(Ns::None),
        (Ns::Named(x), Ns::Named(y)) if x == y => Some(Ns::Named(x.clone())),
        _ => None,
    }
}

fn find_pseudo_element(c: &Compound) -> Option<&Simple> {
    c.simples.iter().find(|s| s.is_pseudo_element())
}

fn pseudo_element_name(s: &Simple) -> Option<&str> {
    match s {
        Simple::PseudoElement { name, .. } => Some(name),
        _ => None,
    }
}

/// Whether both compounds carry the same pseudo-element name — such compounds
/// denote one element and must merge (used by the complex glue).
pub(crate) fn shares_pseudo_element(a: &Compound, b: &Compound) -> bool {
    match (find_pseudo_element(a), find_pseudo_element(b)) {
        (Some(x), Some(y)) => pseudo_element_name(x) == pseudo_element_name(y),
        _ => false,
    }
}

/// Whether a compound carries a root-ish pseudo (`:root`/`:scope`/`:host`…).
pub(crate) fn has_root(c: &Compound) -> bool {
    c.simples.iter().any(|s| {
        matches!(s, Simple::PseudoClass { name, .. }
            if name == "root" || name == "scope" || name == "host" || name == "host-context"
                || name.ends_with("-root") || name.ends_with("-host") || name.ends_with("-host-context"))
    })
}
