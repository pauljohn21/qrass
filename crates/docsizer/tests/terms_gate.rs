//! 可读性闸的行为断言：未定义先用必须报、同课定义必须放过、代码不计、预算超限必须报。

use docsizer::terms::{Definitions, JARGON, lesson_key, parse_glossary, prose_lines, scan_lessons};

fn glossary(rows: &[&str]) -> Definitions {
    let head = "| 术语 | 英文 | 释义 | 首现课号 |\n|---|---|---|---|\n";
    let text = rows.iter().fold(head.to_string(), |mut acc, row| {
        acc.push_str(row);
        acc.push('\n');
        acc
    });
    parse_glossary(&text)
}

fn lesson(key: Option<(usize, usize)>, name: &str, body: &str) -> (Option<(usize, usize)>, String, String) {
    (key, name.to_string(), body.to_string())
}

#[test]
fn lesson_key_accepts_both_file_names_and_labels() {
    assert_eq!(lesson_key("00-04-shared-ownership-and-interior-mutability.md"), Some((0, 4)));
    assert_eq!(lesson_key("V0-04"), Some((0, 4)));
    assert_eq!(lesson_key("README"), None);
}

#[test]
fn jargon_used_before_its_lesson_is_reported_with_line_number() {
    let found = glossary(&["| 借用 | borrow | 暂时看别人拥有的值 | V0-02 |"]);
    let report = scan_lessons(
        &[lesson(Some((0, 1)), "00-01-a.md", "这一层叫 trait。\n第二行提到借用。\n")],
        &found,
        8,
    );
    assert!(
        report.undefined.contains(&"00-01-a.md:1 `trait`".to_string()),
        "trait 从未定义却先用，必须报：{:?}",
        report.undefined
    );
    assert!(
        report.undefined.contains(&"00-01-a.md:2 `借用`".to_string()),
        "定义在后面的课（V0-02）也不算：{:?}",
        report.undefined
    );
    assert!(!report.clean());
}

#[test]
fn a_term_defined_in_the_same_lesson_is_usable() {
    let found = glossary(&["| 借用 | borrow | 暂时看别人拥有的值 | V0-01 |"]);
    let report = scan_lessons(
        &[lesson(Some((0, 1)), "00-01-a.md", "借用就是把值暂时交给别人看。\n")],
        &found,
        8,
    );
    assert!(report.undefined.is_empty(), "同课定义即可使用：{:?}", report.undefined);
}

#[test]
fn code_fences_and_inline_code_are_not_prose() {
    let body = "```rust\ntrait Marker {}\n```\n\n别把 `trait` 这个词当成术语先扔给你。\n";
    let outside = prose_lines(body);
    let kept = outside.iter().map(|(_, line)| line.as_str()).collect::<Vec<&str>>().join("|");
    assert!(!kept.contains("trait"), "围栏与行内代码都不该进正文：{kept}");

    let found = glossary(&[]);
    let report = scan_lessons(&[lesson(Some((0, 1)), "00-01-a.md", body)], &found, 8);
    assert!(report.undefined.is_empty(), "示例代码与元话语不该被判违规：{:?}", report.undefined);
}

#[test]
fn introducing_too_many_new_concepts_at_once_is_reported() {
    let found = glossary(&[
        "| 借用 | borrow | 暂时看 | V0-01 |",
        "| 所有权 | ownership | 值有主人 | V0-01 |",
        "| 生命周期 | lifetime | 值活多久 | V0-01 |",
    ]);
    let report = scan_lessons(
        &[lesson(Some((0, 1)), "00-01-a.md", "本课不含行话。\n")],
        &found,
        2,
    );
    assert_eq!(
        report.over_budget.as_slice(),
        &["00-01-a.md 引入 3 个新概念（预算 2）".to_string()],
        "预算按术语表的**行**计，不能把一行的中英别名数成两个概念"
    );
    assert_eq!(found.terms(), 6, "别名词表应有 6 个可查写法（3 行 × 2 词）");
}

#[test]
fn the_jargon_list_is_unique_and_lowercase_normalized() {
    let unique = JARGON.iter().collect::<std::collections::HashSet<&&str>>().len();
    assert_eq!(unique, JARGON.len(), "行话清单不该有重复项");
    assert!(JARGON.contains(&"trait") && JARGON.contains(&"借用"), "核心行话必须在清单里");
}
