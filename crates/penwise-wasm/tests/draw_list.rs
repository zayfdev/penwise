//! `DrawList::from_scene` layout invariants and per-kind command shapes.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    reason = "test harness: failures are reported by panicking"
)]

mod common;

use common::{
    BALLPOINT, HIGHLIGHTER, PENCIL, WAVE, erase, fill, image, item_id, scene, shape, stroke, text,
};
use penwise_core::brush::Color;
use penwise_core::item::LayerSet;
use penwise_wasm::draw_list::{COMMAND_STRIDE, DAB_STRIDE, DrawKind, DrawList, STYLE_STRIDE};

const NORMAL: f32 = 0.0;
const MULTIPLY: f32 = 1.0;

struct Command {
    kind: u32,
    first: usize,
    count: usize,
    style: usize,
    aux: usize,
}

fn commands(list: &DrawList) -> Vec<Command> {
    list.commands()
        .chunks(COMMAND_STRIDE)
        .map(|c| Command {
            kind: c[0],
            first: c[1] as usize,
            count: c[2] as usize,
            style: c[3] as usize,
            aux: c[4] as usize,
        })
        .collect()
}

fn of_kind(list: &DrawList, kind: DrawKind) -> Vec<Command> {
    commands(list)
        .into_iter()
        .filter(|c| c.kind == kind as u32)
        .collect()
}

fn blend_of(list: &DrawList, style: usize) -> f32 {
    list.styles()[style * STYLE_STRIDE + 5]
}

/// Every index points inside its array; groups are balanced and only contain masks inside them.
fn assert_well_formed(list: &DrawList) {
    let points = list.points();
    let runs = list.runs();
    assert_eq!(points.len() % 2, 0);
    assert!(runs.windows(2).all(|w| w[0] < w[1]), "runs strictly grow");
    assert_eq!(runs.last().map_or(0, |&e| e as usize), points.len() / 2);
    assert_eq!(list.styles().len() % STYLE_STRIDE, 0);
    assert_eq!(list.dabs().len() % DAB_STRIDE, 0);
    assert_eq!(list.commands().len() % COMMAND_STRIDE, 0);
    let (styles, dabs) = (
        list.styles().len() / STYLE_STRIDE,
        list.dabs().len() / DAB_STRIDE,
    );
    let mut open: Option<usize> = None;
    for c in commands(list) {
        assert!(c.style < styles, "style in range");
        let kind = c.kind;
        if kind == DrawKind::PathNonZero as u32 || kind == DrawKind::PathEvenOdd as u32 {
            assert!(c.count > 0 && c.first + c.count <= runs.len());
        } else if kind == DrawKind::Dabs as u32 {
            assert!(c.count > 0 && c.first + c.count <= dabs);
        } else if kind == DrawKind::Mask as u32 {
            assert!(open.is_some(), "mask outside a group");
            assert!(c.count > 0 && c.first + c.count <= runs.len());
            assert!(c.aux < list.mask_widths().len());
            assert_eq!(open, Some(c.style), "mask carries its group's style");
        } else if kind == DrawKind::Text as u32 {
            assert_eq!((c.count, c.first < runs.len()), (1, true));
            assert!(c.aux < list.texts().len() && c.aux < list.text_sizes().len());
            assert_four_corners(&runs, c.first);
        } else if kind == DrawKind::Image as u32 {
            assert_eq!((c.count, c.first < runs.len()), (1, true));
            assert!(c.aux < list.images().len());
            assert_four_corners(&runs, c.first);
        } else if kind == DrawKind::GroupBegin as u32 {
            assert!(open.replace(c.style).is_none(), "groups do not nest");
        } else if kind == DrawKind::GroupEnd as u32 {
            assert_eq!(open.take(), Some(c.style), "end matches begin");
        } else {
            panic!("unknown kind {kind}");
        }
    }
    assert!(open.is_none(), "group left open");
}

fn assert_four_corners(runs: &[u32], run: usize) {
    let start = if run == 0 { 0 } else { runs[run - 1] };
    assert_eq!(runs[run] - start, 4);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn empty_scene_gives_empty_list() {
    let list = DrawList::from_scene(&scene([]), LayerSet::ALL);
    assert!(list.is_empty());
    assert_well_formed(&list);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn every_item_kind_draws_in_band_and_z_order() {
    let pencil = item_id(2);
    let hl_shape = item_id(7);
    let scene = scene([
        stroke(item_id(1), 10, BALLPOINT, &WAVE),
        stroke(pencil, 20, PENCIL, &WAVE),
        erase(item_id(3), 20, pencil, &[[40.0, 20.0], [100.0, 25.0]], 6.0),
        shape(item_id(4), 30, BALLPOINT, Some(Color::rgb(10, 200, 10))),
        fill(item_id(5), 5),
        text(item_id(6), 40, "hello"),
        shape(hl_shape, 50, HIGHLIGHTER, None),
        erase(item_id(8), 50, hl_shape, &[[250.0, 260.0]], 4.0),
        image(item_id(9), 60, [0xAB; 32]),
    ]);
    let list = DrawList::from_scene(&scene, LayerSet::ALL);
    assert_well_formed(&list);

    let all = commands(&list);
    assert_eq!(all[0].kind, DrawKind::PathEvenOdd as u32, "fill band first");
    let even_odd = of_kind(&list, DrawKind::PathEvenOdd);
    assert_eq!(
        (even_odd.len(), even_odd[0].count),
        (1, 2),
        "outer ring + hole"
    );
    assert!(
        (list.styles()[4] - 0.5).abs() < f32::EPSILON,
        "paint opacity"
    );

    assert_eq!(of_kind(&list, DrawKind::GroupBegin).len(), 2);
    assert_eq!(of_kind(&list, DrawKind::GroupEnd).len(), 2);
    let masks = of_kind(&list, DrawKind::Mask);
    assert_eq!(masks.len(), 2);
    assert_eq!(
        list.mask_widths(),
        vec![8.0, 12.0],
        "multiply band first; width = 2·radius"
    );

    let groups = of_kind(&list, DrawKind::GroupBegin);
    assert_eq!(
        blend_of(&list, groups[0].style),
        MULTIPLY,
        "multiply item sits in the under-ink band"
    );
    assert_eq!(blend_of(&list, groups[1].style), NORMAL);

    assert!(!of_kind(&list, DrawKind::Dabs).is_empty(), "pencil is dabs");
    let dabs = of_kind(&list, DrawKind::Dabs);
    assert!(
        list.styles()[dabs[0].style * STYLE_STRIDE + 6] > 0.0,
        "pencil grain scale"
    );

    assert_eq!(list.texts(), vec!["hello".to_owned()]);
    assert_eq!(list.text_sizes(), vec![24.0]);
    assert_eq!(list.text_boxes(), vec![120.0, 40.0]);
    assert_eq!(list.images(), vec!["ab".repeat(32)]);
    assert_eq!(
        all.last().unwrap().kind,
        DrawKind::Image as u32,
        "top z last"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn shape_fill_precedes_its_stroke_as_nonzero_paths() {
    let scene = scene([shape(item_id(1), 1, BALLPOINT, Some(Color::rgb(1, 2, 3)))]);
    let list = DrawList::from_scene(&scene, LayerSet::ALL);
    assert_well_formed(&list);
    let all = commands(&list);
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|c| c.kind == DrawKind::PathNonZero as u32));
    let styles = list.styles();
    assert_eq!(
        &styles[all[0].style * STYLE_STRIDE..][..3],
        &[1.0 / 255.0, 2.0 / 255.0, 3.0 / 255.0]
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn single_point_mask_path_is_doubled() {
    let target = item_id(1);
    let scene = scene([
        shape(target, 1, BALLPOINT, None),
        erase(item_id(2), 1, target, &[[5.0, 5.0]], 3.0),
    ]);
    let list = DrawList::from_scene(&scene, LayerSet::ALL);
    assert_well_formed(&list);
    let mask = &of_kind(&list, DrawKind::Mask)[0];
    let runs = list.runs();
    let start = if mask.first == 0 {
        0
    } else {
        runs[mask.first - 1]
    };
    assert_eq!(runs[mask.first] - start, 2);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn layer_filter_hides_items() {
    let scene = scene([stroke(item_id(1), 1, BALLPOINT, &WAVE)]);
    assert!(DrawList::from_scene(&scene, LayerSet::NONE).is_empty());
    assert!(!DrawList::from_scene(&scene, LayerSet::INK).is_empty());
}
