//! Live ink chunk rule, limits and ownership.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "test harness: failures are reported by panicking"
)]

mod common;

use common::{device, page_id};
use penwise_core::StrokeBuilder;
use penwise_core::brush::BrushParams;
use penwise_core::codec::stroke::encode_params;
use penwise_core::sample::Sample;
use penwise_proto::live::{MAX_DELTA_SAMPLES, MAX_LIVE_STROKES_PER_DEVICE};
use penwise_proto::{ItemRef, LiveError, LiveSample, LiveStrokes, StrokeDelta};

fn params() -> BrushParams {
    BrushParams::for_preset(0, 0xFF00_0000, 1.0)
}

fn brush() -> Vec<u8> {
    let mut out = Vec::new();
    encode_params(&params(), &mut out);
    out
}

fn stroke_id(n: u128) -> ItemRef {
    ItemRef::from_parts(1_700_000_000_000, n)
}

fn grid(i: i32) -> LiveSample {
    LiveSample {
        x: i * 40,
        y: 80 + (i % 3) * 16,
        pressure: 2000,
        tilt: 0,
        azimuth: 0,
        t_us: u32::try_from(i).unwrap() * 8_000,
    }
}

fn delta(id: u128, start: u32, range: std::ops::Range<i32>) -> StrokeDelta {
    StrokeDelta {
        stroke: stroke_id(id),
        page: page_id(1),
        brush: brush(),
        seed: 0xDEAD_BEEF_0000_0001,
        start,
        samples: range.map(grid).collect(),
    }
}

#[test]
fn chunked_deltas_build_the_same_geometry_as_one_builder_run() {
    let mut live = LiveStrokes::new();
    assert!(live.apply_delta(device(1), &delta(1, 0, 0..4)).unwrap());
    assert!(live.apply_delta(device(1), &delta(1, 4, 4..5)).unwrap());
    assert!(live.apply_delta(device(1), &delta(1, 5, 5..12)).unwrap());

    let mut reference = StrokeBuilder::new(&params(), 0xDEAD_BEEF_0000_0001);
    let samples: Vec<Sample> = (0..12).map(|i| grid(i).to_sample()).collect();
    reference.push(&samples);

    let open = live.on_page(page_id(1));
    assert_eq!(open.len(), 1);
    let (id, stroke) = &open[0];
    assert_eq!(*id, stroke_id(1));
    assert_eq!((stroke.device(), stroke.page()), (device(1), page_id(1)));
    assert_eq!(
        format!("{:?}", stroke.geometry()),
        format!("{:?}", reference.geometry())
    );
}

#[test]
fn a_gap_drops_the_preview_until_the_next_restart() {
    let mut live = LiveStrokes::new();
    live.apply_delta(device(1), &delta(1, 0, 0..3)).unwrap();
    assert_eq!(
        live.apply_delta(device(1), &delta(1, 5, 5..7)),
        Err(LiveError::OutOfOrder)
    );
    assert!(
        live.on_page(page_id(1)).is_empty(),
        "preview removed on a gap"
    );
    // Later chunks of the dropped stroke are ignored (no state to continue)…
    assert_eq!(
        live.apply_delta(device(1), &delta(1, 7, 7..8)),
        Err(LiveError::OutOfOrder)
    );
    // …until a delta restarts it at 0.
    assert!(live.apply_delta(device(1), &delta(1, 0, 0..2)).unwrap());
    assert_eq!(live.on_page(page_id(1)).len(), 1);
}

#[test]
fn an_overlapping_chunk_is_out_of_order_too() {
    let mut live = LiveStrokes::new();
    live.apply_delta(device(1), &delta(1, 0, 0..5)).unwrap();
    assert_eq!(
        live.apply_delta(device(1), &delta(1, 3, 3..6)),
        Err(LiveError::OutOfOrder)
    );
    assert!(live.on_page(page_id(1)).is_empty());
}

#[test]
fn restart_at_zero_replaces_the_stroke() {
    let mut live = LiveStrokes::new();
    live.apply_delta(device(1), &delta(1, 0, 0..6)).unwrap();
    live.apply_delta(device(1), &delta(1, 0, 0..2)).unwrap();
    // After the restart the next expected index is 2.
    assert!(live.apply_delta(device(1), &delta(1, 2, 2..3)).unwrap());
    assert_eq!(
        live.apply_delta(device(1), &delta(1, 6, 6..7)),
        Err(LiveError::OutOfOrder)
    );
}

#[test]
fn bad_brush_and_bad_samples_leave_state_unchanged() {
    let mut live = LiveStrokes::new();
    live.apply_delta(device(1), &delta(1, 0, 0..3)).unwrap();

    let mut garbage = delta(2, 0, 0..2);
    garbage.brush = vec![1, 2, 3];
    assert_eq!(
        live.apply_delta(device(1), &garbage),
        Err(LiveError::BadBrush)
    );
    let mut huge = BrushParams::for_preset(0, 0xFF00_0000, 1.0);
    huge.size_max = 1.0e9;
    let mut bytes = Vec::new();
    encode_params(&huge, &mut bytes);
    let mut oversized = delta(2, 0, 0..2);
    oversized.brush = bytes;
    assert_eq!(
        live.apply_delta(device(1), &oversized),
        Err(LiveError::BadBrush)
    );
    let mut trailing = delta(2, 0, 0..2);
    trailing.brush.push(0);
    assert_eq!(
        live.apply_delta(device(1), &trailing),
        Err(LiveError::BadBrush)
    );

    let empty = delta(1, 3, 3..3);
    assert_eq!(
        live.apply_delta(device(1), &empty),
        Err(LiveError::BadSamples)
    );
    let too_many = delta(1, 3, 0..i32::try_from(MAX_DELTA_SAMPLES).unwrap() + 1);
    assert_eq!(
        live.apply_delta(device(1), &too_many),
        Err(LiveError::BadSamples)
    );
    let mut off_grid = delta(1, 3, 3..4);
    off_grid.samples[0].pressure = 4097;
    assert_eq!(
        live.apply_delta(device(1), &off_grid),
        Err(LiveError::BadSamples)
    );
    let mut far = delta(1, 3, 3..4);
    far.samples[0].x = (1 << 24) + 1;
    assert_eq!(
        live.apply_delta(device(1), &far),
        Err(LiveError::BadSamples)
    );
    let mut tilt = delta(1, 3, 3..4);
    tilt.samples[0].tilt = 1025;
    assert_eq!(
        live.apply_delta(device(1), &tilt),
        Err(LiveError::BadSamples)
    );
    let mut azimuth = delta(1, 3, 3..4);
    azimuth.samples[0].azimuth = -2048;
    assert_eq!(
        live.apply_delta(device(1), &azimuth),
        Err(LiveError::BadSamples)
    );

    // The stroke from before is intact and continues at index 3.
    assert!(live.apply_delta(device(1), &delta(1, 3, 3..4)).unwrap());
    assert!(live.on_page(page_id(1)).len() == 1);
}

#[test]
fn a_device_may_have_a_bounded_number_of_open_strokes() {
    let mut live = LiveStrokes::new();
    for n in 0..MAX_LIVE_STROKES_PER_DEVICE as u128 {
        live.apply_delta(device(1), &delta(n, 0, 0..2)).unwrap();
    }
    assert_eq!(
        live.apply_delta(device(1), &delta(100, 0, 0..2)),
        Err(LiveError::TooMany)
    );
    // Restarting an open stroke is not a new stroke; another device has its own budget.
    assert!(live.apply_delta(device(1), &delta(0, 0, 0..2)).unwrap());
    assert!(live.apply_delta(device(2), &delta(200, 0, 0..2)).unwrap());
    // Freeing a slot (cancel or commit) lets the device start another.
    assert!(live.cancel(stroke_id(1)));
    assert!(live.apply_delta(device(1), &delta(100, 0, 0..2)).unwrap());
    assert!(live.commit(stroke_id(2)));
    assert!(!live.commit(stroke_id(2)));
    assert!(!live.cancel(stroke_id(999)));
}

#[test]
fn another_device_cannot_hijack_an_open_stroke() {
    let mut live = LiveStrokes::new();
    live.apply_delta(device(1), &delta(1, 0, 0..3)).unwrap();
    assert_eq!(
        live.apply_delta(device(2), &delta(1, 0, 0..2)),
        Err(LiveError::WrongOwner)
    );
    assert_eq!(
        live.apply_delta(device(2), &delta(1, 3, 3..4)),
        Err(LiveError::WrongOwner)
    );
    let mut other_page = delta(1, 3, 3..4);
    other_page.page = page_id(2);
    assert_eq!(
        live.apply_delta(device(1), &other_page),
        Err(LiveError::WrongOwner)
    );
    // The owner's preview is untouched and still continues.
    assert!(live.apply_delta(device(1), &delta(1, 3, 3..4)).unwrap());
}

#[test]
fn on_page_lists_strokes_by_page_in_id_order_and_clear_empties() {
    let mut live = LiveStrokes::new();
    live.apply_delta(device(1), &delta(3, 0, 0..2)).unwrap();
    live.apply_delta(device(2), &delta(1, 0, 0..2)).unwrap();
    let mut elsewhere = delta(2, 0, 0..2);
    elsewhere.page = page_id(7);
    live.apply_delta(device(1), &elsewhere).unwrap();
    let ids: Vec<_> = live.on_page(page_id(1)).iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, vec![stroke_id(1), stroke_id(3)]);
    assert_eq!(live.on_page(page_id(7)).len(), 1);
    live.clear();
    assert!(live.on_page(page_id(1)).is_empty() && live.on_page(page_id(7)).is_empty());
}

#[test]
fn live_samples_convert_to_the_core_grid_and_back() {
    let sample = LiveSample {
        x: -17,
        y: 33,
        pressure: 4096,
        tilt: 1024,
        azimuth: -2047,
        t_us: 9,
    };
    assert_eq!(LiveSample::from_grid(sample.to_grid()), sample);
    assert_eq!(sample.to_sample(), sample.to_grid().to_sample());
}
