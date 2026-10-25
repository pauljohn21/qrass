//! The structured head (tasks 0′.7): a selector is pieces, and `&` is one of them — never a character
//! searched inside a rendered string. Every expectation here is a `sass-spec` fixture, cited per test.

use sasspile::compile;

/// `css/selector/parent.hrx`: the three ways a `&` sits against its parent — alone at the root, glued to
/// a compound (`&.b`), glued to a suffix (`&b`) — plus the descendant case with no `&` at all.
#[tokio::test]
async fn parent_reference_is_spliced_from_pieces() {
    // `alone/first`: a root-level `&` has no parent to substitute, and the corpus still prints it.
    assert_eq!(
        compile("& {a: b}").await.expect("root &"),
        "& {\n  a: b;\n}\n"
    );
    // `compound`: joined with no space — which is what trimming the head's *edge* whitespace buys, since
    // `a` and `&.b` are spliced at the piece level rather than canon'd after concatenation.
    assert_eq!(
        compile("a {\n  &.b {c: d}\n}").await.expect("compound"),
        "a.b {\n  c: d;\n}\n"
    );
    // `suffix`
    assert_eq!(
        compile("a {\n  &b {c: d}\n}").await.expect("suffix"),
        "ab {\n  c: d;\n}\n"
    );
    // descendant: no `&`, so the parent is prepended with one space
    assert_eq!(
        compile("a {\n  .b {c: d}\n}").await.expect("descendant"),
        "a .b {\n  c: d;\n}\n"
    );
}

/// `css/selector/combinator/has::leading/single/parent` and `css/selector/placeholder::pseudoselectors/
/// where/nesting`: a `&` nested inside a pseudo-class functional argument splices in place, and the
/// whitespace around it belongs to the compound, not to the join.
#[tokio::test]
async fn parent_reference_inside_a_pseudo_class_splices_in_place() {
    assert_eq!(
        compile("a {\n  :has(~ &) {b: c}\n}")
            .await
            .expect(":has(~ &)"),
        ":has(~ a) {\n  b: c;\n}\n"
    );
    assert_eq!(
        compile("a {\n  :where(&) {\n    b: c;\n  }\n}")
            .await
            .expect(":where(&)"),
        ":where(a) {\n  b: c;\n}\n"
    );
}

/// `libsass-closed-issues/issue_1043`: an interpolation whose whole content is `&` *is* the parent —
/// `.selector#{&}` inside `.test` prints `.test .selector.test`, not text that happens to hold `&`.
#[tokio::test]
async fn interpolation_of_only_the_parent_is_the_parent() {
    assert_eq!(
        compile(".test {\n  .selector#{&} {\n    color: blue;\n  }\n}")
            .await
            .expect("#{&}"),
        ".test .selector.test {\n  color: blue;\n}\n"
    );
}

/// The lexeme-level consequence of 0′.7 that a string form could not express: the newline that ends a
/// silent comment is whitespace, so the `$name` of the binding on the next line stays findable. Emitted
/// as a `Word(" ")` before, it hid `$ids` behind a fake identifier and the reference below failed.
#[tokio::test]
async fn a_comment_does_not_disguise_the_next_binding() {
    assert_eq!(
        compile("// value.\n$ids: ();\na { b: $ids }")
            .await
            .expect("binding after a comment"),
        "a {\n  b: ();\n}\n"
    );
}
