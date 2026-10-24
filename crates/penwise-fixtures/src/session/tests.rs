use penwise_core::DigitizerRanges;
use penwise_core::codec::pwrec::{Channels, RecordingSource};
use penwise_core::codec::{decode_pwrec, encode_pwrec};

use super::{PEN_NAME, Session, SessionError};

const DEVICE_TXT: &str = r#"session: 20261024-094722
model: SM-X710
--- getevent -lp ---
add device 2: /dev/input/event10
  name:     "sec_e-pen"
  events:
    ABS (0003): ABS_X                 : value 12809, min 0, max 14752, fuzz 4, flat 0, resolution 0
                ABS_Y                 : value 10700, min 0, max 23603, fuzz 4, flat 0, resolution 0
                ABS_PRESSURE          : value 0, min 0, max 4095, fuzz 0, flat 0, resolution 0
                ABS_DISTANCE          : value 0, min 0, max 255, fuzz 0, flat 0, resolution 0
                ABS_TILT_X            : value -19, min -63, max 63, fuzz 0, flat 0, resolution 0
                ABS_TILT_Y            : value 44, min -63, max 63, fuzz 0, flat 0, resolution 0
add device 4: /dev/input/event8
  name:     "sec_touchscreen"
  events:
    ABS (0003): ABS_X                 : value 0, min 0, max 1599, fuzz 0, flat 0, resolution 0
                ABS_Y                 : value 0, min 0, max 2559, fuzz 0, flat 0, resolution 0
"#;

const CAPTURE: &str = "\
add device 2: /dev/input/event10
  name:     \"sec_e-pen\"
[ 5.000000] /dev/input/event10: 0001 014a 00000001
[ 5.000000] /dev/input/event10: 0003 0018 00000800
[ 5.000000] /dev/input/event10: 0000 0000 00000000
[ 5.002000] /dev/input/event10: 0003 0000 00003200
[ 5.002000] /dev/input/event10: 0000 0000 00000000
[ 5.004000] /dev/input/event10: 0003 0000 00003210
[ 5.004000] /dev/input/event10: 0000 0000 00000000
[ 5.006000] /dev/input/event10: 0001 014a 00000000
[ 5.006000] /dev/input/event10: 0000 0000 00000000
";

#[test]
fn header_matches_the_tab_s9_digitizer() {
    let rec = Session::from_device_txt(DEVICE_TXT)
        .unwrap()
        .convert(CAPTURE)
        .unwrap();
    let h = &rec.header;
    assert_eq!(h.device, format!("SM-X710/{PEN_NAME}"));
    assert_eq!(h.channels, Channels::V1_DEFAULT);
    assert_eq!(h.source, RecordingSource::Getevent);
    assert_eq!(h.rate_hint, 500, "2 intervals over 4 ms");
    let sm = DigitizerRanges::SM_X710;
    assert_eq!((h.x_scale, h.y_scale), (sm.x_scale, sm.y_scale));
    assert_eq!(
        [
            h.ranges[0],
            h.ranges[1],
            h.ranges[2],
            h.ranges[3],
            h.ranges[4]
        ],
        [sm.x, sm.y, sm.pressure, sm.tilt_x, sm.tilt_y]
    );
    assert_eq!(rec.digitizer_ranges(), Ok(sm));
}

#[test]
fn capture_starts_from_listed_axis_values_and_round_trips() {
    let rec = Session::from_device_txt(DEVICE_TXT)
        .unwrap()
        .convert(CAPTURE)
        .unwrap();
    assert_eq!(rec.strokes.len(), 1);
    let frames = &rec.strokes[0].frames;
    assert_eq!(frames.len(), 3);
    assert_eq!(
        (frames[0].x, frames[0].y, frames[0].pressure),
        (12809, 10700, 2048)
    );
    assert_eq!((frames[0].tilt_x, frames[0].tilt_y), (-19, 44));
    assert_eq!((frames[2].x, frames[2].t_us), (0x3210, 4_000));
    let mut bytes = Vec::new();
    encode_pwrec(&rec, &mut bytes);
    assert_eq!(decode_pwrec(&bytes), Ok(rec));
}

#[test]
fn missing_touchscreen_is_reported() {
    let pen_only = DEVICE_TXT.split("add device 4").next().unwrap();
    assert_eq!(
        Session::from_device_txt(pen_only).unwrap_err(),
        SessionError::MissingDevice("sec_touchscreen")
    );
}

#[test]
fn malformed_capture_line_reports_its_number() {
    let session = Session::from_device_txt(DEVICE_TXT).unwrap();
    let err = session
        .convert("[ 1.0] /dev/input/event10: 0001\n")
        .unwrap_err();
    assert!(matches!(err, SessionError::Line { line: 1, .. }), "{err:?}");
}
