use super::*;

fn id(n: u8) -> ItemId {
    ItemId([n; 16])
}

fn boxed(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Bounds {
    Bounds {
        min_x,
        min_y,
        max_x,
        max_y,
    }
}

/// Enough entries to overflow rstar nodes (forced reinsertion runs `partial_cmp().unwrap()` on areas).
fn grid(index: &mut SpatialIndex, count: u8) {
    for n in 0..count {
        let x = f32::from(n) * 10.0;
        index.insert(id(n), &boxed(x, 0.0, x + 5.0, 5.0));
    }
}

#[test]
fn non_finite_and_huge_bounds_are_never_indexed() {
    let hostile = [
        boxed(0.0, 0.0, f32::INFINITY, 2.0),
        boxed(f32::NEG_INFINITY, 0.0, 0.0, 2.0),
        boxed(0.0, f32::NAN, 2.0, 2.0),
        boxed(-3.0e38, -3.0e38, 3.0e38, 3.0e38),
    ];
    let mut index = SpatialIndex::default();
    grid(&mut index, 40);
    for (k, b) in hostile.iter().enumerate() {
        let n = u8::try_from(200 + k).unwrap();
        index.insert(id(n), b);
        grid(&mut index, 40); // re-inserting duplicates exercises node splits around the hostile entry
    }
    assert_eq!(index.len(), 40 * 5);
    for (k, b) in hostile.iter().enumerate() {
        index.remove(id(u8::try_from(200 + k).unwrap()), b);
    }
    assert_eq!(index.len(), 40 * 5);
    let mut hits = Vec::new();
    index.query(&boxed(-1.0e9, -1.0e9, 1.0e9, 1.0e9), &mut hits);
    assert!(hits.iter().all(|h| h.0[0] < 200));
}

#[test]
fn bulk_skips_hostile_bounds_and_keeps_normal_ones() {
    let mut entries: Vec<(ItemId, Bounds)> = (0..50u8)
        .map(|n| {
            let x = f32::from(n);
            (id(n), boxed(x, x, x + 1.0, x + 1.0))
        })
        .collect();
    entries.push((id(250), boxed(0.0, 0.0, f32::INFINITY, 1.0)));
    entries.push((id(251), boxed(0.0, 0.0, 2.0e30, 1.0)));
    let index = SpatialIndex::bulk(&entries);
    assert_eq!(index.len(), 50);
    let mut hits = Vec::new();
    index.query(&boxed(0.0, 0.0, 0.5, 0.5), &mut hits);
    assert_eq!(hits, vec![id(0)]);
}
