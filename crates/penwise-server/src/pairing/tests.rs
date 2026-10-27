use super::*;

#[test]
fn a_code_is_six_unambiguous_characters() {
    let pairing = Pairing::new();
    let code = pairing.current();
    assert_eq!(CODE_LEN, code.len());
    assert!(code.chars().all(|c| ALPHABET.contains(&(c as u8))));
    assert!(!code.contains('O') && !code.contains('0') && !code.contains('I'));
}

#[test]
fn exchanging_the_code_works_once() {
    let pairing = Pairing::new();
    let code = pairing.current();
    assert!(pairing.exchange(&code), "first exchange");
    assert!(!pairing.exchange(&code), "single use");
}

#[test]
fn case_spaces_and_dashes_do_not_matter() {
    let pairing = Pairing::new();
    let code = pairing.current().to_lowercase();
    let typed = format!(" {} -{} ", &code[..3], &code[3..]);
    assert!(pairing.exchange(&typed));
}

#[test]
fn a_wrong_code_counts_toward_the_lockout() {
    let pairing = Pairing::new();
    for _ in 0..MAX_FAILURES {
        assert!(!pairing.exchange("ZZZZZZ"));
    }
    assert!(pairing.locked(), "locked after the budget");
    assert!(
        !pairing.exchange(&pairing.current()),
        "a correct code is refused while locked"
    );
}

#[test]
fn refreshing_invalidates_the_previous_code() {
    let pairing = Pairing::new();
    let old = pairing.current();
    let new = pairing.refresh();
    assert_ne!(old, new);
    assert!(!pairing.exchange(&old));
    assert!(pairing.exchange(&new));
}

#[test]
fn codes_do_not_repeat_in_a_row() {
    let pairing = Pairing::new();
    let first = pairing.refresh();
    let second = pairing.refresh();
    assert_ne!(first, second);
}

#[test]
fn remaining_time_shrinks() {
    let pairing = Pairing::new();
    assert!(pairing.remaining() <= CODE_TTL);
    assert!(pairing.remaining() > Duration::from_secs(590));
}
