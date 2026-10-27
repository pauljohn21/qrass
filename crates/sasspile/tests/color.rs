//! The colour law, pinned on `core_functions/color/rgb/**` goldens. Each expectation is that fixture's own
//! `output.css`, not a derivation: the representation a colour was created in decides how it prints, so
//! the same channels come out as `rgb(…)`, `rgb(…%…)`, `rgba(…)`, or `#112233`.

use sasspile::compile;

async fn css(input: &str) -> String {
    compile(input)
        .await
        .unwrap_or_else(|e| panic!("{input} failed: {}", e.message))
}

/// `rgb/three_args/unitless`: channels print as written, and out-of-range ones clamp to `0–255`
/// (`clamped/{red,green,blue}`).
#[tokio::test]
async fn unitless_channels_print_and_clamp() {
    for (input, want) in [
        ("a {b: rgb(18, 52, 86)}", "rgb(18, 52, 86)"),
        ("a {b: rgb(190, 173, 237)}", "rgb(190, 173, 237)"),
        ("a {b: rgb(0, 255, 127)}", "rgb(0, 255, 127)"),
        ("a {b: rgb(256, 0, 0)}", "rgb(255, 0, 0)"),
        ("a {b: rgb(0, -1, 0)}", "rgb(0, 0, 0)"),
        ("a {b: rgb(0, 0, 9999)}", "rgb(0, 0, 255)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `rgb/three_args/percents`: a percentage anywhere makes the whole colour percentage-form, so a unitless
/// channel is converted (`173` → `67.8431372549%`) while a written percentage is kept exactly as spelled.
#[tokio::test]
async fn one_percentage_channel_makes_all_percentages() {
    assert_eq!(
        css("a {b: rgb(7.1%, 20.4%, 33.9%)}").await,
        "a {\n  b: rgb(7.1%, 20.4%, 33.9%);\n}\n"
    );
    assert_eq!(
        css("a {b: rgb(74.7%, 173, 93%)}").await,
        "a {\n  b: rgb(74.7%, 67.8431372549%, 93%);\n}\n"
    );
}

/// `rgb/two_args::opaque_to/*` and `rgb/four_args/clamped/*`: a colour built from hex prints as hex while
/// opaque and as `rgba(…)` the moment it is translucent, and an alpha argument selects the four-argument
/// spelling. A negative zero is `0`, not `-0`.
#[tokio::test]
async fn representation_and_alpha_choose_the_spelling() {
    for (input, want) in [
        ("a {b: rgb(#123, 1)}", "#112233"),
        ("a {b: rgb(#123, 0.5)}", "rgba(17, 34, 51, 0.5)"),
        ("a {b: rgb(256, 0, 0, 0.5)}", "rgba(255, 0, 0, 0.5)"),
        ("a {b: rgb(0, -0, 0, 0.5)}", "rgba(0, 0, 0, 0.5)"),
        ("a {b: rgb(0, 0, 9999, 0.5)}", "rgba(0, 0, 255, 0.5)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `hsl/three_args/units.hrx` and `…/bounds.hrx`, read row by row: the **hue is folded to degrees and
/// wrapped into `0–360`** (`60rad` → `197.7467707849`, `0deg` → `0`), saturation and lightness **always**
/// carry a `%` (`50` → `50%`), and unlike an `rgb` channel they are **not clamped** (`500%` stays).
/// `hsl/four_args/alpha.hrx` adds the alpha law: a percentage alpha converts, and an alpha of one returns
/// the three-argument spelling.
#[tokio::test]
async fn hsl_folds_hue_and_prints_percentages() {
    for (input, want) in [
        ("a {b: hsl(0, 100%, 50%)}", "hsl(0, 100%, 50%)"),
        ("a {b: hsl(0deg, 100%, 50%)}", "hsl(0, 100%, 50%)"),
        (
            "a {b: hsl(60rad, 100%, 50%)}",
            "hsl(197.7467707849, 100%, 50%)",
        ),
        ("a {b: hsl(0, 50, 50%)}", "hsl(0, 50%, 50%)"),
        ("a {b: hsl(0, 500%, 50%)}", "hsl(0, 500%, 50%)"),
        ("a {b: hsl(0, 0%, 0%, 45.6%)}", "hsla(0, 0%, 0%, 0.456)"),
        ("a {b: hsl(0, 0%, 0%, 100%)}", "hsl(0, 0%, 0%)"),
        ("a {b: hsl(0, 0%, 0%, 250%)}", "hsl(0, 0%, 0%)"),
        ("a {b: hsl(0, 0%, 0%, -10%)}", "hsla(0, 0%, 0%, 0)"),
        // Labels bind, for `hsl` and `rgb` alike — the reason the registry carries optional parameters at
        // all (`hsl/three_args/named.hrx`, `rgb/three_args/unitless::named`).
        (
            "a {b: hsl($hue: 0, $saturation: 100%, $lightness: 50%)}",
            "hsl(0, 100%, 50%)",
        ),
        (
            "a {b: rgb($red: 0, $green: 255, $blue: 127)}",
            "rgb(0, 255, 127)",
        ),
        // An unmodellable argument keeps the call as written (`hsl/three_args/special_functions::var/*`).
        ("a {b: hsl(var(--foo), 2%, 3%)}", "hsl(var(--foo), 2%, 3%)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// What must **not** happen. An argument the colour law cannot read declines the call, and the run keeps
/// its verbatim text — `three_args::1_of_3` (`var()` is substituted before parsing, so the list can hold
/// anything) is a golden of exactly that. And a hex written straight into a declaration is not a `Color`
/// at all: `values/colors/alpha_hex/initial_letter` prints `#AbCd`, capitalisation intact, so normalising
/// it here would be a regression dressed as a win.
#[tokio::test]
async fn unmodellable_arguments_decline_and_hex_is_left_alone() {
    assert_eq!(
        css("a {b: rgb(var(--foo), 3, 0.4)}").await,
        "a {\n  b: rgb(var(--foo), 3, 0.4);\n}\n"
    );
    assert_eq!(css("a {b: #AbCd}").await, "a {\n  b: #AbCd;\n}\n");
    assert_eq!(css("a {b: #98765432}").await, "a {\n  b: #98765432;\n}\n");
    // Channels are numbers or percentages; `px` is neither, and no error message is invented for it (P7).
    assert_eq!(
        css("a {b: rgb(1px, 2px, 3px)}").await,
        "a {\n  b: rgb(1px, 2px, 3px);\n}\n"
    );
}

/// The `color.*` accessors, and with them the audit that option (a) promised: the keyword table comes from
/// the `color` crate's keyword table, so every expectation here is a name the **corpus itself** pins.
/// `gray` is fixed twice over — `equality.hrx` states `gray == hsl(none 0% 50.196078431373%)` (128/255) and
/// `color.invert(gray)` → `#7f7f7f` (128 inverted is 127); `springgreen` is what
/// `rgb/three_args/unitless::springgreen` writes for `rgb(0, 255, 127)`; `purple` and `pink` come from
/// `equality.hrx` and `channel/hwb::foreign/whiteness` (`75.2941176471%` is `192/255`).
#[tokio::test]
async fn accessors_read_channels_and_the_keyword_table_matches_the_corpus() {
    for (input, want) in [
        // The fixture rows themselves (`core_functions/color/red.hrx::{max,min,middle,named}`).
        ("a {b: color.red(rgb(255, 0, 0))}", "255"),
        ("a {b: color.red(rgb(0, 0, 0))}", "0"),
        ("a {b: color.red(rgb(123, 0, 0))}", "123"),
        ("a {b: color.red($color: rgb(234, 0, 0))}", "234"),
        // `color.alpha` (`alpha.hrx::color/{max,middle,min,named}`).
        ("a {b: color.alpha(red)}", "1"),
        ("a {b: color.alpha(rgba(red, 0.42))}", "0.42"),
        ("a {b: color.alpha(rgba(red, 0))}", "0"),
        ("a {b: color.alpha($color: rgba(red, 0.73))}", "0.73"),
        // Keyword parsing, cross-checked against the corpus values above.
        ("a {b: color.red(black)}", "0"),
        ("a {b: color.red(white)}", "255"),
        ("a {b: color.red(gray)}", "128"),
        // The CSS `-grey` spellings are the same entry, and the corpus does use them (`invert(grey)`,
        // `color.adjust(grey, …)`), so the alias has to resolve to the same bytes.
        ("a {b: color.red(grey)}", "128"),
        ("a {b: color.green(springgreen)}", "255"),
        ("a {b: color.blue(springgreen)}", "127"),
        ("a {b: color.red(purple)}", "128"),
        ("a {b: color.blue(pink)}", "203"),
        // A hex argument arrives as text and is read from there, not from a parsed declaration value.
        ("a {b: color.red(#123)}", "17"),
        // `transparent` is a keyword the dependency knows and the old hand-list did not carry: the two
        // alphas below are `rgb/two_args::transparent_to/{partial,transparent}`. The third row of that
        // group, `rgb(transparent, 1)` → `black`, is **not** covered here — printing a colour as its name
        // is the reverse-lookup law, still open.
        ("a {b: rgb(transparent, 0.5)}", "rgba(0, 0, 0, 0.5)"),
        ("a {b: rgb(transparent, 0)}", "rgba(0, 0, 0, 0)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The honest edge: a colour held in the **hsl** space has no `0–255` channels to hand out, and converting
/// is a separate change (its own fixture rows). Reporting a number computed off the wrong axes would be a
/// wrong answer scored as a pass, so the call declines and the text stands.
#[tokio::test]
async fn an_hsl_colour_declines_an_rgb_channel() {
    assert_eq!(
        css("a {b: color.red(hsl(0, 100%, 50%))}").await,
        "a {\n  b: color.red(hsl(0, 100%, 50%));\n}\n"
    );
}

/// `color.hue` / `color.saturation` / `color.lightness`, one output per fixture row and one input per row
/// actually read out of `core_functions/color/{hue,saturation,lightness}.hrx` — including the axis units,
/// which no reasoning would supply: a hue prints `359deg`, saturation and lightness print `42%`.
///
/// Every value row in these three files feeds an `hsl(…)` colour, so this slice needs **no** rgb→hsl
/// conversion; the non-`hsl` arguments in them (`1`, `lch(0% 0 0deg)`, `red, green`) are `error/*` rows
/// wanting messages this project has not built (P7).
#[tokio::test]
async fn hue_saturation_lightness_read_the_hsl_axes() {
    for (input, want) in [
        ("a {b: color.hue(hsl(359, 50%, 50%))}", "359deg"),
        // The fold happened at creation (`hsl/three_args/units.hrx`), the accessor only reads it back.
        ("a {b: color.hue(hsl(540, 50%, 50%))}", "180deg"),
        ("a {b: color.hue(hsl(0, 50%, 50%))}", "0deg"),
        ("a {b: color.hue(hsl(123, 50%, 50%))}", "123deg"),
        ("a {b: color.hue($color: hsl(234, 50%, 50%))}", "234deg"),
        ("a {b: color.saturation(hsl(0, 100%, 50%))}", "100%"),
        ("a {b: color.saturation(hsl(0, 50%, 50%))}", "50%"),
        ("a {b: color.saturation($color: hsl(0, 42%, 50%))}", "42%"),
        ("a {b: color.lightness(hsl(0, 100%, 100%))}", "100%"),
        ("a {b: color.lightness(hsl(0, 100%, 0%))}", "0%"),
        ("a {b: color.lightness(hsl(0, 100%, 50%))}", "50%"),
        ("a {b: color.lightness($color: hsl(0, 100%, 42%))}", "42%"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The other direction of the same honesty: an `rgb` colour asked for a hue has no hsl axis stored, and
/// inventing one without a golden to check the rounding against would be a wrong answer scored as a pass.
/// The call declines, so the text stands — until the conversion lands with its own fixtures.
#[tokio::test]
async fn an_rgb_colour_declines_a_hue_request() {
    for call in [
        "color.hue(#f00)",
        "color.saturation(red)",
        "color.lightness(#ff0000)",
    ] {
        assert_eq!(
            css(&format!("a {{b: {call}}}")).await,
            format!("a {{\n  b: {call};\n}}\n"),
            "an unconvertible request must not be answered: {call}"
        );
    }
}

/// The hsl round trip and the printing rule it exposes, one assertion per fixture row
/// (`core_functions/color/grayscale.hrx`). The pair `#633736` / `#494949` is the whole law: the same
/// operation prints `rgb(30%, 30%, 30%)` when the round trip lands between bytes and `#494949` when it
/// lands on one — so rounding belongs to printing, not to the conversion.
#[tokio::test]
async fn grayscale_round_trips_and_prints_by_whole_bytes() {
    for (input, want) in [
        ("a {b: color.grayscale(#633736)}", "rgb(30%, 30%, 30%)"),
        ("a {b: color.grayscale(#494949)}", "#494949"),
        ("a {b: color.grayscale(red)}", "rgb(50%, 50%, 50%)"),
        (
            "a {b: color.grayscale(rgba(#633736, 0.3))}",
            "rgba(30%, 30%, 30%, 0.3)",
        ),
        // `color.grayscale(hsl(120deg 10% 20%))` → `hsl(120, 0%, 20%)` is a real row of the same file and
        // is **not** claimed here: its input is the modern space-separated spelling, which this compiler
        // does not parse yet, while its output is the legacy comma form.
        //
        // The one hsl-source case that is legacy-shaped (`hsl(120, 10%, 20%)`) has no golden either, so no
        // output is asserted for it — printing an hsl colour from a converted hsl colour stays unclaimed
        // until a fixture states it.
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// The CSS overload that motivated the variadic registration: `grayscale(10%)` is a filter function, not a
/// colour call, so it must survive untouched (`saturate.hrx` states the same for `saturate(50%)`).
#[tokio::test]
async fn a_filter_argument_is_not_a_colour_call() {
    assert_eq!(
        css("a {b: grayscale(50%)}").await,
        "a {\n  b: grayscale(50%);\n}\n"
    );
}

/// The legacy hsl manipulators, one assertion per row read out of `darken.hrx` / `lighten.hrx` /
/// `desaturate.hrx`. They exercise both halves of the axis law at once: `desaturate(plum, 100%)` and
/// `desaturate(plum, 48%)` are the same colour because saturation clamps at 100, and the two printings
/// (`rgb(74.7058823529%, …)` vs `rgba(0, 0, 0, 0.2)`) differ only because one lands between whole bytes.
///
/// Rows whose expected output is a colour **name** (`desaturate(plum, 0%)` → `plum`, `darken(red, 100%)` →
/// `black`) are not claimed here — reverse name printing is still open. `saturate` is registered nowhere
/// yet: see tasks.md 1.5 for the decline-path defect that would turn `saturate($c)` into un-substituted
/// text.
#[tokio::test]
async fn legacy_hsl_manipulators_move_the_axes_and_clamp() {
    for (input, want) in [
        ("a {b: darken(red, 14%)}", "rgb(72%, 0%, 0%)"),
        ("a {b: darken(red, 0.5%)}", "rgb(99%, 0%, 0%)"),
        (
            "a {b: darken($color: red, $amount: 14%)}",
            "rgb(72%, 0%, 0%)",
        ),
        ("a {b: darken(rgba(red, 0.2), 100%)}", "rgba(0, 0, 0, 0.2)"),
        ("a {b: lighten(red, 14%)}", "rgb(100%, 28%, 28%)"),
        ("a {b: lighten(red, 0.5%)}", "rgb(100%, 1%, 1%)"),
        (
            "a {b: desaturate(plum, 100%)}",
            "rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)",
        ),
        (
            "a {b: desaturate(plum, 48%)}",
            "rgb(74.7058823529%, 74.7058823529%, 74.7058823529%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// What a **declined** call must keep. Rebuilding the call from its evaluated arguments was tried first and
/// measured as 40 regressions, so this pins the three rules the corpus actually asks for; the code keeps the
/// token run and resolves only its variable reads (`expr::interpolated`).
#[tokio::test]
async fn a_declined_call_keeps_its_text_and_still_resolves_variables() {
    for (input, want) in [
        // A Sass variable becomes its value (`saturate.hrx::css_overload/with_sass_var`).
        ("$c: 1;\na {b: saturate($c)}", "saturate(1)"),
        // Spaces inside one argument survive: `rgb(18 52 none)` is a single modern-syntax argument whose
        // channel is unmodellable (`rgb/one_arg/no_alpha::missing/red`), and the run is printed as written.
        ("a {b: rgb(none 52 66)}", "rgb(none 52 66)"),
        // So does the space before a comma inside another function's argument
        // (`rgb/three_args/special_functions::attr/arg_1`).
        ("a {b: rgb(attr(c, %), 2, 3)}", "rgb(attr(c, %), 2, 3)"),
        // A CSS filter spelling is not a colour call at all (`saturate.hrx::css_overload/unit`).
        ("a {b: saturate(50%)}", "saturate(50%)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// A colour **created from a keyword** prints that keyword again while its bytes still name one, and the
/// corpus separates this from a value-only rule: a colour built from `rgb(0, 0, 0)` prints
/// `rgb(0, 0, 0)`, not `black` (six rows under `core_functions/color/rgb/**`), so the origin is what matters.
///
/// The keyword list itself carries no RGB values — every name resolves through `color::parse_color` — so
/// these rows also audit the spellings: a mistyped name would not parse and would print hex instead.
#[tokio::test]
async fn a_keyword_created_colour_prints_its_keyword() {
    for (input, want) in [
        // Unchanged value, same keyword back (`lighten.hrx::min`, `darken.hrx::min`).
        ("a {b: lighten(red, 0%)}", "red"),
        ("a {b: darken(red, 0%)}", "red"),
        // Greys keep the keyword they were made from (`grayscale.hrx::{max,min}`).
        ("a {b: color.grayscale(white)}", "white"),
        ("a {b: color.grayscale(black)}", "black"),
        // A colour with no saturation is unchanged (`desaturate.hrx::min`).
        ("a {b: desaturate(plum, 0%)}", "plum"),
        // `transparent` is a keyword too; at alpha 1 its bytes name `black`
        // (`rgb/two_args::transparent_to/opaque`).
        ("a {b: rgb(transparent, 1)}", "black"),
        // The built-from-numbers case, which must **not** become a keyword.
        ("a {b: rgb(0, 0, 0)}", "rgb(0, 0, 0)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// `color.mix`, one row per fixture in `core_functions/color/mix/{unweighted,explicit_weight,alpha,units}.hrx`.
/// The two `rgba` rows are what pins the **alpha-weighted** formula rather than a plain average: with
/// `0.8`/`0.3` the first colour carries `0.75` (`145 × 0.75 + 1 × 0.25 = 109` → `42.7450980392%`), and the
/// mirrored `0.4`/`0.9` row gives `0.25` → `37` → `14.5098039216%`. Alpha itself is the average, and the
/// result keeps the **first** colour's representation — which is why `mix(#91e16f, transparent)` prints
/// whole-byte numbers while `mix(#91e16f, #0144bf)` prints percentages (73 of 255 is not a whole byte).
#[tokio::test]
async fn mix_averages_channels_weighted_by_alpha() {
    for (input, want) in [
        ("a {b: color.mix(#ff00ff, #00ff00)}", "rgb(50%, 50%, 50%)"),
        ("a {b: color.mix(#123456, #123456)}", "#123456"),
        (
            "a {b: color.mix(#91e16f, #0144bf)}",
            "rgb(28.6274509804%, 57.4509803922%, 59.2156862745%)",
        ),
        ("a {b: color.mix(#91e16f, #0144bf, 100%)}", "#91e16f"),
        ("a {b: color.mix(#91e16f, #0144bf, 0%)}", "#0144bf"),
        // A weight is read as a percentage whatever unit it carries (`mix/units.hrx`).
        (
            "a {b: color.mix(#91e16f, #0144bf, 50px)}",
            "rgb(28.6274509804%, 57.4509803922%, 59.2156862745%)",
        ),
        (
            "a {b: color.mix(rgba(#91e16f, 0.8), rgba(#0144bf, 0.3))}",
            "rgba(42.7450980392%, 72.8431372549%, 51.3725490196%, 0.55)",
        ),
        (
            "a {b: color.mix(rgba(#91e16f, 0.4), rgba(#0144bf, 0.9))}",
            "rgba(14.5098039216%, 42.0588235294%, 67.0588235294%, 0.65)",
        ),
        (
            "a {b: color.mix(#91e16f, transparent)}",
            "rgba(145, 225, 111, 0.5)",
        ),
        (
            "a {b: color.mix(transparent, #0144bf)}",
            "rgba(1, 68, 191, 0.5)",
        ),
        // Labels bind (`mix/named.hrx`'s rectangular rows are the same numbers with `$weight: 92%`).
        (
            "a {b: color.mix($color1: #91e16f, $color2: #0144bf, $weight: 92%)}",
            "rgb(52.3450980392%, 83.3098039216%, 46.0392156863%)",
        ),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}

/// A colour written in a space this compiler has not implemented must not be answered in sRGB. The parser
/// behind the keyword table would happily read `color(display-p3 1 0 0)`; refusing parenthesised text is
/// what keeps those rows verbatim instead of a wrong value (`mix/predefined.hrx`).
#[tokio::test]
async fn a_colour_in_an_unimplemented_space_is_not_answered_in_srgb() {
    let input = "a {b: color.mix(color(display-p3 1 0 0), color(display-p3 0 1 0), 20%)}";
    assert_eq!(
        css(input).await,
        "a {\n  b: color.mix(color(display-p3 1 0 0), color(display-p3 0 1 0), 20%);\n}\n"
    );
}

/// What the registry now refuses, and what it must still resolve.
///
/// A `$label:` that names no parameter of the callee's signature is not this builtin's call to answer, so
/// the run keeps its own text: answering `color.mix(…, $method: oklab)` with a legacy sRGB mix would be a
/// wrong value that the pass count cannot see, because those rows were already failing for other reasons
/// (`mix/explicit_method`, `named::polar_space`, `hue_interpolation`). Declared labels keep working, and a
/// declined call with **no** label still resolves its variables — the row this rule nearly broke
/// (`saturate.hrx::css_overload/with_sass_var`) is pinned here so it cannot be "fixed" by guessing again.
#[tokio::test]
async fn an_unknown_parameter_label_is_a_refusal_not_a_legacy_answer() {
    for (input, want) in [
        (
            "a {b: color.mix(#91e16f, #0144bf, $method: oklab)}",
            "color.mix(#91e16f, #0144bf, $method: oklab)",
        ),
        (
            "a {b: color.mix(#91e16f, #0144bf, $space: hsl)}",
            "color.mix(#91e16f, #0144bf, $space: hsl)",
        ),
        // The same call with a declared label still mixes.
        (
            "a {b: color.mix($color1: #91e16f, $color2: #0144bf, $weight: 100%)}",
            "#91e16f",
        ),
        // And a decline with no label still resolves the variable (`css_overload/with_sass_var`).
        ("$c: 1;\na {b: saturate($c)}", "saturate(1)"),
    ] {
        assert_eq!(
            css(input).await,
            format!("a {{\n  b: {want};\n}}\n"),
            "for input {input}"
        );
    }
}
