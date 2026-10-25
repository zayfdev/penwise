//! Fixture converter: `penwise-fixtures convert <fixtures/raw/SESSION> <OUT_DIR>` turns every
//! `NN-<name>.getevent.gz` capture of a recording session into `<OUT_DIR>/NN-<name>.pwrec` (`.pwrec` v1,
//! core-determinism.md §6), using the session's `device.txt` for digitizer ranges. Gzip and text parsing live
//! here so `penwise-core` stays free of I/O; core only owns the `.pwrec` codec.

mod device;
mod getevent;
mod runs;
mod session;

use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail, ensure};
use flate2::read::MultiGzDecoder;
use penwise_core::codec::{decode_pwrec, encode_pwrec};
use penwise_core::sample::Sample;

use crate::session::Session;

const CAPTURE_SUFFIX: &str = ".getevent.gz";
const USAGE: &str = "usage: penwise-fixtures convert <fixtures/raw/SESSION> <OUT_DIR>\n       penwise-fixtures render <fixture.pwrec> <out.png> [scale]\n       penwise-fixtures strokes-json <fixture.pwrec>";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, input, output] if cmd == "convert" => {
            convert_session(Path::new(input), Path::new(output))
        }
        [cmd, input] if cmd == "strokes-json" => dump_strokes(Path::new(input), &mut io::stdout()),
        [cmd, input, output] if cmd == "render" => {
            render_fixture(Path::new(input), Path::new(output), 1.0)
        }
        [cmd, input, output, scale] if cmd == "render" => {
            let scale: f32 = scale.parse().context("scale")?;
            render_fixture(Path::new(input), Path::new(output), scale)
        }
        _ => bail!(USAGE),
    }
}

fn convert_session(input: &Path, output: &Path) -> anyhow::Result<()> {
    let device_txt = input.join("device.txt");
    let device = fs::read_to_string(&device_txt)
        .with_context(|| format!("reading {}", device_txt.display()))?;
    let session = Session::from_device_txt(&device)
        .with_context(|| format!("parsing {}", device_txt.display()))?;
    let captures = list_captures(input)?;
    ensure!(
        !captures.is_empty(),
        "no *{CAPTURE_SUFFIX} captures in {}",
        input.display()
    );
    fs::create_dir_all(output).with_context(|| format!("creating {}", output.display()))?;
    let mut stdout = io::stdout().lock();
    for (name, path) in &captures {
        let summary = convert_capture(&session, path, &output.join(format!("{name}.pwrec")))
            .with_context(|| format!("converting {}", path.display()))?;
        writeln!(stdout, "{name}: {summary}")?;
    }
    Ok(())
}

/// Prints the fixture as server wire strokes (`{"t":"page","strokes":[…]}`), so a recording can be replayed
/// onto a page for tests: the same JSON the tablet sends.
fn dump_strokes(input: &Path, out: &mut dyn Write) -> anyhow::Result<()> {
    let bytes = fs::read(input).with_context(|| format!("reading {}", input.display()))?;
    let capture = decode_pwrec(&bytes).with_context(|| format!("decoding {}", input.display()))?;
    let header = &capture.header;
    let ranges = penwise_core::DigitizerRanges {
        x: header.ranges[0],
        y: header.ranges[1],
        pressure: header.ranges[2],
        tilt_x: header.ranges[3],
        tilt_y: header.ranges[4],
        x_scale: header.x_scale,
        y_scale: header.y_scale,
    };
    // Seeds must be unique per recording, not per stroke index: two fixtures would otherwise carry the same
    // stroke identities and a server that treats seeds as identity would drop the second recording.
    let recording_seed = fnv1a(&bytes);
    let mut strokes = Vec::new();
    let mut samples = Vec::new();
    for (index, stroke) in capture.strokes.iter().enumerate() {
        stroke.to_samples(&ranges, &mut samples);
        if samples.is_empty() {
            continue;
        }
        let flat: Vec<f32> = samples
            .iter()
            .flat_map(|s| [s.x, s.y, s.pressure, s.tilt, s.azimuth])
            .collect();
        let times: Vec<i32> = samples
            .iter()
            .map(|s| i32::try_from(s.t_us).unwrap_or(i32::MAX))
            .collect();
        strokes.push(serde_json::json!({
            "layer": "ink",
            "preset": 0,
            "argb": -16_777_216,
            "scale": 1.0,
            "seed": format!("{}", recording_seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)),
            "samples": flat,
            "times": times,
        }));
    }
    serde_json::to_writer(
        &mut *out,
        &serde_json::json!({ "t": "page", "strokes": strokes }),
    )?;
    out.write_all(b"\n")?;
    Ok(())
}

/// FNV-1a over the file, so a recording's strokes carry ids of their own.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the seed is opaque; only its uniqueness matters"
)]
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

/// Draws a recorded stroke to a PNG: the same engine the tablet, the browser and the server use.
///
/// Useful for looking at what a capture actually contains, and as the input for handwriting tests.
#[allow(
    clippy::cast_possible_truncation,
    reason = "pixel counts from a rendered page size, bounded by the capture's own coordinates"
)]
fn render_fixture(input: &Path, output: &Path, scale: f32) -> anyhow::Result<()> {
    use penwise_core::brush::Preset;
    use penwise_core::builder::StrokeBuilder;
    use penwise_core::raster::png::encode_png;
    use penwise_core::raster::{CoverageBuffer, Viewport, render_strokes};

    let bytes = fs::read(input).with_context(|| format!("reading {}", input.display()))?;
    let capture = decode_pwrec(&bytes).with_context(|| format!("decoding {}", input.display()))?;
    let header = &capture.header;
    let ranges = penwise_core::DigitizerRanges {
        x: header.ranges[0],
        y: header.ranges[1],
        pressure: header.ranges[2],
        tilt_x: header.ranges[3],
        tilt_y: header.ranges[4],
        x_scale: header.x_scale,
        y_scale: header.y_scale,
    };
    let params = Preset::ALL[0].params();
    let mut geometries = Vec::with_capacity(capture.strokes.len());
    let mut samples = Vec::new();
    let mut total = 0;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for stroke in &capture.strokes {
        stroke.to_samples(&ranges, &mut samples);
        if samples.is_empty() {
            continue;
        }
        total += samples.len();
        for sample in &samples {
            min_x = min_x.min(sample.x);
            min_y = min_y.min(sample.y);
            max_x = max_x.max(sample.x);
            max_y = max_y.max(sample.y);
        }
        let mut builder = StrokeBuilder::new(&params, 0x5EED_u64 + stroke.t0_us);
        builder.push(&samples);
        geometries.push(builder.finish());
    }
    let pad = 40.0_f32;
    let width = ((max_x - min_x) + pad * 2.0) * scale;
    let height = ((max_y - min_y) + pad * 2.0) * scale;
    let viewport = Viewport {
        origin_x: min_x - pad,
        origin_y: min_y - pad,
        scale,
    };
    let pixels_x = u32::try_from(width.ceil().max(1.0) as i64).unwrap_or(1);
    let pixels_y = u32::try_from(height.ceil().max(1.0) as i64).unwrap_or(1);
    let mut pixmap = tiny_skia::Pixmap::new(pixels_x, pixels_y).context("page size")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let mut scratch = CoverageBuffer::default();
    render_strokes(&geometries, &viewport, &mut pixmap, &mut scratch)?;
    fs::write(output, encode_png(&pixmap)?)
        .with_context(|| format!("writing {}", output.display()))?;
    let mut stdout = io::stdout().lock();
    writeln!(
        stdout,
        "{}: {}x{} px, {} strokes, {} samples",
        output.display(),
        pixmap.width(),
        pixmap.height(),
        geometries.len(),
        total
    )?;
    Ok(())
}

/// `(name, path)` of every capture in `dir`, sorted by name.
fn list_captures(dir: &Path) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let mut captures = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("listing {}", dir.display()))? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(CAPTURE_SUFFIX))
            .map(str::to_owned);
        if let Some(name) = name {
            captures.push((name, path));
        }
    }
    captures.sort();
    Ok(captures)
}

/// Converts one capture, checks that it decodes back identically and maps to finite samples, writes it, and
/// returns a one-line summary.
fn convert_capture(session: &Session, path: &Path, out: &Path) -> anyhow::Result<String> {
    let mut text = String::new();
    MultiGzDecoder::new(BufReader::new(File::open(path)?)).read_to_string(&mut text)?;
    let rec = session.convert(&text)?;
    let mut bytes = Vec::new();
    encode_pwrec(&rec, &mut bytes);
    ensure!(
        decode_pwrec(&bytes)? == rec,
        "encoded recording does not decode back identically"
    );
    let ranges = rec.digitizer_ranges()?;
    let mut samples: Vec<Sample> = Vec::new();
    let mut frames = 0usize;
    for stroke in &rec.strokes {
        stroke.to_samples(&ranges, &mut samples);
        ensure!(
            samples
                .iter()
                .all(|s| s.x.is_finite() && s.y.is_finite() && s.pressure.is_finite()),
            "stroke at t0 {} µs maps to non-finite samples",
            stroke.t0_us
        );
        frames += samples.len();
    }
    fs::write(out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    Ok(format!(
        "{} strokes, {frames} frames, {} Hz, {} bytes -> {}",
        rec.strokes.len(),
        rec.header.rate_hint,
        bytes.len(),
        out.display()
    ))
}
