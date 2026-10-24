use super::*;
use crate::brush::Preset;

fn p(index: u32, frac: u32) -> SampleParam {
    SampleParam((index << SampleParam::FRAC_BITS) | frac)
}

fn r(start: SampleParam, end: SampleParam, start_cut: bool, end_cut: bool) -> SampleRange {
    SampleRange {
        start,
        end,
        start_cut,
        end_cut,
    }
}

fn samples(n: u32) -> Vec<Sample> {
    (0..n)
        .map(|i| {
            Sample {
                x: 10.0 + 0.5 * f32::from(u16::try_from(i).unwrap()),
                y: 20.0,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: i * 2075,
            }
            .quantize()
        })
        .collect()
}

#[test]
fn sample_param_round_trips_index_and_frac() {
    let u = p(7, 2048);
    assert_eq!(u.index(), 7);
    assert_eq!(u.frac().to_bits(), 0.5f32.to_bits());
    assert_eq!(SampleParam::at_index(7), p(7, 0));
    assert_eq!(SampleParam::at_index(u32::MAX).index(), u32::MAX >> 12);
}

#[test]
fn intersection_keeps_overlaps_and_cut_flags() {
    let a = [
        r(p(0, 0), p(10, 0), false, true),
        r(p(20, 0), p(30, 0), true, false),
    ];
    let b = [r(p(5, 100), p(25, 0), true, true)];
    assert_eq!(
        intersect_ranges(&a, &b),
        vec![
            r(p(5, 100), p(10, 0), true, true),
            r(p(20, 0), p(25, 0), true, true)
        ]
    );
    // Commutative.
    assert_eq!(intersect_ranges(&b, &a), intersect_ranges(&a, &b));
}

#[test]
fn intersection_drops_touching_ranges_but_keeps_dots() {
    let a = [r(p(0, 0), p(10, 0), false, true)];
    let touching = [r(p(10, 0), p(20, 0), true, false)];
    assert!(intersect_ranges(&a, &touching).is_empty());
    let dot = [r(p(10, 0), p(10, 0), false, false)];
    assert_eq!(
        intersect_ranges(&a, &dot),
        vec![r(p(10, 0), p(10, 0), false, true)]
    );
    assert!(intersect_ranges(&a, &[]).is_empty());
}

#[test]
fn intersection_with_everything_is_identity() {
    let all = [r(p(0, 0), p(100, 0), false, false)];
    let pieces = [
        r(p(0, 0), p(40, 9), false, true),
        r(p(60, 1), p(100, 0), true, false),
    ];
    assert_eq!(intersect_ranges(&pieces, &all), pieces.to_vec());
}

#[test]
fn untrimmed_ranges_span_all_samples() {
    let stroke = StrokeItem {
        params: Preset::Ballpoint.params(),
        seed: 1,
        samples: samples(5),
        trim: None,
    };
    let id = ItemId([3; 16]);
    assert_eq!(stroke.origin(id), id);
    assert_eq!(stroke.own_range(), r(p(0, 0), p(4, 0), false, false));
    assert_eq!(stroke.family_visible(), vec![stroke.own_range()]);
}

#[test]
fn centreline_interpolates_range_ends() {
    let stroke = StrokeItem {
        params: Preset::Fineliner.params(),
        seed: 1,
        samples: samples(10),
        trim: None,
    };
    let mut line = Centreline::default();
    let visible = [
        r(p(2, 2048), p(5, 0), true, false),
        r(p(7, 0), p(8, 1024), true, true),
    ];
    stroke.centreline(&Affine::IDENTITY, &visible, &mut line);
    let params: Vec<SampleParam> = line.points.iter().map(|c| c.param).collect();
    assert_eq!(
        params,
        vec![
            p(2, 2048),
            p(3, 0),
            p(4, 0),
            p(5, 0),
            p(7, 0),
            p(8, 0),
            p(8, 1024)
        ]
    );
    assert_eq!(line.run_ends, vec![4, 7]);
    stroke.centreline(&Affine::IDENTITY, &[], &mut line);
    assert!(line.points.is_empty() && line.run_ends.is_empty());
}

#[test]
fn empty_visible_gives_empty_geometry_with_style() {
    let stroke = StrokeItem {
        params: Preset::Ballpoint.params(),
        seed: 1,
        samples: samples(10),
        trim: None,
    };
    match stroke.build_geometry(&Affine::IDENTITY, &[]) {
        StrokeGeometry::Outline { outline, style } => {
            assert!(outline.points.is_empty());
            assert_eq!(style.color, stroke.params.color);
        }
        StrokeGeometry::Stamps { .. } => panic!("outline brush"),
    }
}
