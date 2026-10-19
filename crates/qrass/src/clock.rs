//! Target-aware stopwatch for the pipeline's `elapsed_ms` span fields.
//!
//! `std::time::Instant::now()` **panics at runtime** on
//! `wasm32-unknown-unknown` with the stock std (no clock source is linked
//! there), and the wasm contract says the whole compiler must run on that
//! target — so measurement is a host/wasip1 luxury, never a wasm trap. Every
//! stage uses [`StopWatch`] instead of `Instant` and only pays for the branch
//! at record time.

/// A clock that starts at [`start`](Self::start) and is read once at span close.
pub struct StopWatch(Inner);

// Stock-std clock: native targets and `wasm32-wasip1` (clock_time_get).
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
struct Inner(std::time::Instant);

// `wasm32-unknown-unknown`: the no-op clock. The type carries no `Instant` at
// all, so no call site can accidentally reach the panicking std path.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
struct Inner(());

impl StopWatch {
    /// Start (or, on `wasm32-unknown-unknown`, decline to start) the clock.
    pub fn start() -> Self {
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        {
            StopWatch(Inner(std::time::Instant::now()))
        }
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        {
            StopWatch(Inner(()))
        }
    }

    /// Whole milliseconds since [`start`](Self::start); `0` where the target
    /// has no stock-std clock source.
    pub fn elapsed_ms(&self) -> u64 {
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        {
            self.0.0.elapsed().as_millis() as u64
        }
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        {
            0
        }
    }
}
