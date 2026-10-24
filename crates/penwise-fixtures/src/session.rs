//! One recording session (`fixtures/raw/<session>/`): the `.pwrec` header derived from `device.txt`, and the
//! conversion of each capture into a [`Recording`].

use penwise_core::codec::pwrec::{
    CHANNEL_COUNT, Channels, RawFrame, RecordedStroke, Recording, RecordingHeader, RecordingSource,
};
use penwise_core::sample::{AxisRange, PxScale};
use thiserror::Error;

use crate::device::{AbsInfo, DeviceListing, InputDevice};
use crate::getevent::{LineError, parse_line};
use crate::runs::{RunError, RunSplitter};

/// Pen digitizer name (`scripts/record-pen.sh` `PEN_NAME`).
pub const PEN_NAME: &str = "sec_e-pen";
/// Touchscreen whose `ABS_X`/`ABS_Y` ranges are the display size in px.
pub const SCREEN_NAME: &str = "sec_touchscreen";

/// Conversion failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("device.txt lists no `{0}` device")]
    MissingDevice(&'static str),
    #[error("device `{device}` lists no {axis} range")]
    MissingAxis {
        device: &'static str,
        axis: &'static str,
    },
    #[error("device `{device}` {axis} range {min}..={max} is empty or too wide")]
    BadRange {
        device: &'static str,
        axis: &'static str,
        min: i32,
        max: i32,
    },
    #[error("capture line {line}: {source}")]
    Line { line: usize, source: LineError },
    #[error(transparent)]
    Run(#[from] RunError),
}

/// Header and initial pen state shared by every capture of a session.
#[derive(Debug, Clone)]
pub struct Session {
    pen_node: String,
    header: RecordingHeader,
    /// Pen axis values from the listing: the state before a capture's first event of each axis.
    initial: RawFrame,
}

impl Session {
    /// Reads the pen and touchscreen ranges from a session's `device.txt`.
    ///
    /// # Errors
    /// [`SessionError`] if the pen or touchscreen, or one of their axes, is missing or has an unusable range.
    pub fn from_device_txt(text: &str) -> Result<Self, SessionError> {
        let listing = DeviceListing::parse(text);
        let pen = listing
            .by_name(PEN_NAME)
            .ok_or(SessionError::MissingDevice(PEN_NAME))?;
        let screen = listing
            .by_name(SCREEN_NAME)
            .ok_or(SessionError::MissingDevice(SCREEN_NAME))?;
        let [x, y, pressure, tilt_x, tilt_y] =
            ["ABS_X", "ABS_Y", "ABS_PRESSURE", "ABS_TILT_X", "ABS_TILT_Y"]
                .map(|axis| axis_of(pen, PEN_NAME, axis));
        let (x, y, pressure, tilt_x, tilt_y) = (x?, y?, pressure?, tilt_x?, tilt_y?);
        let range = |a: AbsInfo| AxisRange {
            min: a.min,
            max: a.max,
        };
        let mut ranges = [AxisRange { min: 0, max: 0 }; CHANNEL_COUNT];
        ranges[0] = range(x);
        ranges[1] = range(y);
        ranges[2] = range(pressure);
        ranges[3] = range(tilt_x);
        ranges[4] = range(tilt_y);
        // Slot 5 (distance) is not written by v1; slot 6 (t_us) is µs since stroke start.
        ranges[6] = AxisRange {
            min: 0,
            max: i32::MAX,
        };
        let header = RecordingHeader {
            channels: Channels::V1_DEFAULT,
            source: RecordingSource::Getevent,
            device: listing.model.as_deref().map_or_else(
                || PEN_NAME.to_owned(),
                |model| format!("{model}/{PEN_NAME}"),
            ),
            ranges,
            x_scale: px_scale(x, axis_of(screen, SCREEN_NAME, "ABS_X")?, "ABS_X")?,
            y_scale: px_scale(y, axis_of(screen, SCREEN_NAME, "ABS_Y")?, "ABS_Y")?,
            rate_hint: 0,
        };
        let initial = RawFrame {
            x: x.value,
            y: y.value,
            pressure: pressure.value,
            tilt_x: tilt_x.value,
            tilt_y: tilt_y.value,
            distance: 0,
            t_us: 0,
        };
        Ok(Self {
            pen_node: pen.node.clone(),
            header,
            initial,
        })
    }

    /// Converts one decompressed capture. The pen node comes from the capture's own `add device` lines, falling
    /// back to `device.txt`. `rate_hint` is the measured contact frame rate (0 when no stroke has 2 frames).
    ///
    /// # Errors
    /// [`SessionError::Line`] for malformed event lines, [`SessionError::Run`] for unstorable timing.
    pub fn convert(&self, capture: &str) -> Result<Recording, SessionError> {
        let listing = DeviceListing::parse(capture);
        let node = listing
            .by_name(PEN_NAME)
            .map_or(self.pen_node.as_str(), |d| d.node.as_str());
        let mut splitter = RunSplitter::new(node, self.initial);
        for (index, line) in capture.lines().enumerate() {
            let Some(ev) = parse_line(line) else { continue };
            let ev = ev.map_err(|source| SessionError::Line {
                line: index + 1,
                source,
            })?;
            splitter.event(&ev)?;
        }
        let strokes = splitter.finish();
        let mut header = self.header.clone();
        header.rate_hint = frame_rate(&strokes);
        Ok(Recording { header, strokes })
    }
}

fn axis_of(
    dev: &InputDevice,
    device: &'static str,
    axis: &'static str,
) -> Result<AbsInfo, SessionError> {
    dev.abs(axis)
        .ok_or(SessionError::MissingAxis { device, axis })
}

/// `px = (raw − pen.min) · (screen span) / (pen span)`, spans counted inclusively (`max − min + 1`).
fn px_scale(pen: AbsInfo, screen: AbsInfo, axis: &'static str) -> Result<PxScale, SessionError> {
    let span = |a: AbsInfo, device: &'static str| {
        i64::from(a.max)
            .checked_sub(i64::from(a.min))
            .and_then(|d| u32::try_from(d + 1).ok())
            .filter(|&n| n > 0)
            .ok_or(SessionError::BadRange {
                device,
                axis,
                min: a.min,
                max: a.max,
            })
    };
    Ok(PxScale {
        num: span(screen, SCREEN_NAME)?,
        den: span(pen, PEN_NAME)?,
    })
}

/// Rounded mean contact frame rate in Hz over every inter-frame interval.
fn frame_rate(strokes: &[RecordedStroke]) -> u32 {
    let (intervals, span_us) = strokes.iter().fold((0u64, 0u64), |(n, span), s| {
        let last = s
            .frames
            .last()
            .map_or(0, |f| u64::try_from(f.t_us).unwrap_or(0));
        (n + (s.frames.len() as u64).saturating_sub(1), span + last)
    });
    if span_us == 0 {
        return 0;
    }
    u32::try_from((intervals * 1_000_000 + span_us / 2) / span_us).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
