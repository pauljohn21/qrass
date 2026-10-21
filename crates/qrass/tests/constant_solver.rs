//! W1 numeric-tail solver (OpenSpec `converge-sass-spec-to-100`, tasks 1.2 +
//! 2.1/2.2). Reads the golden hrx fixtures — never dart internals — and:
//!
//! 1. extracts every clean `to-space` evidence pair
//!    `(src space, target space, input channels, golden output channels)`,
//!    dumping them to `reports/w1-golden-pairs.tsv` (task 1.2);
//! 2. fits the effective D65→D50 adaptation matrix + lab white point from the
// Linear-algebra kernels read far clearer with index math than iterator 
// plumbing; the ranges are fixed 3×3/9-unknown blocks throughout.
#![allow(clippy::needless_range_loop)]
//!    `xyz → lab` pairs (CIELAB's `f` has an exact inverse, so every row
//!    yields linear equations for the composite `diag(w)·B`, over-determined
//!    across rows — a least-squares solve in f64, task 2.2);
//! 3. reports the fitted constants next to the currently compiled ones so the
//!    swap decision is made on numeric evidence only.
//!
//! Run with: `cargo test --test constant_solver -- --ignored --nocapture`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// One clean evidence row: the channel list of the input literal and of the
/// golden output, both parsed from the same section.
#[derive(Debug, Clone)]
pub struct Pair {
    pub src: String,
    pub tgt: String,
    pub ins: Vec<f64>,
    pub alpha_in: Option<f64>,
    pub outs: Vec<f64>,
}

/// Parse the `name`/`value` channel list of a color literal body such as
/// `lab(50% 20 -30 / 0.4)` (already stripped of the wrapper).
fn parse_channels(body: &str) -> (Vec<f64>, Option<f64>) {
    let mut vals = Vec::new();
    for tok in body.split_whitespace() {
        let t = tok.trim_end_matches([',', ';']);
        if t == "/" || t.is_empty() {
            continue;
        }
        // The golden spellings are plain decimals/percentages here; anything
        // exotic (none/calc/hue-deg on an unexpected slot) is filtered by the
        // caller before reaching this parser.
        let num: f64 = match t.strip_suffix('%') {
            Some(rest) => rest.parse().ok(),
            None => t.strip_suffix("deg").unwrap_or(t).parse().ok(),
        }
        .unwrap_or(f64::NAN);
        if num.is_nan() {
            return (Vec::new(), None);
        }
        vals.push(num);
    }
    (vals, None)
}

/// Split a color literal `lab(50% 20 -30)` / `color(srgb 0.1 0.2 0.3)` /
/// `rgb(51 102 179)` into `(space, body)`.
fn split_literal(text: &str) -> Option<(String, String)> {
    let open = text.find('(')?;
    let name = text[..open].trim().to_string();
    let body = text[open + 1..].trim_end_matches([')', ';', ' ']).to_string();
    let name = match name.as_str() {
        "color" => {
            let mut it = body.splitn(2, ' ');
            let sp = it.next()?.to_string();
            return Some((sp, it.next()?.to_string()));
        }
        other => (other.to_string(), body),
    };
    Some(name)
}

fn channel_value(body: &str) -> Option<(Vec<f64>, Option<f64>)> {
    let (v, a) = parse_channels(body);
    if v.len() == 3 { Some((v, a)) } else { None }
}

/// Walk the modern-suite to_space directory and collect clean pairs.
pub fn collect_pairs(root: &Path) -> Vec<Pair> {
    let mut out = Vec::new();
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    for entry in fs::read_dir(root).expect("read to_space dir") {
        let p = entry.expect("dir entry").path();
        if p.is_dir() {
            dirs.push(p);
        }
    }
    dirs.sort();
    for dir in dirs {
        let src = dir.file_name().unwrap().to_string_lossy().to_string();
        for file in fs::read_dir(&dir).expect("read src dir") {
            let path = file.expect("file entry").path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".hrx") {
                continue;
            }
            let tgt = name.trim_end_matches(".hrx").replace('_', "-");
            let text = fs::read_to_string(&path).expect("read hrx");
            for section in text.split("================================================================================") {
                // input call: `color.to-space(<literal>, <target>)`
                let Some(call_at) = section.find("to-space(") else { continue };
                let call = &section[call_at + "to-space(".len()..];
                let Some(close) = balanced_end(call) else { continue };
                let inner = &call[..close];
                let Some(comma) = last_split_comma(inner) else { continue };
                let Some((lit_name, lit_body)) = split_literal(inner[..comma].trim()) else {
                    continue;
                };
                if lit_name.replace('_', "-") != src.replace('_', "-") {
                    continue;
                }
                if inner[..comma].contains("none")
                    || inner[..comma].contains("calc(")
                    || inner[..comma].contains("from ")
                {
                    continue;
                }
                let Some((ins, _)) = channel_value(&lit_body) else { continue };
                // golden output: the `b: <literal>` line inside output.css
                // (the input itself contains `a {b: …`, so scope the search).
                let Some(css_at) = section.find("output.css") else {
                    continue;
                };
                let tail = &section[css_at..];
                let Some(b_at) = tail.find("b: ") else { continue };
                let line = tail[b_at + 3..]
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches([';', ' ']);
                if line.contains("color-mix") || line.contains("none") {
                    continue;
                }
                let Some((_o_name, o_body)) = split_literal(line) else { continue };
                let Some((outs, _)) = channel_value(&o_body) else { continue };
                out.push(Pair {
                    src: src.clone(),
                    tgt: tgt.clone(),
                    ins,
                    alpha_in: None,
                    outs,
                });
            }
        }
    }
    out
}

/// Index just past the balanced close of the color call inside `s` (which
/// starts right after `to-space(`).
fn balanced_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 1usize;
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The comma separating the color literal from the target space: the last
/// top-level comma before the closing paren.
fn last_split_comma(inner: &str) -> Option<usize> {
    let bytes = inner.as_bytes();
    let mut depth = 0i32;
    let mut last = None;
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}

/// Solve `diag(w)·B` (9 unknowns, absorbing the lab white point) from rows
/// `X_d50 = g(f(xyz_d65 → lab))`. Each pair yields three equations.
pub fn fit_lab_segment(pairs: &[&Pair]) -> Option<([f64; 9], f64)> {
    // CIELAB f⁻¹.
    let g = |f: f64| {
        if f > 6.0 / 29.0 {
            f * f * f
        } else {
            3.0 * (6.0 / 29.0) * (6.0 / 29.0) * (f - 4.0 / 29.0)
        }
    };
    // Normal equations AᵀA x = Aᵀb with 11 unknowns: row-major B̂ (9, the
    // composite diag(w)·B) — actually 9 linear unknowns directly:
    // X_d50 = C·X_d65 with C = diag(w)·B; the white point then falls out as
    // w_i = C·(1,1,1) / ((1,1,1) mapping at L=100) — recovered later; here we
    // only fit C.
    let n = 9;
    let mut ata = vec![vec![0.0f64; n]; n];
    let mut atb = vec![0.0f64; n];
    for p in pairs {
        let (l, a, b) = (p.outs[0], p.outs[1], p.outs[2]);
        let fy = (l + 16.0) / 116.0;
        let fx = fy + a / 500.0;
        let fz = fy - b / 200.0;
        let target = [g(fx), g(fy), g(fz)];
        // The goldens print unnormalized D50 XYZ, so C maps D65 XYZ directly:
        // each D50 output dim independently, out_r = Σ_c C[r][c]·in_c.
        for r in 0..3 {
            for c in 0..3 {
                for k in 0..3 {
                    ata[r * 3 + c][r * 3 + k] += p.ins[c] * p.ins[k];
                }
                atb[r * 3 + c] += p.ins[c] * target[r];
            }
        }
    }
    let x = solve(ata, atb)?;
    let c = [x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7], x[8]];
    // Residual over the fitted set (max relative deviation).
    let mut worst = 0.0f64;
    for p in pairs {
        let (l, a, b) = (p.outs[0], p.outs[1], p.outs[2]);
        let fy = (l + 16.0) / 116.0;
        let t = [g(fy + a / 500.0), g(fy), g(fy - b / 200.0)];
        for r in 0..3 {
            let got = c[r * 3] * p.ins[0] + c[r * 3 + 1] * p.ins[1] + c[r * 3 + 2] * p.ins[2];
            let denom = t[r].abs().max(1e-9);
            worst = worst.max((got - t[r]).abs() / denom);
        }
    }
    Some((c, worst))
}

/// Gaussian elimination with partial pivoting on a dense square system.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let piv = (col..n)
            .max_by(|i, j| {
                a[*i][col]
                    .abs()
                    .partial_cmp(&a[*j][col].abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
        a.swap(col, piv);
        b.swap(col, piv);
        if a[col][col].abs() < 1e-14 {
            return None;
        }
        for i in col + 1..n {
            let f = a[i][col] / a[col][col];
            for j in col..n {
                a[i][j] -= f * a[col][j];
            }
            b[i] -= f * b[col];
        }
    }
    let mut x = vec![0.0f64; n];
    for i in (0..n).rev() {
        let mut s = b[i];
        for j in i + 1..n {
            s -= a[i][j] * x[j];
        }
        x[i] = s / a[i][i];
    }
    Some(x)
}

#[test]
#[ignore]
fn dump_pairs_and_fit() {
    let root = Path::new("../../sass-spec/spec/core_functions/color/to_space");
    let pairs = collect_pairs(root);
    let mut by_group: BTreeMap<(String, String), Vec<&Pair>> = BTreeMap::new();
    for p in &pairs {
        by_group
            .entry((p.src.clone(), p.tgt.clone()))
            .or_default()
            .push(p);
    }
    let mut dump = String::new();
    for p in &pairs {
        dump.push_str(&format!(
            "{}\t{}\t{:?}\t{:?}\n",
            p.src, p.tgt, p.ins, p.outs
        ));
    }
    fs::write("../../reports/w1-golden-pairs.tsv", dump).expect("write pairs tsv");
    println!("pairs total: {}", pairs.len());
    for ((src, tgt), group) in &by_group {
        println!("{src} → {tgt}: {} clean rows", group.len());
    }
    // The lab-segment fit on xyz → lab/oklab groups (pure D65 XYZ input).
    for key in [("xyz", "lab"), ("xyz", "oklab")] {
        let (k0, k1) = key;
        match by_group.get(&(k0.into(), k1.into())) {
            None => println!("fit {k0}->{k1}: group missing"),
            Some(group) => match fit_lab_segment(group) {
                None => println!("fit {k0}->{k1}: solve failed"),
                Some((c, worst)) => println!("fit {k0}->{k1} C={c:?} max-rel-residual={worst:e}"),
            },
        }
    }
}

#[test]
#[ignore]
fn fit_lab_white_point() {
    // xyz-d50 → lab rows give w directly: the lab segment consumes x_d50/w,
    // so per row and per dim w_i = x_d50_i / g(f_i). D50 inputs keep the
    // Bradford round-trip exact (B and its numeric inverse cancel), so only
    // the white-point division needs fitting.
    let root = Path::new("../../sass-spec/spec/core_functions/color/to_space");
    let pairs = collect_pairs(root);
    let g = |f: f64| {
        if f > 6.0 / 29.0 {
            f * f * f
        } else {
            3.0 * (6.0 / 29.0) * (6.0 / 29.0) * (f - 4.0 / 29.0)
        }
    };
    let mut cand: Vec<[f64; 3]> = Vec::new();
    for p in pairs.iter().filter(|p| p.src == "xyz_d50" && p.tgt == "lab") {
        let (l, a, b) = (p.outs[0], p.outs[1], p.outs[2]);
        let fy = (l + 16.0) / 116.0;
        let t = [g(fy + a / 500.0), g(fy), g(fy - b / 200.0)];
        if t.iter().any(|v| v.abs() < 1e-6) {
            continue;
        }
        cand.push([
            p.ins[0] / t[0],
            p.ins[1] / t[1],
            p.ins[2] / t[2],
        ]);
    }
    let med = |i: usize| {
        let mut v: Vec<f64> = cand.iter().map(|c| c[i]).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    println!("white candidates: {}", cand.len());
    println!(
        "w_fit = [{}, {}, {}]",
        med(0),
        med(1),
        med(2)
    );
    // The fitted composite B' = diag(w)·C so the forward division and the
    // exact-inverse round trips close on each other for every D50 path.
    let c = fit_lab_segment(
        &pairs
            .iter()
            .filter(|p| p.src == "xyz" && p.tgt == "lab")
            .collect::<Vec<_>>(),
    )
    .expect("lab fit");
    let w = [med(0), med(1), med(2)];
    println!("C_fit = {:?}", c.0);
    println!("lab-fit residual {:e}", c.1);
    for r in 0..3 {
        println!(
            "B'_row{r} = [{}, {}, {}]",
            c.0[r * 3] * w[r],
            c.0[r * 3 + 1] * w[r],
            c.0[r * 3 + 2] * w[r]
        );
    }
    for c in &cand {
        println!(
            "  {:e} {:e} {:e}",
            (c[0] - med(0)).abs(),
            (c[1] - med(1)).abs(),
            (c[2] - med(2)).abs()
        );
    }
}

/// W2 surface matrix (task 1.3): every color-function golden section that
/// carries a `none` channel becomes a row `(space, in-mask, surface) ->
/// golden text`, where surface is the output shape (`value`, `inspect`,
/// `channels:`, `color-space:`…). Dumps `reports/w2-surface-matrix.tsv`.
#[test]
#[ignore]
fn dump_w2_matrix() {
    let root = Path::new("../../sass-spec/spec/core_functions/color");
    let mut rows: Vec<String> = Vec::new();
    walk_color(root, &mut rows);
    rows.sort();
    let mut out = String::from("file\tsec\tsurface\tspace\tmask\tgolden\n");
    out.push_str(&rows.join("\n"));
    std::fs::write("../../reports/w2-surface-matrix.tsv", out).expect("write");
    println!("w2 rows: {}", rows.len());
}

fn walk_color(dir: &Path, rows: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_color(&p, rows);
        } else if p.extension().is_some_and(|x| x == "hrx") {
            collect_surface_rows(&p, rows);
        }
    }
}

fn collect_surface_rows(path: &Path, rows: &mut Vec<String>) {
    let Ok(text) = fs::read_to_string(path) else { return };
    let rel = path.to_string_lossy().replace("../../sass-spec/spec/core_functions/color/", "");
    for (i, section) in text
        .split("================================================================================")
        .enumerate()
        .skip(1)
    {
        let flat = section.replace('\n', " | ");
        if !flat.contains("none") {
            continue;
        }
        // surface lines
        for key in ["value: ", "channels: ", "color-space: ", "b: ", "p: "] {
            let mut pos = 0;
            while let Some(at) = flat[pos..].find(key) {
                let start = pos + at + key.len();
                let line = flat[start..].split(" | ").next().unwrap_or("");
                let val = line.trim_end_matches(';').trim();
                if !val.is_empty() {
                    let space = val
                        .find('(')
                        .map(|o| val[..o].replace("color", "").trim().to_string())
                        .unwrap_or_default();
                    let inside = val
                        .find('(')
                        .and_then(|o| val.rfind(')').map(|c| &val[o + 1..c]))
                        .unwrap_or("");
                    let mask: String = inside
                        .split([' ', '/'])
                        .filter(|t| !t.is_empty())
                        .map(|t| if t == "none" { "N" } else { "v" })
                        .collect::<Vec<_>>()
                        .join("");
                    rows.push(format!(
                        "{rel}\t#{}\t{key}\t{space}\t{mask}\t{}",
                        i - 1,
                        val.replace('\t', " ")
                    ));
                }
                pos = start;
            }
        }
    }
}

/// Fit the oklab→XYZ-D65 composite `G = M_srgb→xyz · M1⁻¹` (9 unknowns) from
/// `to_space/oklab/xyz.hrx` golden pairs: xyz = G·(M2⁻¹·lab)∘3, linear in G
/// once the cbrt-domain vector is computed with the published inverse M2.
///
/// Per-kernel scope (bisect FINAL, w2-surface-matrix notes): the fitted G is
/// ONLY for lossy-mix payloads — never the to_rgb/to_gamut kernel (that swap
/// regressed 225). Data = oklab AND oklch sources (polar → rect here), far
/// rows excluded (|a|,|b| ≥ 100 distorts conditioning).
#[test]
#[ignore]
fn fit_oklab_segment() {
    let root = Path::new("../../sass-spec/spec/core_functions/color/to_space");
    let pairs = collect_pairs(root);
    // M2⁻¹ (published OKLAB_LMS_TO_LAB inverse, same numbers production uses).
    let m2 = [
        [0.2104542553f64, 0.7936177850, -0.0040720469],
        [1.9779984951, -2.4285922050, 0.4505937099],
        [0.0259040371, 0.7827717662, -0.8086757540],
    ];
    let m2i = inv_mat(&m2);
    let group: Vec<&Pair> = pairs
        .iter()
        .filter(|p| {
            p.tgt == "xyz"
                && p.ins[0].abs() < 200.0
                && p.ins[1].abs() < 100.0
                && p.ins[2].abs() < 400.0
        })
        .filter(|p| {
            // rect-domain selection: oklab (a,b) or oklch (C,h → rect below)
            p.src == "oklab" || p.src == "oklch"
        })
        .collect();
    let only_lab: Vec<&Pair> = pairs
        .iter()
        .filter(|p| {
            p.tgt == "xyz"
                && (p.src == "oklab" || p.src == "oklch")
                && p.ins[0].abs() < 200.0
                && p.ins[1].abs() < 100.0
        })
        .collect();
    let fit = |rows: Vec<&Pair>, polar: bool| -> Option<([f64; 9], f64, usize)> {
        let n = 9;
        let mut ata = vec![vec![0.0f64; n]; n];
        let mut atb = vec![0.0f64; n];
        let mut work: Vec<([f64; 3], [f64; 3])> = Vec::new();
        for p in rows {
            let l = p.ins[0] / 100.0;
            let (a, b) = if polar {
                let h = p.ins[2].to_radians();
                (p.ins[1] * h.cos(), p.ins[1] * h.sin())
            } else {
                (p.ins[1], p.ins[2])
            };
            let u = [l, a, b];
            let mut w = [0.0f64; 3];
            for i in 0..3 {
                w[i] = m2i[i][0] * u[0] + m2i[i][1] * u[1] + m2i[i][2] * u[2];
                w[i] = w[i] * w[i] * w[i];
            }
            for r in 0..3 {
                for c in 0..3 {
                    for k in 0..3 {
                        ata[r * 3 + c][r * 3 + k] += w[c] * w[k];
                    }
                    atb[r * 3 + c] += w[c] * p.outs[r];
                }
            }
            work.push((w, [p.outs[0], p.outs[1], p.outs[2]]));
        }
        let x = solve_mat(ata, atb)?;
        let mut worst = 0.0f64;
        for (w, outs) in &work {
            for r in 0..3 {
                let got = x[r * 3] * w[0] + x[r * 3 + 1] * w[1] + x[r * 3 + 2] * w[2];
                let d = outs[r].abs().max(1e-9);
                worst = worst.max((got - outs[r]).abs() / d);
            }
        }
        Some(([x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7], x[8]], worst, work.len()))
    };
    let _ = group;
    for polar in [false, true] {
        let rows: Vec<&Pair> = only_lab
            .iter()
            .filter(|p| (p.src == "oklch") == polar)
            .copied()
            .collect();
        if let Some((g, worst, k)) = fit(rows, polar) {
            println!(
                "mix-payload G ({} sources, {k} rows) = [{}, {}, {}], [{}, {}, {}], [{}, {}, {}]\n  max-rel-residual {worst:e}",
                if polar { "oklch" } else { "oklab" },
                g[0], g[1], g[2], g[3], g[4], g[5], g[6], g[7], g[8]
            );
        }
    }
    // Joint fit (both sources together) is the candidate constant.
    let rows: Vec<&Pair> = only_lab.clone();
    if let Some((g, worst, k)) = fit_polar_mix(&rows, &m2i) {
        println!(
            "MIX_OKLAB_TO_XYZ joint ({k} rows) = [{}, {}, {}], [{}, {}, {}], [{}, {}, {}]\n  max-rel-residual {worst:e}",
            g[0], g[1], g[2], g[3], g[4], g[5], g[6], g[7], g[8]
        );
    }
}

/// Joint fit over mixed oklab/oklch source rows (polar converted per row).
fn fit_polar_mix(rows: &[&Pair], m2i: &[[f64; 3]; 3]) -> Option<([f64; 9], f64, usize)> {
    let n = 9;
    let mut ata = vec![vec![0.0f64; n]; n];
    let mut atb = vec![0.0f64; n];
    let mut work: Vec<([f64; 3], [f64; 3])> = Vec::new();
    for p in rows {
        let l = p.ins[0] / 100.0;
        let (a, b) = if p.src == "oklch" {
            let h = p.ins[2].to_radians();
            (p.ins[1] * h.cos(), p.ins[1] * h.sin())
        } else {
            (p.ins[1], p.ins[2])
        };
        let u = [l, a, b];
        let mut w = [0.0f64; 3];
        for i in 0..3 {
            w[i] = m2i[i][0] * u[0] + m2i[i][1] * u[1] + m2i[i][2] * u[2];
            w[i] = w[i] * w[i] * w[i];
        }
        for r in 0..3 {
            for c in 0..3 {
                for k in 0..3 {
                    ata[r * 3 + c][r * 3 + k] += w[c] * w[k];
                }
                atb[r * 3 + c] += w[c] * p.outs[r];
            }
        }
        work.push((w, [p.outs[0], p.outs[1], p.outs[2]]));
    }
    let x = solve_mat(ata, atb)?;
    let mut worst = 0.0f64;
    for (w, outs) in &work {
        for r in 0..3 {
            let got = x[r * 3] * w[0] + x[r * 3 + 1] * w[1] + x[r * 3 + 2] * w[2];
            let d = outs[r].abs().max(1e-9);
            worst = worst.max((got - outs[r]).abs() / d);
        }
    }
    Some(([x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7], x[8]], worst, work.len()))
}

/// 3×3 inverse via Gauss-Jordan (solver-local, mirrors production `inv`).
fn inv_mat(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut a = [
        [m[0][0], m[0][1], m[0][2], 1.0, 0.0, 0.0],
        [m[1][0], m[1][1], m[1][2], 0.0, 1.0, 0.0],
        [m[2][0], m[2][1], m[2][2], 0.0, 0.0, 1.0],
    ];
    for col in 0..3 {
        let piv = (col..3)
            .max_by(|i, j| {
                a[*i][col]
                    .abs()
                    .partial_cmp(&a[*j][col].abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        a.swap(col, piv);
        let d = a[col][col];
        for j in 0..6 {
            a[col][j] /= d;
        }
        for i in 0..3 {
            if i != col {
                let f = a[i][col];
                for j in 0..6 {
                    a[i][j] -= f * a[col][j];
                }
            }
        }
    }
    [
        [a[0][3], a[0][4], a[0][5]],
        [a[1][3], a[1][4], a[1][5]],
        [a[2][3], a[2][4], a[2][5]],
    ]
}

fn solve_mat(a: Vec<Vec<f64>>, b: Vec<f64>) -> Option<Vec<f64>> {
    solve(a, b)
}

#[test]
#[ignore]
fn diagnose_parse() {
    let root = Path::new("../../sass-spec/spec/core_functions/color/to_space");
    let f = root.join("a98_rgb/lab.hrx");
    let text = std::fs::read_to_string(&f).expect("read");
    let secs: Vec<&str> = text
        .split("================================================================================")
        .collect();
    println!("sections: {} collect={}", secs.len(), collect_pairs(root).len());
    let s = secs[3];
    let call_at = s.find("to-space(").unwrap();
    let call = &s[call_at + 9..];
    let close = balanced_end(call).unwrap();
    let inner = &call[..close];
    println!("inner={inner:?}");
    let comma = last_split_comma(inner);
    println!("comma-literal={:?}", comma.map(|c| &inner[..c]));
    let lit = comma.map(|c| split_literal(inner[..c].trim()));
    println!("lit={lit:?}");
    let ch = lit.flatten().as_ref().and_then(|(_, b)| channel_value(b));
    println!("ch={ch:?}");
    let css_at = s.find("output.css");
    println!("css_at={css_at:?}");
    if let Some(ca) = css_at {
        let tail = &s[ca..];
        let b_at = tail.find("b: ");
        println!("b_at={b_at:?}");
        if let Some(ba) = b_at {
            let line = tail[ba + 3..].lines().next().unwrap_or("").trim_end_matches([';', ' ']);
            println!("line={line:?}");
            println!("oline={:?}", split_literal(line));
        }
    }
}

/// Layer-2 candidate hunt (approach b): find an exact-matrix composition that
/// reproduces golden `2.0602077969 1.6344741917 1.0169248199` for the
/// change/oklch L=120% payload, and the golden oklab→xyz rows. Variants:
/// published M1/M2 vs Ottosson's exact coefficients, left/right multiply of
/// the srgb→xyz lift, lightness /100 vs *100 rounding paths.
#[test]
#[ignore]
fn hunt_oklab_payload_exact() {
    // change/oklch(50% 0.2 30deg){$lightness:120%} → l=1.2, a=0.2cos30, b=0.2sin30
    let a = 0.2f64 * 30f64.to_radians().cos();
    let b = 0.2f64 * 30f64.to_radians().sin();
    let u = [1.2f64, a, b];
    // Ottosson's published M1 (linear sRGB → LMS) and M2 exactly as the CSS
    // spec / his blog rounds them; the current crate uses the same.
    let m1 = [
        [0.4122214708f64, 0.5363325363, 0.0514459929],
        [0.2119034982, 0.6806995451, 0.1073969566],
        [0.0883024619, 0.2817188376, 0.6299787005],
    ];
    let m2 = [
        [0.2104542553f64, 0.7936177850, -0.0040720469],
        [1.9779984951, -2.4285922050, 0.4505937099],
        [0.0259040371, 0.7827717662, -0.8086757540],
    ];
    let m2i = inv_mat(&m2);
    let mut w = [0.0f64; 3];
    for i in 0..3 {
        w[i] = m2i[i][0] * u[0] + m2i[i][1] * u[1] + m2i[i][2] * u[2];
        w[i] = w[i] * w[i] * w[i];
    }
    let m1i = inv_mat(&m1);
    // lin-srgb = M1⁻¹·w  (two orders: as written vs transpose-rounded published inverse)
    let lin = [
        m1i[0][0] * w[0] + m1i[0][1] * w[1] + m1i[0][2] * w[2],
        m1i[1][0] * w[0] + m1i[1][1] * w[1] + m1i[1][2] * w[2],
        m1i[2][0] * w[0] + m1i[2][1] * w[1] + m1i[2][2] * w[2],
    ];
    // srgb→xyz exact rationals (production lin_srgb_to_xyz)
    let s = [
        [506752.0f64 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
        [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
        [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
    ];
    let xyz = [
        s[0][0] * lin[0] + s[0][1] * lin[1] + s[0][2] * lin[2],
        s[1][0] * lin[0] + s[1][1] * lin[1] + s[1][2] * lin[2],
        s[2][0] * lin[0] + s[2][1] * lin[1] + s[2][2] * lin[2],
    ];
    println!(
        "reproduce golden 2.0602077969 1.6344741917 1.0169248199\n  got {xyz:?}"
    );
    // Also the exact-decimal Ottosson M1 from his blog (4th/5th digit variants)
    // is unknown locally — print the residual against golden for the record.
    println!(
        "deltas: {:e} {:e} {:e}",
        xyz[0] - 2.0602077969,
        xyz[1] - 1.6344741917,
        xyz[2] - 1.0169248199
    );
}

/// Print the raw oklab/oklch→xyz golden rows (diagnose whether stored l/a/b
/// are display-% or working 0..1 — the 4-row fit mixed domains and the
/// extrapolation collapsed).
#[test]
#[ignore]
fn dump_oklab_xyz_rows() {
    let root = Path::new("../../sass-spec/spec/core_functions/color/to_space");
    for p in collect_pairs(root)
        .iter()
        .filter(|p| (p.src == "oklab" || p.src == "oklch") && p.tgt == "xyz")
    {
        println!("{} ins={:?} outs={:?}", p.src, p.ins, p.outs);
    }
}

/// Clean joint fit on verified display-domain rows (oklab 4 + oklch 3 with
/// local polar→rect), excluding zeros and the -999999 far row.
#[test]
#[ignore]
fn fit_oklab_joint_clean() {
    let root = Path::new("../../sass-spec/spec/core_functions/color/to_space");
    let pairs = collect_pairs(root);
    // (l_display_pct, a, b, xyz)
    let rows: Vec<([f64; 3], [f64; 3])> = pairs
        .iter()
        .filter(|p| (p.src == "oklab" || p.src == "oklch") && p.tgt == "xyz")
        .filter(|p| !(p.ins[0] == 0.0 && p.ins[1] == 0.0 && p.ins[2] == 0.0))
        .filter(|p| p.ins[1].abs() < 1.0 && p.ins[2].abs() < 360.0)
        .map(|p| {
            let u = if p.src == "oklab" {
                [p.ins[0], p.ins[1], p.ins[2]]
            } else {
                let h = p.ins[2].to_radians();
                [p.ins[0], p.ins[1] * h.cos(), p.ins[1] * h.sin()]
            };
            (u, [p.outs[0], p.outs[1], p.outs[2]])
        })
        .collect();
    println!("joint clean rows: {}", rows.len());
    let m2 = [
        [0.2104542553f64, 0.7936177850, -0.0040720469],
        [1.9779984951, -2.4285922050, 0.4505937099],
        [0.0259040371, 0.7827717662, -0.8086757540],
    ];
    let m2i = inv_mat(&m2);
    let n = 9;
    let mut ata = vec![vec![0.0f64; n]; n];
    let mut atb = vec![0.0f64; n];
    let mut ws: Vec<([f64; 3], [f64; 3])> = Vec::new();
    for (u, o) in &rows {
        let l = u[0] / 100.0;
        let mut w = [0.0f64; 3];
        for i in 0..3 {
            w[i] = m2i[i][0] * l + m2i[i][1] * u[1] + m2i[i][2] * u[2];
            w[i] = w[i] * w[i] * w[i];
        }
        for r in 0..3 {
            for c in 0..3 {
                for k in 0..3 {
                    ata[r * 3 + c][r * 3 + k] += w[c] * w[k];
                }
                atb[r * 3 + c] += w[c] * o[r];
            }
        }
        ws.push((w, *o));
    }
    let x = solve_mat(ata, atb).expect("solve");
    let mut worst = 0.0f64;
    for (w, o) in &ws {
        for r in 0..3 {
            let got = x[r * 3] * w[0] + x[r * 3 + 1] * w[1] + x[r * 3 + 2] * w[2];
            let d = o[r].abs().max(1e-9);
            worst = worst.max((got - o[r]).abs() / d);
        }
    }
    println!(
        "G-clean = [{}, {}, {}], [{}, {}, {}], [{}, {}, {}]\n  worst rel {worst:e}",
        x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7], x[8]
    );
    // Extrapolate the change/oklch payload point with G-clean.
    let a = 0.2f64 * 30f64.to_radians().cos();
    let b = 0.2f64 * 30f64.to_radians().sin();
    let u = [1.2f64, a, b];
    let mut w = [0.0f64; 3];
    for i in 0..3 {
        w[i] = m2i[i][0] * u[0] + m2i[i][1] * u[1] + m2i[i][2] * u[2];
        w[i] = w[i] * w[i] * w[i];
    }
    let got = [
        x[0] * w[0] + x[1] * w[1] + x[2] * w[2],
        x[3] * w[0] + x[4] * w[1] + x[5] * w[2],
        x[6] * w[0] + x[7] * w[1] + x[8] * w[2],
    ];
    println!("payload pred {got:?} vs golden 2.0602077969 1.6344741917 1.0169248199");
}
