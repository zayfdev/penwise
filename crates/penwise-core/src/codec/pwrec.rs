//! `.pwrec` v1 pen-recording fixtures (core-determinism.md §6). The converter from raw `getevent` captures lives
//! outside core (`crates/penwise-fixtures`); core only encodes/decodes and maps to samples.
//!
//! Owner: slice d.
//!
//! Layout v1 (fixed fields LE; `uv`/`sv` per `varint`):
//! ```text
//! magic     4   b"PWRC"
//! version   u16 = 1
//! channels  u16 bitmask: 0 x, 1 y, 2 pressure, 3 tilt_x, 4 tilt_y, 5 distance, 6 t_us
//! source    u8  0 = getevent raw device units, 1 = Android MotionEvent
//! reserved  u8  = 0
//! device    uv len + UTF-8
//! ranges        per present channel in bit order: sv min, sv max
//! px map    uv x_num, uv x_den, uv y_num, uv y_den     px = (raw − min)·num/den
//! rate_hint uv  nominal Hz (informational)
//! strokes   uv  count
//!   kind    u8  0 contact, 1 hover
//!   t0_us   uv  (64-bit) µs since recording start
//!   n       uv  sample count (≥ 1)
//!   sample 0    sv per present channel, absolute (t channel = 0)
//!   sample i    sv per present channel, delta vs sample i−1 (row-major)
//! trailer   u64 FNV-1a 64 of every preceding byte
//! ```

use super::error::CodecError;
use crate::sample::{AxisRange, DigitizerRanges, PxScale, RawSample, Sample};

/// `.pwrec` magic.
pub const PWREC_MAGIC: [u8; 4] = *b"PWRC";
/// Current `.pwrec` layout version.
pub const PWREC_VERSION: u16 = 1;
/// Number of channel slots in v1.
pub const CHANNEL_COUNT: usize = 7;

/// Channel bitmask (bit `i` = channel `i`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Channels(pub u16);

impl Channels {
    pub const X: u16 = 1 << 0;
    pub const Y: u16 = 1 << 1;
    pub const PRESSURE: u16 = 1 << 2;
    pub const TILT_X: u16 = 1 << 3;
    pub const TILT_Y: u16 = 1 << 4;
    pub const DISTANCE: u16 = 1 << 5;
    pub const T_US: u16 = 1 << 6;
    /// What the v1 getevent converter writes: `x, y, pressure, tilt_x, tilt_y, t_us` (`0x5F`; no distance).
    /// Decoders accept any subset of the 7 known bits.
    pub const V1_DEFAULT: Self = Self(0x5F);

    /// True if every bit of `mask` is set.
    #[must_use]
    pub const fn contains(self, mask: u16) -> bool {
        self.0 & mask == mask
    }
}

/// Who produced the samples (decides units).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordingSource {
    /// Raw `getevent` device units (code 0).
    Getevent,
    /// Android `MotionEvent` recorder (code 1; own quantization declared in the header).
    MotionEvent,
}

/// Run kind of a recorded stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunKind {
    /// `BTN_TOUCH` down run (code 0).
    Contact,
    /// Hover run (code 1).
    Hover,
}

/// Header of a recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingHeader {
    pub channels: Channels,
    pub source: RecordingSource,
    /// e.g. `"SM-X710/sec_e-pen"`.
    pub device: String,
    /// Range per channel slot (index = bit); absent channels hold `{0, 0}` and are not serialized.
    pub ranges: [AxisRange; CHANNEL_COUNT],
    pub x_scale: PxScale,
    pub y_scale: PxScale,
    /// Nominal Hz (482 for the Tab S9 digitizer).
    pub rate_hint: u32,
}

/// One recorded frame; absent channels are 0. `t_us` is relative to the stroke's first frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawFrame {
    pub x: i32,
    pub y: i32,
    pub pressure: i32,
    pub tilt_x: i32,
    pub tilt_y: i32,
    pub distance: i32,
    pub t_us: i32,
}

/// One recorded run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedStroke {
    pub kind: RunKind,
    /// µs since recording start.
    pub t0_us: u64,
    /// `≥ 1` frames.
    pub frames: Vec<RawFrame>,
}

/// A decoded `.pwrec` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub header: RecordingHeader,
    pub strokes: Vec<RecordedStroke>,
}

/// Appends the v1 encoding of `rec` (only channels in `header.channels` are written; deltas are wrapping
/// `i32`). Streams with `n == 0` are written as-is (the decoder rejects them, so converters must not emit them).
pub fn encode_pwrec(rec: &Recording, out: &mut Vec<u8>) {
    todo!()
}

/// Strict decode of a whole file.
///
/// # Errors
/// `BadMagic`, `UnsupportedVersion`, `UnexpectedEof`, `VarintOverflow` (> 5 bytes for 32-bit fields, > 10 for
/// `t0_us`), `ChecksumMismatch`, `TrailingBytes`, `InvalidValue` (unknown channel bits, source, kind,
/// non-zero reserved byte, invalid UTF-8, stroke with `n == 0`, sample 0 `t ≠ 0`, negative `t`, any
/// `den == 0`), `OutOfRange` (delta accumulation overflowing `i32`).
pub fn decode_pwrec(bytes: &[u8]) -> Result<Recording, CodecError> {
    todo!()
}

impl RawFrame {
    /// Raw sample for [`DigitizerRanges::map`] (`tilt_x/tilt_y` as degrees, negative `t_us` → 0).
    #[must_use]
    pub fn to_raw_sample(&self) -> RawSample {
        todo!()
    }
}

impl Recording {
    /// Digitizer ranges from the header (x/y/pressure/tilt ranges + px mapping); absent tilt channels give
    /// range `{0, 0}` (tilt maps to upright).
    ///
    /// # Errors
    /// `UnsupportedSource(1)` for `MotionEvent` recordings (v1 maps getevent units only);
    /// `InvalidValue` if x, y or pressure is absent.
    pub fn digitizer_ranges(&self) -> Result<DigitizerRanges, CodecError> {
        todo!()
    }
}

impl RecordedStroke {
    /// Clears `out` and appends every frame mapped with `ranges.map` then [`Sample::quantize`].
    pub fn to_samples(&self, ranges: &DigitizerRanges, out: &mut Vec<Sample>) {
        todo!()
    }
}
