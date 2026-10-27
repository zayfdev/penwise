use super::*;

struct Counter(u8);

impl Identity for Counter {
    fn next(&mut self) -> (ItemId, u64) {
        self.0 += 1;
        (ItemId([self.0; 16]), u64::from(self.0))
    }
}

fn request(strokes: Vec<AnnotatedStroke>) -> AnnotateRequest {
    AnnotateRequest {
        page: None,
        strokes,
    }
}

fn line(points: Vec<[f32; 2]>, color: Option<&str>, width: Option<f32>) -> AnnotatedStroke {
    AnnotatedStroke {
        points,
        color: color.map(str::to_owned),
        width,
    }
}

fn items_of(strokes: Vec<AnnotatedStroke>) -> Vec<Item> {
    to_items(&request(strokes), 100, &mut Counter(0))
}

fn stroke(item: &Item) -> &StrokeItem {
    match &item.kind {
        ItemKind::Stroke(s) => s,
        other => panic!("not a stroke: {other:?}"),
    }
}

#[test]
fn a_line_becomes_a_dense_agent_stroke() {
    let items = items_of(vec![line(vec![[0.0, 0.0], [40.0, 0.0]], None, None)]);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].layer, Layer::Agent);
    assert_eq!(items[0].z, 100);
    let s = stroke(&items[0]);
    // The start point plus 40 px in 4 px steps.
    assert_eq!(s.samples.len(), 11);
    let last = s.samples.last().expect("samples");
    assert!((last.x - 40.0).abs() < 0.1, "ends on the far point");
    assert!(s.samples.windows(2).all(|w| w[0].t_us < w[1].t_us));
    assert_eq!(s.seed, 1);
}

#[test]
fn items_stack_upwards_and_get_distinct_identities() {
    let items = items_of(vec![
        line(vec![[0.0, 0.0], [10.0, 0.0]], None, None),
        line(vec![[0.0, 5.0], [10.0, 5.0]], None, None),
    ]);
    assert_eq!(items.len(), 2);
    assert_eq!(items[1].z, 100 + Z_STEP);
    assert_ne!(items[0].id, items[1].id);
}

#[test]
fn degenerate_or_non_finite_input_is_dropped() {
    assert!(items_of(vec![line(vec![[5.0, 5.0]], None, None)]).is_empty());
    assert!(items_of(vec![line(vec![[f32::NAN, 0.0], [10.0, 10.0]], None, None)]).is_empty());
    assert!(items_of(vec![line(vec![[1.0, 1.0], [1.0, 1.0]], None, None)]).is_empty());
}

#[test]
fn colours_and_width_map_to_brush_params() {
    let red = items_of(vec![line(
        vec![[0.0, 0.0], [10.0, 0.0]],
        Some("#ff0000"),
        Some(6.4),
    )]);
    let default = items_of(vec![line(vec![[0.0, 0.0], [10.0, 0.0]], None, None)]);
    let (red, default) = (stroke(&red[0]), stroke(&default[0]));
    assert_eq!(
        red.params.color,
        penwise_core::Color::from_argb(0xFFFF_0000)
    );
    assert!((red.params.size_max / default.params.size_max - 2.0).abs() < 1e-4);
    let translucent = items_of(vec![line(
        vec![[0.0, 0.0], [10.0, 0.0]],
        Some("#8000ff00"),
        None,
    )]);
    assert_eq!(
        stroke(&translucent[0]).params.color,
        penwise_core::Color::from_argb(0x8000_FF00)
    );
    let fallback = items_of(vec![line(
        vec![[0.0, 0.0], [10.0, 0.0]],
        Some("not-a-colour"),
        None,
    )]);
    assert_eq!(
        stroke(&fallback[0]).params.color,
        penwise_core::Color::from_argb(DEFAULT_ARGB)
    );
}

#[test]
fn absurdly_long_annotations_are_rejected_without_allocating_them() {
    assert!(
        items_of(vec![line(vec![[0.0, 0.0], [3.0e38, 0.0]], None, None)]).is_empty(),
        "infinite-length segment"
    );
    assert!(
        items_of(vec![line(vec![[0.0, 0.0], [1.0e9, 0.0]], None, None)]).is_empty(),
        "250M samples is over the cap"
    );
    assert_eq!(
        items_of(vec![line(vec![[0.0, 0.0], [300_000.0, 0.0]], None, None)]).len(),
        1,
        "75k samples is within the cap"
    );
}
