//! Coverage — does the lesson ledger own every teachable line, exactly once?

use std::collections::{BTreeMap, BTreeSet};

use crate::inventory::FileReport;

/// One ledger row. `kind` is `主` (primary owner) or `复习` (a later lesson revisiting it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub lesson: String,
    pub kind: String,
}

/// Parse `path<TAB>start-end<TAB>lesson<TAB>kind` rows.
pub fn parse_ledger(text: &str) -> Vec<Row> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let path = fields.next()?;
            let span = fields.next()?;
            let lesson = fields.next()?;
            let kind = fields.next().unwrap_or("主");
            let (start, end) = span.split_once('-')?;
            Some(Row {
                path: path.to_string(),
                start: start.parse().ok()?,
                end: end.parse().ok()?,
                lesson: lesson.to_string(),
                kind: kind.to_string(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub teachable: usize,
    pub assigned: usize,
    pub unassigned: Vec<String>,
    pub duplicates: Vec<String>,
}

impl Report {
    pub fn clean(&self) -> bool {
        self.unassigned.is_empty() && self.duplicates.is_empty()
    }
}

/// Compare primary ownership against the teachable set. A line covered by two `主` rows is a
/// duplicate even when nothing is unassigned, because the ledger then lies about who teaches it.
pub fn check(items: &BTreeMap<String, FileReport>, rows: &[Row]) -> Report {
    // `path -> teachable lines` as sets, so membership tests stay cheap on a 30k-line source tree.
    let sets: BTreeMap<&str, BTreeSet<usize>> = items
        .iter()
        .map(|(path, report)| (path.as_str(), report.teachable.iter().copied().collect()))
        .collect();
    let key = |path: &str, line: usize| format!("{path}\t{line}");

    let owners = rows.iter().filter(|row| row.kind == "主").fold(BTreeMap::new(), |mut acc: BTreeMap<String, usize>, row| {
        let Some(teachable) = sets.get(row.path.as_str()) else { return acc };
        (row.start..=row.end)
            .filter(|line| teachable.contains(line))
            .for_each(|line| *acc.entry(key(&row.path, line)).or_insert(0) += 1);
        acc
    });

    let (teachable, assigned, unassigned, duplicates) = items.values().fold(
        (0usize, 0usize, Vec::new(), Vec::new()),
        |(mut teachable, mut assigned, mut unassigned, mut duplicates), report| {
            let owned = |line: usize| owners.get(&key(&report.path, line));
            teachable += report.teachable.len();
            assigned += report.teachable.iter().filter(|line| owned(**line).is_some()).count();
            unassigned.extend(
                report
                    .teachable
                    .iter()
                    .filter(|line| owned(**line).is_none())
                    .map(|line| format!("{}:{}", report.path, line)),
            );
            duplicates.extend(report.teachable.iter().filter_map(|line| {
                owned(*line).filter(|count| **count > 1).map(|count| format!("{}:{} x{}", report.path, line, count))
            }));
            (teachable, assigned, unassigned, duplicates)
        },
    );

    Report { teachable, assigned, unassigned, duplicates }
}
