use super::{Fnv1a64, fnv1a64};

#[test]
fn reference_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
}

#[test]
fn chunking_is_invisible() {
    let data = b"penwise geometry goldens";
    let mut h = Fnv1a64::new();
    h.write(&data[..7]);
    h.write(&data[7..]);
    assert_eq!(h.finish(), fnv1a64(data));
}
