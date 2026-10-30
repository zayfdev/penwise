//! Numbered badges as polylines: a dot and the digits of a number, ready for the core stroke and raster path.
//!
//! Each digit is a handful of segments on a unit box, scaled to the badge. The strokes are plain
//! [`AnnotatedStroke`]s, so they become ordinary agent-layer strokes through [`crate::annotate::to_items`] and
//! render exactly like any other ink; nothing here draws a pixel itself.

use crate::annotate::AnnotatedStroke;

/// Width of one digit at scale 1, page px.
const DIGIT_W: f32 = 12.0;
/// Height of one digit at scale 1, page px.
const DIGIT_H: f32 = 20.0;
/// Space between digits at scale 1, page px.
const DIGIT_GAP: f32 = 5.0;
/// Width of the pen that draws the dot: the brush caps a stroke at 6x its nominal width.
const DOT_PEN: f32 = 19.0;
/// Width of a digit's pen at scale 1.
const PEN_WIDTH: f32 = 3.2;
/// Smallest dot radius at scale 1.
const MIN_RADIUS: f32 = 18.0;
/// Segments of a ring.
const RING_SEGMENTS: usize = 16;
const DOT_COLOUR: &str = "#C92A2A";
const PEN_COLOUR: &str = "#FFFFFF";

/// A glyph: polylines on `[0, 1] × [0, 1]`, y down.
type Glyph = &'static [&'static [[f32; 2]]];

/// The ten digits.
const GLYPHS: [Glyph; 10] = [
    &[&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]],
    &[&[[0.15, 0.2], [0.55, 0.0], [0.55, 1.0]]],
    &[&[
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 0.5],
        [0.0, 0.5],
        [0.0, 1.0],
        [1.0, 1.0],
    ]],
    &[
        &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        &[[0.25, 0.5], [1.0, 0.5]],
    ],
    &[
        &[[0.0, 0.0], [0.0, 0.5], [1.0, 0.5]],
        &[[1.0, 0.0], [1.0, 1.0]],
    ],
    &[&[
        [1.0, 0.0],
        [0.0, 0.0],
        [0.0, 0.5],
        [1.0, 0.5],
        [1.0, 1.0],
        [0.0, 1.0],
    ]],
    &[&[
        [1.0, 0.0],
        [0.0, 0.0],
        [0.0, 1.0],
        [1.0, 1.0],
        [1.0, 0.5],
        [0.0, 0.5],
    ]],
    &[&[[0.0, 0.0], [1.0, 0.0], [0.4, 1.0]]],
    &[
        &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
        &[[0.0, 0.5], [1.0, 0.5]],
    ],
    &[&[
        [1.0, 0.5],
        [0.0, 0.5],
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ]],
];

/// Page-px radius of the badge for `number` at `scale` (for placing badges apart).
pub(super) fn radius(number: usize, scale: f32) -> f32 {
    (width_of(number) / 2.0 + 6.0).max(MIN_RADIUS) * scale
}

fn width_of(number: usize) -> f32 {
    let digits = number.to_string().len();
    #[allow(
        clippy::cast_precision_loss,
        reason = "a mark index has a handful of digits"
    )]
    let count = digits as f32;
    count * DIGIT_W + (count - 1.0) * DIGIT_GAP
}

/// A filled disc as rings of the dot pen, from the rim inwards, plus a speck in the middle.
fn disc(center: [f32; 2], radius: f32) -> Vec<AnnotatedStroke> {
    let mut strokes = Vec::new();
    let mut ring = radius - DOT_PEN / 2.0;
    while ring > 0.0 {
        let points = (0..=RING_SEGMENTS)
            .map(|step| {
                #[allow(clippy::cast_precision_loss, reason = "a handful of segments")]
                let angle = step as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
                let (sin, cos) = angle.sin_cos();
                [center[0] + ring * cos, center[1] + ring * sin]
            })
            .collect();
        strokes.push(AnnotatedStroke {
            points,
            color: Some(DOT_COLOUR.to_owned()),
            width: Some(DOT_PEN),
        });
        ring -= DOT_PEN - 2.0;
    }
    strokes.push(AnnotatedStroke {
        points: vec![center, [center[0] + 0.5, center[1]]],
        color: Some(DOT_COLOUR.to_owned()),
        width: Some(DOT_PEN),
    });
    strokes
}

/// The strokes of badge `number` centred on `center` (page px) at `scale`: a red disc, then the digits in white.
pub(super) fn badge(number: usize, center: [f32; 2], scale: f32) -> Vec<AnnotatedStroke> {
    let mut strokes = disc(center, radius(number, scale));
    let total = width_of(number) * scale;
    let left = center[0] - total / 2.0;
    let top = center[1] - DIGIT_H * scale / 2.0;
    for (index, digit) in number.to_string().bytes().enumerate() {
        let Some(glyph) = GLYPHS.get(usize::from(digit.wrapping_sub(b'0'))) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss, reason = "at most a few digits")]
        let x0 = left + index as f32 * (DIGIT_W + DIGIT_GAP) * scale;
        for line in *glyph {
            // One stroke per segment: the brush rounds the corners of a polyline, which makes digits unreadable.
            for pair in line.windows(2) {
                let place = |point: &[f32; 2]| {
                    [
                        x0 + point[0] * DIGIT_W * scale,
                        top + point[1] * DIGIT_H * scale,
                    ]
                };
                strokes.push(AnnotatedStroke {
                    points: vec![place(&pair[0]), place(&pair[1])],
                    color: Some(PEN_COLOUR.to_owned()),
                    width: Some(PEN_WIDTH * scale),
                });
            }
        }
    }
    strokes
}
