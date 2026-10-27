//! Carrying a replica over to another server: every live notebook, page and item becomes a fresh `Create`/`Add`
//! op with a **deterministic** op id (a pure function of the entity id), so carrying the same state twice, or
//! on two devices, converges by server dedupe instead of duplicating.

use penwise_core::edit::Edit;
use sha2::{Digest, Sha256};

use crate::hlc::Hlc;
use crate::ids::{DeviceId, OpId};
use crate::notebook::{NotebookChange, NotebookPatch, PageChange, PagePatch};
use crate::op::{AnyOp, NotebookOp, Op, PageOp};
use crate::replica::Replica;

#[derive(Clone, Copy)]
enum Kind {
    NotebookCreate = 0,
    NotebookDeleted = 1,
    PageCreate = 2,
    PageDeleted = 3,
    ItemAdd = 4,
}

fn carry_op_id(kind: Kind, id: &[u8; 16]) -> OpId {
    let mut hasher = Sha256::new();
    hasher.update(b"penwise-carry-op");
    hasher.update([kind as u8]);
    hasher.update(id);
    let hash = hasher.finalize();
    let mut out = [0_u8; 16];
    out.copy_from_slice(hash.get(..16).unwrap_or(&[0; 16]));
    OpId(out)
}

/// The ops that rebuild `replica`'s live content on a server that has none of it, in create-before-reference
/// order: notebooks (a tombstoned one as `Create` + `deleted` update), pages likewise, then every live item
/// as `Add`. `stamp` issues one fresh stamp per op (the device's clock).
pub fn carry_ops(
    replica: &Replica,
    device: DeviceId,
    mut stamp: impl FnMut() -> Hlc,
) -> Vec<AnyOp> {
    let mut ops = Vec::new();
    let catalog = replica.catalog();
    for state in catalog.notebook_states() {
        ops.push(AnyOp::Notebook(NotebookOp {
            op_id: carry_op_id(Kind::NotebookCreate, &state.id.0),
            device,
            hlc: stamp(),
            notebook: state.id,
            change: NotebookChange::Create {
                title: state.title.clone(),
                position: state.position,
            },
        }));
        if state.deleted {
            ops.push(AnyOp::Notebook(NotebookOp {
                op_id: carry_op_id(Kind::NotebookDeleted, &state.id.0),
                device,
                hlc: stamp(),
                notebook: state.id,
                change: NotebookChange::Update(NotebookPatch {
                    deleted: Some(true),
                    ..NotebookPatch::default()
                }),
            }));
        }
    }
    let pages = catalog.page_states();
    for state in &pages {
        ops.push(AnyOp::Page(PageOp {
            op_id: carry_op_id(Kind::PageCreate, &state.id.0),
            device,
            hlc: stamp(),
            page: state.id,
            change: PageChange::Create {
                notebook: state.notebook,
                title: state.title.clone(),
                position: state.position,
                kind: state.kind,
                background: state.background,
            },
        }));
        if state.deleted {
            ops.push(AnyOp::Page(PageOp {
                op_id: carry_op_id(Kind::PageDeleted, &state.id.0),
                device,
                hlc: stamp(),
                page: state.id,
                change: PageChange::Update(PagePatch {
                    deleted: Some(true),
                    ..PagePatch::default()
                }),
            }));
        }
    }
    for state in &pages {
        let Some(page) = replica.page(state.id) else {
            continue;
        };
        for item in page.items().filter(|m| !m.deleted) {
            ops.push(AnyOp::Item(Box::new(Op {
                op_id: carry_op_id(Kind::ItemAdd, &item.item.id.0),
                device,
                hlc: stamp(),
                page: state.id,
                edit: Edit::Add(item.item.clone()),
            })));
        }
    }
    ops
}
