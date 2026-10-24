use super::*;

fn uv32_bytes(v: u32) -> Vec<u8> {
    let mut out = Vec::new();
    write_uv32(&mut out, v);
    out
}

#[test]
fn zigzag_maps_small_magnitudes_to_small_codes() {
    assert_eq!(zigzag32(0), 0);
    assert_eq!(zigzag32(-1), 1);
    assert_eq!(zigzag32(1), 2);
    assert_eq!(zigzag32(-2), 3);
    assert_eq!(zigzag32(i32::MAX), u32::MAX - 1);
    assert_eq!(zigzag32(i32::MIN), u32::MAX);
}

#[test]
fn zigzag_round_trips_extremes_and_sweep() {
    let mut v: i64 = i64::from(i32::MIN);
    while v <= i64::from(i32::MAX) {
        let x = i32::try_from(v).unwrap();
        assert_eq!(unzigzag32(zigzag32(x)), x);
        v += 65_537;
    }
    for x in [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX] {
        assert_eq!(unzigzag32(zigzag32(x)), x);
    }
}

#[test]
fn uv32_encoding_is_minimal_leb128() {
    assert_eq!(uv32_bytes(0), [0x00]);
    assert_eq!(uv32_bytes(0x7F), [0x7F]);
    assert_eq!(uv32_bytes(0x80), [0x80, 0x01]);
    assert_eq!(uv32_bytes(300), [0xAC, 0x02]);
    assert_eq!(uv32_bytes(u32::MAX), [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    let mut out = Vec::new();
    write_uv64(&mut out, u64::MAX);
    assert_eq!(out.len(), 10);
    assert_eq!(out[9], 0x01);
    let mut sv = Vec::new();
    write_sv32(&mut sv, -1);
    assert_eq!(sv, [0x01]);
}

#[test]
fn varints_round_trip() {
    let mut out = Vec::new();
    let u32s = [0, 1, 127, 128, 16_383, 16_384, 1 << 28, u32::MAX];
    let u64s = [0, 1 << 35, u64::from(u32::MAX) + 1, u64::MAX];
    let i32s = [0, -1, 1, -64, 64, i32::MIN, i32::MAX];
    for v in u32s {
        write_uv32(&mut out, v);
    }
    for v in u64s {
        write_uv64(&mut out, v);
    }
    for v in i32s {
        write_sv32(&mut out, v);
    }
    let mut r = Reader::new(&out);
    for v in u32s {
        assert_eq!(r.uv32().unwrap(), v);
    }
    for v in u64s {
        assert_eq!(r.uv64().unwrap(), v);
    }
    for v in i32s {
        assert_eq!(r.sv32().unwrap(), v);
    }
    assert_eq!(r.remaining(), 0);
    assert_eq!(r.position(), out.len());
}

#[test]
fn uv32_rejects_six_byte_and_overwide_encodings() {
    let six = [0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
    assert_eq!(
        Reader::new(&six).uv32(),
        Err(CodecError::VarintOverflow { offset: 0 })
    );
    // Fifth byte may only carry 4 bits.
    let wide = [0xFF, 0xFF, 0xFF, 0xFF, 0x1F];
    assert_eq!(
        Reader::new(&wide).uv32(),
        Err(CodecError::VarintOverflow { offset: 0 })
    );
    let eleven = [0x80; 11];
    assert_eq!(
        Reader::new(&eleven).uv64(),
        Err(CodecError::VarintOverflow { offset: 0 })
    );
    let wide64 = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x02];
    assert_eq!(
        Reader::new(&wide64).uv64(),
        Err(CodecError::VarintOverflow { offset: 0 })
    );
}

#[test]
fn truncated_reads_report_eof() {
    let buf = [0x80, 0x80];
    assert_eq!(
        Reader::new(&buf).uv32(),
        Err(CodecError::UnexpectedEof { offset: 2 })
    );
    let mut r = Reader::new(&[1, 2, 3]);
    assert_eq!(r.u8().unwrap(), 1);
    assert_eq!(r.u16_le().unwrap(), 0x0302);
    assert_eq!(r.u8(), Err(CodecError::UnexpectedEof { offset: 3 }));
    assert_eq!(
        Reader::new(&[0; 7]).u64_le(),
        Err(CodecError::UnexpectedEof { offset: 0 })
    );
    assert_eq!(
        Reader::new(&[0; 4]).bytes(usize::MAX),
        Err(CodecError::UnexpectedEof { offset: 0 })
    );
}

#[test]
fn fixed_width_reads_are_little_endian() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&0x0102_0304_0506_0708_u64.to_le_bytes());
    buf.extend_from_slice(&1.5_f32.to_bits().to_le_bytes());
    let mut r = Reader::new(&buf);
    assert_eq!(r.u64_le().unwrap(), 0x0102_0304_0506_0708);
    assert_eq!(r.f32_le("v").unwrap().to_bits(), 1.5_f32.to_bits());
}

#[test]
fn f32_rejects_non_finite() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let bytes = bad.to_bits().to_le_bytes();
        assert_eq!(
            Reader::new(&bytes).f32_le("field"),
            Err(CodecError::InvalidValue { field: "field" })
        );
    }
}
