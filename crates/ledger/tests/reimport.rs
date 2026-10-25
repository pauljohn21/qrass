//! Re-importing a run must replace its verdict wholesale. Only failures are stored, so a stale
//! `run_fail` row left over from an earlier snapshot turns a fixed case into a phantom REGRESS in
//! `ledger diff` — which would make the project's main safety net lie.

use std::path::PathBuf;

use ledger::record;

fn temp_db(tag: &str) -> PathBuf {
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("ledger-test-{tag}-{pid}.sqlite"));
    let _ = std::fs::remove_file(&path);
    path
}

fn fails(conn: &rusqlite::Connection, run: &str) -> Vec<String> {
    conn.prepare(
        "SELECT c.key FROM run_fail f JOIN case_key c ON c.id = f.case_id JOIN run r ON r.id = f.run_id WHERE r.name = ?1",
    )
    .unwrap()
    .query_map(rusqlite::params![run], |r| r.get::<_, String>(0))
    .unwrap()
    .collect::<Result<Vec<String>, _>>()
    .unwrap()
}

#[test]
fn a_case_that_now_passes_stops_being_reported_as_failed() {
    let db = temp_db("fixed");
    record(
        "r1",
        &db,
        &[
            "PASS\tkept".to_string(),
            "FAIL\tfixed\tsome detail".to_string(),
        ],
    )
    .expect("first snapshot records");
    let conn = rusqlite::Connection::open(&db).unwrap();
    assert_eq!(fails(&conn, "r1"), vec!["fixed".to_string()]);
    drop(conn);

    // Same run name, second snapshot: the case is fixed, so nothing may remain failed.
    record(
        "r1",
        &db,
        &["PASS\tkept".to_string(), "PASS\tfixed".to_string()],
    )
    .expect("re-import records");
    let conn = rusqlite::Connection::open(&db).unwrap();
    assert_eq!(
        fails(&conn, "r1"),
        Vec::<String>::new(),
        "re-import must clear the stale failure row"
    );
    let (_total, pass): (i64, i64) = conn
        .query_row("SELECT total, pass FROM run WHERE name='r1'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(pass, 2, "the run row carries the newest snapshot");
    drop(conn);
    let _ = std::fs::remove_file(&db);
}

#[test]
fn a_case_that_newly_fails_is_still_recorded() {
    let db = temp_db("broke");
    record("r1", &db, &["PASS\ta".to_string()]).unwrap();
    record("r1", &db, &["FAIL\ta\tnew detail".to_string()]).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    assert_eq!(fails(&conn, "r1"), vec!["a".to_string()]);
    drop(conn);
    let _ = std::fs::remove_file(&db);
}
