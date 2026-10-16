//! `.hrx` parser verification (task 11.1): the parser must recover the exact
//! input/output/error member set from both a hand-written document and a real
//! file checked out under `sass-spec/`.

mod common;

use common::hrx;

/// A three-case document exercising every member the parser classifies: a plain
/// output case, a multi-file (`_partial.scss`) case, and an `error` case.
fn sample() -> String {
    let sep = hrx::SECTION_SEPARATOR;
    let mut s = String::new();
    s.push_str("<===> input.scss\na {b: c}\n\n<===> output.css\na {\n  b: c;\n}\n\n");
    s.push_str(sep);
    s.push_str("<===> sub/input.scss\n@use 'p';\na {b: p.$x}\n\n<===> sub/_p.scss\n$x: 1;\n\n<===> sub/output.css\na {\n  b: 1;\n}\n\n");
    s.push_str(sep);
    s.push_str("<===> bad/input.scss\na {b: 1 +}\n\n<===> bad/error\nError: expected number.\n");
    s
}

#[test]
fn parses_sections_and_members() {
    let cases = hrx::parse(&sample());
    assert_eq!(cases.len(), 3, "three sections");

    // Case 1: root input + output, no directory prefix.
    let c0 = &cases[0];
    assert_eq!(c0.dir, "");
    assert!(c0.input.is_some());
    assert!(!c0.indented);
    assert_eq!(c0.output.as_deref().unwrap().trim(), "a {\n  b: c;\n}");
    assert!(c0.error.is_none());

    // Case 2: a `sub/` directory with a sibling partial materialized beside it.
    let c1 = &cases[1];
    assert_eq!(c1.dir, "sub");
    assert!(c1.files.iter().any(|(p, _)| p == "sub/_p.scss"));
    assert!(c1.output.as_deref().unwrap().contains("b: 1;"));

    // Case 3: an error-expectation case with no output.
    let c2 = &cases[2];
    assert_eq!(c2.dir, "bad");
    assert!(c2.output.is_none());
    assert!(c2.error.as_deref().unwrap().contains("expected number"));
}

#[test]
fn inherited_fixtures_accumulate_across_sections() {
    // sass-spec hrx semantics: a fixture-only leading section (no input) seeds
    // shared source files for every later case in the same document, exactly
    // like `callable/arguments.hrx` whose `_utils.scss` is `@use`d by all cases.
    let sep = hrx::SECTION_SEPARATOR;
    let mut s = String::new();
    s.push_str("<===> mixin/_utils.scss\n@mixin a($args...) { b {c: 1} }\n\n");
    s.push_str(sep);
    s.push_str("<===> mixin/one/input.scss\n@use 'utils';\n@include utils.a(1, );\n\n<===> mixin/one/output.css\nb {\n  c: 1;\n}\n\n");
    s.push_str(sep);
    // A later section redefining the same path must win over the inherited copy.
    s.push_str("<===> mixin/_utils.scss\n@mixin a($args...) { b {c: 2} }\n\n");
    s.push_str(sep);
    s.push_str("<===> mixin/two/input.scss\n@use 'utils';\n@include utils.a(2, );\n\n<===> mixin/two/output.css\nb {\n  c: 2;\n}\n");

    let cases = hrx::parse(&s);
    assert_eq!(cases.len(), 4, "fixture-only section still occupies a slot");

    let with_one = cases
        .iter()
        .find(|c| c.dir == "mixin/one")
        .expect("mixin/one case");
    assert!(
        with_one
            .files
            .iter()
            .any(|(p, c)| p == "mixin/_utils.scss" && c.contains("c: 1")),
        "inherited fixture visible to a later case: {:?}",
        with_one.files
    );

    let with_two = cases
        .iter()
        .find(|c| c.dir == "mixin/two")
        .expect("mixin/two case");
    let redefined = with_two
        .files
        .iter()
        .filter(|(p, _)| p == "mixin/_utils.scss")
        .count();
    assert_eq!(redefined, 1, "same-path file is not duplicated");
    assert!(
        with_two
            .files
            .iter()
            .any(|(p, c)| p == "mixin/_utils.scss" && c.contains("c: 2")),
        "latest declaration wins for later sections"
    );

    // Expectation members never leak across sections.
    assert_eq!(
        with_two
            .files
            .iter()
            .filter(|(p, _)| p.ends_with("output.css"))
            .count(),
        1,
        "outputs stay section-local"
    );
}

/// Real sass-spec files must parse with every case carrying an input and at
/// least one expected member (output or error).
#[test]
fn parses_real_sass_spec_file() {
    let path = common::sass_spec_dir()
        .join("css")
        .join("percent.hrx");
    let Ok(text) = std::fs::read_to_string(&path) else {
        // Submodule not checked out in this environment: nothing to verify.
        return;
    };
    let cases = hrx::parse(&text);
    assert!(!cases.is_empty(), "{} yielded no cases", path.display());
    for case in &cases {
        assert!(case.input.is_some(), "case without input: {:?}", case.dir);
        assert!(
            case.output.is_some() || case.error.is_some(),
            "case without expectation: {:?}",
            case.dir
        );
    }
}
