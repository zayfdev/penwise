//! SQL-level LWW equals a proto `Replica` fold, whatever the delivery order.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test fixtures: small counts and coordinates"
)]

use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::{Affine, ItemId, Layer};
use penwise_proto::{
    AnyOp, Background, Hlc, NotebookChange, NotebookId, NotebookOp, NotebookPatch, Op, PageChange,
    PageId, PageOp, PagePatch, Replica,
};

use super::{Rig, T0};
use crate::storage::OpFilter;

/// splitmix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap_or(0)
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

fn stamp(rig: &Rig, rng: &mut Rng, index: usize) -> Hlc {
    Hlc {
        wall_ms: T0 - 30_000 + rng.next() % 90_000,
        counter: u16::try_from(index).unwrap_or(u16::MAX),
        device: rig.device,
    }
}

fn random_edit(rng: &mut Rng, id: ItemId) -> Edit {
    let patch = match rng.below(5) {
        0 => return Edit::Delete(id),
        1 => ItemPatch {
            deleted: Some(false),
            ..ItemPatch::default()
        },
        2 => ItemPatch {
            layer: Some(if rng.below(2) == 0 {
                Layer::Agent
            } else {
                Layer::Ink
            }),
            ..ItemPatch::default()
        },
        3 => ItemPatch {
            z: Some(i64::try_from(rng.below(500)).unwrap_or(0)),
            transform: Some(Affine::translate(rng.below(50) as f32, 1.0)),
            ..ItemPatch::default()
        },
        _ => ItemPatch {
            transform: Some(Affine::translate(1.0, rng.below(50) as f32)),
            ..ItemPatch::default()
        },
    };
    Edit::Update { id, patch }
}

fn catalog_ops(rig: &mut Rig, rng: &mut Rng, notebook: NotebookId, page: PageId) -> Vec<AnyOp> {
    (0..12_usize)
        .map(|index| {
            let hlc = stamp(rig, rng, 1000 + index);
            let op_id = rig.ids.op();
            if index % 2 == 0 {
                AnyOp::Notebook(NotebookOp {
                    op_id,
                    device: rig.device,
                    hlc,
                    notebook,
                    change: NotebookChange::Update(NotebookPatch {
                        title: Some(format!("t{index}")),
                        position: Some(i64::try_from(rng.below(9)).unwrap_or(0)),
                        deleted: Some(rng.below(2) == 0),
                    }),
                })
            } else {
                AnyOp::Page(PageOp {
                    op_id,
                    device: rig.device,
                    hlc,
                    page,
                    change: PageChange::Update(PagePatch {
                        title: Some(format!("p{index}")),
                        position: None,
                        background: Some(if rng.below(2) == 0 {
                            Background::Dot
                        } else {
                            Background::Lined
                        }),
                        deleted: Some(rng.below(2) == 0),
                    }),
                })
            }
        })
        .collect()
}

/// Ten adds stamped before every edit, and the ids they create.
fn adds(rig: &mut Rig, page: PageId) -> (Vec<AnyOp>, Vec<ItemId>) {
    let mut ops = Vec::new();
    let mut ids = Vec::new();
    for z in 0..10_i64 {
        let item = rig.stroke(z * 3);
        ids.push(item.id);
        ops.push(AnyOp::Item(Box::new(Op {
            op_id: rig.ids.op(),
            device: rig.device,
            hlc: Hlc {
                wall_ms: T0 - 40_000,
                counter: u16::try_from(z).unwrap_or(0),
                device: rig.device,
            },
            page,
            edit: Edit::Add(item),
        })));
    }
    (ops, ids)
}

#[tokio::test]
async fn materialized_rows_equal_a_replica_fold_in_any_delivery_order() {
    let mut rig = Rig::new();
    let mut rng = Rng(42);
    let (notebook, page) = rig.seed_page().await;
    let (adds, ids) = adds(&mut rig, page);
    let mut tail = Vec::new();
    for index in 0..160_usize {
        let target = ids[rng.below(ids.len())];
        let edit = random_edit(&mut rng, target);
        let hlc = stamp(&rig, &mut rng, index);
        tail.push(AnyOp::Item(Box::new(Op {
            op_id: rig.ids.op(),
            device: rig.device,
            hlc,
            page,
            edit,
        })));
    }
    tail.extend(catalog_ops(&mut rig, &mut rng, notebook, page));

    // Replica: one shuffle. Storage: another, one frame per op (adds first for both).
    let mut replica = Replica::new();
    let seeded = rig
        .storage
        .ops_since(0, OpFilter::All, 10)
        .await
        .expect("seed");
    for op in &seeded.ops {
        replica.apply(&op.op).expect("seed op");
    }
    for op in &adds {
        replica.apply(op).expect("add");
    }
    let mut order = tail.clone();
    rng.shuffle(&mut order);
    for op in &order {
        replica.apply(op).expect("replica op");
    }
    rig.push(adds).await;
    rng.shuffle(&mut tail);
    for op in tail {
        rig.push(vec![op]).await;
    }

    let view = rig
        .storage
        .page_view(page)
        .await
        .expect("view")
        .expect("page");
    let stored: Vec<_> = view.replica.items().cloned().collect();
    let folded: Vec<_> = replica.page(page).expect("page").items().cloned().collect();
    assert_eq!(stored.len(), 10);
    assert_eq!(stored, folded);
    assert_eq!(
        rig.storage.catalog().await.expect("catalog"),
        *replica.catalog()
    );
}
