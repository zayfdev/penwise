use super::*;

#[test]
fn base64_matches_the_standard_alphabet() {
    assert_eq!("", base64(b""));
    assert_eq!("Zg==", base64(b"f"));
    assert_eq!("Zm8=", base64(b"fo"));
    assert_eq!("Zm9v", base64(b"foo"));
    assert_eq!("Zm9vYg==", base64(b"foob"));
    assert_eq!("Zm9vYmE=", base64(b"fooba"));
    assert_eq!("Zm9vYmFy", base64(b"foobar"));
}

#[test]
fn model_chatter_is_trimmed_but_line_breaks_stay() {
    assert_eq!("milk\neggs", tidy("  milk\neggs \n"));
    assert_eq!("milk\neggs", tidy("```\nmilk\neggs\n```"));
    assert_eq!("milk", tidy("```text\nmilk```"));
    assert_eq!("", tidy("   "));
}

#[test]
fn truncation_respects_character_boundaries() {
    assert_eq!("héllo", truncate("héllo wörld", 6));
    assert_eq!("", truncate("anything", 0));
}
