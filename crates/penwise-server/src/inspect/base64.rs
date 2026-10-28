//! Standard base64 decoding for the `data` field of `Page.captureScreenshot` and for resources fetched through
//! `Page.getResourceContent`. Padding and whitespace are accepted.

use super::Error;

#[allow(
    clippy::cast_possible_truncation,
    reason = "the accumulator is masked to the output width before every push"
)]
pub(super) fn decode_base64(input: &str) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' => continue,
            _ => return Err(Error::Cdp("invalid base64".to_owned())),
        };
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Ok(out)
}
