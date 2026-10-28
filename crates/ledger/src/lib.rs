//! hrx ledger — one SQLite table set for all sass-spec A/B snapshots,
//! replacing the per-round `reports/hrx-*.tsv` sprawl (262 files / 154 MB).
//!
//! **The database is the record, and it stays local**: `reports/hrx.db` is gitignored because a
//! multi-megabyte binary would be rewritten into history on every import. A fresh clone therefore starts
//! with an empty ledger; the conclusions of past runs live in `openspec/.../tasks.md` (which names the
//! run, the NET/REGRESS count and the cases that moved), and each new increment re-imports from there.
//! Do not go back to loose snapshot files, and do not open a second database: import every harness run
//! into this one under a name that says what it measured (`ledger runs` lists them, `ledger diff A B`
//! compares two).
//!
//! Model: PASS rows are implicit. `case` is the global key dictionary, `run`
//! records one snapshot (name/total/pass), `run_fail` stores only the failing
//! set per run. A/B semantics fall straight out of set differences:
//!   regression = fails(b) − fails(a);  net gain = fails(a) − fails(b).
//!
//! RFP contract: argument handling and row folds are
//! combinator chains; all output flows through `tracing` (no println).
//!
//! Usage:
//!   ledger import <run-name> <file.tsv> [more.tsv...]   (run-name applies to all)
//!   ledger diff <a> <b> [key-prefix]                        (regressions / net, per cluster if given)
//!   ledger changed <a> <b> [key-prefix]                 (failing BOTH runs with a different detail: the
//!                                                         shape change `regress=0` cannot see)
//!   ledger runs                                           (summary table)
//!   ledger failures <run> [key-or-detail-substring] [--count]
//!   ledger passed   <run> <key-prefix>
//!
//! `passed` exists because the storage is asymmetric: only failures are rows, so “which cases of this
//! cluster PASS right now” has no direct representation. It answers **for one named cluster**: the keys
//! recorded somewhere under that prefix minus the ones this run fails. The prefix is required, not
//! cosmetic — the only alternative universe is “every key the ledger has ever seen”, which silently
//! reports a case as passing when the run never measured it at all (measured: `passed
//! head-600-pathkey operators/` returns 29 keys, and that harness did not scan `operators` until 0.6).

use rusqlite::params;
use rusqlite::{Connection, Result as QResult};
use std::path::Path;
use tracing::{info, warn};

mod changed;
pub use changed::changed_details;

/// Default ledger location, relative to the repository root.
pub const DB: &str = "reports/hrx.db";

/// CLI dispatch over already-split arguments (the binary owns subscriber setup).
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

/// Record one live runner snapshot into the ledger (shared schema with the
/// CLI; the hrx runner calls this instead of growing another `reports/*.tsv`).
pub fn record(name: &str, db_path: &Path, rows: &[String]) -> QResult<(usize, usize)> {
    let conn = Connection::open(db_path)?;
    migrate(&conn)?;
    import_rows(&conn, name, rows)
}

/// Parse one `STATUS\tkey[\tdetail]` snapshot file into the ledger.
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
    // A re-import replaces the whole verdict for this run: only failures are stored, so without
    // this delete a case that now PASSES keeps its old row and `diff` reports a phantom REGRESS.
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

/// Store in-memory lines as a note topic (solver dumps write through here
/// instead of growing a `reports/*.tsv`).
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

/// Import narrative/matrix files into the `note(topic, ord, text)` table so
/// even the prose reports stop living as loose files.
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
        // with several files and one run name, each file keeps its own name
        // stripped of `hrx-` / `.tsv` unless the caller gave exactly one run
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

/// One A/B verdict, as **data** a caller can assert on — the same reason [`failing`] and [`passing`] are
/// exposed: a verdict that only exists as a `tracing` line has to be grepped, and grepping a log is how
/// `sort | comm` set algebra in the shell (and a `/tmp` intermediate file to feed it) comes back to life.
#[derive(Debug)]
pub struct Diff {
    /// failing in `b`, passing in `a` — each with its failure detail
    pub regress: Vec<String>,
    /// failing in `a`, passing in `b`
    pub net: Vec<String>,
    /// the failing-set size of each run **inside the scope**, which is what proves the comparison covered
    /// anything: a scope that matches no key also reports `regress=0 net=0`, and that is not a pass.
    pub fails_a: usize,
    pub fails_b: usize,
}

/// Compare two runs, optionally restricted to one directory cluster (`prefix` on the case key; empty =
/// the whole record).
///
/// The prefix is not a convenience filter. This change's regress policy is *scoped by cluster*
/// (`tasks.md`: each cluster keeps its own baseline and a stage is only required not to move the clusters it
/// owns), so an unscoped diff answers a question the plan never asks.
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

/// List one run's failures. Each pattern matches the **case key or the failure detail**, and **all** of
/// them must match the same row — so a cluster can be bucketed by *expected outcome* without a shell
/// pipeline: `ledger failures <run> core_functions/color/hwb "expected error"` is "how many of hwb's
/// failures want a diagnostic", the split that decides whether a slice is a capability or a message.
///
/// One needle still needs the detail side, because that is the `FAIL → FAIL` blind spot: a pass count cannot
/// see that an invented `Undefined variable` became a different wrong answer, so
/// `ledger failures <run> "unexpected error"` has to be countable straight from the record (the harness no
/// longer keeps a TSV to grep).
fn failures(conn: &Connection, args: &[String]) -> QResult<()> {
    let (run, needles, count) = failure_query(args);
    let rows = failing_any(conn, &run, &needles)?;
    // `--count` exists because "how many rows match these needles" is the reading a step is gated on
    // (`ledger failures <run> "unexpected error"` must equal the recorded baseline), and counting lines
    // with `grep -c` was misread twice when the log styling got in the way. One number, no pipeline.
    if count {
        info!(target: "ledger", "{}", rows.len());
        return Ok(());
    }
    rows.iter()
        .for_each(|line| info!(target: "ledger", "{line}"));
    Ok(())
}

/// What `ledger failures` was asked for: `(run, needles, count-only)`. Two or more needles are combined with
/// AND; an empty list means "every failure of this run".
///
/// `--count` is a flag and never a pattern. Reading it as the needle would not fail — it would answer
/// “zero failures”, the exact shape of silent wrongness this crate has been burned by twice, so the split
/// lives here where a test can pin it.
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

/// List the recorded keys under one cluster prefix that this run does **not** fail — the verdict list a
/// behaviour change needs *before* it is written, because a case that already passes is exactly what an
/// over-eager new rule destroys (`values/calculation/calc/no_operator::function/css` today prints
/// `calc(c())` and the corpus agrees, so a calculation rule that “simplified” that shell away would be a
/// regression the pass count alone cannot point at).
///
/// The prefix is mandatory. A run's *scope* is not recorded anywhere (an in-scope PASS only enters the
/// key dictionary, it grows no row), so a whole-ledger “passing set” would have to read
/// “known-but-not-failing-here” as “passed”, and a case a run never measured looks identical to one it
/// passed. Naming the cluster puts the scope question on the caller, where the corpus can answer it.
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

/// Recorded keys (`case_key` holds every key any run listed, PASS included) under `prefix` that this run
/// does not fail. Exposed for tests for the same reason [`failing`] is: a measuring query that silently
/// returns nothing must not be mistaken for “nothing passes”.
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

/// One run's failures whose key **or** detail contains `needle` (empty = all). The pattern is built here
/// rather than with SQL-side `'%' || ?n`: with that form every filtered lookup returned zero rows —
/// including a substring plainly present in the unfiltered listing — and `line.map(..).ok()` hid the
/// error, so a broken query was indistinguishable from “no such failure”. Verified against the harness
/// measurement: `failing(<baseline>, "unexpected error")` = 55, the number the snapshot had given.
/// Exposed as a function so a test can assert rows instead of parsing log output.
pub fn failing(conn: &Connection, run: &str, needle: &str) -> rusqlite::Result<Vec<String>> {
    failing_any(conn, run, &[needle.to_string()])
}

/// The same rows filtered by **every** needle given, matched against `"key | detail"` as one haystack.
///
/// The first needle goes into SQL (so the query stays index-driven for the common single-pattern case); the
/// rest are applied here. Matching lowercases both sides because SQL `LIKE` is case-insensitive for ASCII —
/// a second needle that behaved differently from the first would make `failures <run> A B` depend on the
/// order of its arguments, which is how a reading silently changes meaning.
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
