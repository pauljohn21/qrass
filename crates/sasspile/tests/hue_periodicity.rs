//! The gap this file closes: **a hue shifted past a full turn, on the whiteness/blackness branch**.
//!
//! The law is not new and neither is its value. Three rows read a wrapped hue as its residue —
//! `core_functions/color/adjust/hsl.hrx::hue/arg_above_max`, `core_functions/color/change/hsl.hrx::hue/above_max`
//! and `core_functions/color/adjust_hue/above_max.hrx`, all of them answering 540 with `aqua` — and
//! `tests/color_adjust.rs::the_hue_wraps_and_otherwise_prints_bytes` already pins that by value, together with
//! `adjust/hwb.hrx::all` (`color.adjust(red, $hue: 150, $whiteness: 20%, $blackness: 40%)` → `#339966`).
//!
//! What no row exercises is the *sum* passing 360 on the hwb branch: the rows that move a hue alongside
//! whiteness and blackness start at `red` (hue 0) and add 150, and the row that names a hue next to the pair
//! under `change` starts at `blue` but **sets** it (`change/hwb.hrx::all`), so nothing added two hues there.
//! That missing shape is how the branch could hand the conversion an angle past a turn.
//!
//! So this file adds no colour literal — the absolute values live in `color_adjust.rs`, and copying them here
//! would be a second guard for one law. It states the attested equivalence in the shape the corpus leaves
//! unexercised, and says plainly what it can and cannot see: it goes red when the two branches **disagree**
//! about the wrap. Both branches losing the fold together is a symmetric failure no equality can catch —
//! that pair is held by the absolute values in `color_adjust.rs`, which is why this file deliberately
//! duplicates none of them.

use sasspile::compile;

async fn css(sass: &str) -> String {
    let built = compile(sass)
        .await
        .unwrap_or_else(|e| panic!("{sass} failed: {}", e.message));
    // A call that was not evaluated keeps its own source text, and then every comparison below would agree
    // for the wrong reason. Both spellings must have produced a colour.
    assert!(
        !built.contains("color.adjust"),
        "the call was kept verbatim instead of evaluated: {built}"
    );
    built
}

/// `#abcdef` sits at hue 210 (`ab/cd/ef` = 171/205/239, whose maximum is blue: `60 * ((171 - 205) / 68 + 4)`),
/// so `$hue: 200` reaches 410. Moving whiteness and blackness by `0%` adds nothing to them, so the two calls
/// below differ only in **which branch writes the hue** — the first through the whiteness/blackness branch,
/// the second through the axes branch — and must therefore be the same colour.
#[tokio::test]
async fn a_hue_shift_past_a_turn_wraps_on_the_hwb_space_too() {
    let through_hwb =
        css("a {b: color.adjust(#abcdef, $hue: 200, $whiteness: 0%, $blackness: 0%)}").await;
    let through_axes = css("a {b: color.adjust(#abcdef, $hue: 200)}").await;
    assert_eq!(through_hwb, through_axes);
}
