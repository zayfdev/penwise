use super::Seed;

const EDGES: [i64; 9] = [
    i64::MIN,
    i64::MIN + 1,
    -4_611_686_018_427_387_904,
    -2,
    -1,
    0,
    1,
    i64::MAX - 1,
    i64::MAX,
];

#[test]
fn signed_wire_round_trips_over_the_edges() {
    for wire in EDGES {
        let seed = Seed::from_i64_wire(wire);
        assert_eq!(seed.to_i64_wire(), wire);
        assert_eq!(Seed::from_i64_wire(seed.to_i64_wire()), seed);
    }
}

#[test]
fn unsigned_round_trips_through_the_signed_wire() {
    for bits in [0, 1, u64::MAX, u64::MAX - 1, 1 << 63, (1 << 63) - 1] {
        assert_eq!(Seed(bits).to_i64_wire().cast_unsigned(), bits);
        assert_eq!(Seed::from_i64_wire(Seed(bits).to_i64_wire()), Seed(bits));
    }
}

#[test]
fn negative_wire_values_map_to_their_two_complement_bits() {
    assert_eq!(Seed::from_i64_wire(-1).get(), u64::MAX);
    assert_eq!(Seed::from_i64_wire(i64::MIN).get(), 1 << 63);
}

#[test]
fn decimal_text_matches_the_signed_wire_and_accepts_large_unsigned() {
    for wire in EDGES {
        assert_eq!(
            Seed::parse_decimal(&wire.to_string()),
            Some(Seed::from_i64_wire(wire))
        );
    }
    assert_eq!(
        Seed::parse_decimal("18446744073709551615"),
        Some(Seed(u64::MAX))
    );
    assert_eq!(
        Seed::parse_decimal("9223372036854775808"),
        Some(Seed(1 << 63))
    );
}

#[test]
fn non_numbers_and_out_of_range_text_are_rejected() {
    for text in [
        "",
        "abc",
        "1.5",
        "-",
        "18446744073709551616",
        "-9223372036854775809",
        " 1",
    ] {
        assert_eq!(Seed::parse_decimal(text), None, "{text:?}");
    }
}
