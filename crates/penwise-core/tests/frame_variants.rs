//! Design-harness M4 codec compatibility: the frame patch/label/group/chosen fields round-trip at the new
//! layouts, and blobs written before them keep decoding with the defaults.

use penwise_core::CodecError;
use penwise_core::codec::edit::{EDIT_VERSION_V3, EDIT_VERSION_V4, decode_edit, encode_edit};
use penwise_core::codec::item::{ITEM_VERSION, decode_item, encode_item};
use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::hash::fnv1a64;
use penwise_core::item::{
    Affine, FrameExtent, FrameItem, FramePreset, Item, ItemId, ItemKind, Layer, MAX_PATCH_LEN,
};

fn id(n: u8) -> ItemId {
    ItemId([n; 16])
}

fn variant_frame() -> Item {
    Item {
        id: id(7),
        layer: Layer::Ink,
        z: 3 << 16,
        transform: Affine::translate(530.0, 20.0),
        kind: ItemKind::Frame(FrameItem {
            project: "portfolio".to_owned(),
            route: "/about".to_owned(),
            preset: FramePreset::Phone,
            css_w: 390,
            css_h: 844,
            patch: Some(r#"{"css":"h1{color:red}","dom":[]}"#.to_owned()),
            label: "bigger hero".to_owned(),
            group: Some("grp-1".to_owned()),
            chosen: true,
            extent: FrameExtent::Viewport,
            doc_h: 0,
        }),
    }
}

fn plain_frame() -> Item {
    Item {
        id: id(8),
        layer: Layer::Ink,
        z: 4 << 16,
        transform: Affine::translate(0.0, 0.0),
        kind: ItemKind::Frame(FrameItem {
            extent: FrameExtent::Viewport,
            ..FrameItem::new(
                "portfolio".to_owned(),
                "/".to_owned(),
                FramePreset::Desktop,
                1440,
                900,
            )
        }),
    }
}

fn item_blob(item: &Item) -> Vec<u8> {
    let mut out = Vec::new();
    encode_item(item, &mut out);
    out
}

fn edit_blob(edit: &Edit) -> Vec<u8> {
    let mut out = Vec::new();
    encode_edit(edit, &mut out);
    out
}

/// Replaces the trailer with the checksum of the (modified) content.
fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let content = bytes.len() - 8;
    bytes.truncate(content);
    bytes.extend_from_slice(&fnv1a64(&bytes).to_le_bytes());
    bytes
}

#[test]
fn item_v3_round_trips_the_variant_fields() {
    let item = variant_frame();
    let bytes = item_blob(&item);
    assert_eq!(bytes[4..6], ITEM_VERSION.to_le_bytes());
    assert_eq!(ITEM_VERSION, 4);
    assert_eq!(decode_item(&bytes).as_ref(), Ok(&item));
    assert_eq!(item_blob(&decode_item(&bytes).unwrap()), bytes, "canonical");
}

#[test]
fn item_v2_blob_decodes_with_variant_defaults() {
    let plain = plain_frame();
    let bytes = item_blob(&plain);
    // A plain viewport frame's v3 + v4 tail is exactly six zero bytes: patch flag, empty label length, group flag,
    // chosen, then the v4 extent (viewport) and unknown document height.
    let tail = bytes.len() - 8 - 6;
    assert_eq!(&bytes[tail..bytes.len() - 8], &[0, 0, 0, 0, 0, 0]);
    let mut older = bytes[..tail].to_vec();
    older.extend_from_slice(&bytes[bytes.len() - 8..]);
    older[4..6].copy_from_slice(&2_u16.to_le_bytes());
    let decoded = decode_item(&reseal(older)).expect("v2 frame decodes");
    assert_eq!(decoded, plain, "absent v3 fields default");
}

#[test]
fn frame_variant_updates_round_trip_and_need_v4() {
    let patch = ItemPatch {
        frame_patch: Some(Some(r#"{"css":"p{color:blue}"}"#.to_owned())),
        frame_label: Some("B".to_owned()),
        frame_group: Some(Some("grp-1".to_owned())),
        frame_chosen: Some(true),
        ..ItemPatch::default()
    };
    let edit = Edit::Update { id: id(7), patch };
    let bytes = edit_blob(&edit);
    assert_eq!(bytes[4..6], EDIT_VERSION_V4.to_le_bytes());
    assert_eq!(decode_edit(&bytes).as_ref(), Ok(&edit));
    assert_eq!(edit_blob(&decode_edit(&bytes).unwrap()), bytes, "canonical");

    // Clearing a patch or a group is `Some(None)` and survives the round trip.
    let clearing = Edit::Update {
        id: id(7),
        patch: ItemPatch {
            frame_patch: Some(None),
            frame_group: Some(None),
            ..ItemPatch::default()
        },
    };
    assert_eq!(decode_edit(&edit_blob(&clearing)).as_ref(), Ok(&clearing));

    let mut older = bytes;
    older[4..6].copy_from_slice(&EDIT_VERSION_V3.to_le_bytes());
    assert_eq!(
        decode_edit(&reseal(older)),
        Err(CodecError::InvalidValue {
            field: "field mask"
        }),
        "mask bits 17–20 did not exist in v3"
    );
}

#[test]
fn adding_a_variant_frame_needs_edit_v4_and_item_v3() {
    let edit = Edit::Add(variant_frame());
    let bytes = edit_blob(&edit);
    assert_eq!(bytes[4..6], EDIT_VERSION_V4.to_le_bytes());
    assert_eq!(decode_edit(&bytes).as_ref(), Ok(&edit));
    // The viewport variant frame is written at item layout v3 inside edit v4; the item codec itself always
    // writes the current layout.
    let item_bytes = item_blob(&variant_frame());
    assert_eq!(item_bytes[4..6], ITEM_VERSION.to_le_bytes());
    assert_eq!(decode_item(&item_bytes).as_ref(), Ok(&variant_frame()));
}

#[test]
fn patch_size_limit_is_enforced_by_item_validation_and_decode() {
    let mut item = plain_frame();
    let ItemKind::Frame(frame) = &mut item.kind else {
        panic!("frame expected");
    };
    frame.patch = Some("x".repeat(MAX_PATCH_LEN + 1));
    assert_eq!(
        item.validate(),
        Err(penwise_core::edit::EditError::InvalidValue {
            field: "frame_patch"
        })
    );
    // A hostile blob carrying an oversized patch is rejected on decode too.
    assert_eq!(
        decode_item(&item_blob(&item)),
        Err(CodecError::InvalidValue {
            field: "frame_patch"
        })
    );
}
