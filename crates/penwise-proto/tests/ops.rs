//! Stateless op validation and accessors.
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

use common::{device, hlc, item_id, notebook_id, op_id, page_id, text_item};
use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::ItemKind;
use penwise_proto::hlc::MAX_WALL_MS;
use penwise_proto::{
    AnyOp, Background, NotebookChange, NotebookOp, NotebookPatch, Op, OpRejection, PageChange,
    PageKind, PageOp, PagePatch,
};

fn item(edit: Edit) -> Op {
    Op {
        op_id: op_id(1),
        device: device(1),
        hlc: hlc(1_000, 0, 1),
        page: page_id(1),
        edit,
    }
}

fn page(change: PageChange) -> PageOp {
    PageOp {
        op_id: op_id(2),
        device: device(1),
        hlc: hlc(1_000, 0, 1),
        page: page_id(1),
        change,
    }
}

fn notebook(change: NotebookChange) -> NotebookOp {
    NotebookOp {
        op_id: op_id(3),
        device: device(1),
        hlc: hlc(1_000, 0, 1),
        notebook: notebook_id(1),
        change,
    }
}

fn add() -> Edit {
    Edit::Add(text_item(item_id(1), 0, "x"))
}

#[test]
fn accessors_expose_the_header_and_target() {
    let op = AnyOp::Item(Box::new(item(add())));
    assert_eq!(
        (op.op_id(), op.device(), op.hlc(), op.page(), op.kind_code()),
        (op_id(1), device(1), hlc(1_000, 0, 1), Some(page_id(1)), 0)
    );
    assert_eq!(op.item().map(|i| i.0), Some(item_id(1).0));
    let page_op = AnyOp::Page(page(PageChange::Update(PagePatch {
        deleted: Some(true),
        ..PagePatch::default()
    })));
    assert_eq!(
        (page_op.page(), page_op.item(), page_op.kind_code()),
        (Some(page_id(1)), None, 1)
    );
    let nb = AnyOp::Notebook(notebook(NotebookChange::Create {
        title: "t".into(),
        position: 0,
    }));
    assert_eq!((nb.page(), nb.item(), nb.kind_code()), (None, None, 2));
}

#[test]
fn valid_ops_pass() {
    assert_eq!(AnyOp::Item(Box::new(item(add()))).validate(), Ok(()));
    assert_eq!(
        AnyOp::Item(Box::new(item(Edit::Delete(item_id(1))))).validate(),
        Ok(())
    );
    let patch = ItemPatch {
        z: Some(1),
        ..ItemPatch::default()
    };
    let update = Edit::Update {
        id: item_id(1),
        patch,
    };
    assert_eq!(AnyOp::Item(Box::new(item(update))).validate(), Ok(()));
    assert_eq!(
        AnyOp::Page(page(PageChange::Create {
            notebook: notebook_id(1),
            title: String::new(),
            position: 0,
            kind: PageKind::Fixed {
                width: 100.0,
                height: 100.0
            },
            background: Background::Dot,
        }))
        .validate(),
        Ok(())
    );
}

#[test]
fn another_devices_stamp_is_not_own_device() {
    let mut op = item(add());
    op.hlc = hlc(1_000, 0, 2);
    assert_eq!(
        AnyOp::Item(Box::new(op)).validate(),
        Err(OpRejection::NotOwnDevice)
    );
    let mut page_op = page(PageChange::Update(PagePatch {
        deleted: Some(true),
        ..PagePatch::default()
    }));
    page_op.device = device(5);
    assert_eq!(
        AnyOp::Page(page_op).validate(),
        Err(OpRejection::NotOwnDevice)
    );
}

#[test]
fn nonsense_content_is_invalid() {
    let mut late = item(add());
    late.hlc = hlc(MAX_WALL_MS + 1, 0, 1);
    assert_eq!(
        AnyOp::Item(Box::new(late)).validate(),
        Err(OpRejection::Invalid)
    );

    let mut bad_text = text_item(item_id(1), 0, "x");
    if let ItemKind::Text(t) = &mut bad_text.kind {
        t.font_size = f32::NAN;
    }
    assert_eq!(
        AnyOp::Item(Box::new(item(Edit::Add(bad_text)))).validate(),
        Err(OpRejection::Invalid)
    );

    let empty = Edit::Update {
        id: item_id(1),
        patch: ItemPatch::default(),
    };
    assert_eq!(
        AnyOp::Item(Box::new(item(empty))).validate(),
        Err(OpRejection::Invalid)
    );
}

#[test]
fn page_and_notebook_changes_are_checked() {
    let create = |kind: PageKind, title: &str| {
        AnyOp::Page(page(PageChange::Create {
            notebook: notebook_id(1),
            title: title.to_owned(),
            position: 0,
            kind,
            background: Background::Blank,
        }))
    };
    for kind in [
        PageKind::Fixed {
            width: 0.0,
            height: 10.0,
        },
        PageKind::Fixed {
            width: -1.0,
            height: 10.0,
        },
        PageKind::Fixed {
            width: f32::NAN,
            height: 10.0,
        },
        PageKind::Fixed {
            width: 10.0,
            height: f32::INFINITY,
        },
        PageKind::Fixed {
            width: 40_001.0,
            height: 10.0,
        },
    ] {
        assert_eq!(
            create(kind, "").validate(),
            Err(OpRejection::Invalid),
            "{kind:?}"
        );
    }
    assert_eq!(
        create(PageKind::Infinite, &"x".repeat(257)).validate(),
        Err(OpRejection::Invalid)
    );
    assert_eq!(
        create(PageKind::Infinite, &"x".repeat(256)).validate(),
        Ok(())
    );

    let empty_page = AnyOp::Page(page(PageChange::Update(PagePatch::default())));
    assert_eq!(empty_page.validate(), Err(OpRejection::Invalid));

    let nb = |change| AnyOp::Notebook(notebook(change));
    assert_eq!(
        nb(NotebookChange::Create {
            title: "   ".into(),
            position: 0
        })
        .validate(),
        Err(OpRejection::Invalid)
    );
    assert_eq!(
        nb(NotebookChange::Create {
            title: "x".repeat(257),
            position: 0
        })
        .validate(),
        Err(OpRejection::Invalid)
    );
    assert_eq!(
        nb(NotebookChange::Update(NotebookPatch::default())).validate(),
        Err(OpRejection::Invalid)
    );
    assert_eq!(
        nb(NotebookChange::Update(NotebookPatch {
            title: Some(String::new()),
            ..NotebookPatch::default()
        }))
        .validate(),
        Err(OpRejection::Invalid)
    );
    assert_eq!(
        nb(NotebookChange::Update(NotebookPatch {
            position: Some(4),
            ..NotebookPatch::default()
        }))
        .validate(),
        Ok(())
    );
}
