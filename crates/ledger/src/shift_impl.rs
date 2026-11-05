//! `ledger changed` 列出的那些行的距离拆分。把它留在二进制之外，是为了让这个分类器有一条阳性对照
//! （`crates/ledger/tests/shift_split.rs`），它的 include 方式和 sasspile 测试 include
//! `tests/common/tracing_bridge.rs` 一样——账本的 `lib.rs` 已经顶到自己的行数预算，而这不是库对外的一面：
//! 除了 `shift` 命令，没有谁要回答这个问题。

/// 从 `start` 起一个数占多少个字符；不是数则 `None`。一个定义同时服务 [`skeleton`] 与 [`numbers`]，于是
/// 掩码和距离不会对“什么算一个数”各说一套。前导正负号**就是**它的一部分：一个换了符号的值因此算一个大
/// 距离，而不是一次种类变化，这正是规则要的（符号错是值错，不是另一种答案）。
fn number_len(chars: &[char], start: usize) -> Option<usize> {
    let digit = |i: usize| chars.get(i).is_some_and(|c| c.is_ascii_digit());
    let signed = matches!(chars.get(start), Some('+') | Some('-'));
    let body = if signed { start + 1 } else { start };
    if !digit(body) && !(chars.get(body) == Some(&'.') && digit(body + 1)) {
        return None;
    }
    let mut i = body;
    while digit(i) {
        i += 1;
    }
    // 小数点只有在后面跟着数字时才属于这一段。
    if chars.get(i) == Some(&'.') {
        let mut j = i + 1;
        while digit(j) {
            j += 1;
        }
        if j > i + 1 {
            i = j;
        }
    }
    // 指数也一样，只有当它后面跟着一个（可带正负号的）数字时才并入。
    if matches!(chars.get(i), Some('e') | Some('E')) {
        let mut j = i + 1;
        if matches!(chars.get(j), Some('+') | Some('-')) {
            j += 1;
        }
        if digit(j) {
            while digit(j) {
                j += 1;
            }
            i = j;
        }
    }
    Some(i - start)
}

/// 把数字掩掉，好让两段文本能按**形状**比较，而不是按数字比较。
pub fn skeleton(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        match number_len(&chars, at) {
            Some(length) => {
                out.push('#');
                at += length;
            }
            None => {
                out.push(chars[at]);
                at += 1;
            }
        }
    }
    out
}

/// 一段文本里的数字，按出现顺序：距离只能逐位相加，所以调用方必须知道两段文本**何时**没能对齐。
pub fn numbers(text: &str) -> Vec<f64> {
    let chars: Vec<char> = text.chars().collect();
    let mut found: Vec<f64> = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        match number_len(&chars, at) {
            Some(length) => {
                let token: String = chars[at..at + length].iter().collect();
                found.extend(token.parse::<f64>().ok());
                at += length;
            }
            None => at += 1,
        }
    }
    found
}

/// 一行为什么没有距离可谈。分开命名是因为规则对它们的处置不同：一个答案变成一个**错误**，而这个错误
/// 语料从没说过，那是要回退的缺陷；另一类则是一行的期望本身就是一句错误消息，落在距离能意味着什么之外。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Reason {
    /// 存下的详情不是一个 `got … want …` 对（一条 `expected error` 行，或一个无效用例）
    ErrorExpectation,
    /// 两侧拿着同一类答案，但数字之外的文本不同——一句原样输出变成了算出的值，一个通道出现或消失了
    ShapeOfText,
}

/// 一行的判定。`Distance` 带着两侧总量，好让调用方打印出移动了多少；`Shape` 说这两个答案不是同*一类*
/// 答案，那是一个列清单的问题，从来不是计数的问题。
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Distance { old: f64, new: f64 },
    Shape(Reason),
}

/// 一次 harness 详情里的两段文本：`got "<css>" want "<css>"`。任何不是这个精确形状的——一条 `expected error`
/// 行、一行 harness 用别的方式存下的——都给 `None`，调用方把它当作种类变化，而不是去猜一个距离。
fn got_and_want(detail: &str) -> Option<(&str, &str)> {
    let rest = detail.strip_prefix("got \"")?;
    let (got, tail) = rest.split_once("\" want \"")?;
    Some((got, tail.strip_suffix('"')?))
}

/// 判定一条 `changed` 行：`<key> | a: <detail> | b: <detail>`，每个详情由 harness 写成
/// `got "<css>" want "<css>"`——或者，对一行期望是错误的情形，写成
/// `expected error "<msg>", got "<css>"`，那没有距离可谈。
pub fn judge(line: &str) -> Verdict {
    let (key_and_rest, detail_b) = match line.split_once(" | b: ") {
        Some(parts) => parts,
        None => return Verdict::Shape(Reason::ErrorExpectation),
    };
    let (_, detail_a) = match key_and_rest.split_once(" | a: ") {
        Some(parts) => parts,
        None => return Verdict::Shape(Reason::ErrorExpectation),
    };
    let Some((got_a, want)) = got_and_want(detail_a) else {
        return Verdict::Shape(Reason::ErrorExpectation);
    };
    let Some((got_b, want_b)) = got_and_want(detail_b) else {
        return Verdict::Shape(Reason::ErrorExpectation);
    };
    // 一份归档期望，两个 run：存下的期望如果不同，这一行就不可比。
    if want != want_b || skeleton(got_a) != skeleton(got_b) || skeleton(got_a) != skeleton(want) {
        return Verdict::Shape(Reason::ShapeOfText);
    }
    let (va, vb, vw) = (numbers(got_a), numbers(got_b), numbers(want));
    if vw.is_empty() || va.len() != vw.len() || vb.len() != vw.len() {
        return Verdict::Shape(Reason::ShapeOfText);
    }
    let total =
        |got: &[f64]| -> f64 { got.iter().zip(vw.iter()).map(|(g, w)| (g - w).abs()).sum() };
    Verdict::Distance {
        old: total(&va),
        new: total(&vb),
    }
}

/// 一整份 `changed` 列表的计数，外加计数不该吞掉的那两个桶里的 key：那些**远离**归档的行，和那些换了
/// **种类**的行。
#[derive(Debug, Default)]
pub struct Split {
    pub closer: usize,
    pub farther: usize,
    pub tie: usize,
    pub error_expectation: usize,
    pub shape_of_text: usize,
    pub farther_keys: Vec<String>,
    pub error_keys: Vec<String>,
    pub shape_keys: Vec<String>,
}

pub fn split_of(rows: &[String]) -> Split {
    let mut out = Split::default();
    rows.iter().for_each(|line| {
        let key = line.split(" | ").next().unwrap_or(line).to_string();
        match judge(line) {
            Verdict::Shape(reason) => match reason {
                Reason::ErrorExpectation => {
                    out.error_expectation += 1;
                    out.error_keys.push(key);
                }
                Reason::ShapeOfText => {
                    out.shape_of_text += 1;
                    out.shape_keys.push(key);
                }
            },
            Verdict::Distance { old, new } => match new.partial_cmp(&old) {
                Some(std::cmp::Ordering::Less) => out.closer += 1,
                Some(std::cmp::Ordering::Greater) => {
                    out.farther += 1;
                    out.farther_keys.push(key);
                }
                _ => out.tie += 1,
            },
        }
    });
    out
}
