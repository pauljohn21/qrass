//! Lexer stage tests: token shapes, interpolation folding, error locations.

use qrass::lexer::lex;
use qrass::token::{Piece, Punct, Quote, Token};

fn toks(src: &str) -> Vec<Token> {
    lex(0, "t.scss".into(), src.into())
        .expect("lex ok")
        .into_iter()
        .map(|s| s.token)
        .collect()
}

fn puncts(src: &str) -> Vec<Token> {
    toks(src)
}

#[test]
fn basic_ruleset() {
    assert_eq!(
        toks("a { color: red; }"),
        vec![
            Token::Ident("a".into()),
            Token::Punct(Punct::LBrace),
            Token::Ident("color".into()),
            Token::Punct(Punct::Colon),
            Token::Ident("red".into()),
            Token::Punct(Punct::Semicolon),
            Token::Punct(Punct::RBrace),
        ]
    );
}

#[test]
fn variable_declaration_with_dimension_math() {
    assert_eq!(
        toks("$v: 1px + 2;"),
        vec![
            Token::Variable("v".into()),
            Token::Punct(Punct::Colon),
            Token::Number {
                value: 1.0,
                unit: Some("px".into())
            },
            Token::Punct(Punct::Plus),
            Token::Number {
                value: 2.0,
                unit: None
            },
            Token::Punct(Punct::Semicolon),
        ]
    );
}

#[test]
fn hex_colors_and_hash_boundary() {
    let t = toks("c: #0d6efd #abc #{$x} 12#;");
    assert_eq!(t[2], Token::HexColor("#0d6efd".into()));
    assert_eq!(t[3], Token::HexColor("#abc".into()));
    // interpolated hash is a word run, not a color
    match &t[4] {
        Token::Interpolated { pieces, .. } => {
            assert_eq!(pieces[0], Piece::Expr("$x".into()));
        }
        other => panic!("expected interpolated, got {other:?}"),
    }
    // bare '#' before punctuation is the hash punct
    assert_eq!(t[6], Token::Punct(Punct::Hash));
}

#[test]
fn ident_interpolation_folded_into_pieces() {
    assert_eq!(
        toks("#{$a}-style"),
        vec![Token::Interpolated {
            quote: None,
            pieces: vec![Piece::Expr("$a".into()), Piece::Text("-style".into())],
        }]
    );
}

#[test]
fn string_interpolation_keeps_quote_info() {
    let t = toks("\"foo#{$x}bar\"");
    assert_eq!(
        t[0],
        Token::Interpolated {
            quote: Some(Quote::Double),
            pieces: vec![
                Piece::Text("foo".into()),
                Piece::Expr("$x".into()),
                Piece::Text("bar".into()),
            ],
        }
    );
}

#[test]
fn plain_strings_and_escapes() {
    let t = toks("'hello' \"a'b\" \"q\\\"z\"");
    assert_eq!(
        t[0],
        Token::Str {
            text: "hello".into(),
            quote: Quote::Single
        }
    );
    assert_eq!(
        t[1],
        Token::Str {
            text: "a'b".into(),
            quote: Quote::Double
        }
    );
    assert_eq!(
        t[2],
        Token::Str {
            text: "q\\\"z".into(),
            quote: Quote::Double
        }
    );
}

#[test]
fn comments_quiet_dropped_block_kept() {
    let t = toks("// gone\n/* kept */ a {}");
    assert_eq!(t[0], Token::Comment("/* kept */".into()));
    assert_eq!(t[1], Token::Ident("a".into()));
}

#[test]
fn url_consumed_whole() {
    let t = toks("background: url(https://cdn.site/a//b.png?v=1);");
    assert_eq!(
        t[2],
        Token::Url("url(https://cdn.site/a//b.png?v=1)".into())
    );
    // quoted url() stays as normal tokens
    let q = toks("url(\"x\")");
    assert_eq!(q[0], Token::Ident("url".into()));
    assert_eq!(q[1], Token::Punct(Punct::LParen));
}

#[test]
fn negative_numbers_are_lexed_as_signs() {
    let t = toks("margin: 0 -10px;");
    assert_eq!(
        t[2],
        Token::Number {
            value: 0.0,
            unit: None
        }
    );
    assert_eq!(
        t[3],
        Token::Number {
            value: -10.0,
            unit: Some("px".into())
        }
    );
}

#[test]
fn selectors_and_combinators() {
    let t = puncts(".a:hover > & %ph ::after");
    assert_eq!(t[0], Token::Ident(".a".into()));
    assert_eq!(t[1], Token::Punct(Punct::Colon));
    assert_eq!(t[2], Token::Ident("hover".into()));
    assert_eq!(t[3], Token::Punct(Punct::Gt));
    assert_eq!(t[4], Token::Ampersand);
    assert_eq!(t[5], Token::Placeholder("ph".into()));
    assert_eq!(t[6], Token::Punct(Punct::Colon));
    assert_eq!(t[7], Token::Punct(Punct::Colon));
}

#[test]
fn at_rules_and_flags() {
    let t = toks("@use \"sass:map\" as m;\n$x: 1 !default;");
    assert_eq!(t[0], Token::AtKeyword("use".into()));
    assert_eq!(
        t[1],
        Token::Str {
            text: "sass:map".into(),
            quote: Quote::Double
        }
    );
    assert_eq!(t[8], Token::Punct(Punct::Bang));
    assert_eq!(t[9], Token::Ident("default".into()));
}

#[test]
fn comparison_operators_and_spread() {
    let t = toks("a>=b c<=d e==f g!=h i...j");
    let kinds: Vec<_> = t
        .iter()
        .filter_map(|tok| match tok {
            Token::Punct(p) => Some(*p),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            Punct::Ge,
            Punct::Le,
            Punct::EqualsEquals,
            Punct::Ne,
            Punct::Ellipsis,
        ]
    );
}

#[test]
fn number_units_and_exponents() {
    let t = toks("1.5rem 50% 1e3 3em .25 10ms");
    assert_eq!(
        t[0],
        Token::Number {
            value: 1.5,
            unit: Some("rem".into())
        }
    );
    assert_eq!(
        t[1],
        Token::Number {
            value: 50.0,
            unit: Some("%".into())
        }
    );
    assert_eq!(
        t[2],
        Token::Number {
            value: 1000.0,
            unit: None
        }
    );
    assert_eq!(
        t[3],
        Token::Number {
            value: 3.0,
            unit: Some("em".into())
        }
    );
    assert_eq!(
        t[4],
        Token::Number {
            value: 0.25,
            unit: None
        }
    );
    assert_eq!(
        t[5],
        Token::Number {
            value: 10.0,
            unit: Some("ms".into())
        }
    );
}

#[test]
fn crlf_line_endings_ok() {
    let t = toks("a {\r\n  color: red;\r\n}\r\n");
    assert_eq!(t[0], Token::Ident("a".into()));
    assert_eq!(t[6], Token::Punct(Punct::RBrace));
}

#[test]
fn nested_interp_braces_in_string_expr() {
    let t = toks("\"a#{ if($x) {1} else {2} }b\"");
    match &t[0] {
        Token::Interpolated { pieces, .. } => {
            assert_eq!(pieces.len(), 3);
            assert_eq!(pieces[1], Piece::Expr(" if($x) {1} else {2} ".into()));
        }
        other => panic!("expected interpolated, got {other:?}"),
    }
}

#[test]
fn error_unterminated_string_reports_line() {
    let err = lex(0, "t.scss".into(), "a\nb: \"x".into()).expect_err("must fail");
    let text = err.to_string();
    let loc = err.located.expect("located");
    assert_eq!((loc.line, loc.col), (2, 4));
    assert!(text.contains("unterminated string"), "{text}");
}

#[test]
fn error_unterminated_comment() {
    let err = lex(0, "t.scss".into(), "/* forever\n".into()).expect_err("must fail");
    assert!(
        err.to_string().contains("unterminated block comment"),
        "{err}"
    );
    assert_eq!(err.located.expect("located").line, 1);
}

#[test]
fn error_unterminated_interpolation() {
    let err = lex(0, "t.scss".into(), "x: #{oops".into()).expect_err("must fail");
    assert!(
        err.to_string().contains("unterminated interpolation"),
        "{err}"
    );
}

#[test]
fn bare_dollar_lexes_to_an_empty_variable_token() {
    // Round-67: a lone `$` no longer fails lexing — it becomes an empty-named
    // Variable token so each parse context can raise dart's own wording
    // (`Expected identifier.` / `expected ":".` / forward member lists).
    let tokens = lex(0, "t.scss".into(), "a: $;".into()).expect("lex ok");
    assert!(
        tokens
            .iter()
            .any(|s| matches!(&s.token, qrass::token::Token::Variable(v) if v.is_empty())),
        "{tokens:?}"
    );
    let err = qrass::parser::parse(0, "t.scss".into(), "a: $;".into(), tokens)
        .expect_err("parse must reject the empty value");
    assert!(err.to_string().contains("expected \":\"."), "{err}");
}

#[test]
fn error_unexpected_character_line_col() {
    let err = lex(0, "t.scss".into(), "a {}\n`bogus".into()).expect_err("must fail");
    let loc = err.located.expect("located");
    assert_eq!((loc.line, loc.col), (2, 1));
}
