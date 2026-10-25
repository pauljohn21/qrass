//! Selector / PropName — a block head and a property name as **structured values**.
//!
//! The shape this replaces rendered a head to a `String` and produced the nested selector with
//! `child.replace('&', parent)`. That makes the parent reference a *character inside an opaque string*,
//! so every question about it — how many `&` there are, whether one is glued to an ident, whether it
//! came from `#{&}` — had to be answered by scanning text. Here `&` is a `Piece::Parent` built from the
//! lexeme the lexer emits ([`crate::lexer::STRUCTURAL`]) and nesting splices pieces: selectors and
//! property names are structured values, and combining `&` is not a string replacement.
//!
//! This models exactly what `Exec` needs to place a rule in the document, in order. P2's full algebra
//! (compound/combinator/pseudo-class parsing, `unify`, superselector) grows here without a re-parse.

use crate::lexer::canon;

/// One piece of a selector head, in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// resolved text of a run of lexemes; never contains a parent reference
    Text(String),
    /// the parent selector, from the `&` **lexeme** (`#{&}` is different: the corpus substitutes the
    /// parent as text there and still nests, so it arrives as a `Text` piece)
    Parent,
}

/// A selector as its pieces. Deliberately not a parsed tree: `Exec` only needs to know where the parent
/// reference sits and how the whole thing prints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selector {
    pieces: Vec<Piece>,
}

impl Selector {
    pub fn from_pieces(pieces: Vec<Piece>) -> Self {
        Self { pieces }
    }

    /// The pieces back out, for the caller that builds a name from them rather than a selector.
    pub fn into_pieces(self) -> Vec<Piece> {
        self.pieces
    }

    /// `a & b`, `&:hover`, `&--x`: any parent reference at all means the child *replaces* it rather
    /// than being appended as a descendant.
    pub fn references_parent(&self) -> bool {
        self.pieces.contains(&Piece::Parent)
    }

    /// Nest this child inside `parent` — the structural form of Sass's two rules: a child holding `&`
    /// substitutes the parent at each occurrence, otherwise it is a descendant selector.
    ///
    /// A `flat_map` over owned pieces: no string is built and re-searched, and a parent that is itself
    /// nested has already been resolved, so its pieces carry no `Parent`. An empty parent (a root-level
    /// `&`) leaves the reference unresolved, which [`Selector::to_css`] prints as `&` — the form
    /// `css/selector/parent::alone/first` expects.
    pub fn nest(&self, parent: &Self) -> Self {
        if parent.pieces.is_empty() {
            return self.clone();
        }
        if self.references_parent() {
            return Self::from_pieces(
                self.pieces
                    .iter()
                    .flat_map(|piece| match piece {
                        Piece::Parent => parent.pieces.clone(),
                        text => vec![text.clone()],
                    })
                    .collect(),
            );
        }
        Self::from_pieces(
            parent
                .pieces
                .iter()
                .cloned()
                .chain([Piece::Text(" ".to_string())])
                .chain(self.pieces.iter().cloned())
                .collect(),
        )
    }

    /// Whether the head carries nothing a rule could be printed under (the root scope's empty selector).
    pub fn is_empty(&self) -> bool {
        self.to_css().is_empty()
    }

    /// The one place a selector becomes text; [`canon`] keeps whitespace runs collapsed exactly as the
    /// rendered-string form did, so nesting output is unchanged by this restructuring. Boundary
    /// whitespace is already gone (the head trimmed it at the piece level), which is what makes
    /// `a { &.b {} }` print `a.b` rather than `a .b`.
    pub fn to_css(&self) -> String {
        canon(
            &self
                .pieces
                .iter()
                .map(|piece| match piece {
                    Piece::Text(text) => text.clone(),
                    // only reachable for a `&` with no parent, i.e. a root-level one
                    Piece::Parent => "&".to_string(),
                })
                .collect::<String>(),
        )
    }

    /// A head whose edges carry nothing but whitespace: the pieces are the selector, and the padding
    /// around it belongs to the source layout, not to the name. Trimming here (rather than after
    /// splicing) is what lets `a` and `&.b` join into `a.b`.
    pub fn trimmed(self) -> Self {
        let blank = |piece: &Piece| match piece {
            Piece::Text(text) => text.chars().all(char::is_whitespace),
            Piece::Parent => false,
        };
        let end = self
            .pieces
            .iter()
            .rposition(|p| !blank(p))
            .map_or(0, |i| i + 1);
        let start = self.pieces.iter().position(|p| !blank(p)).unwrap_or(end);
        let mut pieces = self.pieces;
        let kept = pieces.drain(start..end).collect();
        Self { pieces: kept }
    }
}

/// A property name, with the one semantic decision a declaration needs made **once, at construction**:
/// whether it is a custom property (`--x`), whose value is raw CSS text rather than SassScript.
///
/// Callers ask [`PropName::is_custom`] instead of re-reading the name's shape, so the rule lives at the
/// boundary where the structured pieces turn into a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropName {
    text: String,
    custom: bool,
}

impl PropName {
    /// Build a name from the pieces of a declaration's left side. The `--` test runs on the canonical
    /// text because leading whitespace is part of the *token run*, not of the name (`  --x: y`).
    pub fn from_pieces(pieces: &[Piece]) -> Self {
        let text = Selector::from_pieces(pieces.to_vec()).to_css();
        let custom = text.starts_with("--");
        Self { text, custom }
    }

    pub fn is_custom(&self) -> bool {
        self.custom
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}
