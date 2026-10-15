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
