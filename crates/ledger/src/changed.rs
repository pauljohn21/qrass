//! The A/B reading `diff` cannot give: cases that fail in **both** runs with a *different* detail.
//!
//! `diff` answers "what started or stopped failing". A row that fails before and after moves neither
//! count, yet its failure can change kind — an honest verbatim turning into a wrong value, or a wrong
//! value turning into an error the corpus never stated. The project has been caught by exactly that: one
//! batch reported `regress=0 net=11` while its own cluster had gone from "kept the source text" to
//! "computed the value the golden says the user's own function should return", and another added six
//! invalid errors behind the same clean reading. So the comparison is a command, not a habit of
//! eyeballing two listings: `ledger changed <a> <b> [key-prefix]`.

use rusqlite::params;
use rusqlite::{Connection, Result as QResult};
use tracing::info;

/// A shape comparison as data a caller can assert on: the differing lines, plus **how many rows failed in
/// each run inside the scope**. The counts are not decoration — without them `changed = 0` cannot be told
/// apart from "the scope matched nothing", the same confusion [`crate::Diff::fails_a`]/`fails_b` exist for.
#[derive(Debug)]
pub struct Changed {
    pub rows: Vec<String>,
    pub fails_a: usize,
    pub fails_b: usize,
}

/// Keys failing in both runs whose stored detail differs, as `key | a: <detail> | b: <detail>` lines,
/// scoped by key prefix (empty = the whole record) exactly like [`crate::diff_runs`] so a scoped
/// verification cannot silently be reading nothing.
pub fn changed_details(conn: &Connection, a: &str, b: &str, prefix: &str) -> QResult<Changed> {
    let (ra, rb) = (crate::run_id(conn, a)?, crate::run_id(conn, b)?);
    let pattern = format!("{prefix}%");
    let mut st = conn.prepare(
        "SELECT c.key || ' | a: ' || COALESCE(da.text, '') || ' | b: ' || COALESCE(db.text, '')
         FROM run_fail fa
         JOIN run_fail fb ON fb.run_id = ?2 AND fb.case_id = fa.case_id
         JOIN case_key c ON c.id = fa.case_id
         LEFT JOIN fail_detail da ON da.id = fa.detail_id
         LEFT JOIN fail_detail db ON db.id = fb.detail_id
         WHERE fa.run_id = ?1
           AND c.key LIKE ?3
           AND COALESCE(da.text, '') <> COALESCE(db.text, '')
         ORDER BY c.key",
    )?;
    let rows = st
        .query_map(params![ra, rb, pattern], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<String>, _>>()?;
    let mut counting = conn.prepare(
        "SELECT COUNT(*) FROM run_fail f JOIN case_key c ON c.id = f.case_id
         WHERE f.run_id = ?1 AND c.key LIKE ?2",
    )?;
    let mut fails_in =
        |run: i64| -> QResult<usize> { counting.query_row(params![run, pattern], |r| r.get(0)) };
    Ok(Changed {
        rows,
        fails_a: fails_in(ra)?,
        fails_b: fails_in(rb)?,
    })
}

/// `ledger changed <a> <b> [key-prefix]`: one line per shape-changed failure, then the count **beside
/// both scope totals** — a `0` has to be distinguishable from "the scope matched nothing", which is how
/// `diff`'s clean verdict was once misread.
pub fn run_cli(conn: &Connection, args: &[String]) -> QResult<()> {
    let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
        info!("usage: ledger changed <a> <b> [key-prefix]");
        return Ok(());
    };
    let prefix = args.get(2).cloned().unwrap_or_default();
    let changed = changed_details(conn, a, b, &prefix)?;
    changed
        .rows
        .iter()
        .for_each(|line| info!(target: "ledger", "CHANGED {line}"));
    info!(
        target: "ledger",
        a,
        b,
        scope = if prefix.is_empty() { "*" } else { prefix.as_str() },
        fails_a = changed.fails_a,
        fails_b = changed.fails_b,
        changed = changed.rows.len(),
        "failure-shape diff"
    );
    Ok(())
}
