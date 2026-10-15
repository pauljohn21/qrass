//! Proc-macro helpers for qrass. Currently: `#[derive(LexPunct)]`.
//!
//! `LexPunct` reads `#[punct('a')]` / `#[punct('>', '=')]` / `#[punct('.', '.', '.')]`
//! attributes on unit enum variants and generates `Enum::match_at(ch, n1, n2)`
//! — a punctuation dispatch function that greedily prefers the longest match.
//! This eliminates the hand-written punct match table from the lexer.

use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{Data, DeriveInput, Fields, LitChar, parse_macro_input};

/// Derive a `match_at(ch, n1, n2) -> Option<(Self, usize)>` punct dispatcher.
#[proc_macro_derive(LexPunct, attributes(punct))]
pub fn derive_lex_punct(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let enum_name = input.ident.clone();

    let data = match &input.data {
        Data::Enum(e) => e,
        _ => {
            return syn::Error::new_spanned(&input.ident, "LexPunct only supports enums")
                .to_compile_error()
                .into();
        }
    };

    // Collect (char_sequence, variant_ident) from `#[punct(...)]` attributes.
    let mut entries: Vec<(Vec<char>, syn::Ident)> = Vec::new();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return syn::Error::new_spanned(
                &variant.ident,
                "LexPunct variants must be unit (no fields)",
            )
            .to_compile_error()
            .into();
        }
        for attr in &variant.attrs {
            if !attr.path().is_ident("punct") {
                continue;
            }
            let parsed = attr.parse_args_with(Punctuated::<LitChar, Comma>::parse_terminated);
            match parsed {
                Ok(lits) => {
                    let chars: Vec<char> = lits.iter().map(|l| l.value()).collect();
                    if chars.is_empty() || chars.len() > 3 {
                        return syn::Error::new_spanned(
                            attr,
                            "punct sequence must be 1..=3 characters",
                        )
                        .to_compile_error()
                        .into();
                    }
                    entries.push((chars, variant.ident.clone()));
                }
                Err(e) => return e.to_compile_error().into(),
            }
        }
    }

    if entries.is_empty() {
        return syn::Error::new_spanned(&enum_name, "LexPunct: no #[punct(...)] variants found")
            .to_compile_error()
            .into();
    }

    // Group by first character, preserving source order inside each group.
    let mut groups: Vec<(char, Vec<(Vec<char>, syn::Ident)>)> = Vec::new();
    for (seq, var) in entries {
        let first = seq[0];
        if let Some(g) = groups.iter_mut().find(|(c, _)| *c == first) {
            g.1.push((seq, var));
        } else {
            groups.push((first, vec![(seq, var)]));
        }
    }

    // Build one match arm per distinct first character.
    let mut arms = Vec::new();
    for (first, mut seqs) in groups {
        // Longest sequences win: sort by descending length.
        seqs.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
        let mut lookaheads = Vec::new();
        let mut catchall: Option<(syn::Ident, usize)> = None;
        for (seq, var) in seqs {
            let rest = &seq[1..];
            let len = seq.len();
            if rest.is_empty() {
                // Single-char entry — the group's default if no longer match hits.
                catchall = Some((var, len));
                continue;
            }
            let mut cond = quote! {};
            for (idx, &c) in rest.iter().enumerate() {
                let slot = match idx {
                    0 => quote! { n1 },
                    1 => quote! { n2 },
                    _ => unreachable!("validated: rest length <= 2"),
                };
                cond = if idx == 0 {
                    quote! { #slot == Some(#c) }
                } else {
                    quote! { #cond && #slot == Some(#c) }
                };
            }
            lookaheads.push((cond, var, len));
        }

        let mut body = quote! {};
        for (cond, var, len) in lookaheads {
            body = quote! {
                #body
                if #cond { return Some((Self::#var, #len)); }
            };
        }
        let tail = match catchall {
            Some((var, len)) => quote! { Some((Self::#var, #len)) },
            None => quote! { None },
        };
        arms.push(quote! { #first => { #body #tail } });
    }

    let expanded = quote! {
        impl #enum_name {
            /// Match a punctuation token at the cursor, greedily preferring the
            /// longest sequence. Returns `(variant, consumed_char_width)` or `None`.
            pub fn match_at(ch: char, n1: Option<char>, n2: Option<char>) -> Option<(Self, usize)> {
                match ch {
                    #(#arms)*
                    _ => None,
                }
            }
        }
    };

    expanded.into()
}
