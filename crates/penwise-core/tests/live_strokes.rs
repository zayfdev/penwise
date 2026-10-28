//! Real pen recordings fed to the builder one sample at a time, as the pen delivers them: whatever a frame shows as
//! stable is still there in every later frame and in the finished stroke, and the last frame is the finished stroke
//! (no jump on pen-up).

#![allow(
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

use penwise_core::codec::decode_pwrec;
use penwise_core::codec::pwrec::RunKind;
use penwise_core::{BrushFamily, GeometryView, Preset, Sample, StrokeBuilder, StrokeGeometry};

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

const FIXTURES: [(&str, &[u8]); 4] = [
    fixture!("01-sentence"),
    fixture!("02-small"),
    fixture!("03-fast"),
    fixture!("08-curves"),
];

const BRUSHES: [Preset; 4] = [
    Preset::Ballpoint,
    Preset::Fountain,
    Preset::Marker,
    Preset::Pencil,
];

fn contact_strokes(name: &str, bytes: &[u8]) -> Vec<Vec<Sample>> {
    let rec = decode_pwrec(bytes).unwrap_or_else(|e| panic!("{name}.pwrec does not decode: {e}"));
    let ranges = rec
        .digitizer_ranges()
        .unwrap_or_else(|e| panic!("{name}.pwrec has no usable ranges: {e}"));
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

#[test]
fn recorded_strokes_never_move_once_shown() {
    for (name, bytes) in FIXTURES {
        for (index, samples) in contact_strokes(name, bytes).iter().enumerate() {
            for preset in BRUSHES {
                let what = format!("{name} stroke {index} / {}", preset.name());
                let mut builder = StrokeBuilder::new(&preset.params(), 9);
                let mut left: Vec<[f32; 2]> = Vec::new();
                let mut right: Vec<[f32; 2]> = Vec::new();
                let mut dabs = Vec::new();
                let mut last_live = (Vec::new(), Vec::new());
                for s in samples {
                    builder.push(std::slice::from_ref(s));
                    match builder.geometry() {
                        GeometryView::Outline { outline, .. } => {
                            assert!(outline.stable_left.starts_with(&left), "{what}: left moved");
                            assert!(
                                outline.stable_right.starts_with(&right),
                                "{what}: right moved"
                            );
                            left = outline.stable_left.to_vec();
                            right = outline.stable_right.to_vec();
                            let mut polygon = Vec::new();
                            outline.write_polygon(&mut polygon);
                            last_live = (polygon, Vec::new());
                        }
                        GeometryView::Stamps {
                            stable, tentative, ..
                        } => {
                            assert!(stable.starts_with(&dabs), "{what}: stable dabs moved");
                            dabs = stable.to_vec();
                            last_live = (
                                Vec::new(),
                                stable.iter().chain(tentative).copied().collect(),
                            );
                        }
                    }
                }
                match (builder.finish(), preset.params().family) {
                    (StrokeGeometry::Outline { outline, .. }, BrushFamily::Outline(_)) => {
                        assert!(
                            outline.points.starts_with(&left),
                            "{what}: final lost the left side"
                        );
                        assert!(
                            outline.points == last_live.0,
                            "{what}: pen-up changed the ink"
                        );
                    }
                    (StrokeGeometry::Stamps { dabs: fin, .. }, BrushFamily::Stamp(_)) => {
                        assert!(fin.starts_with(&dabs), "{what}: final lost stable dabs");
                        assert!(fin == last_live.1, "{what}: pen-up changed the dabs");
                    }
                    _ => panic!("{what}: family mismatch"),
                }
            }
        }
    }
}
