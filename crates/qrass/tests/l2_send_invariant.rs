//! The `Send + Sync` invariant behind design D1's L2 layering.
//!
//! The module bus multicasts each `@use` singleton through a `Shared`
//! `BehaviorSubject`, and the *reason* that carrier is `Shared` rather than `Local`
//! is exactly what this file pins: the payload and the state that hosts it must stay
//! thread-capable, because the next step of D1 (instantiating disjoint `@use`
//! branches concurrently) is only reachable from a `Send` `EvalState`. A `Local`
//! subject would smuggle an `Rc` into the arena and silently close that door, so a
//! compile failure here is an architectural regression, not a broken test.

use qrass::ast::Stmt;
use qrass::eval::EvalState;
use qrass::module::Module;
use qrass::value::SassValue;

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn multicast_payload_is_send_sync() {
    // The hub's item is `Option<Arc<Module>>`; `Shared` demands `Send`.
    assert_send_sync::<Module>();
    assert_send_sync::<SassValue>();
    assert_send_sync::<std::sync::Arc<Module>>();
}

#[test]
fn module_bodies_are_send_sync() {
    // Concurrent branches would move a parsed program across threads.
    assert_send_sync::<Stmt>();
    assert_send_sync::<Vec<Stmt>>();
}

#[test]
fn arena_stays_send_sync_after_hosting_the_bus() {
    assert_send_sync::<EvalState>();
}
