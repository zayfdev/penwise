use super::*;
use crate::edit::Edit;
use crate::item::{Affine, ItemId, ItemKind, Layer};

fn spec(project: &str, route: &str, preset: FramePreset) -> Result<FrameSpec, EditError> {
    FrameSpec::new(
        project.to_owned(),
        route.to_owned(),
        preset,
        None,
        None,
        10.0,
        20.0,
    )
}

#[test]
fn preset_codes_names_and_sizes_are_frozen() {
    assert_eq!(
        FramePreset::ALL.map(FramePreset::code),
        [0, 1, 2, 3],
        "codes are stored"
    );
    for (code, preset) in FramePreset::ALL.into_iter().enumerate() {
        let code = u8::try_from(code).unwrap();
        assert_eq!(FramePreset::from_code(code), Some(preset));
        assert_eq!(FramePreset::parse(preset.as_str()), Some(preset));
    }
    assert_eq!(FramePreset::from_code(4), None);
    assert_eq!(FramePreset::parse("Phone"), None, "names are lowercase");
    assert_eq!(FramePreset::Phone.size(), Some((390, 844)));
    assert_eq!(FramePreset::Tablet.size(), Some((834, 1194)));
    assert_eq!(FramePreset::Desktop.size(), Some((1440, 900)));
    assert_eq!(FramePreset::Custom.size(), None);
}

#[test]
fn fixed_presets_ignore_given_sizes_and_custom_requires_both() {
    let size = FrameSize::resolve(FramePreset::Phone, Some(10), Some(10)).unwrap();
    assert_eq!(
        size,
        FrameSize {
            preset: FramePreset::Phone,
            css_w: 390,
            css_h: 844
        }
    );
    let size = FrameSize::resolve(FramePreset::Custom, Some(640), Some(480)).unwrap();
    assert_eq!((size.css_w, size.css_h), (640, 480));
    for (w, h) in [(None, None), (Some(640), None), (None, Some(480))] {
        assert_eq!(
            FrameSize::resolve(FramePreset::Custom, w, h),
            Err(EditError::InvalidValue {
                field: "frame_size"
            })
        );
    }
    for (w, h) in [(0, 100), (100, 0), (MAX_FRAME_SIDE + 1, 100)] {
        assert_eq!(
            FrameSize::resolve(FramePreset::Custom, Some(w), Some(h)),
            Err(EditError::InvalidValue {
                field: "frame_size"
            })
        );
    }
    assert!(
        FrameSize::resolve(
            FramePreset::Custom,
            Some(MAX_FRAME_SIDE),
            Some(MAX_FRAME_SIDE)
        )
        .is_ok()
    );
}

#[test]
fn content_invariants() {
    assert!(spec("portfolio", "/", FramePreset::Phone).is_ok());
    assert!(spec("portfolio", "/index.html?x=1", FramePreset::Desktop).is_ok());
    for (project, route) in [("", "/"), ("portfolio", ""), ("portfolio", "index.html")] {
        assert!(matches!(
            spec(project, route, FramePreset::Phone),
            Err(EditError::InvalidValue { .. })
        ));
    }
    assert_eq!(
        spec(&"p".repeat(MAX_PROJECT_LEN + 1), "/", FramePreset::Phone),
        Err(EditError::InvalidValue {
            field: "frame_project"
        })
    );
    assert_eq!(
        spec(
            "portfolio",
            &format!("/{}", "r".repeat(MAX_ROUTE_LEN)),
            FramePreset::Phone
        ),
        Err(EditError::InvalidValue {
            field: "frame_route"
        })
    );
    let bad_position = FrameSpec::new(
        "p".to_owned(),
        "/".to_owned(),
        FramePreset::Phone,
        None,
        None,
        f32::NAN,
        0.0,
    );
    assert_eq!(
        bad_position,
        Err(EditError::InvalidValue {
            field: "frame_position"
        })
    );
}

#[test]
fn a_spec_places_the_frame_with_a_translation() {
    let spec = spec("portfolio", "/work", FramePreset::Tablet).unwrap();
    let item = spec.item(ItemId([7; 16]), Layer::Agent, 42);
    assert_eq!(item.id, ItemId([7; 16]));
    assert_eq!(item.layer, Layer::Agent);
    assert_eq!(item.z, 42);
    assert_eq!(item.transform, Affine::translate(10.0, 20.0));
    let ItemKind::Frame(frame) = &item.kind else {
        panic!("frame kind expected");
    };
    assert!((frame.css_w, frame.css_h) == (834, 1194));
    assert_eq!(frame.project, "portfolio");
    assert_eq!(frame.route, "/work");
    assert!(matches!(spec.edit(item.id, Layer::Agent, 42), Edit::Add(_)));
}

#[test]
fn item_id_hex_round_trips() {
    let mut bytes = [0_u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::try_from(i).unwrap() * 15 + 1;
    }
    let id = ItemId(bytes);
    let text = id.to_hex();
    assert_eq!(text.len(), 32);
    assert_eq!(text, text.to_lowercase());
    assert_eq!(ItemId::from_hex(&text), Some(id));
    assert_eq!(ItemId::from_hex(&text.to_uppercase()), Some(id));
    for bad in [
        "",
        "00",
        &text[..31],
        &format!("{text}0"),
        &format!("{}g", &text[..31]),
    ] {
        assert_eq!(ItemId::from_hex(bad), None, "{bad:?}");
    }
}
