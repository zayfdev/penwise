use super::{Event, LineError, parse_line};

#[test]
fn parses_event_with_negative_value() {
    let ev = parse_line("[    4177.463145] /dev/input/event10: 0003 001a fffffffc").unwrap();
    assert_eq!(
        ev,
        Ok(Event {
            t_us: 4_177_463_145,
            node: "/dev/input/event10",
            kind: 3,
            code: 0x1a,
            value: -4,
        })
    );
}

#[test]
fn short_fraction_scales_to_microseconds() {
    let ev = parse_line("[ 12.5] /dev/input/event1: 0000 0000 00000000")
        .unwrap()
        .unwrap();
    assert_eq!(ev.t_us, 12_500_000);
}

#[test]
fn non_event_lines_are_skipped() {
    assert!(parse_line("add device 2: /dev/input/event10").is_none());
    assert!(parse_line("  name:     \"sec_e-pen\"").is_none());
    assert!(parse_line("").is_none());
}

#[test]
fn malformed_event_lines_are_errors() {
    assert_eq!(
        parse_line("[ 1.000001 /dev/input/event1: 0 0 0"),
        Some(Err(LineError::UnclosedTimestamp))
    );
    assert_eq!(
        parse_line("[ 1.0000001] /dev/input/event1: 0000 0000 00000000"),
        Some(Err(LineError::Timestamp("1.0000001".into())))
    );
    assert_eq!(
        parse_line("[ 1.5] /dev/input/event1: 0000 0000"),
        Some(Err(LineError::Fields("0000 0000".into())))
    );
    assert_eq!(
        parse_line("[ 1.5] /dev/input/event1: 0000 zz 00000000"),
        Some(Err(LineError::Hex("zz".into())))
    );
}
