use super::{AbsInfo, DeviceListing};

const LISTING: &str = r#"session: 20261024-094722
model: SM-X710
--- getevent -lp ---
add device 2: /dev/input/event10
  name:     "sec_e-pen"
  events:
    KEY (0001): KEY_HOMEPAGE          00fd                  BTN_DIGI              BTN_TOOL_RUBBER
                BTN_TOUCH             BTN_STYLUS
    ABS (0003): ABS_X                 : value 12809, min 0, max 14752, fuzz 4, flat 0, resolution 0
                ABS_Y                 : value 10700, min 0, max 23603, fuzz 4, flat 0, resolution 0
                ABS_TILT_X            : value -19, min -63, max 63, fuzz 0, flat 0, resolution 0
  input props:
    <none>
add device 4: /dev/input/event8
  name:     "sec_touchscreen"
  events:
    ABS (0003): ABS_X                 : value 0, min 0, max 1599, fuzz 0, flat 0, resolution 0
add device 2: /dev/input/event10
  name:     "sec_e-pen"
"#;

#[test]
fn reads_model_nodes_and_ranges() {
    let listing = DeviceListing::parse(LISTING);
    assert_eq!(listing.model.as_deref(), Some("SM-X710"));
    assert_eq!(listing.devices.len(), 2, "repeated node merges");
    let pen = listing.by_name("sec_e-pen").unwrap();
    assert_eq!(pen.node, "/dev/input/event10");
    assert_eq!(
        pen.abs("ABS_X"),
        Some(AbsInfo {
            value: 12809,
            min: 0,
            max: 14752
        })
    );
    assert_eq!(
        pen.abs("ABS_TILT_X").map(|a| (a.min, a.max)),
        Some((-63, 63))
    );
    assert_eq!(pen.abs("ABS_PRESSURE"), None);
    let screen = listing.by_name("sec_touchscreen").unwrap();
    assert_eq!(screen.abs("ABS_X").map(|a| a.max), Some(1599));
}

#[test]
fn capture_header_lines_name_the_pen_node() {
    let capture = "add device 2: /dev/input/event12\n  name:     \"sec_e-pen\"\n[ 1.0] /dev/input/event12: 0000 0000 00000000\n";
    let listing = DeviceListing::parse(capture);
    assert_eq!(
        listing.by_name("sec_e-pen").map(|d| d.node.as_str()),
        Some("/dev/input/event12")
    );
}
