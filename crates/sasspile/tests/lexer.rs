//! Lexer tokenisation tests: structured tokens in ONE pass, spacing kept verbatim
//! inside `Word` (it is data, trimmed later by the value/selector renderer).

use std::sync::mpsc;

use rxrust::prelude::*;
use sasspile::lexer::{Tok, token_stream};

fn toks(src: &str) -> Vec<Tok> {
    let (tx, rx) = mpsc::channel::<Vec<Tok>>();
    token_stream(src.to_string())
        .collect::<Vec<_>>()
        .subscribe(move |v| {
            tx.send(v).ok();
        });
    rx.recv().unwrap()
}

#[test]
fn rule_with_interp_and_var() {
    assert_eq!(
        toks("a { color: #{$x}; }"),
        vec![
            Tok::Word("a ".into()),
            Tok::Punct('{'),
            Tok::Word(" color".into()),
            Tok::Punct(':'),
            Tok::Word(" ".into()),
            Tok::Interp(vec![Tok::Var("x".into())]),
            Tok::Punct(';'),
            Tok::Word(" ".into()),
            Tok::Punct('}'),
        ]
    );
}

#[test]
fn hex_is_verbatim_text_not_interp() {
    let t = toks("a { c: #fff; }");
    // no interpolation for a bare `#`
    assert!(t.iter().all(|x| !matches!(x, Tok::Interp(_))));
    // `#fff` lives inside a verbatim Word
    assert!(t
        .iter()
        .any(|x| matches!(x, Tok::Word(w) if w.contains("#fff"))));
}

#[test]
fn block_comments_stripped() {
    let t = toks("a { /* hi */ b: c; }");
    assert!(
        !t.iter().any(|x| matches!(x, Tok::Word(w) if w.contains("hi"))),
        "comment text leaked: {t:?}"
    );
    assert_eq!(
        t,
        vec![
            Tok::Word("a ".into()),
            Tok::Punct('{'),
            Tok::Word("  b".into()),
            Tok::Punct(':'),
            Tok::Word(" c".into()),
            Tok::Punct(';'),
            Tok::Word(" ".into()),
            Tok::Punct('}'),
        ]
    );
}
