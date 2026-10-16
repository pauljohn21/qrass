//! Expression AST nodes produced by the parser (task group 4) and consumed by
//! the evaluator (task group 5). Owned tree — no borrowed references escape a
//! node; the whole expression is moved by value through the reactive pipeline.
//!
//! `Piece`/`Quote` are shared with the lexer's [`crate::token`] so interpolated
//! text (selectors, property names, string/ident `#{}` runs) has one shape.

use crate::span::Span;
use crate::token::{Piece, Quote};

/// A parsed expression node.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A scalar literal (number, string, color, bool, null, bare identifier).
    Literal(Literal),
    /// `$name` variable reference.
    Variable(String, Span),
    /// `ns.$name` — a module-namespaced variable reference (task 6).
    Member {
        namespace: String,
        name: String,
        span: Span,
    },
    /// `ns.fun(args…)` or bare `fun(args…)` call.
    FunctionCall(FunctionCall),
    /// Modern `if(…)` clause call (`sass()`/`css()` conditions per the
    /// `expressions/if/*.hrx` goldens),
    /// parsed whole and evaluated by three-valued symbolic execution.
    IfCall(Box<IfCall>),
    /// Binary operation with precedence resolved by the parser.
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    /// Unary operation (`not`, `-`).
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    /// Parenthesized expression `( … )` — preserved so `/` division context matters.
    Paren(Box<Expr>, Span),
    /// A list literal with a separator (comma / space / slash). `bracketed`
    /// is set for a CSS-`[]` bracketed list literal (`[a b c]`).
    List {
        items: Vec<Expr>,
        sep: ListSep,
        bracketed: bool,
        span: Span,
    },
    /// A map literal `(k: v, …)`.
    Map {
        pairs: Vec<(Expr, Expr)>,
        span: Span,
    },
    /// Text containing `#{…}` interpolation (selector/value/property fragments).
    Interpolated {
        quote: Option<Quote>,
        pieces: Vec<Piece>,
        span: Span,
    },
}

/// Scalar literal leaves.
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    /// Number with an optional unit; `%` counts as a unit.
    Number {
        value: f64,
        unit: Option<String>,
    },
    /// Quoted or unquoted string.
    Str {
        text: String,
        quote: Quote,
    },
    /// Color kept as its raw source text (`#0d6efd`), normalized during eval.
    Color(String),
    /// A CSS color keyword resolved to its hex value; `name` round-trips on
    /// serialization (`red` stays `red`, not `#ff0000`).
    NamedColor { hex: &'static str, name: &'static str },
    /// `url(…)` consumed whole by the lexer.
    Url(String),
    Bool(bool),
    Null,
    /// A bare identifier used as a value (`red`, `solid`, `none`, …).
    Ident(String),
}

/// A function/method call with an optional namespace prefix.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionCall {
    /// `map.get` → `Some("map")`; bare `nth` → `None`.
    pub namespace: Option<String>,
    pub name: String,
    pub args: Vec<Arg>,
    pub span: Span,
}

/// A whole modern `if(…)`: one or more `;`-separated clauses.
#[derive(Clone, Debug, PartialEq)]
pub struct IfCall {
    pub clauses: Vec<IfClause>,
    pub span: Span,
}

/// One `if()` clause: `[cond:] value`; `cond == None` is the `else:` arm.
#[derive(Clone, Debug, PartialEq)]
pub struct IfClause {
    pub cond: Option<IfCond>,
    /// Always a Sass expression — a CSS-mode arm renders its evaluated value.
    pub value: Expr,
}

/// Syntax-level condition term. Kept separate from [`Expr`] because `css()`
/// bodies and bare CSS terms are raw text whose `#{…}` parts alone are
/// evaluated, and the tree is re-rendered (whitespace-normalized) verbatim
/// when the call survives as a CSS `if()` function.
#[derive(Clone, Debug, PartialEq)]
pub enum IfCond {
    /// `sass(expr)` — a fully Sass-evaluable test.
    Sass(Expr),
    /// Raw CSS term text: `css(…)` body, `var(…)`, a bare ident — with the
    /// `#{…}` interpolation split into [`Piece::Expr`] slots.
    Css(Vec<Piece>),
    /// `not <term>`.
    Not(Box<IfCond>),
    /// Terms joined by connectives; `joins[i]` connects `parts[i]`/`parts[i+1]`
    /// (`parts.len() == joins.len() + 1`).
    Chain {
        parts: Vec<IfCond>,
        joins: Vec<Join>,
    },
    /// `( … )` — parens are restored on CSS rendering.
    Paren(Box<IfCond>),
}

/// Connective between [`IfCond::Chain`] parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    And,
    Or,
    /// `css(1) var(--x) css(2)` — space-separated, implicit conjunction.
    Adjacent,
}

/// One call argument: positional, named (`$k: v`), or a rest spread (`$args…`).
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Positional(Expr),
    Named {
        name: String,
        value: Expr,
    },
    /// `getCssVar($args…)` — forward a collected list into a splat parameter.
    Splat(Expr),
}

/// Binary operators. `Div` is true division; `/` in a non-math context is parsed
/// as part of legacy slash lists, upgraded only where the spec implies division.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    /// A lone `=` (IE `alpha(opacity=0)` filters and other plain-CSS function
    /// bodies). Not a SassScript operator: only ever kept verbatim by the
    /// plain-CSS passthrough renderer, never evaluated.
    Assign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    /// `not <expr>`
    Not,
    /// `-<expr>` numeric negation (distinct from a negative literal).
    Neg,
    /// `+<expr>`
    Plus,
}

/// List separator kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListSep {
    Comma,
    Space,
    /// Legacy `a / b` slash-separated list.
    Slash,
}
