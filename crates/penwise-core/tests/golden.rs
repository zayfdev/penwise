//! Owner: slice f.
//!
//! Geometry goldens (core-determinism.md §4–5): every `.pwrec` fixture × every [`Preset`] is built stroke by
//! stroke and summarized as one line `<fixture> <preset> strokes=<n> <verts|dabs>=<n> bbox=[…] fnv=<hex>`, the
//! hash being FNV-1a 64 over the concatenated `canonical_bytes()` of every stroke. The same comparisons run
//! natively and on wasm32 (`wasm-pack test --node crates/penwise-core`); fixtures and goldens are compiled in.
//!
//! Bless (native only): `PENWISE_BLESS=1 cargo test -p penwise-core --test golden` rewrites
//! `tests/goldens/geometry.txt`. On a native mismatch the actual geometry is dumped to
//! `target/tmp/penwise-golden/<fixture>-<preset>.{bin,svg}`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use penwise_core::codec::decode_pwrec;
use penwise_core::codec::pwrec::RunKind;
use penwise_core::float::max;
use penwise_core::hash::{Fnv1a64, fnv1a64};
use penwise_core::{
    Bounds, BrushFamily, GeometryView, Preset, Sample, StrokeBuilder, StrokeGeometry,
};

macro_rules! fixture {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/pwrec/",
                $name,
                ".pwrec"
            )) as &[u8],
        )
    };
}

/// Session `fixtures/raw/20261024-094722`, converted by `just fixtures`.
const FIXTURES: [(&str, &[u8]); 13] = [
    fixture!("01-sentence"),
    fixture!("02-small"),
    fixture!("03-fast"),
    fixture!("04-pressure-ramp"),
    fixture!("05-pressure-steady"),
    fixture!("06-speed"),
    fixture!("07-tilt-shade"),
    fixture!("08-curves"),
    fixture!("09-dots"),
    fixture!("10-shapes"),
    fixture!("11-scrub"),
    fixture!("12-hover"),
    fixture!("13-palm"),
];

const GOLDEN: &str = include_str!("goldens/geometry.txt");
const GOLDEN_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/goldens/geometry.txt");
const GOLDEN_HEADER: &str = "# penwise geometry goldens v1 — regenerate: just bless (PENWISE_BLESS=1 cargo test -p penwise-core --test golden)";
const DUMP_DIR: &str = concat!(env!("CARGO_TARGET_TMPDIR"), "/penwise-golden");
/// Push chunk sizes for the chunk-invariance golden are drawn from `0..CHUNK_SIZES` (0 = empty push).
const CHUNK_SIZES: u64 = 25;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn geometry_goldens() {
    let expected = parse_golden(GOLDEN);
    let bless = blessing();
    let mut actual = Vec::new();
    let mut failures = Vec::new();
    for (fixture, bytes) in FIXTURES {
        let strokes = contact_strokes(fixture, bytes);
        for preset in Preset::ALL {
            let geoms = build_one_shot(fixture, &strokes, preset);
            let line = summary_line(fixture, preset, &geoms);
            match expected.get(line_key(&line)) {
                Some(want) if *want == line => {}
                _ if bless => {}
                want => {
                    failures.push(format!(
                        "expected: {}\n  actual: {line}",
                        want.unwrap_or(&"<missing>")
                    ));
                    if !cfg!(target_arch = "wasm32")
                        && let Err(err) = dump(fixture, preset, &geoms)
                    {
                        failures.push(format!("  (dump failed: {err})"));
                    }
                }
            }
            actual.push(line);
        }
    }
    if bless {
        write_golden(&actual);
        return;
    }
    for (key, line) in &expected {
        if !actual.iter().any(|a| line_key(a) == *key) {
            failures.push(format!("stale golden: {line}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} geometry golden mismatch(es); review, then `just bless` (native dumps in {DUMP_DIR}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Chunk invariance on real recordings: every stroke fed in pseudo-random chunk sizes (including empty pushes,
/// with a live `geometry()` read between pushes) must summarize to the same line as one `push`.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chunked_push_matches_one_shot() {
    let mut failures = Vec::new();
    for (fixture, bytes) in FIXTURES {
        let strokes = contact_strokes(fixture, bytes);
        for preset in Preset::ALL {
            let one_shot =
                summary_line(fixture, preset, &build_one_shot(fixture, &strokes, preset));
            let chunked = summary_line(fixture, preset, &build_chunked(fixture, &strokes, preset));
            if one_shot != chunked {
                failures.push(format!("one-shot: {one_shot}\n  chunked: {chunked}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} chunking-dependent result(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Contact strokes of a fixture, mapped through the header's digitizer ranges and quantized.
fn contact_strokes(fixture: &str, bytes: &[u8]) -> Vec<Vec<Sample>> {
    let rec =
        decode_pwrec(bytes).unwrap_or_else(|e| panic!("{fixture}.pwrec does not decode: {e}"));
    let ranges = rec
        .digitizer_ranges()
        .unwrap_or_else(|e| panic!("{fixture}.pwrec has no usable ranges: {e}"));
    rec.strokes
        .iter()
        .filter(|s| s.kind == RunKind::Contact)
        .map(|s| {
            let mut samples = Vec::new();
            s.to_samples(&ranges, &mut samples);
            samples
        })
        .collect()
}

/// Per-stroke PRNG seed: FNV-1a 64 of the fixture name xor the stroke index.
fn stroke_seed(fixture: &str, index: u64) -> u64 {
    fnv1a64(fixture.as_bytes()) ^ index
}

fn build_one_shot(fixture: &str, strokes: &[Vec<Sample>], preset: Preset) -> Vec<StrokeGeometry> {
    let params = preset.params();
    (0u64..)
        .zip(strokes)
        .map(|(i, samples)| StrokeBuilder::build(&params, stroke_seed(fixture, i), samples))
        .collect()
}

fn build_chunked(fixture: &str, strokes: &[Vec<Sample>], preset: Preset) -> Vec<StrokeGeometry> {
    let params = preset.params();
    let mut rng = SplitMix64(fnv1a64(preset.name().as_bytes()) ^ fnv1a64(fixture.as_bytes()));
    (0u64..)
        .zip(strokes)
        .map(|(i, samples)| {
            let mut builder = StrokeBuilder::new(&params, stroke_seed(fixture, i));
            let mut rest = samples.as_slice();
            let mut stable_dabs = 0;
            let mut polygon = Vec::new();
            while !rest.is_empty() {
                let n = usize::try_from(rng.next_u64() % CHUNK_SIZES).unwrap();
                let (head, tail) = rest.split_at(n.min(rest.len()));
                builder.push(head);
                match builder.geometry() {
                    GeometryView::Outline { outline, .. } => outline.write_polygon(&mut polygon),
                    GeometryView::Stamps { stable, .. } => {
                        assert!(
                            stable.len() >= stable_dabs,
                            "{fixture} stroke {i}: stable dabs shrank"
                        );
                        stable_dabs = stable.len();
                    }
                }
                rest = tail;
            }
            builder.finish()
        })
        .collect()
}

/// `<fixture> <preset> strokes=<n> <verts|dabs>=<n> bbox=[min_x,min_y,max_x,max_y] fnv=<016x>`.
fn summary_line(fixture: &str, preset: Preset, geoms: &[StrokeGeometry]) -> String {
    let mut hash = Fnv1a64::new();
    let mut items = 0usize;
    let mut bounds = Bounds::EMPTY;
    for (i, g) in geoms.iter().enumerate() {
        items += match g {
            StrokeGeometry::Outline { outline, .. } => outline.points.len(),
            StrokeGeometry::Stamps { dabs, .. } => dabs.len(),
        };
        bounds = bounds.union(&g.bounds());
        let bytes = g.canonical_bytes().unwrap_or_else(|e| {
            panic!(
                "{fixture} {} stroke {i}: non-finite geometry: {e:?}",
                preset.name()
            )
        });
        hash.write(&bytes);
    }
    let unit = match preset.params().family {
        BrushFamily::Outline(_) => "verts",
        BrushFamily::Stamp(_) => "dabs",
    };
    format!(
        "{fixture} {} strokes={} {unit}={items} bbox=[{:?},{:?},{:?},{:?}] fnv={:016x}",
        preset.name(),
        geoms.len(),
        bounds.min_x,
        bounds.min_y,
        bounds.max_x,
        bounds.max_y,
        hash.finish()
    )
}

/// `"<fixture> <preset>"`: the part of a summary line that identifies it.
fn line_key(line: &str) -> &str {
    line.match_indices(' ')
        .nth(1)
        .map_or(line, |(i, _)| &line[..i])
}

/// Golden lines by key; `#` comments and blank lines are skipped. Duplicate keys are a corrupt file.
fn parse_golden(text: &str) -> BTreeMap<&str, &str> {
    let mut lines = BTreeMap::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let previous = lines.insert(line_key(line), line);
        assert!(previous.is_none(), "duplicate golden entry: {line}");
    }
    lines
}

/// `PENWISE_BLESS` set to anything but empty or `0` (never on wasm: no process env).
fn blessing() -> bool {
    std::env::var_os("PENWISE_BLESS").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Rewrites the golden file with exactly `lines` (fixture order × `Preset::ALL` order; stale entries dropped).
fn write_golden(lines: &[String]) {
    let mut text = String::from(GOLDEN_HEADER);
    text.push('\n');
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(GOLDEN_PATH, text).unwrap_or_else(|e| panic!("writing {GOLDEN_PATH}: {e}"));
}

/// Native failure artifacts: concatenated canonical bytes (`.bin`) and a viewable `.svg`.
fn dump(fixture: &str, preset: Preset, geoms: &[StrokeGeometry]) -> std::io::Result<()> {
    let dir = Path::new(DUMP_DIR);
    std::fs::create_dir_all(dir)?;
    let stem = format!("{fixture}-{}", preset.name());
    let mut bin = Vec::new();
    for g in geoms {
        if let Ok(bytes) = g.canonical_bytes() {
            bin.extend_from_slice(&bytes);
        }
    }
    std::fs::write(dir.join(format!("{stem}.bin")), bin)?;
    std::fs::write(dir.join(format!("{stem}.svg")), svg(geoms))
}

fn svg(geoms: &[StrokeGeometry]) -> String {
    let b = geoms
        .iter()
        .fold(Bounds::EMPTY, |acc, g| acc.union(&g.bounds()));
    let (x, y, w, h) = if b.is_empty() {
        (0.0, 0.0, 1.0, 1.0)
    } else {
        (b.min_x, b.min_y, max(b.width(), 1.0), max(b.height(), 1.0))
    };
    let mut s = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x} {y} {w} {h}">"#);
    s.push('\n');
    for g in geoms {
        let style = g.style();
        let c = style.color;
        let fill = format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
        let alpha = f32::from(c.a) / 255.0 * style.opacity;
        match g {
            StrokeGeometry::Outline { outline, .. } => {
                let mut d = String::new();
                let mut start = 0usize;
                for &end in &outline.contour_ends {
                    let end = usize::try_from(end).unwrap();
                    let contour = outline.points.get(start..end).unwrap_or(&[]);
                    for (k, p) in contour.iter().enumerate() {
                        write!(d, "{}{} {} ", if k == 0 { "M" } else { "L" }, p[0], p[1]).unwrap();
                    }
                    d.push_str("Z ");
                    start = end;
                }
                writeln!(
                    s,
                    r#"<path fill-rule="nonzero" fill="{fill}" fill-opacity="{alpha}" d="{d}"/>"#
                )
                .unwrap();
            }
            StrokeGeometry::Stamps { dabs, .. } => {
                for dab in dabs {
                    writeln!(
                        s,
                        r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" transform="rotate({} {} {})" fill="{fill}" fill-opacity="{}"/>"#,
                        dab.x,
                        dab.y,
                        dab.radius_x,
                        dab.radius_y,
                        dab.angle.to_degrees(),
                        dab.x,
                        dab.y,
                        alpha * dab.opacity
                    )
                    .unwrap();
                }
            }
        }
    }
    s.push_str("</svg>\n");
    s
}

/// `SplitMix64` (test-only chunk sizes; geometry randomness lives in `stamp::prng`).
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}
