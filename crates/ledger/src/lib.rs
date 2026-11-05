//! hrx 账本——一整套 SQLite 表，存下所有 sass-spec 的 A/B 快照，
//! 取代按轮次散开的 `reports/hrx-*.tsv`（262 个文件 / 154 MB）。
//!
//! **数据库就是记录本身，而且它留在本地**：`reports/hrx.db` 被 gitignore，因为一个几 MB 的二进制
//! 会在每次导入时被改写进历史。所以一次全新 clone 从一个空账本开始；过去那些 run 的结论住在
//! `openspec/.../tasks.md` 里（它点名 run 名、NET/REGRESS 数、以及哪些用例动了），每个新增量都从那里
//! 重新导入。不要回到散落的快照文件，也不要开第二个数据库：每一次 harness run 都要用一个能说清
//! 它量的是什么的名字，导入这一个（`ledger runs` 列出它们，`ledger diff A B` 比较两个）。
//!
//! 模型：PASS 行是隐含的。`case` 是全局 key 字典，`run`
//! 记录一次快照（name/total/pass），`run_fail` 只存每个 run 的失败集合。A/B 语义直接由集合差得出：
//!   regression = fails(b) − fails(a);  net gain = fails(a) − fails(b).
//!
//! RFP 约定：参数处理与行折叠都是
//! 组合子链；所有输出都走 `tracing`（无 println）。
//!
//! Usage:
//!   ledger import <run-name> <file.tsv> [more.tsv...]   (run 名对所有文件生效)
//!   ledger diff <a> <b> [key-prefix]                        (回归 / 净增，给了前缀就按簇)
//!   ledger changed <a> <b> [key-prefix]                 (两个 run 都失败、详情却不同的行：那是
//!                                                         `regress=0` 看不见的形状变化)
//!   ledger runs                                           (汇总表)
//!   ledger failures <run> [key-or-detail-substring] [--count]
//!   ledger passed   <run> <key-prefix>
//!
//! `passed` 之所以存在，是因为存储是不对称的：只有失败才是行，所以“这个簇现在哪些用例 PASS”没有
//! 直接表示。它**只对一个具名簇**作答：在该前缀下某处记录过的 key，减去这个 run 失败的那些。前缀是
//! 必需的、不是装饰——唯一可选的另一种论域是“账本见过的每一个 key”，那会在 run 根本没量过某个用例时把它
//! 悄悄报成通过（实测：`passed head-600-pathkey operators/` 返回 29 个 key，而那次 harness 到 0.6 都没
//! 扫过 `operators`）。

use rusqlite::params;
use rusqlite::{Connection, Result as QResult};
use std::path::Path;
use tracing::{info, warn};

mod changed;
pub use changed::changed_details;

/// 账本的默认位置，相对仓库根。
pub const DB: &str = "reports/hrx.db";

/// 在已切好的参数上做 CLI 分派（订阅者的装配归二进制自己）。
pub fn run(conn: &Connection, argv: &[String]) -> QResult<()> {
    migrate(conn)?;
    match argv.first().map(String::as_str) {
        Some("import") => import(conn, &argv[1..]),
        Some("note-import") => note_import(conn, &argv[1..]),
        Some("note-dump") => note_dump(conn, &argv[1..]),
        Some("diff") => diff(conn, &argv[1..]),
        Some("changed") => changed::run_cli(conn, &argv[1..]),
        Some("runs") => runs(conn),
        Some("failures") => failures(conn, &argv[1..]),
        Some("passed") => passed(conn, &argv[1..]),
        _ => {
            info!("usage: ledger import|diff|changed|runs|failures|passed [--db=<path>] ...");
            Ok(())
        }
    }
}

pub fn migrate(conn: &Connection) -> QResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS case_key(
            id    INTEGER PRIMARY KEY,
            key   TEXT UNIQUE NOT NULL
         );
         CREATE TABLE IF NOT EXISTS run(
            id    INTEGER PRIMARY KEY,
            name  TEXT UNIQUE NOT NULL,
            ts    TEXT NOT NULL,
            total INTEGER NOT NULL,
            pass  INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS fail_detail(
            id    INTEGER PRIMARY KEY,
            text  TEXT UNIQUE NOT NULL
         );
         CREATE TABLE IF NOT EXISTS run_fail(
            run_id    INTEGER NOT NULL REFERENCES run(id),
            case_id   INTEGER NOT NULL REFERENCES case_key(id),
            detail_id INTEGER REFERENCES fail_detail(id),
            PRIMARY KEY (run_id, case_id)
         );
         CREATE TABLE IF NOT EXISTS note(
            topic TEXT NOT NULL,
            ord   INTEGER NOT NULL,
            text  TEXT NOT NULL,
            PRIMARY KEY (topic, ord)
         );",
    )
}

/// 把一个 live runner 快照记进账本（与 CLI 共用同一张 schema；hrx runner 调这个，而不是
/// 再长出一份 `reports/*.tsv`）。
pub fn record(name: &str, db_path: &Path, rows: &[String]) -> QResult<(usize, usize)> {
    let conn = Connection::open(db_path)?;
    migrate(&conn)?;
    import_rows(&conn, name, rows)
}

/// 把一个 `STATUS\tkey[\tdetail]` 快照文件解析进账本。
fn import_file(conn: &Connection, name: &str, path: &str) -> QResult<usize> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
        warn!(path, ?e, "unreadable snapshot file");
        String::new()
    });
    let rows: Vec<String> = text.lines().map(str::to_string).collect();
    import_rows(conn, name, &rows).map(|(total, _)| total)
}

fn import_rows(conn: &Connection, name: &str, lines: &[String]) -> QResult<(usize, usize)> {
    let rows: Vec<(String, String, String)> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let (status, rest) = l.split_once('\t')?;
            let (key, detail) = rest.split_once('\t').unwrap_or((rest, ""));
            Some((
                status.trim().to_string(),
                key.trim().to_string(),
                detail.to_string(),
            ))
        })
        .collect();
    let total = rows.len();
    let pass = rows.iter().filter(|(s, _, _)| s == "PASS").count();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO run(name, ts, total, pass) VALUES (?1, datetime('now'), ?2, ?3)
         ON CONFLICT(name) DO UPDATE SET ts = excluded.ts, total = excluded.total, pass = excluded.pass",
        params![name, total as i64, pass as i64],
    )?;
    let ref_run: i64 = tx.query_row("SELECT id FROM run WHERE name=?1", params![name], |r| {
        r.get(0)
    })?;
    // 一次重新导入会替换掉这个 run 的全部判定：只存失败，所以不删的话，如今 PASS 的用例还留着旧行，
    // `diff` 就报出一条幽灵 REGRESS。
    tx.execute("DELETE FROM run_fail WHERE run_id = ?1", params![ref_run])?;
    let mut case_ins = tx.prepare("INSERT OR IGNORE INTO case_key(key) VALUES (?1)")?;
    let mut detail_ins = tx.prepare("INSERT OR IGNORE INTO fail_detail(text) VALUES (?1)")?;
    let mut fail_ins = tx.prepare(
        "INSERT OR REPLACE INTO run_fail(run_id, case_id, detail_id)
         SELECT ?1,
                COALESCE((SELECT id FROM case_key WHERE key = ?2), 0),
                (SELECT id FROM fail_detail WHERE text = ?3)",
    )?;
    rows.iter()
        .try_fold((), |(), (status, key, detail)| -> QResult<()> {
            case_ins.execute(params![key])?;
            (status != "PASS")
                .then(|| {
                    detail_ins.execute(params![detail])?;
                    fail_ins.execute(params![ref_run, key, detail])
                })
                .transpose()?;
            Ok(())
        })?;
    drop(case_ins);
    drop(detail_ins);
    drop(fail_ins);
    tx.commit()?;
    info!(run = name, total, pass, "ledger snapshot");
    Ok((total, pass))
}

/// 把内存里的若干行按一个 note topic 存下来（solver 的 dump 走这里，而不是再长出一份
/// `reports/*.tsv`）。
pub fn note_store(topic: &str, db_path: &Path, lines: &[String]) -> QResult<usize> {
    let conn = Connection::open(db_path)?;
    migrate(&conn)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM note WHERE topic = ?1", params![topic])?;
    let mut ins =
        tx.prepare("INSERT OR REPLACE INTO note(topic, ord, text) VALUES (?1, ?2, ?3)")?;
    let n = lines.iter().enumerate().fold(0usize, |n, (o, line)| {
        n + ins
            .execute(params![topic, o as i64, line])
            .map(|_| 1)
            .unwrap_or(0)
    });
    drop(ins);
    tx.commit()?;
    info!(topic, rows = n, "note stored");
    Ok(n)
}

/// 把叙述/矩阵文件导入 `note(topic, ord, text)` 表，这样连散文报告也不再是散落的文件。
pub fn note_import(conn: &Connection, args: &[String]) -> QResult<()> {
    let Some((topic, files)) = args.split_first() else {
        info!("usage: ledger note-import <topic> <file>...");
        return Ok(());
    };
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM note WHERE topic = ?1", params![topic])?;
    let mut ins =
        tx.prepare("INSERT OR REPLACE INTO note(topic, ord, text) VALUES (?1, ?2, ?3)")?;
    files
        .iter()
        .try_fold(0usize, |mut ord, f| -> QResult<usize> {
            let text = std::fs::read_to_string(f).unwrap_or_else(|e| {
                warn!(file = f, ?e, "unreadable note file");
                String::new()
            });
            ord = text.lines().fold(ord, |o, line| {
                ins.execute(params![topic, o as i64, line]).ok();
                o + 1
            });
            Ok(ord)
        })?;
    drop(ins);
    tx.commit()?;
    info!(topic, files = files.len(), "note imported");
    Ok(())
}

fn note_dump(conn: &Connection, args: &[String]) -> QResult<()> {
    let topic = args.first().cloned().unwrap_or_default();
    let mut st = conn.prepare("SELECT ord, text FROM note WHERE topic = ?1 ORDER BY ord")?;
    let rows = st.query_map(params![topic], |r| {
        let (o, t): (i64, String) = (r.get(0)?, r.get(1)?);
        Ok(format!("{o}\t{t}"))
    })?;
    rows.for_each(|line| {
        line.map(|l| info!(target: "ledger", "{l}")).ok();
    });
    Ok(())
}

fn import(conn: &Connection, args: &[String]) -> QResult<()> {
    let (run, files) = args
        .split_first()
        .map_or(("", args), |(r, f)| (r.as_str(), f));
    files.iter().enumerate().try_fold((), |(), (i, f)| {
        // 给一个 run 名却有多份文件时，每份文件沿用自己的名字（去掉 `hrx-` 与 `.tsv`），
        // 除非调用方只给了一个 run 名
        let name = if files.len() == 1 {
            run.to_string()
        } else {
            f.rsplit('/')
                .next()
                .unwrap_or(f)
                .trim_start_matches("hrx-")
                .trim_end_matches(".tsv")
                .to_string()
        };
        let _ = i;
        import_file(conn, &name, f).map(|_| ())
    })
}

pub(crate) fn run_id(conn: &Connection, name: &str) -> QResult<i64> {
    conn.query_row("SELECT id FROM run WHERE name=?1", params![name], |r| {
        r.get(0)
    })
}

/// 一次 A/B 判定，作为调用方能直接断言的**数据**——这与 [`failing`]、[`passing`] 一起被公开是同一个理由：
/// 只作为一条 `tracing` 行存在的判定得靠 grep，而 grep 日志正是 shell 里 `sort | comm` 那套集合代数
/// （以及喂给它的 `/tmp` 中间文件）复活的路径。
#[derive(Debug)]
pub struct Diff {
    /// 在 `b` 里失败、在 `a` 里通过 —— 每条都带着自己的失败详情
    pub regress: Vec<String>,
    /// 在 `a` 里失败、在 `b` 里通过
    pub net: Vec<String>,
    /// 每个 run 的失败集合大小，**在 scope 之内**；它正是证明“这次比较确实覆盖了东西”的那条读数：一个匹配不到
    /// 任何 key 的 scope 同样报 `regress=0 net=0`，而那不算通过。
    pub fails_a: usize,
    pub fails_b: usize,
}

/// 比较两个 run，可以只限定在一个目录簇（对 case key 用 `prefix`；空 = 整份记录）。
///
/// 这个前缀不是一个方便用的过滤器。本 change 的回归策略是*按簇限定 scope 的*（`tasks.md`：每个簇守住
/// 自己的基线，一个阶段只被要求不改动它拥有的那些簇），所以一次不带 scope 的 diff 回答的是计划从没问过
/// 的问题。
pub fn diff_runs(conn: &Connection, a: &str, b: &str, prefix: &str) -> QResult<Diff> {
    let (ra, rb) = (run_id(conn, a)?, run_id(conn, b)?);
    let pattern = format!("{prefix}%");
    let list = |sql: &str| -> QResult<Vec<String>> {
        let mut st = conn.prepare(sql)?;
        st.query_map(params![ra, rb, pattern], |r| r.get::<_, String>(0))?
            .collect()
    };
    let regress = list(
        "SELECT c.key || ' | ' || COALESCE(d.text, '')
         FROM run_fail f JOIN case_key c ON c.id = f.case_id
         LEFT JOIN fail_detail d ON d.id = f.detail_id
         WHERE f.run_id = ?2
           AND c.key LIKE ?3
           AND NOT EXISTS (SELECT 1 FROM run_fail g WHERE g.run_id = ?1 AND g.case_id = f.case_id)",
    )?;
    let net = list(
        "SELECT c.key
         FROM run_fail f JOIN case_key c ON c.id = f.case_id
         WHERE f.run_id = ?1
           AND c.key LIKE ?3
           AND NOT EXISTS (SELECT 1 FROM run_fail g WHERE g.run_id = ?2 AND g.case_id = f.case_id)",
    )?;
    let fails = |run: i64| -> QResult<usize> {
        let mut st = conn.prepare(
            "SELECT COUNT(*) FROM run_fail f JOIN case_key c ON c.id = f.case_id
             WHERE f.run_id = ?1 AND c.key LIKE ?2",
        )?;
        st.query_row(params![run, pattern], |r| r.get(0))
    };
    Ok(Diff {
        regress,
        net,
        fails_a: fails(ra)?,
        fails_b: fails(rb)?,
    })
}

fn diff(conn: &Connection, args: &[String]) -> QResult<()> {
    let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
        info!("usage: ledger diff <a> <b> [key-prefix]");
        return Ok(());
    };
    let prefix = args.get(2).cloned().unwrap_or_default();
    let verdict = diff_runs(conn, a, b, &prefix)?;
    verdict
        .regress
        .iter()
        .for_each(|k| info!(target: "ledger", "REGRESS {k}"));
    verdict
        .net
        .iter()
        .for_each(|k| info!(target: "ledger", "NET {k}"));
    info!(
        a,
        b,
        scope = if prefix.is_empty() {
            "*"
        } else {
            prefix.as_str()
        },
        fails_a = verdict.fails_a,
        fails_b = verdict.fails_b,
        regress = verdict.regress.len(),
        net = verdict.net.len(),
        "AB diff"
    );
    Ok(())
}

fn runs(conn: &Connection) -> QResult<()> {
    let mut st = conn.prepare(
        "SELECT name, total, pass, ROUND(100.0 * pass / NULLIF(total, 0), 2),
                (SELECT COUNT(*) FROM run_fail f WHERE f.run_id = run.id), ts
         FROM run ORDER BY id",
    )?;
    let mut rows = st.query_map([], |r| {
        let (name, total, pass, rate, fails, ts): (String, i64, i64, Option<f64>, i64, String) = (
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        );
        Ok(format!(
            "{name}\t{pass}/{total}\t{}%\tfails={fails}\t{ts}",
            rate.unwrap_or_default()
        ))
    })?;
    rows.try_fold((), |(), line| {
        line.map(|l: String| info!(target: "ledger", "{l}"))
    })
}

/// 列出某个 run 的失败。每个 pattern 都匹配 **case key 或失败详情**，而且**全部** needle 必须命中同一行
/// ——于是一个簇可以按*期望结果*分桶，不需要 shell 管道。拿这条命令：
/// `ledger failures <run> core_functions/color/hwb "expected error"`
/// 它问的就是“hwb 的失败里有多少条想要一句诊断”，而这个划分决定这一片是能力还是消息。
///
/// 单个 needle 也需要详情那一侧，因为那正是 `FAIL → FAIL` 的盲区：pass 数看不见“一句自己发明的
/// `Undefined variable` 变成了另一种错值”，所以 `ledger failures <run> "unexpected error"` 必须能直接从
/// 记录里数出来（harness 已经不再留一份可 grep 的 TSV）。
fn failures(conn: &Connection, args: &[String]) -> QResult<()> {
    let (run, needles, count) = failure_query(args);
    let rows = failing_any(conn, &run, &needles)?;
    // `--count` 之所以存在，是因为“多少行匹配这些 needle”正是某一步被闸住的那个读数
    // （`ledger failures <run> "unexpected error"` 必须等于记下的基线），而靠 `grep -c` 数行这件事，在
    // 日志带上样式之后已经被读错过两回。一个数字，不走管道。
    if count {
        info!(target: "ledger", "{}", rows.len());
        return Ok(());
    }
    rows.iter()
        .for_each(|line| info!(target: "ledger", "{line}"));
    Ok(())
}

/// `ledger failures` 被要求做什么：`(run, needles, count-only)`。两个以上 needle 按 AND 合并；空列表
/// 意思是“这个 run 的全部失败”。
///
/// `--count` 是一个开关参数，永远不是一个 pattern。把它当 needle 读不会报错——它会答“零条失败”，而那正是本
/// crate 已经被烧过两回的“悄悄错”的形状，所以这个切分住在这里，好让测试把它钉住。
pub fn failure_query(args: &[String]) -> (String, Vec<String>, bool) {
    let count = args.iter().any(|arg| arg == "--count");
    let given = args
        .iter()
        .filter(|arg| arg.as_str() != "--count")
        .cloned()
        .collect::<Vec<String>>();
    (
        given.first().cloned().unwrap_or_default(),
        given.iter().skip(1).cloned().collect::<Vec<String>>(),
        count,
    )
}

/// 列出在一个簇前缀下记录过、而这个 run **没有**失败的 key——一个行为改动*在写之前*需要的判定清单，
/// 因为一个本来就通过的用例恰好就是过于热心的新规则最容易撞坏的东西（`values/calculation/calc/no_operator::function/css`
/// 今天打印 `calc(c())`，语料也认同，所以一条把那个壳“简化”掉的 calc 规则会是一场单靠 pass 数指不出来的回归）。
///
/// 前缀是强制的。一个 run 的*覆盖范围*没在任何地方记录（范围内的 PASS 只进 key 字典，不长出行），所以一份
/// 全账本的“通过集合”只能把“已知但在这里不失败”读成“通过”，而一个 run 从没量过的用例与它通过的用例长得一模一样。
/// 点名簇就把 scope 这个问题交给调用方，那里有语料可答。
fn passed(conn: &Connection, args: &[String]) -> QResult<()> {
    let (Some(run), Some(prefix)) = (args.first(), args.get(1)) else {
        info!("usage: ledger passed <run> <key-prefix>");
        return Ok(());
    };
    passing(conn, run, prefix)?
        .iter()
        .for_each(|key| info!(target: "ledger", "{key}"));
    Ok(())
}

/// 在 `prefix` 下记录过的 key（`case_key` 存着任何 run 列过的每一个 key，PASS 也在），减去这个 run 失败的。
/// 为了测试而公开，理由与 [`failing`] 相同：一条量具悄悄返回空，不能误读成“没有东西通过”。
pub fn passing(conn: &Connection, run: &str, prefix: &str) -> rusqlite::Result<Vec<String>> {
    let rid = run_id(conn, run)?;
    let pattern = format!("{prefix}%");
    let mut st = conn.prepare(
        "SELECT c.key FROM case_key c
         WHERE c.id != 0 AND c.key LIKE ?2
           AND NOT EXISTS (SELECT 1 FROM run_fail f WHERE f.run_id = ?1 AND f.case_id = c.id)
         ORDER BY c.key",
    )?;
    st.query_map(params![rid, pattern], |r| r.get::<_, String>(0))?
        .collect()
}

/// 某个 run 里 key **或**详情含 `needle` 的失败（空 = 全部）。pattern 在这里拼，而不是用 SQL 侧的
/// `'%' || ?n`：那种写法下每一次带过滤的查询都返回零行——连未过滤列表里明摆着存在的子串也一样——而
/// `line.map(..).ok()` 把错误藏了起来，于是一条坏掉的查询和“没有这种失败”无从区分。已对着 harness 的测量
/// 验证过：`failing(<baseline>, "unexpected error")` = 55，正是快照给过的那个数。做成公开函数，好让测试
/// 断言行本身而不是解析日志输出。
pub fn failing(conn: &Connection, run: &str, needle: &str) -> rusqlite::Result<Vec<String>> {
    failing_any(conn, run, &[needle.to_string()])
}

/// 同一批行，按给出的**每一个** needle 过滤，把 `"key | detail"` 当作一整块干草堆来匹配。
///
/// 第一个 needle 进 SQL（于是常见的单 pattern 场景仍由索引驱动）；其余在这里施加。匹配时两侧都转小写，
/// 因为 SQL `LIKE` 对 ASCII 大小写不敏感——第二个 needle 若与第一个行为不同，就会让 `failures <run> A B`
/// 依赖参数的顺序，而那正是一个读数悄悄换掉含义的方式。
pub fn failing_any(
    conn: &Connection,
    run: &str,
    needles: &[String],
) -> rusqlite::Result<Vec<String>> {
    let rid = run_id(conn, run)?;
    let first = needles.first().cloned().unwrap_or_default();
    let pattern = format!("%{first}%");
    let mut st = conn.prepare(
        "SELECT c.key || ' | ' || COALESCE(d.text, '')
         FROM run_fail f JOIN case_key c ON c.id = f.case_id
         LEFT JOIN fail_detail d ON d.id = f.detail_id
         WHERE f.run_id = ?1 AND (c.key LIKE ?2 OR COALESCE(d.text, '') LIKE ?3)",
    )?;
    let rows = st
        .query_map(params![rid, pattern, pattern], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(needles.iter().skip(1).fold(rows, |kept, needle| {
        let needle = needle.to_lowercase();
        kept.into_iter()
            .filter(|row| row.to_lowercase().contains(&needle))
            .collect()
    }))
}
