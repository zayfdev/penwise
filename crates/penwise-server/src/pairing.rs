//! Short pairing codes: type six characters on the tablet instead of copying a 43-character token.
//!
//! A code is single-use and expires; exchanging it returns the server's token. Codes are deliberately short, so
//! the exchange is rate limited: after [`MAX_FAILURES`] bad attempts the window locks for [`LOCKOUT`].

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// How long a freshly minted code stays valid.
pub const CODE_TTL: Duration = Duration::from_secs(600);
/// Failed exchanges tolerated inside [`LOCKOUT`].
pub const MAX_FAILURES: usize = 8;
/// Window for counting failures (and the lockout itself).
pub const LOCKOUT: Duration = Duration::from_secs(60);
/// Characters that cannot be mistaken for one another when read off a screen.
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// Code length.
pub const CODE_LEN: usize = 6;

/// The live pairing code and its attempt accounting.
pub struct Pairing {
    code: Mutex<Code>,
    failures: Mutex<VecDeque<Instant>>,
    /// Monotonic entropy; advanced per mint so codes do not repeat within a process.
    counter: AtomicU64,
}

struct Code {
    value: String,
    minted: Instant,
}

impl Pairing {
    /// A fresh pairing state with one code ready to print.
    #[must_use]
    pub fn new() -> Self {
        let pairing = Self {
            code: Mutex::new(Code {
                value: String::new(),
                minted: Instant::now(),
            }),
            failures: Mutex::new(VecDeque::new()),
            counter: AtomicU64::new(0),
        };
        pairing.refresh();
        pairing
    }

    /// The current code, expiring it first when its time is up.
    pub fn current(&self) -> String {
        let mut code = self
            .code
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if code.minted.elapsed() >= CODE_TTL {
            code.value = self.mint();
            code.minted = Instant::now();
        }
        code.value.clone()
    }

    /// Mints a new code, invalidating the previous one.
    pub fn refresh(&self) -> String {
        let value = self.mint();
        let mut code = self
            .code
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        code.value.clone_from(&value);
        code.minted = Instant::now();
        value
    }

    /// How long the current code still has.
    pub fn remaining(&self) -> Duration {
        let code = self
            .code
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        CODE_TTL.saturating_sub(code.minted.elapsed())
    }

    /// True when the attempt budget for this window is spent.
    pub fn locked(&self) -> bool {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while failures.front().is_some_and(|at| at.elapsed() >= LOCKOUT) {
            failures.pop_front();
        }
        failures.len() >= MAX_FAILURES
    }

    /// Checks `candidate` and, on success, burns it. Comparison ignores case, spaces and dashes.
    pub fn exchange(&self, candidate: &str) -> bool {
        if self.locked() {
            return false;
        }
        let normalized = normalize(candidate);
        let mut code = self
            .code
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fresh = code.minted.elapsed() < CODE_TTL;
        if fresh && code.value == normalized {
            // Single use: the next exchange needs a refresh.
            code.minted = Instant::now()
                .checked_sub(CODE_TTL)
                .unwrap_or_else(Instant::now);
            return true;
        }
        drop(code);
        self.record_failure();
        false
    }

    fn record_failure(&self) {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        failures.push_back(Instant::now());
        while failures.len() > MAX_FAILURES {
            failures.pop_front();
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "the modulus keeps the value inside the 32-character alphabet"
    )]
    fn mint(&self) -> String {
        let mut value = String::with_capacity(CODE_LEN);
        // xorshift on a counter mixed with the clock: no CSPRNG needed, the code is single-use and rate limited.
        let seed = self.counter.fetch_add(1, Ordering::Relaxed)
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| u64::from(since.subsec_nanos()));
        let mut state = seed | 1;
        for _ in 0..CODE_LEN {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let index = (state % ALPHABET.len() as u64) as usize;
            value.push(char::from(ALPHABET[index]));
        }
        value
    }
}

impl Default for Pairing {
    fn default() -> Self {
        Self::new()
    }
}

/// Uppercases and drops spaces and dashes so `7k2m-9q` matches `7K2M9Q`.
#[must_use]
pub fn normalize(candidate: &str) -> String {
    candidate
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_uppercase)
        .collect()
}

#[cfg(test)]
mod tests {
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
}
