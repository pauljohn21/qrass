//! Plan — turn the teachable inventory plus a volume/file map into the lesson ledger.
//!
//! The ledger is what makes "explain every line" checkable: every teachable line gets exactly one
//! owning lesson, so a missing lesson shows up as *unassigned lines* instead of a vague "we'll get
//! to that". Small files are packed into one lesson, so the lesson count follows the curriculum's
//! size band rather than the file count.

use std::collections::BTreeMap;

use crate::inventory::FileReport;

/// One row of `MAP.tsv`: which volume teaches which file, and in what order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub volume: String,
    pub order: u32,
    pub path: String,
}

/// A contiguous span of source lines owned by one lesson.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub lesson: String,
}

/// Parse `volume<TAB>order<TAB>path` rows, ignoring blank lines and `#` comments.
pub fn parse_map(text: &str) -> Vec<Entry> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (volume, rest) = line.split_once('\t')?;
            let (order, path) = rest.split_once('\t')?;
            Some(Entry { volume: volume.to_string(), order: order.parse().ok()?, path: path.to_string() })
        })
        .collect()
}

/// The lesson being filled: where it came from, how many teachable lines it already holds.
#[derive(Debug, Clone)]
struct Lesson {
    volume: String,
    label: String,
    used: usize,
}

impl Lesson {
    fn open(volume: &str, seq: u32) -> Lesson {
        Lesson { volume: volume.to_string(), label: format!("{volume}-{seq:02}"), used: 0 }
    }

    /// Would adding `count` lines overflow the budget? A lesson is never split when empty, so a
    /// single big file still gets a lesson rather than being dropped.
    fn overflows(&self, target: usize, count: usize) -> bool {
        self.used > 0 && self.used + count > target
    }
}

/// One `(start, end, teachable count)` span inside one file.
type Span = (usize, usize, usize);

/// State threaded through the labelling fold: what has been emitted, which lesson number each
/// volume is on, and the lesson currently being filled.
#[derive(Default)]
struct Packing {
    segments: Vec<Segment>,
    seqs: BTreeMap<String, u32>,
    open: Option<Lesson>,
}

/// Assign every mapped file's teachable spans to lessons, packing until `target` lines are used.
///
/// `target` is the preferred teachable lines per lesson; `max` bounds a single span, so no lesson
/// becomes an unreadable page (curriculum spec: 200–600 lines).
pub fn plan(items: &BTreeMap<String, FileReport>, map: &[Entry], target: usize, max: usize) -> Vec<Segment> {
    let mut order: Vec<&Entry> = map.iter().collect();
    order.sort_by(|a, b| (a.volume.as_str(), a.order, &a.path).cmp(&(b.volume.as_str(), b.order, &b.path)));

    // Flatten to (volume, path, span) first so the labelling pass is a single fold over owned data.
    let spans: Vec<(String, String, Span)> = order
        .iter()
        .flat_map(|entry| {
            let report = items.get(&entry.path);
            let spans = report.map_or(Vec::new(), |report| chunks(&report.teachable, target.max(1), max.max(1)));
            spans.into_iter().map(move |span| (entry.volume.clone(), entry.path.clone(), span))
        })
        .collect();

    let packed = spans.iter().fold(Packing::default(), |mut state, (volume, path, (start, end, count))| {
        // Keep filling the current lesson only while it belongs to this volume and still fits.
        let reuse = state
            .open
            .as_ref()
            .is_some_and(|current| current.volume == *volume && !current.overflows(target, *count));
        if !reuse {
            let next = *state.seqs.get(volume).unwrap_or(&0) + 1;
            state.seqs.insert(volume.clone(), next);
            state.open = Some(Lesson::open(volume, next));
        }
        if let Some(current) = &mut state.open {
            current.used += count;
            let label = current.label.clone();
            state.segments.push(Segment { path: path.clone(), start: *start, end: *end, lesson: label });
        }
        state
    });
    packed.segments
}

/// Chunk ascending line numbers into `(first, last, count)` spans of at most `target` teachable
/// lines and at most `max` span, flushing as soon as either bound would be exceeded.
fn chunks(lines: &[usize], target: usize, max: usize) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let open = lines.iter().fold(None, |open: Option<Span>, &line| match open {
        None => Some((line, line, 1)),
        // Either bound tripped: close the span and start a fresh one at this line.
        Some((start, end, count)) if count >= target || line.saturating_sub(start) >= max => {
            spans.push((start, end, count));
            Some((line, line, 1))
        }
        Some((start, _, count)) => Some((start, line, count + 1)),
    });
    if let Some(span) = open {
        spans.push(span);
    }
    spans
}

/// Ledger rows: `path<TAB>start-end<TAB>lesson<TAB>主`.
pub fn rows(segments: &[Segment]) -> Vec<String> {
    segments
        .iter()
        .map(|s| format!("{}\t{}-{}\t{}\t主", s.path, s.start, s.end, s.lesson))
        .collect()
}
