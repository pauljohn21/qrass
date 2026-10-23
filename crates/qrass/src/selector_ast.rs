//! Selector AST (Round 23 Phase 1): an owned, canonical model of a CSS/SCSS
//! selector, shared by the parser ([`crate::selector_parse`]) and the selector
//! builtins (`is-superselector`, `unify`, `extend`).
//!
//! Ownership-first: every node owns its text (no borrowed spans, no `Rc`). The
//! shapes mirror the sass selector grammar closely enough that a parsed selector
//! re-renders (`Display`) to the exact canonical text the `.hrx` goldens expect,
//! so a round-trip (`parse -> render`) is itself the correctness check.
//!
//! Canonical rendering rules (pinned by the is_superselector/unify/extend
//! goldens):
//! - a selector list joins complexes with `", "`;
//! - combinators render as `" "` (descendant), `" > "`, `" + "`, `" ~ "`;
//! - an attribute selector keeps its source form verbatim (`[c=d]` stays `[c=d]`,
//!   no added quotes);
//! - a functional pseudo-class renders `:name(arg)`; the `of` form of `nth-*`
//!   renders `:nth-child(<an+b> of <list>)`;
//! - legacy single-colon pseudo-elements keep the single colon as written.

use std::fmt;

/// A comma-separated list of complex selectors — the top-level unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorList {
    pub complexes: Vec<Complex>,
}

impl SelectorList {
    pub fn new(complexes: Vec<Complex>) -> Self {
        Self { complexes }
    }

    pub fn is_empty(&self) -> bool {
        self.complexes.is_empty()
    }

    /// Every nested selector list this tree carries, in document order — the
    /// `:is(...)`/`:where(...)`/`:has(...)`/`nth-* of ...` arguments. Used by
    /// `is-superselector` to recurse into functional pseudo-class arguments.
    pub fn selector_lists(&self) -> Vec<&SelectorList> {
        self.complexes
            .iter()
            .flat_map(|complex| complex.components.iter())
            .flat_map(|component| component.compound.simples.iter())
            .flat_map(|simple| simple.collect_selector_lists())
            .collect()
    }
}

impl fmt::Display for SelectorList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // comma-joined rendering in one write (an empty list is the empty text)
        let text = self
            .complexes
            .iter()
            .map(|complex| complex.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        f.write_str(&text)
    }
}

/// One complex selector: a chain of compounds joined by combinators. The first
/// component carries no combinator; later ones record how they attach to the
/// previous compound. `trailing` holds bogus combinators dangling after the last
/// compound (e.g. `.c +`, or the bare `+` of a combinator-only selector — for
/// which `components` is empty); they are re-rendered verbatim and, per the
/// `extend/complex/**` goldens, take part in extension only as a single token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Complex {
    pub components: Vec<Component>,
    pub trailing: Vec<Combinator>,
}

/// A compound plus the combinator that attaches it to its predecessor. `extras`
/// records any *additional* combinators following `combinator` in a bogus chain
/// (`.c ~ ~ .d` keeps `combinator = ~` and `extras = [~]`); it is empty for every
/// valid selector, so structural logic reads only `combinator`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    pub combinator: Option<Combinator>,
    pub extras: Vec<Combinator>,
    pub compound: Compound,
}

/// The relation between two adjacent compounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    /// A space: ancestor relationship.
    Descendant,
    /// `>`: strict parent.
    Child,
    /// `+`: immediately-preceding sibling.
    NextSibling,
    /// `~`: any preceding sibling.
    Sibling,
}

impl Combinator {
    /// Canonical separator text, including surrounding spaces where the grammar
    /// uses them.
    pub fn as_str(self) -> &'static str {
        match self {
            Combinator::Descendant => " ",
            Combinator::Child => " > ",
            Combinator::NextSibling => " + ",
            Combinator::Sibling => " ~ ",
        }
    }

    /// A combinator pinned to the *front* of a selector (`> .c`, kept after a
    /// bogus-initial-combinator unify). Renders the marker plus one trailing
    /// space, no leading space. A leading descendant is meaningless, so it (and
    /// `None`) renders as nothing.
    pub fn as_leading_str(self) -> &'static str {
        match self {
            Combinator::Child => "> ",
            Combinator::NextSibling => "+ ",
            Combinator::Sibling => "~ ",
            Combinator::Descendant => "",
        }
    }
}

impl Complex {
    pub fn new(components: Vec<Component>) -> Self {
        Self {
            components,
            trailing: Vec::new(),
        }
    }

    /// A single compound with no combinator.
    pub fn from_compound(compound: Compound) -> Self {
        Self {
            components: vec![Component {
                combinator: None,
                extras: Vec::new(),
                compound,
            }],
            trailing: Vec::new(),
        }
    }

    fn fmt_render(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // each component renders as `edge + compound`, where the edge is the
        // primary combinator plus any bogus extras; a leading edge uses the
        // front-pinned form (`> .c`), the rest the spaced form (`.c ~ ~ .d`)
        let chain = self
            .components
            .iter()
            .enumerate()
            .map(|(i, component)| {
                let render: fn(Combinator) -> &'static str = match i {
                    0 => Combinator::as_leading_str,
                    _ => Combinator::as_str,
                };
                let edge = match (i, component.combinator) {
                    (0, None) => String::new(),
                    (_, Some(combinator)) => render(combinator).to_string(),
                    (_, None) => " ".to_string(),
                };
                let extras: String = component.extras.iter().map(|c| render(*c)).collect();
                edge + &extras + &component.compound.to_string()
            })
            .collect::<String>();
        // Dangling combinators after the last compound (`.c +`, or a lone `+`).
        // Each keeps one leading space but no trailing space, so a following comma
        // in the list stays tight (`.c +, .d +`).
        let dangling = self
            .trailing
            .iter()
            .enumerate()
            .map(|(j, combinator)| {
                let sep = if j > 0 || !self.components.is_empty() {
                    " "
                } else {
                    ""
                };
                format!("{sep}{}", combinator.as_str().trim())
            })
            .collect::<String>();
        f.write_str(&(chain + &dangling))
    }
}

impl fmt::Display for Complex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_render(f)
    }
}

/// A compound selector: a sequence of simple selectors with no combinator
/// between them (e.g. `a.b#c[x]:hover::before`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Compound {
    pub simples: Vec<Simple>,
}

impl Compound {
    pub fn new(simples: Vec<Simple>) -> Self {
        Self { simples }
    }

    fn fmt_render(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // sequential writes; the first error short-circuits the fold
        self.simples
            .iter()
            .try_fold((), |(), simple| simple.fmt_render(f))
    }

    /// The type/element universal, if the compound opens with one — the "base"
    /// other compounds are unified against.
    pub fn type_simple(&self) -> Option<&Simple> {
        self.simples.iter().find(|s| s.is_type_like())
    }

    /// Whether the compound contains any pseudo-element.
    pub fn has_pseudo_element(&self) -> bool {
        self.simples.iter().any(Simple::is_pseudo_element)
    }

    /// Split the compound at its (first) pseudo-element into the three ordered
    /// segments `::el` requires: everything before, the pseudo-element itself,
    /// and everything after. Order is significant — `:e::d` is *not* the
    /// superselector of `::d:e` — so the golden for fake-pseudo-element-order
    /// depends on keeping the three parts distinct rather than sorting them.
    ///
    /// Returns `(before, element, after)`; `element` is `None` when the compound
    /// has no pseudo-element, in which case `before` is the whole compound and
    /// `after` is empty.
    pub fn split_at_element(&self) -> (&[Simple], Option<&Simple>, &[Simple]) {
        match self.simples.iter().position(Simple::is_pseudo_element) {
            Some(i) => (
                &self.simples[..i],
                Some(&self.simples[i]),
                &self.simples[i + 1..],
            ),
            None => (&self.simples[..], None, &[]),
        }
    }
}

impl fmt::Display for Compound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_render(f)
    }
}

/// The namespace qualifier on a type or universal selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ns {
    /// No bar at all (`a`) — inherits the default namespace.
    Bare,
    /// An empty namespace (`|a`) — explicitly no namespace.
    None,
    /// A named prefix (`ns|a`).
    Named(String),
    /// A wildcard prefix (`*|a`).
    Wild,
}

impl Ns {
    /// Renders the prefix plus its trailing bar, or nothing for `Bare`.
    fn fmt_render(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ns::Bare => Ok(()),
            Ns::None => f.write_str("|"),
            Ns::Named(name) => write!(f, "{name}|"),
            Ns::Wild => f.write_str("*|"),
        }
    }
}

/// The operator of an attribute selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrOp {
    Equals,
    Includes,
    DashMatch,
    Prefix,
    Suffix,
    Substring,
}

impl AttrOp {
    pub fn as_str(self) -> &'static str {
        match self {
            AttrOp::Equals => "=",
            AttrOp::Includes => "~=",
            AttrOp::DashMatch => "|=",
            AttrOp::Prefix => "^=",
            AttrOp::Suffix => "$=",
            AttrOp::Substring => "*=",
        }
    }
}

/// An attribute selector, kept as its verbatim source text alongside the
/// structured pieces needed for `[c=d]`-style subsumption tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    /// The whole `[...]` group exactly as written (canonical re-render source).
    pub raw: String,
    pub name: String,
    pub op: Option<AttrOp>,
    pub value: Option<String>,
    /// Case-insensitivity flag (`i`) or other trailing modifier.
    pub modifier: Option<char>,
}

/// A functional pseudo-class argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PseudoArg {
    /// A bare (non-functional) pseudo-class with no argument.
    None,
    /// A raw text argument that is not a selector list (e.g. `:lang(en)`,
    /// the `<an+b>` of `:nth-child`).
    Text(String),
    /// A selector-list argument (`:is(...)`, `:where(...)`, `:has(...)`, ...).
    Selectors(SelectorList),
    /// A `nth-*` argument: `<an+b>`, optionally `of <selector list>`.
    Nth {
        anbn: String,
        of: Option<SelectorList>,
    },
}

/// A single simple selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Simple {
    /// `*` or a namespace-only `*|*` / `|*`.
    Universal { ns: Ns },
    /// An element/type name, possibly namespace-qualified.
    Type { ns: Ns, name: String },
    /// `.class`.
    Class(String),
    /// `#id`.
    Id(String),
    /// `%placeholder`.
    Placeholder(String),
    /// `[attribute]`.
    Attribute(Attribute),
    /// `:pseudo-class` (single colon), possibly functional.
    PseudoClass {
        name: String,
        arg: PseudoArg,
    },
    /// `::pseudo-element` (double colon) or a legacy single-colone form kept as
    /// written via `legacy_colon`.
    PseudoElement {
        name: String,
        arg: PseudoArg,
        legacy_colon: bool,
    },
}

impl Simple {
    /// Whether this is a pseudo-element (`::x`, or a legacy single-colon element
    /// the parser flagged as such).
    pub fn is_pseudo_element(&self) -> bool {
        matches!(self, Simple::PseudoElement { .. })
    }

    /// Whether this behaves as the type/element base of a compound: a type name,
    /// a universal, or a namespace-only selector.
    pub fn is_type_like(&self) -> bool {
        matches!(self, Simple::Type { .. } | Simple::Universal { .. })
    }

    /// Recurse into selector-list arguments to collect nested lists, in document
    /// order (own arguments first, then the nested ones).
    fn collect_selector_lists(&self) -> Vec<&SelectorList> {
        let arg = match self {
            Simple::PseudoClass { arg, .. } | Simple::PseudoElement { arg, .. } => Some(arg),
            _ => None,
        };
        let own: Vec<&SelectorList> = match arg {
            Some(PseudoArg::Selectors(list)) => vec![list],
            Some(PseudoArg::Nth {
                of: Some(list), ..
            }) => vec![list],
            _ => Vec::new(),
        };
        let nested: Vec<&SelectorList> = arg
            .into_iter()
            .flat_map(|a| {
                a.nested_lists()
                    .into_iter()
                    .flat_map(|list| list.selector_lists())
            })
            .collect();
        own.into_iter().chain(nested).collect()
    }

    fn fmt_render(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Simple::Universal { ns } => {
                ns.fmt_render(f)?;
                f.write_str("*")
            }
            Simple::Type { ns, name } => {
                ns.fmt_render(f)?;
                f.write_str(name)
            }
            Simple::Class(name) => write!(f, ".{name}"),
            Simple::Id(name) => write!(f, "#{name}"),
            Simple::Placeholder(name) => write!(f, "%{name}"),
            Simple::Attribute(attr) => write!(f, "{}", attr.raw),
            Simple::PseudoClass { name, arg } => {
                f.write_str(":")?;
                f.write_str(name)?;
                fmt_arg(f, name, arg)
            }
            Simple::PseudoElement {
                name,
                arg,
                legacy_colon,
            } => {
                if *legacy_colon {
                    f.write_str(":")?;
                } else {
                    f.write_str("::")?;
                }
                f.write_str(name)?;
                fmt_arg(f, name, arg)
            }
        }
    }
}

impl PseudoArg {
    /// Nested selector lists carried by this argument, for recursive visiting.
    fn nested_lists(&self) -> Vec<&SelectorList> {
        match self {
            PseudoArg::Selectors(list) => vec![list],
            PseudoArg::Nth { of: Some(list), .. } => vec![list],
            _ => Vec::new(),
        }
    }
}

/// Renders a functional argument, applying the `of` spelling for `nth-*`.
fn fmt_arg(f: &mut fmt::Formatter<'_>, name: &str, arg: &PseudoArg) -> fmt::Result {
    match arg {
        PseudoArg::None => Ok(()),
        PseudoArg::Text(text) => write!(f, "({text})"),
        PseudoArg::Selectors(list) => write!(f, "({list})"),
        PseudoArg::Nth { anbn, of } => {
            let _ = name;
            match of {
                Some(list) => write!(f, "({anbn} of {list})"),
                None => write!(f, "({anbn})"),
            }
        }
    }
}
