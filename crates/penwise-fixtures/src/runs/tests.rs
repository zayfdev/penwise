use penwise_core::codec::pwrec::{RawFrame, RunKind};

use super::{RunError, RunSplitter};
use crate::getevent::parse_line;

const PEN: &str = "/dev/input/event10";

const INITIAL: RawFrame = RawFrame {
    x: 100,
    y: 200,
    pressure: 0,
    tilt_x: -5,
    tilt_y: 20,
    distance: 0,
    t_us: 0,
};

fn split(capture: &str) -> Result<Vec<penwise_core::codec::pwrec::RecordedStroke>, RunError> {
    let mut splitter = RunSplitter::new(PEN, INITIAL);
    for line in capture.lines() {
        if let Some(ev) = parse_line(line) {
            splitter.event(&ev.unwrap())?;
        }
    }
    Ok(splitter.finish())
}

#[test]
fn splits_on_touch_and_carries_axis_state() {
    let capture = "\
[ 10.000000] /dev/input/event8: 0003 0035 00000010
[ 10.000500] /dev/input/event10: 0003 0000 00000065
[ 10.000500] /dev/input/event10: 0000 0000 00000000
[ 10.001000] /dev/input/event10: 0003 0018 00000100
[ 10.001000] /dev/input/event10: 0001 014a 00000001
[ 10.001000] /dev/input/event10: 0000 0000 00000000
[ 10.003000] /dev/input/event10: 0003 0001 000000c9
[ 10.003000] /dev/input/event10: 0003 001a fffffffa
[ 10.003000] /dev/input/event10: 0000 0000 00000000
[ 10.005000] /dev/input/event10: 0003 0018 00000000
[ 10.005000] /dev/input/event10: 0001 014a 00000000
[ 10.005000] /dev/input/event10: 0000 0000 00000000
[ 10.007000] /dev/input/event10: 0000 0000 00000000
[ 10.009000] /dev/input/event10: 0001 014a 00000001
[ 10.009000] /dev/input/event10: 0000 0000 00000000
";
    let strokes = split(capture).unwrap();
    assert_eq!(strokes.len(), 2);
    let first = &strokes[0];
    assert_eq!(first.kind, RunKind::Contact);
    assert_eq!(
        first.t0_us, 1_000,
        "relative to the first event of any device"
    );
    assert_eq!(
        first.frames,
        vec![
            RawFrame {
                x: 101,
                pressure: 256,
                ..INITIAL
            },
            RawFrame {
                x: 101,
                y: 201,
                pressure: 256,
                tilt_x: -6,
                t_us: 2_000,
                ..INITIAL
            },
        ]
    );
    let second = &strokes[1];
    assert_eq!(second.t0_us, 9_000);
    assert_eq!(
        second.frames.len(),
        1,
        "stroke open at end of capture is closed"
    );
    assert_eq!(second.frames[0].t_us, 0);
}

#[test]
fn other_devices_never_make_frames() {
    let capture = "\
[ 1.0] /dev/input/event8: 0001 014a 00000001
[ 1.0] /dev/input/event8: 0000 0000 00000000
";
    assert!(split(capture).unwrap().is_empty());
}

#[test]
fn time_going_back_is_an_error() {
    let capture = "\
[ 2.0] /dev/input/event10: 0001 014a 00000001
[ 2.0] /dev/input/event10: 0000 0000 00000000
[ 1.0] /dev/input/event10: 0000 0000 00000000
";
    assert_eq!(
        split(capture),
        Err(RunError::TimeWentBack {
            t_us: 1_000_000,
            start_us: 2_000_000
        })
    );
}
