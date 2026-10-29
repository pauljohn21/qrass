//! The pipeline's own diagnostic shell — the only place that decides *what is observable*, so no other
//! module imports `tracing` types beyond macros.
//!
//! Two levels, because they answer different questions:
//! * [`Phase`] — one compile phase, counted in items and milliseconds. It has to outlive `subscribe`,
//!   which is why the handle is carried inside the phase's observer (see `crate::phases`);
//! * [`ValueRead`] — one declaration's value: what the source said, what it became, and whether this
//!   compiler read it at all. That is the question every conformance round ends up asking ("why did this
//!   line stay verbatim?"), and answering it today means adding a throwaway probe and rebuilding.
//!
//! Both are opened at `DEBUG`, so a normal run pays only the field borrows and `RUST_LOG` decides.
//!
//! The field *list* is the pipeline convention in `.qoder/rules/rust.md` and is not restated here; what is
//! settled here is **what each field refers to on each surface**, because a log line is only searchable if
//! the same name always means the same thing:
//! * a `phase` span — `stage` is the phase, `module` the code module whose work it drives, `items` and
//!   `elapsed_ms` its output and duration. It carries no `id`: there is exactly one of each phase per
//!   compile, so the identity that names *which program* is on the enclosing `compile` span
//!   ([`source_label`]) and the four phases nest under it;
//! * a `value` span — `module` is the property or variable the value belongs to and `id` the block it sits
//!   in, alongside `raw_value`/`resolved`, the pair that shows a decline as a decline rather than as an
//!   empty string;
//! * the decision events (`call`, `calculation read`) — `module` is the sub-stage speaking, and
//! * the loss warning in [`crate::exec`] — `module` stays the code module, while the selectors it lost go
//!   in `dropped`/`inside` rather than borrowing `module`/`id`.
//!
//! Two surfaces do share `module` with different referents — a phase's code module and a value's owning
//! property — because they answer different questions (`stage`+`module` joins the pipeline phases, the
//! value pair asks about one line of CSS), and that is stated rather than left implicit. No surface puts a
//! *selector* in `module`: those have their own names, and a field whose meaning changes per line is a
//! field nobody can grep.

/// The label a stream is named by in a trace: its first non-blank line, cut short. Nothing else about the
/// source is invented here, so an empty program says so instead of printing a blank field.
pub(crate) fn source_label(program: &str) -> String {
    let first = program.lines().find(|line| !line.trim().is_empty());
    first.map_or_else(
        || "<empty>".to_string(),
        |line| line.trim().chars().take(40).collect::<String>(),
    )
}

/// One compile phase: the span plus the clock that starts when the observer is created.
pub(crate) struct Phase {
    span: tracing::Span,
    started: std::time::Instant,
}

impl Phase {
    /// `stage` is the phase in the pipeline, `module` the one whose code does the work — the two differ,
    /// because `Render` serialises what `crate::exec` built.
    pub(crate) fn new(stage: &'static str, module: &'static str) -> Self {
        Self {
            span: tracing::debug_span!(
                "phase",
                stage,
                module,
                items = tracing::field::Empty,
                elapsed_ms = tracing::field::Empty
            ),
            started: std::time::Instant::now(),
        }
    }

    /// The span to work under, so what a phase emits nests inside it.
    pub(crate) fn enter(&self) -> tracing::span::Entered<'_> {
        self.span.enter()
    }

    /// End the phase: record its output and duration, then release this handle, which is what closes the
    /// span. Swapping in the empty span also makes a second call (a phase that failed *and* completed)
    /// a no-op instead of a double close.
    ///
    /// The duration is a fractional millisecond: the mandated field name is `elapsed_ms`, but whole
    /// milliseconds would report every small phase as `0`, which reads as "instantaneous" and is a lie.
    pub(crate) fn close(&mut self, items: usize) {
        self.span.record("items", items as i64);
        self.span.record("elapsed_ms", millis(self.started));
        self.span = tracing::Span::none();
    }
}

/// A duration in fractional milliseconds, the unit the pipeline convention names.
fn millis(since: std::time::Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1_000.0
}

/// One declaration value's read. Closing happens on drop, so the `?` paths are recorded too — a failure
/// is exactly the case a diagnostic must not swallow.
pub(crate) struct ValueRead {
    span: tracing::Span,
    started: std::time::Instant,
}

impl ValueRead {
    /// `module` is what the value belongs to (a property name, or the variable a binding names), `id`
    /// the block it sits in, `raw_value` the source text of the run.
    pub(crate) fn begin(module: &str, id: &str, raw_value: &str) -> Self {
        Self {
            span: tracing::debug_span!(
                "value",
                module,
                id,
                raw_value,
                resolved = tracing::field::Empty,
                elapsed_ms = tracing::field::Empty
            ),
            started: std::time::Instant::now(),
        }
    }

    /// What the read produced: a value, an explicit decline, or a failure that travels the `Err` channel.
    pub(crate) fn record(
        self,
        outcome: &Result<Option<crate::value::Value>, crate::SassError>,
    ) -> Self {
        let resolved = match outcome {
            Ok(Some(value)) => value.to_css(),
            Ok(None) => "<declined: kept verbatim>".to_string(),
            Err(error) => format!("<error: {}>", error.message),
        };
        self.span.record("resolved", resolved.as_str());
        self
    }
}

impl Drop for ValueRead {
    fn drop(&mut self) {
        self.span.record("elapsed_ms", millis(self.started));
    }
}
