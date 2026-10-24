//! Structure parser tests (task 2.2): tokens fold into an owned `Block` tree with head /
//! decls / nested body / document-order key. Assertions check shape, not exact token
//! spacing (the lexer keeps whitespace verbatim inside `Word`).

use std::sync::mpsc;

use rxrust::prelude::*;
use sasspile::lexer::{Tok, token_stream};
use sasspile::structure::Block;

fn toks(src: &str) -> Vec<Tok> {
    let (tx, rx) = mpsc::channel::<Vec<Tok>>();
    token_stream(src.to_string())
        .collect::<Vec<_>>()
        .subscribe(move |v| {
            tx.send(v).ok();
        });
    rx.recv().unwrap()
}

fn head_text(b: &Block) -> String {
    b.head
        .iter()
        .map(|t| match t {
            Tok::Word(w) => w.clone(),
            Tok::Var(n) => format!("${n}"),
            Tok::Punct(c) => c.to_string(),
            Tok::Interp(_) => "#{}".to_string(),
        })
        .collect::<String>()
}

#[test]
fn flat_block_has_head_and_one_decl() {
    let root = Block::parse(&toks("a { b: c; }"));
    assert!(root.decls.is_empty());
    assert_eq!(root.body.len(), 1);
    let blk = &root.body[0];
    assert_eq!(head_text(blk).trim(), "a");
    assert_eq!(blk.decls.len(), 1);
    assert!(blk.body.is_empty());
}

#[test]
fn nesting_produces_child_blocks() {
    let root = Block::parse(&toks("a { b { c: d; } }"));
    let a = &root.body[0];
    assert_eq!(head_text(a).trim(), "a");
    assert_eq!(a.body.len(), 1);
    let b = &a.body[0];
    assert_eq!(head_text(b).trim(), "b");
    assert_eq!(b.decls.len(), 1);
}

#[test]
fn order_key_is_document_order() {
    let root = Block::parse(&toks("a { x: 1; } b { y: 2; } c { z: 3; }"));
    let orders: Vec<u32> = root.body.iter().map(|b| b.order).collect();
    assert!(orders.windows(2).all(|w| w[0] < w[1]), "orders must strictly increase: {orders:?}");
}
