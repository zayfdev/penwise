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
const USAGE: &str = "usage: penwise-fixtures convert <fixtures/raw/SESSION> <OUT_DIR>";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, input, output] if cmd == "convert" => {
            convert_session(Path::new(input), Path::new(output))
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
