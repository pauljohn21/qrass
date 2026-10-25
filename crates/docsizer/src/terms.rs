//! 可读性闸：一篇号称"零基础"的课文里出现的 Rust 行话，必须**先在术语表定义过**，
//! 且定义所在课号不得晚于使用它的课号；每课引入的新概念（术语表的**行**）也必须有预算上限。
//!
//! 这条闸是被用户逼出来的：字数、行覆盖、编译全绿的 V0，对没写过代码的人仍然读不懂——
//! 因为我只验收了"我做了多少工作"，没验收"读者能不能跟上"。
//! 实测（2026-10-25）：旧 V0 五课里 147 处行话先用后定义、每课引入 16~22 个新概念（预算 2）。

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// 课号排序键：`(卷号, 课序)`。`00-04-x.md` 与 `V0-04` 都解析成 `(0, 4)`。
pub type LessonKey = (usize, usize);

/// 需要被定义后才允许在正文里使用的行话。这是**数据**：课文教到一个词，就往 `GLOSSARY.md`
/// 写一行并标注首现课号；没教到就不许用。
pub const JARGON: &[&str] = &[
    "trait",
    "impl",
    "泛型",
    "generic",
    "关联类型",
    "associated type",
    "GAT",
    "HRTB",
    "生命周期",
    "lifetime",
    "解引用",
    "deref",
    "copy",
    "clone",
    "签名",
    "所有权",
    "ownership",
    "借用",
    "borrow",
    "move",
    "闭包",
    "closure",
    "宏",
    "macro",
    "令牌",
    "token",
    "协议",
    "object-safety",
    "blanket",
    "coherence",
    "hygiene",
    "dyn",
];

/// 术语表解析结果：词 → 可用起点 + 该词来自哪一行；以及每行（概念）归属哪一课。
#[derive(Debug, Default)]
pub struct Definitions {
    /// 小写词 → `(首现课号, 行号)`
    usable: BTreeMap<String, (LessonKey, usize)>,
    /// 课号 → 该课引入的不同概念行数（别名不重复计数）
    introduced: BTreeMap<LessonKey, usize>,
}

impl Definitions {
    pub fn terms(&self) -> usize {
        self.usable.len()
    }

    /// 这个词从哪一课起可以出现在正文里（未收录返回 `None`）。
    pub fn usable_from(&self, term: &str) -> Option<LessonKey> {
        self.usable.get(&term.to_lowercase()).map(|(key, _)| *key)
    }

    /// 这一课引入了几个**新概念**（按术语表的行数，不按别名拼写数）。
    pub fn introduced_at(&self, key: Option<LessonKey>) -> usize {
        key.and_then(|at| self.introduced.get(&at)).copied().unwrap_or(0)
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub lessons: usize,
    pub defined_terms: usize,
    /// 用了却没定义（或定义在后面的课）的行话：`课文件:行 `词``
    pub undefined: Vec<String>,
    /// 一节课引入的新概念超过预算
    pub over_budget: Vec<String>,
}

impl Report {
    pub fn clean(&self) -> bool {
        self.undefined.is_empty() && self.over_budget.is_empty()
    }
}

/// 从文件名或课号标签里取前两组数字当排序键；只有一组时按 `(那组, 0)`；没有数字返回 `None`。
pub fn lesson_key(text: &str) -> Option<LessonKey> {
    let groups: Vec<usize> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<usize>().ok())
        .take(2)
        .collect();
    match (groups.first().copied(), groups.get(1).copied()) {
        (None, _) => None,
        (Some(first), None) => Some((first, 0)),
        (Some(first), Some(second)) => Some((first, second)),
    }
}

/// 解析 `GLOSSARY.md` 的表格行：`| 术语 | 英文 | 释义 | 首现课号 |`。
///
/// 一格里的 `` `Rc` / `Arc` `` 会拆成多个词（正文可能只用其中一种写法），但它们属于**同一个概念**，
/// 计入预算时只算一行。
pub fn parse_glossary(text: &str) -> Definitions {
    text.lines().enumerate().filter(|(_, line)| line.trim_start().starts_with('|')).fold(
        Definitions::default(),
        |mut acc, (index, line)| {
            let cells: Vec<&str> = line.split('|').map(str::trim).filter(|cell| !cell.is_empty()).collect();
            let first = cells.first().copied().unwrap_or("");
            let separator = first.chars().all(|c| matches!(c, '-' | ':'));
            let key = cells.get(3).and_then(|cell| lesson_key(cell));
            let usable = cells.len() >= 4 && first != "术语" && !separator && key.is_some();
            if usable {
                let key = key.unwrap_or((0, 0));
                let row = index + 1;
                split_terms(&format!("{} {}", cells[0], cells[1])).into_iter().for_each(|term| {
                    let entry = acc.usable.entry(term).or_insert((key, row));
                    if key < entry.0 {
                        *entry = (key, row);
                    }
                });
                *acc.introduced.entry(key).or_insert(0) += 1;
            }
            acc
        },
    )
}

fn split_terms(raw: &str) -> Vec<String> {
    raw.split(['/', '`', '（', '）', '(', ')', '、', '，', ',', ' '])
        .map(str::trim)
        .map(str::to_lowercase)
        .filter(|token| token.chars().count() >= 2)
        .collect()
}

/// 取正文行（跳过 ``` 围栏内的代码），带原始行号：违规要能指到具体那一行。
pub fn prose_lines(text: &str) -> Vec<(usize, String)> {
    let (lines, _inside_fence) = text.lines().enumerate().fold(
        (Vec::<(usize, String)>::new(), false),
        |(mut kept, in_fence): (Vec<(usize, String)>, bool), (index, line)| {
            let fenced = line.trim_start().starts_with("```");
            if !fenced && !in_fence {
                kept.push((index + 1, strip_inline_code(line)));
            }
            (kept, if fenced { !in_fence } else { in_fence })
        },
    );
    lines
}

/// 行内代码（反引号包起来的部分）不计：正文写"别把 `trait` 当术语先扔给你"是允许的元话语，
/// 真正的问题是拿行话当**解释工具**写在普通句子里。
fn strip_inline_code(line: &str) -> String {
    line.split('`').step_by(2).collect::<Vec<&str>>().join("")
}

fn contains_term(haystack: &str, term: &str) -> bool {
    let bytes = haystack.as_bytes();
    let ascii = term.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    haystack.match_indices(term).any(|(at, _)| match ascii {
        false => true,
        true => {
            let before_ok = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
            let after = at + term.len();
            let after_ok = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
            before_ok && after_ok
        }
    })
}

/// 扫描输入：`(课号键, 文件名, 全文)`。
pub type Lessons = Vec<(Option<LessonKey>, String, String)>;

/// 目录里找到的课文文件：`(课号键, 路径)`。
type Found = Vec<(Option<LessonKey>, PathBuf)>;

/// 读目录 + 术语表，产出报告。
pub fn check(dir: &Path, glossary_text: &str, max_new: usize) -> Result<Report, Box<dyn Error>> {
    let lessons: Lessons = collect_markdown(dir)?
        .into_iter()
        .map(|(key, path)| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (key, name, fs::read_to_string(&path).unwrap_or_default())
        })
        .collect();
    Ok(scan_lessons(&lessons, &parse_glossary(glossary_text), max_new))
}

/// 纯内存扫描接缝（测试直接驱动）：每项为 `(课号键, 文件名, 全文)`。
pub fn scan_lessons(
    lessons: &[(Option<LessonKey>, String, String)],
    defined: &Definitions,
    max_new: usize,
) -> Report {
    let mut ordered = lessons.to_vec();
    ordered.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    ordered.into_iter().fold(
        Report {
            lessons: 0,
            defined_terms: defined.terms(),
            undefined: Vec::new(),
            over_budget: Vec::new(),
        },
        |mut acc, (key, file, text)| {
            acc.lessons += 1;

            // ① 预算：这一课引入的新概念数（按术语表的行，别名不重复计）
            let introduced = defined.introduced_at(key);
            if introduced > max_new {
                acc.over_budget.push(format!("{file} 引入 {introduced} 个新概念（预算 {max_new}）"));
            }

            // ② 未定义先用：本课正文用的行话，必须在同一课或更早的课里定义过
            prose_lines(&text).into_iter().for_each(|(lineno, line)| {
                JARGON
                    .iter()
                    .filter(|term| contains_term(&line, &term.to_lowercase()))
                    .for_each(|term| match defined.usable_from(term) {
                        Some(from) if Some(from) <= key => {}
                        _ => acc.undefined.push(format!("{file}:{lineno} `{term}`")),
                    });
            });
            acc
        },
    )
}

fn collect_markdown(dir: &Path) -> Result<Found, Box<dyn Error>> {
    let skip = ["README.md", "GLOSSARY.md", "VERIFICATION.tsv", "MAP.tsv", "COVERAGE.tsv"];
    let mut out = Vec::new();
    walk(dir, &skip, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, skip: &[&str], out: &mut Found) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, skip, out)?;
            continue;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let markdown = path.extension().map(|e| e == "md").unwrap_or(false);
        let is_lesson = markdown && name != "README.md" && !skip.contains(&name.as_str());
        if is_lesson {
            out.push((lesson_key(&name), path));
        }
    }
    Ok(())
}
