//! The law this file keeps: **a `none` channel survives `color.to-space` in the position it was written,
//! but only inside the index-aligned family.**
//!
//! The archive separates two behaviours and states each with its own rows:
//!
//! * **Inside the family whose channels are one index-aligned RGB-or-XYZ triple** (`srgb`, `srgb-linear`,
//!   `display-p3`, `display-p3-linear`, `a98-rgb`, `prophoto-rgb`, `rec2020`, `xyz`, `xyz-d50`), a missing
//!   channel stays missing at the **same index** while the other two are computed — which also proves the
//!   missing channel still contributes `0` to the maths: `to_space/srgb/xyz.hrx::missing/red` answers
//!   `color(xyz none 0.028962782 0.0735618977)`, and those two numbers are what `0` for red produces.
//! * **Everywhere else the missing channel is substituted**, so the triple becomes three numbers. Both
//!   directions of that are covered by the same condition in `Color::converted` (one side derived ⇒ no
//!   propagation); the green witness is the derived-**source** direction,
//!   `to_space/lab/srgb.hrx::missing/lightness` → `color(srgb 0.1653748669 -0.0646023407 -0.1599982587)`.
//!   The derived-**target** direction is stated by the same rows in the archive but is still red in the
//!   tenth decimal (`to_space/srgb/lab.hrx::missing/red` wants `-7.7361660103`, we print `-7.7361660069`),
//!   which is the ceiling `tasks.md` 9.14 records — this batch did not earn it and does not claim it.
//! * **All three missing is all three missing** into a space: `to_space/srgb/lab.hrx::missing/all` →
//!   `lab(none none none)` and `to_space/srgb/oklab.hrx::missing/all` → `oklab(none none none)`. The
//!   `hsl` target is the exception the archive states in its own row —
//!   `to_space/srgb/hsl.hrx::missing/all` → `hsl(0, 0%, 0%)`, zeros, because that form has no way to print
//!   `none`. (`hwb` is a separate open question: `to_space/hwb/hwb.hrx::missing/non_hue` *does* print
//!   `hwb(10deg none none)` and that file uses no mixin, yet `color.to-space` declines the `hwb` target
//!   altogether — named in `tasks.md` 9.15.)
//!
//! No fixture states what an all-missing **rectangular** colour converts to another rectangular space, so
//! this file does not assert one: the positional rule covers it (`none` stays at its index), but that is my
//! reading of the rule, not a row the archive prints.
//!
//! What this file deliberately does **not** pin, with the measured reason each time:
//!
//! * **The polar pair (`lab`↔`lch`, `oklab`↔`oklch`) is unimplemented.** Its 22 `missing/*` rows
//!   (`lab/lch` 5, `lch/lab` 6, `oklab/oklch` 5, `oklch/oklab` 6) fit one candidate law on 20 of them —
//!   carry the lightness by position, and let a derived channel be `none` only when **all** the channels it
//!   is computed from are `none` (`lab(10% none 30)` → `lch(10% 30 90deg)`, `lab(10% none none)` →
//!   `lch(10% none none)`, `lch(10% none 30deg)` → `lab(10% 0 0)`, `oklch(none none 10deg)` →
//!   `oklab(none 0 0)`). Two rows break it, both in `lch` → `lab`: `to_space/lch/lab.hrx::missing/lightness`
//!   (`lch(none 20 30deg)` → `lab(none none none)`) and `…::missing/non_hue` (`lch(none none 10deg)` →
//!   `lab(none none none)`), where the same shapes through `oklch` → `oklab` follow the law. So the break is
//!   specific to `lab`, not to the pair algebra — and writing the law to fit 20 rows would silently invent
//!   those two, so nothing here claims a rule for the family.
//! * `to_space/xyz_d50/srgb.hrx::missing/y` — the `none` is right now and one digit is not
//!   (`0.6863615753` against the archive's `0.6863615754`), the tenth-decimal ceiling of `tasks.md` 9.14.
//! * `to_space/xyz/xyz_d50.hrx::gray` — same ceiling, on the side of the pair the library derives
//!   (`0.5014942182` against our `0.5014942183`).

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// The index-aligned family: `none` stays at its own index, the other channels keep their numbers.
#[tokio::test]
async fn a_missing_channel_stays_missing_at_its_own_index() {
    let cases = [
        (
            "color(srgb none 0.2 0.3)",
            "xyz",
            "color(xyz none 0.028962782 0.0735618977)",
        ),
        (
            "color(srgb 0.1 0.2 none)",
            "xyz",
            "color(xyz 0.0159710671 0.0258067358 none)",
        ),
        (
            "color(xyz none 0.2 0.3)",
            "srgb",
            "color(srgb none 0.6558430379 0.5622938017)",
        ),
        (
            "color(display-p3-linear 0.1 0.2 none)",
            "display-p3",
            "color(display-p3 0.3491902126 0.4845292045 none)",
        ),
    ];
    for (source, target, printed) in cases {
        let built = css(&format!("a {{b: color.to-space({source}, {target})}}")).await;
        assert!(
            built.contains(printed),
            "`{source}` to `{target}`: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// A derived target takes no `none` from a single missing channel — the same colour that keeps `none` on
/// the way to `xyz` becomes three numbers on the way to `srgb`.
#[tokio::test]
async fn a_derived_target_substitutes_the_missing_channel() {
    let built = css("a {b: color.to-space(lab(none 20 30), srgb)}").await;
    assert!(
        built.contains("color(srgb 0.1653748669 -0.0646023407 -0.1599982587)"),
        "`to_space/lab/srgb.hrx::missing/lightness` substitutes `0` for the lightness; this compiler answered:\n{built}"
    );
}

/// And the rule that predates this batch stays: a colour with nothing at all is powerless into a space,
/// while a legacy target — which has no way to print `none` — substitutes zeros.
#[tokio::test]
async fn a_colour_with_no_channels_at_all_is_powerless_or_substituted_by_target_form() {
    let built = css("a {b: color.to-space(color(srgb none none none), lab)}").await;
    assert!(
        built.contains("lab(none none none)"),
        "`to_space/srgb/lab.hrx::missing/all` is `lab(none none none)`; this compiler answered:\n{built}"
    );
    let hsl = css("a {b: color.to-space(color(srgb none none none), hsl)}").await;
    assert!(
        hsl.contains("hsl(0, 0%, 0%)"),
        "`to_space/srgb/hsl.hrx::missing/all` substitutes — the legacy form cannot print `none`; got:\n{hsl}"
    );
    let oklab = css("a {b: color.to-space(color(srgb none none none), oklab)}").await;
    assert!(
        oklab.contains("oklab(none none none)"),
        "`to_space/srgb/oklab.hrx::missing/all` keeps all three; this compiler answered:\n{oklab}"
    );
}
