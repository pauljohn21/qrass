//! Statement AST nodes produced by the parser (task group 3) and consumed by
//! the evaluator (task groups 5–7). An owned tree: nested bodies are `Vec<Stmt>`
//! moved by value, matching the design's "栈式累加器折叠出 owned AST" model.
//!
//! Selector / property-name / at-rule-parameter text that may embed `#{…}` is
//! represented as `Vec<Piece>` (interleaved literal text and raw expression
//! source), shared with the lexer's interpolation shape.

use crate::ast_expr::{Arg, Expr};
use crate::span::Span;
use crate::token::Piece;

/// Interpolated static text: literal segments alternating with `#{expr}` source.
pub type InterpText = Vec<Piece>;

/// A SCSS statement — the top-level and block-body unit.
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// Style rule: selectors + nested body.
    Ruleset(Ruleset),
    /// Property declaration `prop: value [!important]`.
    Declaration(Declaration),
    /// Variable assignment `$v: expr [!default] [!global]`.
    Variable(VariableDecl),
    /// `@each $a, $b in list { … }`.
    Each(Each),
    /// `@for $i from a through|to b { … }`.
    For(For),
    /// `@if`/`@else if`/`@else` chain.
    If(If),
    /// `@while cond { … }`.
    While(While),
    /// `@mixin name(params) { … }`.
    Mixin(MixinDecl),
    /// `@include name(args) using (…)? { content-body }`.
    Include(Include),
    /// `@function name(params) { … }`.
    Function(FunctionDecl),
    /// `@return expr?`.
    Return(Return),
    /// `@content(args?)` invocation inside a mixin body.
    Content(Content),
    /// `@use "url" [as ns | as *] [with (…)]`.
    Use(Use),
    /// `@forward "url" [as prefix*] [show/hide …] [with (…)]`.
    Forward(Forward),
    /// `@at-root (with|without: …)? { … }` or `@at-root <selector> { … }`.
    AtRoot(AtRoot),
    /// `@media <queries> { … }`.
    Media(Media),
    /// `@supports <condition> { … }`.
    Supports(Supports),
    /// `@keyframes <name> { … }`.
    Keyframes(Keyframes),
    /// `@extend <selector> [!optional]`.
    Extend(Extend),
    /// `@warn`/`@error`/`@debug` message node.
    Diagnostic(Diagnostic),
    /// `/* … */` comment preserved into output.
    Comment(Comment),
    /// Fallback for any other `@name value;` or `@name { … }` at-rule.
    AtRule(AtRule),
}

/// Block of statements (a rule body, control-flow branch, etc.).
pub type Block = Vec<Stmt>;

#[derive(Clone, Debug, PartialEq)]
pub struct Ruleset {
    pub selectors: InterpText,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Declaration {
    /// Property name, possibly interpolated (`#{$prop}-style`).
    pub prop: InterpText,
    /// Declaration value expression (often a list or interpolated string).
    pub value: Expr,
    pub important: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariableDecl {
    pub name: String,
    pub value: Expr,
    pub default: bool,
    pub global: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Each {
    /// One or more iteration variables (`@each $k, $v in …`).
    pub vars: Vec<String>,
    pub list: Expr,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct For {
    pub var: String,
    pub from: Expr,
    pub to: Expr,
    /// `through` (inclusive) vs `to` (exclusive).
    pub inclusive: bool,
    pub body: Block,
    pub span: Span,
}

/// One `@if`/`@else if` branch; the final `@else` has `cond == None`.
#[derive(Clone, Debug, PartialEq)]
pub struct IfClause {
    pub cond: Option<Expr>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct If {
    pub clauses: Vec<IfClause>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct While {
    pub cond: Expr,
    pub body: Block,
    pub span: Span,
}

/// A mixin/function parameter: `$name` with optional default and rest flag.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub default: Option<Expr>,
    /// `@mixin m($args…)` — collects remaining positional args.
    pub rest: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MixinDecl {
    pub name: InterpText,
    pub params: Vec<Param>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Include {
    pub name: InterpText,
    pub args: Vec<Arg>,
    /// `@include m(...) using ($a, $b) { … }` — bound content parameters.
    pub content_params: Vec<String>,
    /// The `@content` block, if the include passes one.
    pub content: Option<Block>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Return {
    pub expr: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Content {
    pub args: Vec<Arg>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Use {
    pub url: String,
    /// `as <name>`; `Some(None)` means `as *`; `None` = default (path basename).
    pub alias: UseAlias,
    /// `with ($k: v, …)` configuration.
    pub config: Vec<(String, Expr)>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UseAlias {
    /// Default namespace from the URL basename.
    Default,
    /// `as *` — merge into the importing scope.
    Wildcard,
    /// `as <name>`.
    Named(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Forward {
    pub url: String,
    /// `as <prefix>*` renaming prefix (empty when absent).
    pub prefix: String,
    /// `show` list (names) — mutually exclusive with `hide`.
    pub shown: Vec<String>,
    pub hidden: Vec<String>,
    pub config: Vec<(String, Expr)>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AtRoot {
    /// `(with|without: <directive list>)` header, if present.
    pub header: Option<(String, Vec<String>)>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub queries: InterpText,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Supports {
    pub condition: InterpText,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Keyframes {
    /// `@keyframes <name>` — the (possibly interpolated) name text.
    pub name: InterpText,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Extend {
    pub selector: InterpText,
    pub optional: bool,
    pub span: Span,
}

/// `@warn`/`@error`/`@debug`.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub level: DiagLevel,
    pub message: Expr,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagLevel {
    Warn,
    Error,
    Debug,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    /// Raw `/* … */` text including the delimiters.
    pub text: String,
    pub span: Span,
}

/// Fallback generic at-rule: `@<name> <value>;` or `@<name> <value> { … }`.
#[derive(Clone, Debug, PartialEq)]
pub struct AtRule {
    /// The at-keyword without the leading `@` (e.g. `charset`, `-ms-viewport`).
    pub name: String,
    pub value: InterpText,
    pub body: Option<Block>,
    pub span: Span,
}
