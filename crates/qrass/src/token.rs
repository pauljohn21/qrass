//! Token definitions produced by the lexer stage. No behaviour here.

use qrass_macros::LexPunct;

/// Quote style of an unquoted/quoted string token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quote {
    Single,
    Double,
}

/// A segment of an interpolated text run: literal text or a `#{expr}` source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    /// Raw source text between `#{` and the matching `}`; re-lexed by the parser.
    Expr(String),
}

impl Piece {
    pub fn is_expr(&self) -> bool {
        matches!(self, Piece::Expr(_))
    }
}

/// Punctuation / operator tokens (named for readability in the parser).
///
/// `#[derive(LexPunct)]` reads the `#[punct(...)]` attributes below and
/// generates `Punct::match_at(ch, n1, n2)` — the lexer's punct dispatch table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, LexPunct)]
pub enum Punct {
    #[punct('{')]
    LBrace,
    #[punct('}')]
    RBrace,
    #[punct('(')]
    LParen,
    #[punct(')')]
    RParen,
    #[punct('[')]
    LBracket,
    #[punct(']')]
    RBracket,
    #[punct(',')]
    Comma,
    #[punct(';')]
    Semicolon,
    #[punct(':')]
    Colon,
    #[punct('.')]
    Dot,
    #[punct('.', '.', '.')]
    Ellipsis,
    #[punct('+')]
    Plus,
    #[punct('-')]
    Minus,
    #[punct('*')]
    Star,
    #[punct('/')]
    Slash,
    #[punct('%')]
    Percent,
    #[punct('~')]
    Tilde,
    #[punct('>')]
    Gt,
    #[punct('<')]
    Lt,
    #[punct('>', '=')]
    Ge,
    #[punct('<', '=')]
    Le,
    #[punct('=')]
    Eq,
    #[punct('!', '=')]
    Ne,
    #[punct('!')]
    Bang,
    #[punct('#')]
    Hash,
    #[punct('^')]
    Caret,
    #[punct('$')]
    Dollar,
    #[punct('=', '=')]
    EqualsEquals,
}

/// One lexed SCSS token.
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    /// Identifier (may start with `.` or `-`, may embed `--`), no interpolation.
    Ident(String),
    /// `$name` without the `$`.
    Variable(String),
    /// Number with an optional unit (`%` counts as a unit).
    Number {
        value: f64,
        unit: Option<String>,
    },
    /// `#rrggbb` / `#rgb` (raw text, normalized later).
    HexColor(String),
    /// Plain string literal.
    Str {
        text: String,
        quote: Quote,
    },
    /// Text run that contains `#{...}` interpolation (optionally quoted).
    Interpolated {
        quote: Option<Quote>,
        pieces: Vec<Piece>,
    },
    /// `@name` without the `@`.
    AtKeyword(String),
    /// `/* ... */` comment (raw text, `//` comments are dropped).
    Comment(String),
    /// `url(...)` consumed whole so that `//` and `;` inside stay literal.
    Url(String),
    /// `%placeholder` selector name without the `%`.
    Placeholder(String),
    /// `&` parent selector reference.
    Ampersand,
    Punct(Punct),
}

/// Token plus its source span.
#[derive(Clone, Debug, PartialEq)]
pub struct Spanned {
    pub token: Token,
    pub span: crate::span::Span,
}
