//! Value — the **owned value** a declaration carries forward through the push stream.
//!
//! P0 lays only the floor: a rendered token run becomes [`Value::Raw`] and
//! [`Value::to_css`] emits it verbatim, so the value path is already shaped as
//! "structured value moves through operators, serialises once at the end" instead of
//! "string travels through the pipeline and gets re-parsed". P1 grows this type into the
//! full algebra (`Number` with the unit-conversion table, `Calculation`, `Color`,
//! `List` with space/comma/slash separators, insertion-ordered `Map`, `String` with quote
//! state, `Bool`/`Null`/`Argument`) per `openspec/changes/.../specs/value-types`, and
//! wires `Expr` + `eval(owned Env)` in front of it. Until then nothing may special-case
//! value text here: evaluation arriving is what turns `Raw` into the other variants.
//!
//! The type stays crate-internal (`mod value` is private in the crate root) — behaviour is
//! observable only through the sole `compile` exit, never by leaking a `pub` for tests.

/// An owned declaration value flowing forward through the pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Source text carried to CSS unchanged (no evaluation wired in yet — see P1).
    Raw(String),
}

impl Value {
    /// Adopt an already-rendered declaration value (the lexer token run is the only
    /// producer at this stage: it canonicalises whitespace, evaluation is not wired in).
    pub fn from_text(text: String) -> Self {
        Value::Raw(text)
    }

    /// Serialise one value to CSS text — the single place a value becomes a string.
    pub fn to_css(&self) -> String {
        match self {
            Value::Raw(text) => text.clone(),
        }
    }
}
