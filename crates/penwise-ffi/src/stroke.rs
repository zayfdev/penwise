//! Safe conversion layer between flat JNI arrays and `penwise-core` (no JNI types here).

use std::fmt;

use penwise_core::{
    BlendMode, BrushParams, GeometryView, Preset, Sample, StrokeBuilder, StrokeGeometry,
};

/// Floats per input sample on the Kotlin side: x, y, pressure, tilt, azimuth.
pub const SAMPLE_FLOATS: usize = 5;
/// Floats per output dab: x, y, `radius_x`, `radius_y`, angle, opacity, hardness, `grain_strength`, `grain_level`.
pub const DAB_FLOATS: usize = 9;

/// Errors surfaced to Kotlin as `RuntimeException`s.
#[derive(Debug)]
pub enum FfiError {
    Jni(jni::errors::Error),
    BadArgument(&'static str),
    Finished,
}

impl fmt::Display for FfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Jni(e) => write!(f, "jni: {e}"),
            Self::BadArgument(what) => write!(f, "invalid argument: {what}"),
            Self::Finished => f.write_str("stroke already finished"),
        }
    }
}

impl std::error::Error for FfiError {}

impl From<jni::errors::Error> for FfiError {
    fn from(e: jni::errors::Error) -> Self {
        Self::Jni(e)
    }
}

/// Preset by its `Preset::ALL` index.
pub fn preset(index: i32) -> Result<Preset, FfiError> {
    usize::try_from(index)
        .ok()
        .and_then(|i| Preset::ALL.get(i).copied())
        .ok_or(FfiError::BadArgument("preset index"))
}

/// Params of the preset at `index` with the colour and size scale applied ([`BrushParams::for_preset`], the one
/// definition shared with the server and the browser). Unlike the lenient core function, a bad index or scale is
/// an error here: the tablet must never silently draw with a different brush than it asked for.
pub fn brush_params(index: i32, argb: i32, size_scale: f32) -> Result<BrushParams, FfiError> {
    let position = usize::try_from(index)
        .ok()
        .filter(|i| *i < Preset::ALL.len())
        .ok_or(FfiError::BadArgument("preset index"))?;
    if !(size_scale.is_finite() && size_scale > 0.0) {
        return Err(FfiError::BadArgument("sizeScale must be finite and > 0"));
    }
    Ok(BrushParams::for_preset(
        position,
        argb.cast_unsigned(),
        size_scale,
    ))
}

/// One in-progress stroke plus scratch buffers reused across JNI calls.
#[derive(Debug)]
pub struct Stroke {
    builder: Option<StrokeBuilder>,
    is_stamps: bool,
    opacity: f32,
    multiply: bool,
    samples: Vec<Sample>,
    xs: Vec<f32>,
    ts: Vec<i32>,
    polygon: Vec<[f32; 2]>,
    out: Vec<f32>,
    contour_ends: Vec<i32>,
}

impl Stroke {
    pub fn new(params: &BrushParams, seed: u64) -> Self {
        let builder = StrokeBuilder::new(params, seed);
        let is_stamps = matches!(builder.geometry(), GeometryView::Stamps { .. });
        Self {
            builder: Some(builder),
            is_stamps,
            opacity: params.opacity,
            multiply: params.blend == BlendMode::Multiply,
            samples: Vec::new(),
            xs: Vec::new(),
            ts: Vec::new(),
            polygon: Vec::new(),
            out: Vec::new(),
            contour_ends: Vec::new(),
        }
    }

    /// `[isStamps, opacity, multiply]` with booleans as 0/1.
    pub fn style(&self) -> [f32; 3] {
        [
            f32::from(u8::from(self.is_stamps)),
            self.opacity,
            f32::from(u8::from(self.multiply)),
        ]
    }

    /// Scratch for the raw sample floats (`count * SAMPLE_FLOATS`) and timestamps (`count`).
    pub fn input_buffers(&mut self, count: usize) -> (&mut [f32], &mut [i32]) {
        self.xs.resize(count * SAMPLE_FLOATS, 0.0);
        self.ts.resize(count, 0);
        (&mut self.xs, &mut self.ts)
    }

    /// Feeds the samples previously copied into [`Self::input_buffers`].
    pub fn push_buffered(&mut self) -> Result<(), FfiError> {
        let builder = self.builder.as_mut().ok_or(FfiError::Finished)?;
        self.samples.clear();
        self.samples.extend(
            self.xs
                .chunks_exact(SAMPLE_FLOATS)
                .zip(&self.ts)
                .map(|(s, &t)| Sample {
                    x: s[0],
                    y: s[1],
                    pressure: s[2],
                    tilt: s[3],
                    azimuth: s[4],
                    t_us: t.cast_unsigned(),
                }),
        );
        builder.push(&self.samples);
        Ok(())
    }

    /// Writes the live geometry (stable + tentative) into the output buffers.
    pub fn snapshot(&mut self) -> Result<(), FfiError> {
        let builder = self.builder.as_ref().ok_or(FfiError::Finished)?;
        self.out.clear();
        self.contour_ends.clear();
        match builder.geometry() {
            GeometryView::Outline { outline, .. } => {
                outline.write_polygon(&mut self.polygon);
                self.out.extend_from_slice(self.polygon.as_flattened());
                if !self.polygon.is_empty() {
                    let n = i32::try_from(self.polygon.len())
                        .map_err(|_| FfiError::BadArgument("outline too large"))?;
                    self.contour_ends.push(n);
                }
            }
            GeometryView::Stamps {
                stable, tentative, ..
            } => extend_dabs(&mut self.out, stable.iter().chain(tentative)),
        }
        Ok(())
    }

    /// Finishes the stroke and writes the final geometry into the output buffers.
    pub fn finish(&mut self) -> Result<(), FfiError> {
        let builder = self.builder.take().ok_or(FfiError::Finished)?;
        self.out.clear();
        self.contour_ends.clear();
        match builder.finish() {
            StrokeGeometry::Outline { outline, .. } => {
                self.out.extend_from_slice(outline.points.as_flattened());
                for &end in &outline.contour_ends {
                    let end = i32::try_from(end)
                        .map_err(|_| FfiError::BadArgument("outline too large"))?;
                    self.contour_ends.push(end);
                }
            }
            StrokeGeometry::Stamps { dabs, .. } => extend_dabs(&mut self.out, dabs.iter()),
        }
        Ok(())
    }

    /// Outline points (x,y pairs) or dab floats from the last snapshot/finish.
    pub fn output(&self) -> &[f32] {
        &self.out
    }

    /// Contour ends (in points) from the last snapshot/finish; empty for stamps.
    pub fn contour_ends(&self) -> &[i32] {
        &self.contour_ends
    }
}

fn extend_dabs<'a>(out: &mut Vec<f32>, dabs: impl Iterator<Item = &'a penwise_core::Dab>) {
    for d in dabs {
        let floats: [f32; DAB_FLOATS] = [
            d.x,
            d.y,
            d.radius_x,
            d.radius_y,
            d.angle,
            d.opacity,
            d.hardness,
            d.grain_strength,
            d.grain_level,
        ];
        out.extend_from_slice(&floats);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_snapshots_and_finishes() -> Result<(), FfiError> {
        for i in 0..Preset::ALL.len() {
            let index = i32::try_from(i).map_err(|_| FfiError::BadArgument("index"))?;
            let params = brush_params(index, -1, 2.0)?;
            let mut stroke = Stroke::new(&params, 7);
            let (xs, ts) = stroke.input_buffers(3);
            for k in 0..3 {
                let kf = f32::from(u8::try_from(k).map_err(|_| FfiError::BadArgument("k"))?);
                let sample = [10.0 + 20.0 * kf, 10.0, 0.5, 0.3, 0.0];
                xs[k * SAMPLE_FLOATS..(k + 1) * SAMPLE_FLOATS].copy_from_slice(&sample);
                ts[k] = i32::try_from(k * 2000).map_err(|_| FfiError::BadArgument("t"))?;
            }
            stroke.push_buffered()?;
            stroke.snapshot()?;
            assert!(!stroke.output().is_empty());
            stroke.finish()?;
            assert!(!stroke.output().is_empty());
            assert!(matches!(stroke.finish(), Err(FfiError::Finished)));
        }
        Ok(())
    }
}
