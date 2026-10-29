//! Authoring helpers: every method creates a stamped op, applies it to the local replica (like the tablet's
//! optimistic apply) and queues it in the outbox. Nothing here touches the network.

use penwise_core::brush::BrushParams;
use penwise_core::edit::{Edit, ItemPatch, erase_precise};
use penwise_core::item::{Affine, Item, ItemId, ItemKind, Layer, LayerSet, StrokeItem, Z_STEP};
use penwise_core::sample::Sample;
use penwise_proto::{
    AnyOp, Background, NotebookChange, NotebookId, NotebookOp, NotebookPatch, Op, PageChange,
    PageId, PageKind, PageOp, PagePatch, ReplicaError,
};

use crate::TestDevice;

/// A pen stroke through `points` (page px): 8.3 ms apart, pressure 0.5, quantized like the tablet builder.
#[must_use]
pub fn stroke_item(id: ItemId, z: i64, layer: Layer, seed: u64, points: &[[f32; 2]]) -> Item {
    let samples = points
        .iter()
        .zip(0_u32..)
        .map(|(p, i)| {
            Sample {
                x: p[0],
                y: p[1],
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: i * 8_333,
            }
            .quantize()
        })
        .collect();
    Item {
        id,
        layer,
        z,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: BrushParams::for_preset(0, 0xFF00_0000, 1.0),
            seed,
            samples,
            trim: None,
            binding: None,
        }),
    }
}

impl TestDevice {
    fn stamp(&mut self) -> penwise_proto::Hlc {
        let now = self.local_now();
        self.hlc.send(now)
    }

    /// Queues `op` and applies it locally. Local apply failures are returned, the op stays queued (the server
    /// is the judge).
    ///
    /// # Errors
    /// The local replica refused the op (e.g. unknown page).
    pub fn push_op(&mut self, op: &AnyOp) -> Result<(), ReplicaError> {
        let _ = self.outbox.insert(op);
        self.replica.apply(op).map(|_| ())
    }

    /// New notebook.
    pub fn create_notebook(&mut self, title: &str) -> NotebookId {
        let id = self.ids.notebook();
        let op = AnyOp::Notebook(NotebookOp {
            op_id: self.ids.op(),
            device: self.id,
            hlc: self.stamp(),
            notebook: id,
            change: NotebookChange::Create {
                title: title.to_owned(),
                position: 0,
            },
        });
        let _ = self.push_op(&op);
        id
    }

    /// New fixed 1000×1000 page (position 0).
    pub fn create_page(&mut self, notebook: NotebookId, title: &str) -> PageId {
        let id = self.ids.page();
        let op = AnyOp::Page(PageOp {
            op_id: self.ids.op(),
            device: self.id,
            hlc: self.stamp(),
            page: id,
            change: PageChange::Create {
                notebook,
                title: title.to_owned(),
                position: 0,
                kind: PageKind::Fixed {
                    width: 1000.0,
                    height: 1000.0,
                },
                background: Background::Blank,
            },
        });
        let _ = self.push_op(&op);
        id
    }

    /// Page tombstone.
    pub fn delete_page(&mut self, page: PageId) {
        let op = AnyOp::Page(PageOp {
            op_id: self.ids.op(),
            device: self.id,
            hlc: self.stamp(),
            page,
            change: PageChange::Update(PagePatch {
                deleted: Some(true),
                ..PagePatch::default()
            }),
        });
        let _ = self.push_op(&op);
    }

    /// Notebook title change.
    pub fn rename_notebook(&mut self, notebook: NotebookId, title: &str) {
        let op = AnyOp::Notebook(NotebookOp {
            op_id: self.ids.op(),
            device: self.id,
            hlc: self.stamp(),
            notebook,
            change: NotebookChange::Update(NotebookPatch {
                title: Some(title.to_owned()),
                ..NotebookPatch::default()
            }),
        });
        let _ = self.push_op(&op);
    }

    /// One item edit as an op.
    pub fn edit_op(&mut self, page: PageId, edit: Edit) -> AnyOp {
        AnyOp::Item(Box::new(Op {
            op_id: self.ids.op(),
            device: self.id,
            hlc: self.stamp(),
            page,
            edit,
        }))
    }

    /// Queues edits in order (callers put Adds before Deletes, like the tablet gesture).
    pub fn push_edits(&mut self, page: PageId, edits: Vec<Edit>) {
        for edit in edits {
            let op = self.edit_op(page, edit);
            let _ = self.push_op(&op);
        }
    }

    fn next_z(&self, page: PageId) -> i64 {
        self.replica
            .page(page)
            .and_then(|p| p.scene().next_z())
            .unwrap_or(Z_STEP)
    }

    /// Adds an ink stroke with a random seed.
    pub fn draw(&mut self, page: PageId, points: &[[f32; 2]]) -> ItemId {
        let seed = self.ids.seed();
        self.draw_with(page, Layer::Ink, seed, points)
    }

    /// Adds a stroke with an explicit layer and seed (negative/zero/huge seed scenarios).
    pub fn draw_with(
        &mut self,
        page: PageId,
        layer: Layer,
        seed: u64,
        points: &[[f32; 2]],
    ) -> ItemId {
        let id = self.ids.next_id_item();
        let z = self.next_z(page);
        let item = stroke_item(id, z, layer, seed, points);
        self.push_edits(page, vec![Edit::Add(item)]);
        id
    }

    /// Tombstone.
    pub fn delete_item(&mut self, page: PageId, id: ItemId) {
        self.push_edits(page, vec![Edit::Delete(id)]);
    }

    /// Absolute transform update (translation).
    pub fn move_item(&mut self, page: PageId, id: ItemId, dx: f32, dy: f32) {
        let patch = ItemPatch {
            transform: Some(Affine::translate(dx, dy)),
            ..ItemPatch::default()
        };
        self.push_edits(page, vec![Edit::Update { id, patch }]);
    }

    /// Layer change (promote agent → ink).
    pub fn set_layer(&mut self, page: PageId, id: ItemId, layer: Layer) {
        let patch = ItemPatch {
            layer: Some(layer),
            ..ItemPatch::default()
        };
        self.push_edits(page, vec![Edit::Update { id, patch }]);
    }

    /// Core precise erase against the local scene; pushes Adds first, Deletes last. Returns the edit count.
    pub fn erase(&mut self, page: PageId, path: &[[f32; 2]], radius: f32) -> usize {
        let Some(replica) = self.replica.page(page) else {
            return 0;
        };
        let mut ids = self.ids.clone();
        let mut edits = erase_precise(replica.scene(), path, radius, LayerSet::ALL, &mut ids);
        self.ids = ids;
        edits.sort_by_key(|e| matches!(e, Edit::Delete(_)));
        let count = edits.len();
        self.push_edits(page, edits);
        count
    }

    /// Ids of live (not deleted) items on `page` in the local replica.
    #[must_use]
    pub fn live_items(&self, page: PageId) -> Vec<ItemId> {
        self.replica
            .page(page)
            .map(|p| {
                p.items()
                    .filter(|m| !m.deleted)
                    .map(|m| m.item.id)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl crate::IdGen {
    /// Fresh item id (inherent twin of the `IdSource` impl, so callers need no trait import).
    pub fn next_id_item(&mut self) -> ItemId {
        penwise_core::item::IdSource::next_id(self)
    }
}
