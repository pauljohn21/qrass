//! `sasspile` 的确定性铁律审核器。
//!
//! 反复出现的违规都出于同一个根因：管线不是一条 owned 值的流。本工具把高信号的结构性禁令
//! 编码成一张规则表，再扫描 Rust 源码，于是违规由机器（CI / 测试闸）接住，而不是靠自觉。
//! 命中的条目用 `tracing` 报告。
//!
//! 证据分三种，因为形状不同：
//! * [`SUBSTRING_BANS`] / [`LINE_START_BANS`] — 单凭一行就带着那个签名；
//! * [`AND_BANS`] — 只有**同一行上的两个签名**才认出这个形状，所以任一签名的合法用法
//!   （当源的 `from_iter`、词素解析上的 `.ok()`）都不会触发；
//! * `FileFacts` — 关于整个文件的规则（“原样兜底必须配一个致命性判定”），任何单行都无从表述。
//!
//! 后两种只是铁律的**代理**，从来不是证明：它们是照着本仓库真正删掉过的那些形状校准出来的，
//! 并且会跳过注释（见 `is_comment`）。

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule: &'static str,
    pub text: String,
}

/// 必须只写在一张表里的单位名。一个文件若对其中四个及以上（`>= 4`）分别判定，它就成了第二张可换算单位表
/// （见 `single-table` 规则）；真正的表是 `sasspile` 的 `crate::unit::UNITS`，它把这些单位列成数组
/// 条目而不是 `match` 臂。
const UNIT_LITERALS: [&str; 12] = [
    "\"px\"", "\"in\"", "\"cm\"", "\"mm\"", "\"pt\"", "\"pc\"", "\"q\"", "\"deg\"", "\"grad\"",
    "\"rad\"", "\"turn\"", "\"ms\"",
];

/// 子串禁令：每条都是一个被禁模式的精确签名（刻意收窄，以免在 `@for`/`@else if` 的字符串字面量
/// 和注释上误报）。
/// 设为 pub，是为了让 `tests/defence_fitness.rs` 从引擎自己派生出闸的清单：测试里手抄一份清单就是
/// 同一个事实写两遍，而某个闸 id 曾经就是这样从清单里消失的。
pub const SUBSTRING_BANS: &[(&str, &[&str])] = &[
    ("no-tree", &["Vec<Rule>", "pub children", ".children"]),
    // 手搭的编排层就是那个形状——它把 rxrust 掰过来跑项目自己的控制流，是反复返工的根。这些名字
    // 是那一层自己的词汇，并像 `no-recursive-descent` 锚定 `fn resolve(` 那样锚定它们的定界符：光写
    // `fn fail` 还会匹配到 `fn failure(&self) -> bool`，而那不是框架层。
    (
        "no-stage-layer",
        &[
            "type Stage",
            "fn drain(",
            "fn drain<",
            "fn fail(",
            "fn fail<",
        ],
    ),
    ("no-relay-vec", &["-> Vec<Rule>"]),
    ("no-string-machine", &["fn subst", "stmt_text"]),
    (
        "no-recursive-descent",
        &["fn write_rule", "fn resolve_all", "fn resolve("],
    ),
    ("no-from-fn-driver", &["from_fn("]),
    ("no-work-queue", &["VecDeque<"]),
    // 启发式：一个**驱动**遍历的可变累加子（帧栈 / @if 链），也就是穿了算子外衣的状态机——即使它一个
    // 循环都没用，照旧禁
    (
        "mutable-accumulator-drives",
        &["Vec<Frame>", "stack: Vec<", "chain_taken"],
    ),
    // 顺序当年是靠给并发产出的规则排序恢复的。语料是在某个*发射时刻*就把规则定下的
    // （css/style_rule declaration/interleaved/**），所以一个排序键就是“把随 P0′.4 删掉的机器再加
    // 回来”的签名。
    (
        "no-sort-for-order",
        &["sort_by_key", "sort_unstable_by", "sort_by(", ".sort()"],
    ),
];

/// 同一行的合取：`(rule, &[(needle_a, needle_b)])`。每一对都是一个只在两半同时出现时才有意义的被禁
/// 形状——这正是让合法的那一半不触发的原因。
pub const AND_BANS: &[(&str, &[(&str, &str)])] = &[
    // 一个阶段把自己的输出攒成 `Vec`、到下游再摊平：那是 P0′.1/.2 拆掉的
    // `scan_map(acc) -> Vec<Tok> -> flat_map(from_iter)` 外壳，而内核本来就用 `observer.next(..)` 发
    // 它自己的 0..N 个条目
    (
        "no-batch-shell",
        &[
            ("scan_map(", "Vec<"),
            ("flat_map(", "from_iter("),
            ("then(", "from_iter("),
        ],
    ),
    // 一次求值失败被变成 `None`、再变成文本：错误从来没有走到流的 `Err` 通道（P0′.3a/b）。词素解析
    // 上的 `.ok()` 不是这个形状，所以判据要成对。
    ("require-err-channel", &[(".ok()", "eval")]),
];

/// `FileFacts::line_findings` 里那些文件级检查所发出的 id。用具名常量而不是切片里的位置：某个调用
/// 点如果从 `FILE_RULE_IDS[0..3]` 取错了下标，就会给一条命中贴错标签，而每个 id 仍然在别处发出过，
/// 阳性对照看不见这件事。
pub const REQUIRE_ERR_CHANNEL: &str = "require-err-channel";
/// 没有先把 `SingleAssignment` 槽位停好，就去订阅高阶源
pub const REQUIRE_SLOT_BEFORE_SUBSCRIBE: &str = "require-slot-before-subscribe";
/// 第二张可换算单位表
pub const SINGLE_TABLE: &str = "single-table";

/// 文件级的 id，由那些检查自己发出的同一批常量装配而成，所以拼法只有一处。
pub const FILE_RULE_IDS: &[&str] = &[
    REQUIRE_ERR_CHANNEL,
    REQUIRE_SLOT_BEFORE_SUBSCRIBE,
    SINGLE_TABLE,
];

/// 这些算子的内层订阅必须**先**停进槽位，然后才订阅：rc.6 的 `into_future`/高阶源会把槽位还空着时到达
/// 的东西丢掉（或漏掉），见 docs/rxrust-source-analysis 09 §1；而 `merge_all` 里仍在排队的内层会
/// **丢失**发射。
const HIGHER_ORDER: &[&str] = &[
    "merge_all(",
    "concat_all(",
    "switch_map(",
    "flat_map(",
    "connectable(",
    "Subject",
    "from_stream(",
];

/// 非注释、非 `//` 的行以这些控制流关键字**开头**即禁。
pub const LINE_START_BANS: &[&str] = &["for ", "while ", "loop "];

fn is_comment(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}

fn finding(file: &str, line_no: usize, rule: &'static str, raw: &str) -> Finding {
    Finding {
        file: file.to_string(),
        line: line_no,
        rule,
        text: raw.trim().to_string(),
    }
}

/// 整个文件所陈述的事实，供那些任何单行都判不定的规则使用。它只由**代码行**构成，所以一条 doc 注释
/// 永远洗不掉一个违规（比如散文里提到 `is_fatal`，而代码却把错误吞掉）。
#[derive(Debug, Default)]
struct FileFacts {
    /// 本文件按 kind 做致命性判定（`SassError::is_fatal`），所以里面的 `unwrap_or_else(verbatim)` 是
    /// 语料定义的、只针对*某个特定形状*的原样透出——不是 P0′.3 要替掉的那种一律 `.ok()` 兜底
    declares_fatality: bool,
    /// 代码里存在原样兜底
    has_verbatim_fallback: bool,
    /// 本文件驱动一个高阶/多源算子
    uses_higher_order: bool,
    /// 本文件自己订阅了东西
    subscribes: bool,
    /// 本文件把订阅停进框架的槽位
    builds_slot: bool,
    /// 本文件就是那张被认可的可换算单位表（凭它定义了些什么来认，不凭文件名，所以一份副本不能靠改叫
    /// `unit.rs` 绕过闸）
    owns_units_table: bool,
    /// 出现在 `=>` 臂行上的不同单位名——见 `single-table` 检查
    table_units: usize,
    /// 第一条这样的臂行，好让一张表只产一条命中而不是六条
    first_table_arm: Option<usize>,
}

/// 单位表的一条臂行：把一个带引号的单位名绑到一个值上。两种写法都算，因为本项目真正发出过的第二张表用的
/// 是前一种（`"px" => Some(1.0)`），而被认可的那张用的是后一种（`("px", ("length", 1.0))`）——只认 `=>` 臂
/// 的闸会看不见一份写成元组数组的因子表，而那恰恰是一份副本能藏住的方式。
fn is_table_arm(line: &str) -> bool {
    let bound = line.contains("=>") || line.contains("\", (");
    bound && UNIT_LITERALS.iter().any(|needle| line.contains(needle))
}

/// 一行是否算作表的证据：不是注释，且是带单位字面量的一臂。这里的输入**始终是原始内容**——行号必须能跟
/// `scan_file` 报出的行号比较，而文件级布尔量所用的那个剥掉注释的字符串是另一个下标空间（否则一张藏在 doc
/// 注释后面的第二张表就隐形了：实测在开头放一行注释、再放两行注释时，闸先报了错的行号、随后什么都报不出来）。
fn is_table_evidence(line: &str) -> bool {
    !is_comment(line) && is_table_arm(line)
}

/// `content` 的表臂行上出现的不同单位名。
fn distinct_table_units(content: &str) -> usize {
    UNIT_LITERALS
        .into_iter()
        .filter(|needle| {
            content
                .lines()
                .any(|line| is_table_evidence(line) && line.contains(needle))
        })
        .count()
}

/// 这样一张表的第一条臂行，用的是调用者的行号空间，好让一张表只算一条命中。
fn first_table_arm_line(content: &str) -> Option<usize> {
    content
        .lines()
        .enumerate()
        .find(|(_, line)| is_table_evidence(line))
        .map(|(index, _)| index + 1)
}

impl FileFacts {
    fn of(content: &str) -> Self {
        let code = content.lines().filter(|line| !is_comment(line)).fold(
            String::new(),
            |mut acc, line| {
                acc.push_str(line);
                acc.push('\n');
                acc
            },
        );
        let holds = |needles: &[&str]| needles.iter().any(|n| code.contains(n));
        Self {
            declares_fatality: code.contains("is_fatal"),
            has_verbatim_fallback: code.contains("unwrap_or_else("),
            uses_higher_order: holds(HIGHER_ORDER),
            subscribes: code.contains(".subscribe("),
            builds_slot: code.contains("SingleAssignment"),
            owns_units_table: code.contains("pub const UNITS"),
            // 相关性才要紧：一个文件可以合法地点名单位（`lexer` 里的表、序列化器），也可以合法地含 `matches!(…)`。
            // 让第二张换算表成立的是**坐在 `=>` 臂上**的单位字面量，那里每一臂就是那张表的一行。
            table_units: distinct_table_units(content),
            first_table_arm: first_table_arm_line(content),
        }
    }

    /// 文件级的规则，报在承载那次违规调用的行上：一个原样兜底而本文件没有 `is_fatal` 判定，或者一个高阶
    /// 订阅而没有先停好 `SingleAssignment` 槽位。
    fn line_findings(&self, file: &str, line_no: usize, raw: &str) -> Vec<Finding> {
        if is_comment(raw) {
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.has_verbatim_fallback && !self.declares_fatality && raw.contains("unwrap_or_else(")
        {
            out.push(finding(file, line_no, REQUIRE_ERR_CHANNEL, raw));
        }
        if self.uses_higher_order
            && self.subscribes
            && !self.builds_slot
            && raw.contains(".subscribe(")
        {
            out.push(finding(file, line_no, REQUIRE_SLOT_BEFORE_SUBSCRIBE, raw));
        }
        // 一个事实，一处住所。四个及以上（`>= 4`）被绑到值上的单位名，就是一张可换算单位表：`builtins/math.rs` 里的
        // 那份副本（`"px" => Some(1.0)`、`"cm" => Some(37.7952755906)`……）答不出 `hypot.hrx::units`，它需要
        // `q` 以及只有 `unit::UNITS` 才携带的、精确的“一毫米的四分之一”换算关系。定义 `pub const UNITS` 的
        // 文件才是被认可的那一个；没有任何文件靠文件名豁免。
        if self.table_units >= 4 && self.first_table_arm == Some(line_no) && !self.owns_units_table
        {
            out.push(finding(file, line_no, SINGLE_TABLE, raw));
        }
        out
    }
}

fn scan_line(file: &str, line_no: usize, raw: &str) -> Vec<Finding> {
    if is_comment(raw) {
        return Vec::new();
    }
    let matched = |needles: &[&str]| needles.iter().any(|n| raw.contains(n));
    SUBSTRING_BANS
        .iter()
        .filter(|(_, needles)| matched(needles))
        .map(|(rule, _)| finding(file, line_no, rule, raw))
        .chain(AND_BANS.iter().flat_map(|(rule, pairs)| {
            pairs
                .iter()
                .filter(|(a, b)| raw.contains(a) && raw.contains(b))
                .map(move |_| finding(file, line_no, rule, raw))
        }))
        .chain(
            if LINE_START_BANS
                .iter()
                .any(|kw| raw.trim_start().starts_with(kw))
            {
                vec![finding(file, line_no, "no-imperative-loop", raw)]
            } else {
                Vec::new()
            },
        )
        .collect()
}

pub fn scan_file(path: &Path) -> Vec<Finding> {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let facts = FileFacts::of(&content);
    content
        .lines()
        .enumerate()
        .flat_map(|(i, line)| {
            scan_line(name, i + 1, line)
                .into_iter()
                .chain(facts.line_findings(name, i + 1, line))
        })
        .collect()
}

/// 递归收集 `dir` 下的 `.rs` 文件（工具层可以用递归；禁的是编译出的管线去走一棵树，不是禁扫目录）。
fn rust_files(dir: &Path, acc: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .for_each(|p| {
                if p.is_dir() {
                    rust_files(&p, acc);
                } else if p.extension().is_some_and(|e| e == "rs") {
                    acc.push(p);
                }
            });
    }
}

pub fn scan_dir(dir: &Path) -> Vec<Finding> {
    if dir.is_file() {
        return scan_file(dir);
    }
    let mut files = Vec::new();
    rust_files(dir, &mut files);
    files.iter().flat_map(|f| scan_file(f)).collect::<Vec<_>>()
}

/// 一个目录的人类可读审核报告（CLI 与 MCP 工具都用它）。
pub fn review_report(dir: &Path) -> String {
    let findings = scan_dir(dir);
    if findings.is_empty() {
        return format!("{}: no iron-rule violations", dir.display());
    }
    findings.iter().fold(String::new(), |mut s, f| {
        s.push_str(&format!(
            "[{}] {}:{} — {}\n",
            f.rule, f.file, f.line, f.text
        ));
        s
    }) + &format!("{} violation(s)", findings.len())
}

use serde_json::{Value, json};

/// 处理一条 MCP/JSON-RPC 消息，返回响应（通知没有 `id`，所以返回 `None`）。只实现工具客户端需要的
/// 那个子集：`initialize`、`tools/list`、`tools/call`。
pub fn handle_message(req: &Value) -> Option<Value> {
    let id = req.get("id").cloned()?;
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "sasslint", "version": env!("CARGO_PKG_VERSION") }
        }),
        "tools/list" => json!({
            "tools": [{
                "name": "review",
                "description": "Scan a Rust source tree for sasspile iron-rule violations (tree/recursion/relay-Vec/string-machine/imperative loops, batch shells, sorted order, swallowed Err channel, missing subscription slot).",
                "inputSchema": {
                    "type": "object",
                    "properties": { "path": { "type": "string", "description": "directory to scan" } },
                    "required": ["path"]
                }
            }]
        }),
        "tools/call" => {
            let path = req
                .pointer("/params/arguments/path")
                .and_then(Value::as_str)
                .unwrap_or("crates/sasspile/src");
            json!({
                "content": [{ "type": "text", "text": review_report(Path::new(path)) }],
                "isError": false
            })
        }
        "ping" => json!({}),
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "method not found" }
            }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}
