//! Random op sets over notebooks, pages and items, with delivery orders that respect create-before-reference.

use penwise_core::brush::Color;
use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::{Affine, ItemId, ItemKindTag, Layer, Paint, ShapeKind};
use penwise_proto::{
    AnyOp, Background, HlcClock, NotebookChange, NotebookPatch, PageChange, PageKind, PagePatch,
};

use super::{
    Rng, device, item_id, item_op, notebook_id, notebook_op, page_id, page_op, shape_item,
    stroke_item, text_item,
};

/// Ops in creation order; `deps[i]` is the index of the op that created what op `i` references.
#[derive(Debug, Clone)]
pub struct World {
    pub ops: Vec<AnyOp>,
    pub deps: Vec<Option<usize>>,
}

fn random_affine(rng: &mut Rng) -> Affine {
    Affine {
        a: rng.float(0.5, 2.0),
        b: 0.0,
        c: 0.0,
        d: rng.float(0.5, 2.0),
        e: rng.float(-100.0, 100.0),
        f: rng.float(-100.0, 100.0),
    }
}

fn random_color(rng: &mut Rng) -> Color {
    let bytes = rng.next_u64().to_le_bytes();
    Color {
        r: bytes[0],
        g: bytes[1],
        b: bytes[2],
        a: 255,
    }
}

/// A random non-empty patch of fields valid for `tag` (`Deleted` included: undelete and delete).
pub fn random_patch(rng: &mut Rng, tag: ItemKindTag) -> ItemPatch {
    let mut patch = ItemPatch::default();
    for _ in 0..=rng.below(3) {
        match (tag, rng.below(8)) {
            (_, 0) => patch.layer = Some([Layer::Ink, Layer::Agent, Layer::Text][rng.below(3)]),
            (_, 1) => patch.z = Some(i64::try_from(rng.below(9)).unwrap_or(0) * 65_536 - 262_144),
            (_, 2) => patch.transform = Some(random_affine(rng)),
            (_, 3) => patch.deleted = Some(rng.chance(50)),
            (ItemKindTag::Text, 4) => patch.text = Some(format!("t{}", rng.below(1000))),
            (ItemKindTag::Text, 5) => {
                patch.text_box = Some([rng.float(10.0, 300.0), rng.float(10.0, 300.0)]);
            }
            (ItemKindTag::Text, 6) => patch.font_size = Some(rng.float(8.0, 90.0)),
            (ItemKindTag::Text, _) => {
                patch.paint = Some(Paint {
                    color: random_color(rng),
                    opacity: rng.float(0.1, 1.0),
                });
            }
            (ItemKindTag::Shape, 4) => patch.shape_fill = Some(Some(random_color(rng))),
            (ItemKindTag::Shape, 5) => {
                patch.shape_size = Some([rng.float(5.0, 200.0), rng.float(5.0, 200.0)]);
            }
            (ItemKindTag::Shape, 6) => {
                patch.shape_kind = Some(ShapeKind::RoundedRect {
                    radius: rng.float(1.0, 10.0),
                });
            }
            (ItemKindTag::Shape | ItemKindTag::Stroke, _) => {
                let argb = u32::try_from(rng.next_u64() >> 32).unwrap_or(0) | 0xFF00_0000;
                patch.brush = Some(penwise_core::brush::BrushParams::for_preset(
                    0,
                    argb,
                    rng.float(0.5, 2.0),
                ));
            }
            _ => patch.z = Some(0),
        }
    }
    patch
}

struct Target {
    id: ItemId,
    page: usize,
    tag: ItemKindTag,
    add_op: usize,
}

impl World {
    /// `notebooks`/`pages`/`items` creations followed by `mutations` random updates, deletes, page and
    /// notebook edits from three devices with jittery clocks.
    pub fn random(seed: u64, items: usize, mutations: usize) -> Self {
        let mut rng = Rng(seed);
        let mut clocks: Vec<HlcClock> = (1..=3).map(|d| HlcClock::new(device(d))).collect();
        let mut world = Self {
            ops: Vec::new(),
            deps: Vec::new(),
        };
        let mut next_op = 1_u64;
        let mut now = 1_700_000_000_000_u64;
        let mut tick = |rng: &mut Rng, clocks: &mut Vec<HlcClock>| {
            now += rng.below(4) as u64;
            let who = rng.below(clocks.len());
            let jitter = rng.below(40) as u64;
            next_op += 1;
            (who, now + jitter, next_op)
        };

        let mut notebook_ops = Vec::new();
        for n in 1..=2_u64 {
            let (who, at, op) = tick(&mut rng, &mut clocks);
            notebook_ops.push(world.push(
                notebook_op(
                    &mut clocks[who],
                    at,
                    op,
                    notebook_id(n),
                    NotebookChange::Create {
                        title: format!("nb{n}"),
                        position: i64::try_from(n).unwrap_or(0),
                    },
                ),
                None,
            ));
        }
        let mut page_ops = Vec::new();
        for p in 1..=4_u64 {
            let (who, at, op) = tick(&mut rng, &mut clocks);
            let nb = rng.below(2);
            page_ops.push(world.push(
                page_op(
                    &mut clocks[who],
                    at,
                    op,
                    page_id(p),
                    PageChange::Create {
                        notebook: notebook_id(nb as u64 + 1),
                        title: String::new(),
                        position: i64::try_from(p).unwrap_or(0) * 65_536,
                        kind: if p % 2 == 0 {
                            PageKind::Infinite
                        } else {
                            PageKind::Fixed {
                                width: 1200.0,
                                height: 900.0,
                            }
                        },
                        background: Background::Blank,
                    },
                ),
                Some(notebook_ops[nb]),
            ));
        }
        let mut targets = Vec::new();
        for i in 1..=items as u64 {
            let (who, at, op) = tick(&mut rng, &mut clocks);
            let page = rng.below(4);
            let id = item_id(i);
            let z = i64::try_from(rng.below(6)).unwrap_or(0) * 65_536;
            let (item, tag) = match rng.below(3) {
                0 => (text_item(id, z, "text"), ItemKindTag::Text),
                1 => (shape_item(id, z), ItemKindTag::Shape),
                _ => (stroke_item(id, z, rng.next_u64()), ItemKindTag::Stroke),
            };
            let add_op = world.push(
                item_op(
                    &mut clocks[who],
                    at,
                    op,
                    page_id(page as u64 + 1),
                    Edit::Add(item),
                ),
                Some(page_ops[page]),
            );
            targets.push(Target {
                id,
                page,
                tag,
                add_op,
            });
        }
        for _ in 0..mutations {
            let (who, at, op) = tick(&mut rng, &mut clocks);
            match rng.below(10) {
                0 => {
                    let page = rng.below(4);
                    let patch = PagePatch {
                        title: rng.chance(50).then(|| format!("p{}", rng.below(50))),
                        position: rng.chance(50).then(|| rng.below(100) as i64),
                        background: rng.chance(50).then_some(Background::Grid),
                        deleted: Some(rng.chance(30)),
                    };
                    world.push(
                        page_op(
                            &mut clocks[who],
                            at,
                            op,
                            page_id(page as u64 + 1),
                            PageChange::Update(patch),
                        ),
                        Some(page_ops[page]),
                    );
                }
                1 => {
                    let nb = rng.below(2);
                    let patch = NotebookPatch {
                        title: rng.chance(50).then(|| format!("n{}", rng.below(50))),
                        position: rng.chance(50).then(|| rng.below(100) as i64),
                        deleted: Some(rng.chance(30)),
                    };
                    world.push(
                        notebook_op(
                            &mut clocks[who],
                            at,
                            op,
                            notebook_id(nb as u64 + 1),
                            NotebookChange::Update(patch),
                        ),
                        Some(notebook_ops[nb]),
                    );
                }
                _ => {
                    let t = &targets[rng.below(targets.len())];
                    let edit = if rng.chance(20) {
                        Edit::Delete(t.id)
                    } else {
                        Edit::Update {
                            id: t.id,
                            patch: random_patch(&mut rng, t.tag),
                        }
                    };
                    let (page, add_op) = (t.page, t.add_op);
                    world.push(
                        item_op(&mut clocks[who], at, op, page_id(page as u64 + 1), edit),
                        Some(add_op),
                    );
                }
            }
        }
        world
    }

    fn push(&mut self, op: AnyOp, dep: Option<usize>) -> usize {
        self.ops.push(op);
        self.deps.push(dep);
        self.ops.len() - 1
    }

    /// A random delivery order: a linear extension of the dependency order (every op after its creator),
    /// then ~`dup_percent`% of the ops delivered a second time at a random later position.
    pub fn delivery(&self, rng: &mut Rng, dup_percent: u64) -> Vec<usize> {
        let n = self.ops.len();
        let mut done = vec![false; n];
        let mut order = Vec::with_capacity(n * 2);
        while order.len() < n {
            let ready: Vec<usize> = (0..n)
                .filter(|&i| !done[i] && self.deps[i].is_none_or(|d| done[d]))
                .collect();
            let pick = ready[rng.below(ready.len())];
            done[pick] = true;
            order.push(pick);
        }
        for i in 0..n {
            if rng.chance(dup_percent) {
                let from = order.iter().position(|&x| x == i).unwrap_or(0);
                let at = from + 1 + rng.below(order.len() - from);
                order.insert(at, i);
            }
        }
        order
    }
}
