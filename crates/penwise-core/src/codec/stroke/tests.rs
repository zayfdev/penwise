use super::*;
use crate::brush::{
    BlendMode, BrushFamily, CapStyle, GrainParams, Jitter, NibParams, OutlineKind, OutlineParams,
    Preset, SprayDistribution, SprayParams, StampParams,
};
use crate::dynamics::{Dynamics, PressureCurve, TiltResponse, VelocityResponse};
use crate::hash::fnv1a64;
use crate::smoothing::OneEuroParams;

/// Deterministic test PRNG (splitmix64).
struct Mix(u64);

impl Mix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..=hi`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = u64::try_from(hi - lo + 1).unwrap();
        lo + i64::try_from(self.next() % span).unwrap()
    }
}

fn grid(x: i32, y: i32, pressure: u16, tilt: u16, azimuth: i16, t_us: u32) -> Sample {
    GridSample {
        x,
        y,
        pressure,
        tilt,
        azimuth,
        t_us,
    }
    .to_sample()
}

fn random_grid_samples(seed: u64, n: usize) -> Vec<Sample> {
    let mut rng = Mix(seed);
    (0..n)
        .map(|_| {
            let lim = 1_i64 << 24;
            grid(
                i32::try_from(rng.range(-lim, lim)).unwrap(),
                i32::try_from(rng.range(-lim, lim)).unwrap(),
                u16::try_from(rng.range(0, 4096)).unwrap(),
                u16::try_from(rng.range(0, 1024)).unwrap(),
                i16::try_from(rng.range(-2047, 2048)).unwrap(),
                u32::try_from(rng.next() >> 32).unwrap(),
            )
        })
        .collect()
}

fn encode(record: &StrokeRecord) -> Vec<u8> {
    let mut out = Vec::new();
    encode_stroke(record, &mut out);
    out
}

fn record(params: BrushParams, samples: Vec<Sample>) -> StrokeRecord {
    StrokeRecord {
        params,
        seed: 0x0123_4567_89AB_CDEF,
        samples,
    }
}

/// Recomputes the trailer after a deliberate edit so only the edited field is wrong.
fn reseal(mut bytes: Vec<u8>) -> Vec<u8> {
    let split = bytes.len() - 8;
    let hash = fnv1a64(&bytes[..split]);
    bytes[split..].copy_from_slice(&hash.to_le_bytes());
    bytes
}

fn assert_round_trip(rec: &StrokeRecord) {
    let bytes = encode(rec);
    let decoded = decode_stroke(&bytes).unwrap();
    let quantized: Vec<Sample> = rec.samples.iter().map(|s| s.quantize()).collect();
    assert_eq!(decoded.params, rec.params);
    assert_eq!(decoded.seed, rec.seed);
    assert_eq!(decoded.samples, quantized);
    assert_eq!(encode(&decoded), bytes, "encoding is canonical");
}

#[test]
fn every_preset_params_round_trip() {
    for preset in Preset::ALL {
        let params = preset.params();
        let mut out = Vec::new();
        encode_params(&params, &mut out);
        let mut r = Reader::new(&out);
        assert_eq!(decode_params(&mut r).unwrap(), params, "{}", preset.name());
        assert_eq!(r.remaining(), 0);
        assert_round_trip(&record(params, random_grid_samples(7, 16)));
    }
}

#[test]
fn random_grid_samples_round_trip() {
    for seed in 0..32 {
        let n = usize::try_from(seed * 7 % 50).unwrap();
        let rec = record(Preset::Pencil.params(), random_grid_samples(seed, n));
        assert_round_trip(&rec);
    }
}

#[test]
fn extremes_round_trip() {
    let lim = 1 << 24;
    let samples = vec![
        grid(-lim, lim, 0, 0, -2047, 0),
        grid(lim, -lim, 4096, 1024, 2048, u32::MAX),
        grid(-lim, -lim, 0, 1024, -2047, 0),
        grid(-1, -1, 4096, 0, 2048, u32::MAX - 1),
        grid(0, 0, 1, 1, 0, 3),
    ];
    assert_round_trip(&record(Preset::Ballpoint.params(), samples));
}

#[test]
fn t_us_wraps_across_u32_max() {
    let ts = [
        u32::MAX - 2_073,
        u32::MAX,
        0,
        2_074,
        1,
        u32::MAX,
        0x8000_0000,
        0,
    ];
    let samples = ts.iter().map(|&t| grid(16, -16, 2048, 0, 0, t)).collect();
    assert_round_trip(&record(Preset::Ballpoint.params(), samples));
}

#[test]
fn off_grid_and_non_finite_samples_decode_quantized() {
    let samples = vec![
        Sample {
            x: 10.03,
            y: -3.97,
            pressure: 0.333,
            tilt: 0.7,
            azimuth: -3.0,
            t_us: 17,
        },
        Sample {
            x: f32::NAN,
            y: f32::INFINITY,
            pressure: 2.0,
            tilt: -1.0,
            azimuth: 10.0,
            t_us: 4_000,
        },
    ];
    assert_round_trip(&record(Preset::Fountain.params(), samples));
}

#[test]
fn empty_stroke_round_trips() {
    assert_round_trip(&record(Preset::Marker.params(), Vec::new()));
}

#[test]
fn non_finite_params_encode_as_zero() {
    let mut params = Preset::Ballpoint.params();
    params.opacity = f32::NAN;
    params.size_min = -0.0;
    let decoded = decode_stroke(&encode(&record(params, Vec::new()))).unwrap();
    assert_eq!(decoded.params.opacity.to_bits(), 0);
    assert_eq!(decoded.params.size_min.to_bits(), 0);
}

fn golden_params() -> BrushParams {
    let euro = OneEuroParams {
        min_cutoff_hz: 4.0,
        beta: 0.0,
        d_cutoff_hz: 1.0,
    };
    BrushParams {
        family: BrushFamily::Outline(OutlineKind::Ballpoint),
        color: Color::rgb(20, 20, 30),
        opacity: 1.0,
        blend: BlendMode::Normal,
        size_min: 2.0,
        size_max: 4.0,
        size_gamma: 1.0,
        dynamics: Dynamics {
            pressure: PressureCurve {
                floor: 0.0,
                saturation: 1.0,
                gamma: 1.0,
            },
            velocity: VelocityResponse {
                v0: 300.0,
                v1: 3000.0,
                size: 0.0,
                opacity: 0.0,
            },
            tilt: TiltResponse {
                theta0: 0.5,
                theta1: 1.0,
                max_aspect: 1.0,
                max_radius_scale: 1.0,
                side_hardness: 1.0,
                side_alpha: 1.0,
            },
        },
        smoothing: SmoothingParams {
            position: euro,
            pressure: euro,
        },
        outline: OutlineParams {
            smoothing: 0.5,
            taper_start: 0.0,
            taper_end: 0.0,
            cap_start: CapStyle::Round,
            cap_end: CapStyle::Flat,
            nib: NibParams {
                angle: 0.0,
                aspect: 1.0,
                thickness: 0.0,
                fixation: 1.0,
            },
        },
        stamp: StampParams {
            flow: 1.0,
            flow_pressure: 0.0,
            hardness: 1.0,
            spacing: 0.125,
            scatter: 0.0,
            jitter: Jitter::NONE,
            dabs_per_second: 0.0,
            grain: Some(GrainParams {
                scale: 1.0,
                strength: 0.5,
                softness: 0.25,
            }),
            spray: Some(SprayParams {
                particles_max: 300,
                particle_size_min: 0.5,
                particle_size_max: 2.0,
                distribution: SprayDistribution::Gaussian,
            }),
        },
    }
}

fn golden_record() -> StrokeRecord {
    record(
        golden_params(),
        vec![
            grid(160, -40, 2048, 512, -1024, 0),
            grid(161, -32, 3072, 500, 2048, 2074),
            grid(100, -32, 4096, 0, 0, 4148),
        ],
    )
}

/// Stroke blob v1 golden: changing it means a deliberate format change (bump `STROKE_VERSION`).
const GOLDEN: [u8; 241] = [
    0x50, 0x57, 0x53, 0x4B, 0x01, 0x00, 0x00, 0x14, 0x14, 0x1E, 0xFF, 0x00, //
    0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x80, 0x40, //
    0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, //
    0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x96, 0x43, 0x00, 0x80, 0x3B, 0x45, //
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3F, //
    0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x80, 0x3F, //
    0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x80, 0x40, //
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x80, 0x40, //
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x3F, //
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, //
    0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x80, 0x3F, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x80, 0x3F, 0x00, 0x00, 0x00, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x00, 0x01, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x3F, 0x00, //
    0x00, 0x80, 0x3E, 0x01, 0xAC, 0x02, 0x00, 0x00, 0x00, 0x3F, 0x00, 0x00, //
    0x00, 0x40, 0x01, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01, 0x03, //
    0xC0, 0x02, 0x4F, 0x80, 0x10, 0x80, 0x04, 0xFF, 0x0F, 0x00, 0x02, 0x10, //
    0x80, 0x10, 0x17, 0x80, 0x30, 0xB4, 0x20, 0x79, 0x00, 0x80, 0x10, 0xE7, //
    0x07, 0xFF, 0x1F, 0xB4, 0x20, 0x2F, 0xC5, 0x7B, 0x34, 0x31, 0xAD, 0x66, //
    0x9D,
];

#[test]
fn golden_blob_is_stable() {
    let rec = golden_record();
    assert_eq!(encode(&rec), GOLDEN);
    assert_eq!(decode_stroke(&GOLDEN).unwrap(), rec);
}

#[test]
fn encode_appends_after_existing_bytes() {
    let mut out = vec![0xAA, 0xBB];
    encode_stroke(&golden_record(), &mut out);
    assert_eq!(&out[..2], &[0xAA, 0xBB]);
    assert_eq!(&out[2..], GOLDEN);
}

#[test]
fn every_truncation_is_an_error() {
    for len in 0..GOLDEN.len() {
        assert!(decode_stroke(&GOLDEN[..len]).is_err(), "prefix {len}");
    }
    for len in GOLDEN.len() - 8..GOLDEN.len() {
        assert!(matches!(
            decode_stroke(&GOLDEN[..len]),
            Err(CodecError::UnexpectedEof { .. })
        ));
    }
    assert_eq!(
        decode_stroke(&GOLDEN[..3]),
        Err(CodecError::UnexpectedEof { offset: 0 })
    );
}

#[test]
fn bad_magic_and_version() {
    let mut bytes = GOLDEN.to_vec();
    bytes[0] = b'X';
    assert_eq!(
        decode_stroke(&bytes),
        Err(CodecError::BadMagic { found: *b"XWSK" })
    );
    let mut bytes = GOLDEN.to_vec();
    bytes[4] = 2;
    assert_eq!(
        decode_stroke(&bytes),
        Err(CodecError::UnsupportedVersion(2))
    );
}

/// Offset of the sample `count` varint in [`GOLDEN`] (header 6 + params 189 + seed 8).
const COUNT_AT: usize = 6 + 189 + 8;

#[test]
fn six_byte_count_varint_overflows() {
    assert_eq!(GOLDEN[COUNT_AT], 0x03);
    let mut bytes = GOLDEN[..COUNT_AT].to_vec();
    bytes.extend_from_slice(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00]);
    bytes.extend_from_slice(&[0; 8]);
    assert_eq!(
        decode_stroke(&reseal(bytes)),
        Err(CodecError::VarintOverflow { offset: COUNT_AT })
    );
}

#[test]
fn flipped_bits_fail_the_checksum() {
    // Seed byte (fixed width: structure unchanged) and a trailer byte.
    for at in [COUNT_AT - 1, GOLDEN.len() - 1] {
        let mut bytes = GOLDEN.to_vec();
        bytes[at] ^= 0x01;
        assert!(matches!(
            decode_stroke(&bytes),
            Err(CodecError::ChecksumMismatch { .. })
        ));
    }
    // An invalid family code with a stale trailer reports the checksum first.
    let mut bytes = GOLDEN.to_vec();
    bytes[6] = 0x7F;
    assert!(matches!(
        decode_stroke(&bytes),
        Err(CodecError::ChecksumMismatch { .. })
    ));
}

#[test]
fn trailing_byte_is_rejected() {
    let mut bytes = GOLDEN.to_vec();
    bytes.push(0);
    assert_eq!(
        decode_stroke(&bytes),
        Err(CodecError::TrailingBytes { count: 1 })
    );
}

#[test]
fn invalid_codes_with_valid_checksum() {
    let cases = [
        (6, 0x7F, "family"),
        (15, 9, "blend"),
        (
            6 + 1 + 4 + 4 + 1 + 12 + 12 + 16 + 24 + 24 + 12,
            2,
            "outline.cap_start",
        ),
    ];
    for (at, value, field) in cases {
        let mut bytes = GOLDEN.to_vec();
        bytes[at] = value;
        assert_eq!(
            decode_stroke(&reseal(bytes)),
            Err(CodecError::InvalidValue { field })
        );
    }
    // NaN opacity.
    let mut bytes = GOLDEN.to_vec();
    bytes[11..15].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
    assert_eq!(
        decode_stroke(&reseal(bytes)),
        Err(CodecError::InvalidValue { field: "opacity" })
    );
}

#[test]
fn grid_values_out_of_range() {
    let mut params = Vec::new();
    encode_params(&golden_params(), &mut params);
    let blob = |samples: &[u8]| {
        let mut b = b"PWSK\x01\x00".to_vec();
        b.extend_from_slice(&params);
        b.extend_from_slice(&[0; 8]);
        b.extend_from_slice(samples);
        b.extend_from_slice(&[0; 8]);
        reseal(b)
    };
    // count 1; pressure 4097 (uv 0x81 0x20).
    let pressure = blob(&[1, 0, 0, 0x81, 0x20, 0, 0, 0]);
    assert_eq!(
        decode_stroke(&pressure),
        Err(CodecError::OutOfRange { field: "pressure" })
    );
    // count 2; second sample tilt delta −1 from 0.
    let tilt = blob(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0, 0]);
    assert_eq!(
        decode_stroke(&tilt),
        Err(CodecError::OutOfRange { field: "tilt" })
    );
    // count 1; azimuth −2048 (zigzag 4095 = 0xFF 0x1F).
    let azimuth = blob(&[1, 0, 0, 0, 0, 0xFF, 0x1F, 0]);
    assert_eq!(
        decode_stroke(&azimuth),
        Err(CodecError::OutOfRange { field: "azimuth" })
    );
    // count 1; x = 2^24 + 1 (zigzag 2^25 + 2).
    let mut x = vec![1];
    crate::codec::varint::write_sv32(&mut x, (1 << 24) + 1);
    x.extend_from_slice(&[0, 0, 0, 0, 0]);
    assert_eq!(
        decode_stroke(&blob(&x)),
        Err(CodecError::OutOfRange { field: "x" })
    );
}

#[test]
fn hostile_count_does_not_preallocate() {
    let mut bytes = GOLDEN[..COUNT_AT].to_vec();
    bytes.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    bytes.extend_from_slice(&[0; 8]);
    assert!(matches!(
        decode_stroke(&reseal(bytes)),
        Err(CodecError::UnexpectedEof { .. })
    ));
}
