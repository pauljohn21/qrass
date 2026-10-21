//! throwaway probe (deleted after use)
mod common;
use common::css_diff;
#[test]
#[ignore]
fn no_alpha_zero() {
    let ours = "a {\n  value: oklch(100% 0.2 3deg);\n  space: oklch;\n  channels: 100% 0.2 3deg / 1;\n}\n";
    let want = "a {\n  value: oklch(100% 0.2 3deg);\n  space: oklch;\n  channels: 100% 0.2 3deg / 1;\n}\n";
    let o = css_diff::normalize(&css_diff::canonicalize(ours));
    let w = css_diff::normalize(&css_diff::canonicalize(want));
    assert_eq!(o, w, "MUST equal for same text");
    // real golden from hrx:
    let text = std::fs::read_to_string("../../sass-spec/spec/core_functions/color/oklch/no_alpha.hrx").unwrap();
    let first = text.split("================================================================================").nth(1).unwrap();
    println!("SECTION: {}", first.replace('\n', " ¶ ")[..300.min(first.len())].replace('\n', ""));
}
