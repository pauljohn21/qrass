//! qrass hrx ledger — one SQLite table set for all sass-spec A/B snapshots,
//! replacing the per-round `reports/hrx-*.tsv` sprawl (262 files / 154 MB).
//!
//! Model: PASS rows are implicit. `case` is the global key dictionary, `run`
//! records one snapshot (name/total/pass), `run_fail` stores only the failing
//! set per run. A/B semantics fall straight out of set differences:
//!   regression = fails(b) − fails(a);  net gain = fails(a) − fails(b).
//!
//! RFP contract (refactor-rfp-strict): argument handling and row folds are
//! combinator chains; all output flows through `tracing` (no println).
//!
//! Usage:
//!   ledger import <run-name> <file.tsv> [more.tsv...]   (run-name applies to all)
//!   ledger diff <a> <b>                                  (regressions / net)
//!   ledger runs                                           (summary table)
//!   ledger failures <run> [key-substring]

use rusqlite::params;
use rusqlite::{Connection, Result as QResult};
use std::path::Path;
use tracing::{info, warn};

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
        Some("runs") => runs(conn),
        Some("failures") => failures(conn, &argv[1..]),
        _ => {
            info!("usage: ledger import|diff|runs|failures [--db=<path>] ...");
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
            Some((status.trim().to_string(), key.trim().to_string(), detail.to_string()))
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
    let ref_run: i64 = tx.query_row("SELECT id FROM run WHERE name=?1", params![name], |r| r.get(0))?;
    let mut case_ins = tx.prepare("INSERT OR IGNORE INTO case_key(key) VALUES (?1)")?;
    let mut detail_ins = tx.prepare("INSERT OR IGNORE INTO fail_detail(text) VALUES (?1)")?;
    let mut fail_ins = tx.prepare(
        "INSERT OR REPLACE INTO run_fail(run_id, case_id, detail_id)
         SELECT ?1,
                COALESCE((SELECT id FROM case_key WHERE key = ?2), 0),
                (SELECT id FROM fail_detail WHERE text = ?3)",
    )?;
    rows.iter().try_fold((), |(), (status, key, detail)| -> QResult<()> {
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
    let mut ins = tx.prepare("INSERT OR REPLACE INTO note(topic, ord, text) VALUES (?1, ?2, ?3)")?;
    let n = lines.iter().enumerate().fold(0usize, |n, (o, line)| {
        n + ins.execute(params![topic, o as i64, line]).map(|_| 1).unwrap_or(0)
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
    let mut ins = tx.prepare("INSERT OR REPLACE INTO note(topic, ord, text) VALUES (?1, ?2, ?3)")?;
    files.iter().try_fold(0usize, |mut ord, f| -> QResult<usize> {
        let text = std::fs::read_to_string(f).unwrap_or_else(|e| {
            warn!(file = f, ?e, "unreadable note file");
            String::new()
        });
        ord = text
            .lines()
            .fold(ord, |o, line| {
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
        line.map(|l| info!(target: "qrass-ledger", "{l}")).ok();
    });
    Ok(())
}

fn import(conn: &Connection, args: &[String]) -> QResult<()> {
    let (run, files) = args.split_first().map_or(("", args), |(r, f)| (r.as_str(), f));
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

fn run_id(conn: &Connection, name: &str) -> QResult<i64> {
    conn.query_row("SELECT id FROM run WHERE name=?1", params![name], |r| r.get(0))
}

/// A/B verdict: regressions (failing in b, passing in a) and net gains.
fn diff(conn: &Connection, args: &[String]) -> QResult<()> {
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => {
            info!("usage: ledger diff <a> <b>");
            return Ok(());
        }
    };
    let (ra, rb) = (run_id(conn, &a)?, run_id(conn, &b)?);
    let list = |sql: &str| -> QResult<Vec<String>> {
        let mut st = conn.prepare(sql)?;
        let rows = st.query_map(params![ra, rb], |r| r.get::<_, String>(0))?;
        rows.collect()
    };
    let regress = list(
        "SELECT c.key || ' | ' || COALESCE(d.text, '')
         FROM run_fail f JOIN case_key c ON c.id = f.case_id
         LEFT JOIN fail_detail d ON d.id = f.detail_id
         WHERE f.run_id = ?2
           AND NOT EXISTS (SELECT 1 FROM run_fail g WHERE g.run_id = ?1 AND g.case_id = f.case_id)",
    )?;
    let net = list(
        "SELECT c.key
         FROM run_fail f JOIN case_key c ON c.id = f.case_id
         WHERE f.run_id = ?1
           AND NOT EXISTS (SELECT 1 FROM run_fail g WHERE g.run_id = ?2 AND g.case_id = f.case_id)",
    )?;
    regress.iter().for_each(|k| info!(target: "ledger", "REGRESS {k}"));
    net.iter().for_each(|k| info!(target: "ledger", "NET {k}"));
    info!(a, b, regress = regress.len(), net = net.len(), "AB diff");
    Ok(())
}

fn runs(conn: &Connection) -> QResult<()> {
    let mut st = conn.prepare(
        "SELECT name, total, pass, ROUND(100.0 * pass / NULLIF(total, 0), 2),
                (SELECT COUNT(*) FROM run_fail f WHERE f.run_id = run.id), ts
         FROM run ORDER BY id",
    )?;
    let mut rows = st.query_map([], |r| {
        let (name, total, pass, rate, fails, ts): (String, i64, i64, Option<f64>, i64, String) =
            (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?);
        Ok(format!("{name}\t{pass}/{total}\t{}%\tfails={fails}\t{ts}", rate.unwrap_or_default()))
    })?;
    rows.try_fold((), |(), line| line.map(|l: String| info!(target: "ledger", "{l}")))
}

fn failures(conn: &Connection, args: &[String]) -> QResult<()> {
    let run = args.first().cloned().unwrap_or_default();
    let needle = args.get(1).cloned().unwrap_or_default();
    let rid = run_id(conn, &run)?;
    let mut st = conn.prepare(
        "SELECT c.key || ' | ' || COALESCE(d.text, '')
         FROM run_fail f JOIN case_key c ON c.id = f.case_id
         LEFT JOIN fail_detail d ON d.id = f.detail_id
         WHERE f.run_id = ?1 AND c.key LIKE '%' || ?2",
    )?;
    let rows = st.query_map(params![rid, needle], |r| r.get::<_, String>(0))?;
    rows.for_each(|line| {
        line.map(|l| info!(target: "ledger", "{l}")).ok();
    });
    Ok(())
}
