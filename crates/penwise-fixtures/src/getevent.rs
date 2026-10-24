//! `getevent -t` capture lines: `[ <kernel secs> ] /dev/input/eventN: <type> <code> <value>`, all three fields
//! hex, value a signed 32-bit two's-complement word (docs/research/pen-recordings.md).

use thiserror::Error;

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_ABS: u16 = 0x03;
pub const SYN_REPORT: u16 = 0x00;
pub const BTN_TOUCH: u16 = 0x14a;
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_PRESSURE: u16 = 0x18;
pub const ABS_TILT_X: u16 = 0x1a;
pub const ABS_TILT_Y: u16 = 0x1b;

/// Multiplier turning an `n`-digit fraction of a second into µs (index = digit count).
const FRACTION_SCALE: [u64; 7] = [0, 100_000, 10_000, 1_000, 100, 10, 1];

/// One input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event<'a> {
    /// Kernel timestamp in µs.
    pub t_us: u64,
    /// Device node, e.g. `/dev/input/event10`.
    pub node: &'a str,
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

/// Malformed event line.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LineError {
    #[error("missing `]` after the timestamp")]
    UnclosedTimestamp,
    #[error("bad timestamp `{0}` (expected `<secs>.<1-6 digits>`)")]
    Timestamp(String),
    #[error("missing `: ` after the device node")]
    MissingNode,
    #[error("expected `<type> <code> <value>`, got `{0}`")]
    Fields(String),
    #[error("bad hex field `{0}`")]
    Hex(String),
}

/// `None` for lines that are not events (`add device …`, `name: …`, blank); `Some(Err)` for a line that starts
/// like an event (`[`) but does not parse.
#[must_use]
pub fn parse_line(line: &str) -> Option<Result<Event<'_>, LineError>> {
    let rest = line.trim_start().strip_prefix('[')?;
    Some(parse_event(rest))
}

fn parse_event(rest: &str) -> Result<Event<'_>, LineError> {
    let (stamp, rest) = rest.split_once(']').ok_or(LineError::UnclosedTimestamp)?;
    let t_us = parse_timestamp(stamp.trim())?;
    let (node, fields) = rest
        .trim_start()
        .split_once(": ")
        .ok_or(LineError::MissingNode)?;
    let mut it = fields.split_ascii_whitespace();
    let (Some(kind), Some(code), Some(value), None) = (it.next(), it.next(), it.next(), it.next())
    else {
        return Err(LineError::Fields(fields.trim().to_owned()));
    };
    Ok(Event {
        t_us,
        node,
        kind: hex_u16(kind)?,
        code: hex_u16(code)?,
        value: hex_i32(value)?,
    })
}

fn parse_timestamp(stamp: &str) -> Result<u64, LineError> {
    let err = || LineError::Timestamp(stamp.to_owned());
    let (secs, frac) = stamp.split_once('.').ok_or_else(err)?;
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(secs) || !all_digits(frac) {
        return Err(err());
    }
    let scale = *FRACTION_SCALE.get(frac.len()).ok_or_else(err)?;
    let secs: u64 = secs.parse().map_err(|_| err())?;
    let frac: u64 = frac.parse().map_err(|_| err())?;
    secs.checked_mul(1_000_000)
        .and_then(|us| us.checked_add(frac * scale))
        .ok_or_else(err)
}

fn hex_u16(field: &str) -> Result<u16, LineError> {
    u16::from_str_radix(field, 16).map_err(|_| LineError::Hex(field.to_owned()))
}

/// Hex word reinterpreted as two's complement (`fffffffc` → −4).
fn hex_i32(field: &str) -> Result<i32, LineError> {
    let word = u32::from_str_radix(field, 16).map_err(|_| LineError::Hex(field.to_owned()))?;
    Ok(i32::from_ne_bytes(word.to_ne_bytes()))
}

#[cfg(test)]
mod tests;
