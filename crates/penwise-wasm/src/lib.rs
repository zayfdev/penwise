//! Browser bindings for the ink engine: build a stroke's geometry once and hand the viewer flat arrays.
//!
//! No geometry lives here; this only converts between the JS/JSON shapes and [`penwise_core`].

use penwise_core::brush::{BlendMode, BrushParams, Preset};
use penwise_core::builder::StrokeBuilder;
use penwise_core::geometry::StrokeGeometry;
use penwise_core::sample::Sample;
use wasm_bindgen::prelude::*;

/// Values per sample in [`Stroke::push`] (`x, y, pressure, tilt, azimuth`).
const SAMPLE_STRIDE: usize = 5;
/// Values per dab in the serialized geometry.
const DAB_STRIDE: usize = 9;

/// One stroke being measured. Feed every sample in order, then [`Stroke::finish`] once.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Stroke {
    builder: Option<StrokeBuilder>,
    argb: u32,
    batch: Vec<Sample>,
}

#[derive(serde::Serialize)]
struct GeometryJson {
    /// True when the stroke is drawn as dabs (textured brushes) rather than one filled outline.
    stamps: bool,
    /// Outline points, x/y interleaved.
    outline: Vec<f32>,
    /// Exclusive end index (in points) of each outline contour.
    ends: Vec<u32>,
    /// Dabs: `x, y, radiusX, radiusY, angle, opacity, hardness, grainStrength, grainLevel`.
    dabs: Vec<f32>,
    argb: u32,
    opacity: f32,
    multiply: bool,
    /// Number of outline points (JS side convenience; `outline.length / 2`).
    points: usize,
}

#[wasm_bindgen]
impl Stroke {
    /// `preset` indexes [`Preset::ALL`]; `scale` multiplies the preset's size range (the app's size slider).
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(preset: u32, seed: u64, argb: u32, scale: f32) -> Stroke {
        let params = params_for(preset, scale);
        Stroke {
            builder: Some(StrokeBuilder::new(&params, seed)),
            argb,
            batch: Vec::new(),
        }
    }

    /// Appends samples: five floats per sample and one `t_us` per sample (the shorter length wins).
    pub fn push(&mut self, samples: &[f32], times: &[i32]) {
        let Some(builder) = self.builder.as_mut() else {
            return;
        };
        let count = (samples.len() / SAMPLE_STRIDE).min(times.len());
        self.batch.clear();
        for (i, time) in times.iter().take(count).enumerate() {
            let o = i * SAMPLE_STRIDE;
            self.batch.push(Sample {
                x: samples[o],
                y: samples[o + 1],
                pressure: samples[o + 2],
                tilt: samples[o + 3],
                azimuth: samples[o + 4],
                t_us: u32::try_from((*time).max(0)).unwrap_or(0),
            });
        }
        builder.push(&self.batch);
    }

    /// Final geometry as JSON; the stroke is consumed afterwards.
    pub fn finish(&mut self) -> String {
        let Some(builder) = self.builder.take() else {
            return EMPTY.to_owned();
        };
        let geometry = builder.finish();
        to_json(&geometry, self.argb)
    }
}

/// Builds a whole stroke in one call (finished strokes and live previews both use it).
#[wasm_bindgen]
pub fn geometry(
    preset: u32,
    seed: u64,
    argb: u32,
    scale: f32,
    samples: &[f32],
    times: &[i32],
) -> String {
    let mut stroke = Stroke::new(preset, seed, argb, scale);
    stroke.push(samples, times);
    stroke.finish()
}

const EMPTY: &str = "{\"stamps\":false,\"outline\":[],\"ends\":[],\"dabs\":[],\"argb\":0,\"opacity\":0,\"multiply\":false,\"points\":0}";

fn params_for(preset: u32, scale: f32) -> BrushParams {
    let index = (preset as usize).min(Preset::ALL.len().saturating_sub(1));
    let mut params = Preset::ALL[index].params();
    if scale.is_finite() && scale > 0.0 {
        params.size_min *= scale;
        params.size_max *= scale;
    }
    params
}

fn to_json(geometry: &StrokeGeometry, argb: u32) -> String {
    let style = geometry.style();
    let mut out = GeometryJson {
        stamps: false,
        outline: Vec::new(),
        ends: Vec::new(),
        dabs: Vec::new(),
        opacity: style.opacity,
        multiply: matches!(style.blend, BlendMode::Multiply),
        argb,
        points: 0,
    };
    match geometry {
        StrokeGeometry::Outline { outline, .. } => {
            out.outline.reserve(outline.points.len() * 2);
            for point in &outline.points {
                out.outline.push(point[0]);
                out.outline.push(point[1]);
            }
            out.points = outline.points.len();
            out.ends.clone_from(&outline.contour_ends);
        }
        StrokeGeometry::Stamps { dabs, .. } => {
            out.stamps = true;
            out.dabs.reserve(dabs.len() * DAB_STRIDE);
            for dab in dabs {
                out.dabs.extend_from_slice(&[
                    dab.x,
                    dab.y,
                    dab.radius_x,
                    dab.radius_y,
                    dab.angle,
                    dab.opacity,
                    dab.hardness,
                    dab.grain_strength,
                    dab.grain_level,
                ]);
            }
        }
    }
    serde_json::to_string(&out).unwrap_or_else(|_| EMPTY.to_owned())
}
