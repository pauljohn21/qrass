//! The law this file keeps: **an unwritten `rgb` channel is a fact about the colour, carried through
//! conversions and refused by the modify path — never substituted on the way out and never invented.**
//!
//! [`color_missing_channels`](mod@crate::color::space) already states the *shape* of the rule for a colour
//! written as `color(…)`. What these rows add is that a **legacy** triple takes part in the very same rule,
//! because it *is* an srgb triple: `Color::new` now reads `none` and stores an absent channel
//! (`color::Channel`), instead of refusing the call and leaving the whole expression as text. The archive's
//! four readings of that, each with its own row:
//!
//! * **An identity conversion keeps what was written.** `to_space/rgb/rgb.hrx::missing/red` asks for
//!   `color.to-space(rgb(none 20 30), rgb)` and answers `rgb(none 20 30)` — in the space-separated form,
//!   because the legacy comma form has no spelling for `none`. The same call on a *space* colour
//!   (`to_space/srgb/rgb.hrx::missing/red`) answers `rgb(0%, 20%, 30%)`: there the `none` belonged to the
//!   space triple and the legacy target substitutes. The two rows together are why the guard keys on the
//!   representation, not on missing-ness alone.
//! * **Inside the index-aligned family it stays at its own index** — `color(xyz 0.0035949053 none 0.0123994639)`
//!   for `rgb(10 none 30)`, whose other two channels are the numbers `0` produces, so the arithmetic answer
//!   and the printing answer are different questions (`color::missing_as_zero` is the one place the first is
//!   stated).
//! * **A derived target substitutes**: `rgb(none 20 30)` to `hsl` is `hsl(200, 100%, 5.8823529412%)`
//!   (`to_space/rgb/hsl.hrx::missing/red`), and all-three-missing is powerless into a space
//!   (`to_space/rgb/lab.hrx::missing/all` → `lab(none none none)`).
//! * **Modifying an unwritten channel is refused, not answered.** The corpus states it as an error —
//!   `$red: Because the CSS working group is still deciding on the best behavior, Sass doesn't currently
//!   support modifying missing channels (color: rgb(none 0 0))` (`adjust/error/missing.hrx::legacy`, repeated
//!   for `scale` at `scale/error/missing.hrx::legacy`). Without the guard in `Color::with_channel` this
//!   compiler answered that call `rgb(10, 0, 0)` (measured: run `legacy-missing-channel` in `reports/hrx.db`,
//!   row `core_functions/color/adjust/error/missing::legacy`) — a value the archive forbids, which is worse
//!   than the silence this batch settles for.
//!
//! What this file deliberately does **not** pin, with the measured reason each time:
//!
//! * **The thirteen `rgb(none …)` rows whose target is `lab`/`lch`/`oklab`/`oklch`/`display-p3`.** They went
//!   from "no answer at all" to "an answer off in the ninth or tenth decimal" —
//!   `to_space/rgb/lab.hrx::missing/red` wants `5.2408778377%`, we print `5.2408778375%`. That residue is the
//!   un-calibrated cone/primaries tables `tasks.md` 9.14 records, not this batch: the last test below asserts
//!   that the substituted answer is **identical** to the one the same colour with an explicit `0` gets, so
//!   whatever is off is off in the shared route. Pinning our own digits would freeze a known-wrong number.
//! * **The rows that want a named colour** — the archive answers a bare `black`/`white`/`red` where this
//!   compiler prints `#000000`, and the cluster lives in the files whose **target** is `rgb`
//!   (`to_space/{hwb,lch,oklab,oklch,lab,…}/rgb.hrx`). Counted two ways so the number is reproducible:
//!   `grep -rh "^  b: [a-z]*;$" sass-spec/spec/core_functions/color/to_space/ | wc -l` over the corpus side and
//!   `ledger failures … "b: black;" --count` etc. on the library side — both measured in `tasks.md` 9.23.
//!   Pinning any of them here would require the name-on-conversion law, which is unimplemented.
//! * **`hsl`/`hwb` with a `none` channel.** Their constructors still decline one, so `hsl(none 20% 30%)`
//!   remains text; the rows waiting on it are named in `tasks.md` 9.23 (and reading them needs the `hsl`/`hwb`
//!   spaces plus the axes maths, not just the parse). Writing absence into an `hsl` or `hwb` **axis** is the
//!   same wait seen from the other side — `change/hsl.hrx::hue/none` wants `hsl(none 100% 50%)` and
//!   `change/hwb.hrx::whiteness/none` wants `hwb(180deg none 50%)`, so the axes triple has to carry absence too
//!   and `Color::with_hwb` (which currently re-derives bytes) has to keep the pair. `tasks.md` 9.25 has the
//!   measured split of the remaining `change/*/none` rows.
//! * **A `none` channel with an alpha** (`rgb(0 0 0 / none)`) or an out-of-gamut sibling — no fixture prints
//!   either, so `Color::modern`'s `/ alpha` half is a generalisation (corroborated only by
//!   `change/no_space.hrx::alpha/none`, which leaves *alpha* unwritten) and is recorded as one, not asserted.

use sasspile::compile;

async fn css(scss: &str) -> String {
    compile(scss)
        .await
        .unwrap_or_else(|error| panic!("compiling {scss:?}: {error}"))
}

/// The identity conversion keeps the unwritten position and prints the space-separated form.
#[tokio::test]
async fn an_identity_conversion_keeps_the_unwritten_channel() {
    let cases = [
        ("rgb(none 20 30)", "rgb(none 20 30)"),
        ("rgb(10 none 30)", "rgb(10 none 30)"),
        ("rgb(10 20 none)", "rgb(10 20 none)"),
    ];
    for (source, printed) in cases {
        let built = css(&format!("a {{b: color.to-space({source}, rgb)}}")).await;
        assert!(
            built.contains(printed),
            "`to_space/rgb/rgb.hrx` keeps the unwritten channel: `{source}` must print `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// The same `none` propagates by position into the index-aligned family, and the channels around it are the
/// numbers `0` produces — which is what says the arithmetic substituted `0` while the printing did not.
#[tokio::test]
async fn a_missing_channel_propagates_by_position_from_a_legacy_triple() {
    let cases = [
        (
            "rgb(none 20 30)",
            "srgb",
            "color(srgb none 0.0784313725 0.1176470588)",
        ),
        (
            "rgb(10 20 none)",
            "srgb-linear",
            "color(srgb-linear 0.0030352698 0.0069954102 none)",
        ),
        (
            "rgb(10 none 30)",
            "xyz",
            "color(xyz 0.0035949053 none 0.0123994639)",
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

/// A derived target takes no `none` from one unwritten channel, while a colour with nothing at all is
/// powerless into that same space.
#[tokio::test]
async fn a_derived_target_substitutes_unless_nothing_was_written() {
    let built = css("a {b: color.to-space(rgb(none 20 30), hsl)}").await;
    assert!(
        built.contains("hsl(200, 100%, 5.8823529412%)"),
        "`to_space/rgb/hsl.hrx::missing/red` substitutes the unwritten red; this compiler answered:\n{built}"
    );
    let all = css("a {b: color.to-space(rgb(none none none), lab)}").await;
    assert!(
        all.contains("lab(none none none)"),
        "`to_space/rgb/lab.hrx::missing/all` is powerless into `lab`, same as a space colour; got:\n{all}"
    );
}

/// The decline that replaces an invented answer: `adjust` over an unwritten channel must not answer with a
/// number. The positive control is the measurement in the header — the same call did print `rgb(10, 0, 0)`
/// for one batch, and the archive calls that behaviour unsupported outright.
#[tokio::test]
async fn modifying_an_unwritten_channel_is_declined_not_invented() {
    let adjusted = css("a {b: color.adjust(rgb(none 0 0), $red: 10)}").await;
    assert!(
        !adjusted.contains("rgb(10, 0, 0)"),
        "the archive errors on this call (`adjust/error/missing.hrx::legacy`), so no answer is allowed to be \
         computed for the unwritten red; this compiler answered:\n{adjusted}"
    );
    // The refusal is keyed on the index the call names, not on the colour having a `none` anywhere in it, and
    // that narrowness is a fixture's requirement rather than my preference — see the test below.
}

/// **A written channel of a colour with unwritten siblings must still be modified.** The corpus builds
/// `to_gamut/space.hrx::clip/missing/legacy/same/with_mapping`'s answer `rgb(none 255 none)` out of
/// `color.change(rgb(none 0 none), $green: 300)`: green is the written channel, both siblings are `none`, and
/// the final row keeps them `none`. A refusal keyed on “the colour has a `none` anywhere in it” could never
/// produce that row, so this asserts the inner call is answered at all. (The row itself is still red in run
/// `legacy-missing-channel-v3` — `color.to-gamut` does not carry a `none` yet, `tasks.md` 9.23 — and what the
/// inner call *prints* is not a text the archive states on its own, so this file does not pin one.)
#[tokio::test]
async fn a_written_channel_of_a_colour_with_unwritten_siblings_is_still_modified() {
    let built = css("a {b: color.change(rgb(none 0 none), $green: 300)}").await;
    assert!(
        !built.contains("color.change("),
        "`to_gamut/space.hrx::clip/missing/legacy/same/with_mapping` needs this inner call to be answered, not \
         echoed; this compiler returned its text:\n{built}"
    );
}

/// **`change` may write absence.** `color.change(black, $red: none)` answers `rgb(none 0 0)`, and the same for
/// green and blue (`change/rgb.hrx::{red,green,blue}/none`); a colour created from a keyword takes the same
/// write and stops printing its keyword, because the form has to change to say `none` at all
/// (`change/space.hrx::missing/arg/legacy/same/implicit` is `color.change(pink, $red: none)` →
/// `rgb(none 192 203)`). `adjust` and `scale` are not in this law: their reader `amount::byte` refuses a
/// `none` outright, and the only rows that hand one to them expect a plain type error —
/// `color.adjust(red, $alpha: none)` → `$alpha: none is not a number.` (`adjust/error/type.hrx::none`;
/// `scale/error/type.hrx::none` likewise), so no fixture asks a shift or a scale to move an absence.
#[tokio::test]
async fn change_can_write_absence_into_a_byte_channel() {
    let cases = [
        ("black", "$red: none", "rgb(none 0 0)"),
        ("black", "$green: none", "rgb(0 none 0)"),
        ("black", "$blue: none", "rgb(0 0 none)"),
        ("pink", "$red: none", "rgb(none 192 203)"),
    ];
    for (colour, write, printed) in cases {
        let built = css(&format!("a {{b: color.change({colour}, {write})}}")).await;
        assert!(
            built.contains(printed),
            "`color.change({colour}, {write})`: the archive prints `{printed}`, this compiler answered:\n{built}"
        );
    }
}

/// Why the thirteen unfixed rows are not this batch's defect: on the way into a derived space the unwritten
/// channel is substituted, so `rgb(none 20 30)` and `rgb(0 20 30)` must answer **identically**. Whatever is off
/// in those thirteen is then the shared cone/primaries residue `tasks.md` 9.14 records, not a second arithmetic
/// this batch opened — and the assertion fails the day the two spellings drift apart.
#[tokio::test]
async fn a_substituted_channel_leaves_the_route_unchanged() {
    for target in ["lab", "lch", "oklab", "oklch"] {
        let missing = css(&format!(
            "a {{b: color.to-space(rgb(none 20 30), {target})}}"
        ))
        .await;
        let written = css(&format!("a {{b: color.to-space(rgb(0 20 30), {target})}}")).await;
        let value = |text: &str| {
            text.lines()
                .find(|line| line.contains("b:"))
                .unwrap_or(text)
                .trim()
                .to_string()
        };
        assert_eq!(
            value(&missing),
            value(&written),
            "into `{target}` the archive substitutes the unwritten red, so the two spells must not diverge"
        );
    }
}
