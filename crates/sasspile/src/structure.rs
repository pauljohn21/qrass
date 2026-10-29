//! The **structural phase** — `Tok → Event`, decided on the delimiters only, with no tree and no
//! executor state.
//!
//! This is the phase that reads the braces, so it is also the phase that can say where a block ends. A
//! `@for` is therefore cut out **here** and handed on as one owned [`Event::For`], instead of [`crate::exec`]
//! having to discover its own extent — which is the shape the counterexample index bans (a buffer plus a
//! count of blocks still to close, in a phase that has no business knowing either). Depth is a number inside
//! this phase's own accumulator, the same shape [`crate::brackets::Run`] carries for parentheses, and the
//! kernel that drives it is `Structurize` in [`crate::phases`].
//!
//! Like [`crate::lexer`] this is a forward fold with its own accumulator; no observable type appears here.

use crate::brackets;
use crate::lexer::{Emit, Tok};

/// A structural event produced by the delimiter fold.
#[derive(Debug, Clone)]
pub enum Event {
    Open(Vec<Tok>),
    Decl {
        prop: Vec<Tok>,
        value: Vec<Tok>,
    },
    Close,
    /// A `@for` **block cut out by this phase**: the header as written, and every event between its `{`
    /// and its own `}`. The nesting lives in the value (a body event can itself be a `For`), never in a
    /// `children` field a later phase would have to walk — and, decisively, no later phase has to *count
    /// braces* to find out where the loop ends. Reading the header is still [`crate::loops`]'s job, so a
    /// `@for` this compiler does not execute comes back through [`Event::Open`] unchanged.
    For {
        head: Vec<Tok>,
        body: Vec<Event>,
    },
}

/// The shape a `@for` **block that is not executed** takes: opened with its header as the block's text, fed
/// with its body, closed. The declined header (`exec::State::unroll`) and an end of input that never closed
/// the brace ([`MsgAcc::finish`]) both go through this one construction, so "an unexecuted loop is an
/// ordinary block" is stated once instead of in two phases.
///
/// It is a **choice, not a corpus answer**, and said as such: the header then reads as a selector, which is
/// why `directives/for/for.hrx::unit/same` (wants five `b: Npx` rules) and `…::error/from_float` (wants
/// `Error: 1.5 is not an int.`) stay red. This compiler answers neither yet — the unit range is 1.3/P3.2 and
/// the message is P7.
pub(crate) fn as_block(head: Vec<Tok>, body: Vec<Event>) -> Vec<Event> {
    let mut events = vec![Event::Open(head)];
    events.extend(body);
    events.push(Event::Close);
    events
}

/// First structural pass: `Tok → Event` on delimiters only (no tree). Own accumulator.
#[derive(Default)]
pub struct MsgAcc {
    run: Vec<Tok>,
    /// How many `{` stand open. This phase is the one that reads the braces, so it is the one that can say
    /// where a block ends — and it says it **here**, as a number in its own accumulator (the same kind of
    /// fact [`crate::brackets::Run`] carries for parentheses), instead of leaving it to a later phase.
    depth: u32,
    /// The `@for` blocks currently being cut out, outermost first. They leave in the order they were met,
    /// each as one [`Event::For`], when `depth` comes back to the level its `{` stood at.
    cut: Vec<ForRun>,
}

/// One `@for` block whose closing brace has not arrived: the header and the body events found so far.
struct ForRun {
    head: Vec<Tok>,
    body: Vec<Event>,
    /// the depth **before** its `{` was consumed, so its own `}` is the first one that brings `depth` back
    /// to this number — an inner block's `}` returns to a deeper level and is therefore not it
    depth: u32,
}

impl MsgAcc {
    /// Feed one token, emitting 0..N structural events into `emit`.
    pub fn feed(&mut self, tok: Tok, emit: Emit<'_, Event>) {
        match tok {
            Tok::Punct('{') => {
                let head = std::mem::take(&mut self.run);
                if crate::loops::is_for(&head) {
                    self.cut.push(ForRun {
                        head,
                        body: Vec::new(),
                        depth: self.depth,
                    });
                } else {
                    self.route(Event::Open(head), emit);
                }
                self.depth += 1;
            }
            Tok::Punct(';') => self.push_decl(emit),
            Tok::Punct('}') => {
                self.push_decl(emit);
                // **Saturating, on purpose.** A `}` with no brace open is ordinary input, not a broken
                // program: a custom property holds raw CSS text, so `--prop: };` hands this phase a closing
                // brace it never opened (`css/custom_properties/error.hrx::brackets/curly`, whose own answer
                // is `Error: unmatched "}".`). A counter that wrapped there would panic in a debug build, and
                // a `@for` cut at depth 0 would then close on the wrong brace.
                self.depth = self.depth.saturating_sub(1);
                if self.cut.last().is_some_and(|run| run.depth == self.depth) {
                    if let Some(run) = self.cut.pop() {
                        self.route(
                            Event::For {
                                head: run.head,
                                body: run.body,
                            },
                            emit,
                        );
                    }
                } else {
                    self.route(Event::Close, emit);
                }
            }
            other => self.run.push(other),
        }
    }

    /// Hand on what an end-of-input left uncut. Before [`Event::For`] existed this phase emitted an
    /// [`Event::Open`] the moment it saw `{`, so a program truncated inside a loop body still produced the
    /// blocks it had reached; keeping a run here would instead drop a whole block and the rules inside it —
    /// a missing rule leaves no trace in a pass count. Each unclosed run is handed on through [`as_block`],
    /// the same construction a declined header takes: no range is read and no body is executed.
    ///
    /// Written as a fold that builds one owned sequence, innermost first, because the runs nest: the inner
    /// reconstruction has to land inside the outer one's body.
    pub fn finish(&mut self, emit: Emit<'_, Event>) {
        std::mem::take(&mut self.cut)
            .into_iter()
            .rev()
            .fold(Vec::new(), |trailing, run| {
                // Whatever was still open inside this run belongs before its `}`.
                let mut body = run.body;
                body.extend(trailing);
                as_block(run.head, body)
            })
            .into_iter()
            .for_each(|ev| emit(ev));
    }

    /// Send one event where it belongs on the stream: into the innermost `@for` still being cut out, or —
    /// with no loop open — straight downstream.
    fn route(&mut self, ev: Event, emit: Emit<'_, Event>) {
        match self.cut.last_mut() {
            Some(run) => run.body.push(ev),
            None => emit(ev),
        }
    }

    fn push_decl(&mut self, emit: Emit<'_, Event>) {
        let run = std::mem::take(&mut self.run);
        if let Some(colon) = run.iter().position(|t| matches!(t, Tok::Punct(':'))) {
            let prop = run[..colon].to_vec();
            let value = run[colon + 1..].to_vec();
            // Whether a name is there is a lexeme question, answered on the one law the rest of the crate
            // reads — not by rendering the run back to text and scanning its characters.
            if !brackets::meaningful(&prop).is_empty() {
                self.route(Event::Decl { prop, value }, emit);
            }
        }
    }
}
