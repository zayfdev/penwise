use super::*;
use crate::hash::fnv1a64;

const RANGE0: AxisRange = AxisRange { min: 0, max: 0 };

fn frame(x: i32, y: i32, pressure: i32, tilt: (i32, i32), t_us: i32) -> RawFrame {
    RawFrame {
        x,
        y,
        pressure,
        tilt_x: tilt.0,
        tilt_y: tilt.1,
        distance: 0,
        t_us,
    }
}

fn sm_x710_header(channels: Channels, source: RecordingSource) -> RecordingHeader {
    let d = DigitizerRanges::SM_X710;
    let mut ranges = [RANGE0; CHANNEL_COUNT];
    let all = [d.x, d.y, d.pressure, d.tilt_x, d.tilt_y, RANGE0, RANGE0];
    for (slot, range) in ranges.iter_mut().enumerate() {
        if has_channel(channels, slot) {
            *range = all[slot];
        }
    }
    RecordingHeader {
        channels,
        source,
        device: "SM-X710/sec_e-pen".to_owned(),
        ranges,
        x_scale: d.x_scale,
        y_scale: d.y_scale,
        rate_hint: 482,
    }
}

/// Deterministic pseudo-random strokes inside the `SM_X710` ranges (LCG; t steps of ~2074 µs).
fn strokes(seed: u64, count: usize) -> Vec<RecordedStroke> {
    let mut state = seed;
    let mut next = |m: u32| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        i32::try_from((state >> 33) % u64::from(m)).unwrap()
    };
    (0..count)
        .map(|i| {
            let n = 1 + usize::try_from(next(40)).unwrap();
            let mut t = 0;
            let frames = (0..n)
                .map(|k| {
                    if k > 0 {
                        t += 2_000 + next(150);
                    }
                    let tilt = (next(127) - 63, next(127) - 63);
                    frame(next(14_753), next(23_604), next(4_096), tilt, t)
                })
                .collect();
            RecordedStroke {
                kind: if i % 3 == 2 {
                    RunKind::Hover
                } else {
                    RunKind::Contact
                },
                t0_us: u64::try_from(i).unwrap() * 1_000_003,
                frames,
            }
        })
        .collect()
}

fn encode(rec: &Recording) -> Vec<u8> {
    let mut out = Vec::new();
    encode_pwrec(rec, &mut out);
    out
}

fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let split = bytes.len() - 8;
    let hash = fnv1a64(&bytes[..split]);
    bytes[split..].copy_from_slice(&hash.to_le_bytes());
    bytes
}

fn assert_round_trip(rec: &Recording) {
    let bytes = encode(rec);
    let decoded = decode_pwrec(&bytes).unwrap();
    assert_eq!(&decoded, rec);
    assert_eq!(encode(&decoded), bytes, "encoding is canonical");
}

#[test]
fn default_channels_round_trip() {
    for seed in 0..8 {
        let rec = Recording {
            header: sm_x710_header(Channels::V1_DEFAULT, RecordingSource::Getevent),
            strokes: strokes(seed, 5),
        };
        assert_round_trip(&rec);
    }
}

#[test]
fn extremes_and_all_channels_round_trip() {
    let all = Channels((1 << CHANNEL_COUNT) - 1);
    let mut header = sm_x710_header(all, RecordingSource::MotionEvent);
    header.ranges[5] = AxisRange { min: -1, max: 255 };
    header.ranges[6] = AxisRange {
        min: i32::MIN,
        max: i32::MAX,
    };
    header.device = "Ünïcode ✍".to_owned();
    let mut extreme = frame(-1_000_000, i32::MAX / 2, 4_095, (-63, 63), 0);
    extreme.distance = -7;
    let frames = vec![
        extreme,
        frame(i32::MIN / 2, -(i32::MAX / 2), 0, (63, -63), i32::MAX),
        frame(0, 0, 4_095, (0, 0), i32::MAX),
    ];
    let rec = Recording {
        header,
        strokes: vec![RecordedStroke {
            kind: RunKind::Hover,
            t0_us: u64::MAX,
            frames,
        }],
    };
    assert_round_trip(&rec);
}

#[test]
fn empty_recording_and_subset_channels_round_trip() {
    let xy_p = Channels(Channels::X | Channels::Y | Channels::PRESSURE);
    let mut header = sm_x710_header(xy_p, RecordingSource::Getevent);
    header.device = String::new();
    let mut rec = Recording {
        header,
        strokes: Vec::new(),
    };
    assert_round_trip(&rec);
    rec.strokes = vec![RecordedStroke {
        kind: RunKind::Contact,
        t0_us: 0,
        frames: vec![frame(5, 6, 7, (0, 0), 0), frame(4, 9, 0, (0, 0), 0)],
    }];
    assert_round_trip(&rec);
}

/// Tiny `.pwrec` v1 golden: channels x|y|pressure|t, device "d", one contact stroke of two frames.
const GOLDEN: [u8; 51] = [
    0x50, 0x57, 0x52, 0x43, 0x01, 0x00, 0x47, 0x00, 0x00, 0x00, 0x01, 0x64, 0x00, 0x14, 0x00,
    0x28, //
    0x00, 0xFE, 0x3F, 0x00, 0x00, 0x01, 0x01, 0x02, 0x01, 0xE2, 0x03, 0x01, 0x00, 0xE8, 0x07,
    0x02, //
    0x02, 0x04, 0xFE, 0x3F, 0x00, 0x04, 0x01, 0xFD, 0x3F, 0xB4, 0x20, 0x3D, 0xFC, 0x75, 0x46,
    0xAC, //
    0xB5, 0xE6, 0xEE,
];

fn golden_recording() -> Recording {
    let mut ranges = [RANGE0; CHANNEL_COUNT];
    ranges[0] = AxisRange { min: 0, max: 10 };
    ranges[1] = AxisRange { min: 0, max: 20 };
    ranges[2] = AxisRange { min: 0, max: 4095 };
    Recording {
        header: RecordingHeader {
            channels: Channels(0x47),
            source: RecordingSource::Getevent,
            device: "d".to_owned(),
            ranges,
            x_scale: PxScale { num: 1, den: 1 },
            y_scale: PxScale { num: 2, den: 1 },
            rate_hint: 482,
        },
        strokes: vec![RecordedStroke {
            kind: RunKind::Contact,
            t0_us: 1000,
            frames: vec![frame(1, 2, 4095, (0, 0), 0), frame(3, 1, 0, (0, 0), 2074)],
        }],
    }
}

#[test]
fn golden_file_is_stable() {
    assert_eq!(encode(&golden_recording()), GOLDEN);
    assert_eq!(decode_pwrec(&GOLDEN).unwrap(), golden_recording());
}

#[test]
fn truncation_magic_version_trailing() {
    for len in 0..GOLDEN.len() {
        assert!(decode_pwrec(&GOLDEN[..len]).is_err(), "prefix {len}");
    }
    assert!(matches!(
        decode_pwrec(&GOLDEN[..GOLDEN.len() - 1]),
        Err(CodecError::UnexpectedEof { .. })
    ));
    let mut bytes = GOLDEN.to_vec();
    bytes[3] = b'K';
    assert_eq!(
        decode_pwrec(&bytes),
        Err(CodecError::BadMagic { found: *b"PWRK" })
    );
    let mut bytes = GOLDEN.to_vec();
    bytes[5] = 1;
    assert_eq!(
        decode_pwrec(&bytes),
        Err(CodecError::UnsupportedVersion(0x0101))
    );
    let mut bytes = GOLDEN.to_vec();
    bytes.extend_from_slice(&[0, 0]);
    assert_eq!(
        decode_pwrec(&bytes),
        Err(CodecError::TrailingBytes { count: 2 })
    );
}

#[test]
fn checksum_flip_is_detected() {
    // Frame 0 x byte (keeps the structure) and a trailer byte.
    for at in [32, GOLDEN.len() - 3] {
        let mut bytes = GOLDEN.to_vec();
        bytes[at] ^= 0x04;
        assert!(matches!(
            decode_pwrec(&bytes),
            Err(CodecError::ChecksumMismatch { .. })
        ));
    }
}

#[test]
fn over_long_varints_overflow() {
    // rate_hint (offset 25) as a 6-byte varint.
    let mut bytes = GOLDEN[..25].to_vec();
    bytes.extend_from_slice(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00]);
    assert_eq!(
        decode_pwrec(&bytes),
        Err(CodecError::VarintOverflow { offset: 25 })
    );
    // t0_us (offset 29) as an 11-byte varint.
    let mut bytes = GOLDEN[..29].to_vec();
    bytes.extend_from_slice(&[0x80; 10]);
    bytes.push(0x00);
    assert_eq!(
        decode_pwrec(&bytes),
        Err(CodecError::VarintOverflow { offset: 29 })
    );
}

#[test]
fn invalid_fields_with_valid_checksum() {
    let cases: [(&[(usize, u8)], &str); 10] = [
        (&[(7, 0x01)], "channels"),
        (&[(6, 0x00)], "channels"),
        (&[(8, 2)], "source"),
        (&[(9, 1)], "reserved"),
        (&[(11, 0xFF)], "device"),
        (&[(22, 0)], "px_den"),
        (&[(28, 2)], "kind"),
        (&[(31, 0)], "sample count"),
        (&[(36, 0x02)], "t_us"),
        // Frame 1 Δt = −1 as a 2-byte varint: t goes negative.
        (&[(41, 0x81), (42, 0x00)], "t_us"),
    ];
    for (edits, field) in cases {
        let mut bytes = GOLDEN.to_vec();
        for &(at, value) in edits {
            bytes[at] = value;
        }
        assert_eq!(
            decode_pwrec(&reseal(bytes)),
            Err(CodecError::InvalidValue { field }),
            "{edits:?}"
        );
    }
}

#[test]
fn extreme_values_round_trip_through_wrapping_deltas() {
    let mut rec = golden_recording();
    rec.strokes[0].frames = vec![
        frame(i32::MAX, i32::MIN, 0, (0, 0), 0),
        frame(i32::MIN, i32::MAX, 0, (0, 0), 1),
    ];
    assert_eq!(decode_pwrec(&encode(&rec)), Ok(rec));
}

#[test]
fn digitizer_ranges_from_header() {
    let rec = Recording {
        header: sm_x710_header(Channels::V1_DEFAULT, RecordingSource::Getevent),
        strokes: Vec::new(),
    };
    assert_eq!(rec.digitizer_ranges(), Ok(DigitizerRanges::SM_X710));

    let mut no_tilt = rec.clone();
    no_tilt.header.channels = Channels(Channels::X | Channels::Y | Channels::PRESSURE);
    let ranges = no_tilt.digitizer_ranges().unwrap();
    assert_eq!((ranges.tilt_x, ranges.tilt_y), (RANGE0, RANGE0));

    let mut no_pressure = rec.clone();
    no_pressure.header.channels = Channels(Channels::X | Channels::Y | Channels::T_US);
    assert_eq!(
        no_pressure.digitizer_ranges(),
        Err(CodecError::InvalidValue { field: "channels" })
    );

    let mut motion = rec;
    motion.header.source = RecordingSource::MotionEvent;
    assert_eq!(
        motion.digitizer_ranges(),
        Err(CodecError::UnsupportedSource(1))
    );
}

#[test]
fn raw_frame_to_raw_sample() {
    let mut f = frame(1, 2, 3, (-4, 5), 6);
    assert_eq!(
        f.to_raw_sample(),
        RawSample {
            x: 1,
            y: 2,
            pressure: 3,
            tilt_x_deg: -4,
            tilt_y_deg: 5,
            t_us: 6,
        }
    );
    f.t_us = -9;
    assert_eq!(f.to_raw_sample().t_us, 0);
}

#[test]
fn stroke_to_samples_maps_and_quantizes() {
    let ranges = DigitizerRanges::SM_X710;
    let stroke = RecordedStroke {
        kind: RunKind::Contact,
        t0_us: 0,
        frames: vec![
            frame(0, 0, 0, (0, 0), 0),
            frame(14_753, 0, 4_095, (0, 0), 2_074),
        ],
    };
    let mut out = vec![Sample {
        x: 9.0,
        y: 9.0,
        pressure: 1.0,
        tilt: 0.0,
        azimuth: 0.0,
        t_us: 1,
    }];
    stroke.to_samples(&ranges, &mut out);
    assert_eq!(out.len(), 2);
    assert_eq!(
        (out[0].x, out[0].y, out[0].pressure, out[0].t_us),
        (0.0, 0.0, 0.0, 0)
    );
    assert_eq!(
        (out[1].x, out[1].pressure, out[1].t_us),
        (1600.0, 1.0, 2_074)
    );
    assert_eq!((out[1].tilt, out[1].azimuth), (0.0, 0.0));
    for (s, f) in out.iter().zip(&stroke.frames) {
        assert_eq!(*s, ranges.map(&f.to_raw_sample()).quantize());
    }
}
