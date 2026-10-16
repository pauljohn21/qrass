//! Token-to-text rendering helpers split out of [`super::parser`] to keep that
//! file within the size budget. Both are pure functions over the token model:
//! [`token_to_pieces`] re-renders a single token into interpolated text pieces
//! (used when a directive/selector run must fall back to its source spelling),
//! and [`punct_text`] is the reverse of the lexer's `LexPunct` table.

use crate::ast::InterpText;
use crate::token::{Piece, Punct, Quote, Token};

/// Render a single token into interpolated pieces.
pub(crate) fn token_to_pieces(t: &Token, out: &mut InterpText) {
    match t {
        Token::Ident(s) => out.push(Piece::Text(s.clone())),
        Token::Variable(v) => out.push(Piece::Text(format!("${v}"))),
        Token::Number { value, unit } => {
            let mut s = crate::css::number_to_string(*value);
            if let Some(u) = unit {
                s.push_str(u);
            }
            out.push(Piece::Text(s));
        }
        Token::HexColor(raw) | Token::Url(raw) => out.push(Piece::Text(raw.clone())),
        Token::Str { text, quote } => {
            let q = match quote {
                Quote::Single => '\'',
                Quote::Double => '"',
            };
            out.push(Piece::Text(format!("{q}{text}{q}")));
        }
        Token::Placeholder(p) => out.push(Piece::Text(format!("%{p}"))),
        Token::Ampersand => out.push(Piece::Text("&".into())),
        Token::AtKeyword(a) => out.push(Piece::Text(format!("@{a}"))),
        Token::Interpolated { pieces, .. } => out.extend(pieces.iter().cloned()),
        Token::Punct(p) => out.push(Piece::Text(punct_text(*p).to_string())),
        Token::Comment(_) => {}
    }
}

/// Reverse of the `LexPunct` table: source text for a punct token.
pub(crate) fn punct_text(p: Punct) -> &'static str {
    use Punct::*;
    match p {
        LBrace => "{",
        RBrace => "}",
        LParen => "(",
        RParen => ")",
        LBracket => "[",
        RBracket => "]",
        Comma => ",",
        Semicolon => ";",
        Colon => ":",
        Dot => ".",
        Ellipsis => "...",
        Plus => "+",
        Minus => "-",
        Star => "*",
        Slash => "/",
        Percent => "%",
        Tilde => "~",
        Gt => ">",
        Lt => "<",
        Ge => ">=",
        Le => "<=",
        Eq => "=",
        Ne => "!=",
        Bang => "!",
        Hash => "#",
        Caret => "^",
        Dollar => "$",
        EqualsEquals => "==",
    }
}
