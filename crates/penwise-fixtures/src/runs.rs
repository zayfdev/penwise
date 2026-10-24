//! Pen event stream → contact runs. evdev reports only changed axes, so the splitter keeps the current axis
//! state; every `SYN_REPORT` while `BTN_TOUCH` is down emits one frame of that state. A stroke starts at the
//! first such frame and ends at the first `SYN_REPORT` with the touch up (that frame is not part of it).

use penwise_core::codec::pwrec::{RawFrame, RecordedStroke, RunKind};
use thiserror::Error;

use crate::getevent::{
    ABS_PRESSURE, ABS_TILT_X, ABS_TILT_Y, ABS_X, ABS_Y, BTN_TOUCH, EV_ABS, EV_KEY, EV_SYN, Event,
    SYN_REPORT,
};

/// Timing that cannot be stored in a `.pwrec` stroke.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunError {
    #[error("frame at {t_us} µs precedes its stroke start {start_us} µs")]
    TimeWentBack { t_us: u64, start_us: u64 },
    #[error("stroke starting at {start_us} µs lasts longer than i32::MAX µs")]
    StrokeTooLong { start_us: u64 },
}

/// Incremental splitter for one pen node.
#[derive(Debug)]
pub struct RunSplitter<'a> {
    node: &'a str,
    /// Current axis state (`t_us`/`distance` unused).
    state: RawFrame,
    touch: bool,
    /// Timestamp of the first event of any device (recording start).
    origin_us: Option<u64>,
    /// Absolute start time and frames of the open stroke.
    open: Option<(u64, Vec<RawFrame>)>,
    strokes: Vec<RecordedStroke>,
}

impl<'a> RunSplitter<'a> {
    /// Splitter for events of `node`, starting from axis state `initial` with the touch up.
    #[must_use]
    pub fn new(node: &'a str, initial: RawFrame) -> Self {
        Self {
            node,
            state: initial,
            touch: false,
            origin_us: None,
            open: None,
            strokes: Vec::new(),
        }
    }

    /// Feeds one event of any device (other devices only set the recording start).
    ///
    /// # Errors
    /// [`RunError`] if a frame's time is before its stroke start or a stroke outlasts `i32` µs.
    pub fn event(&mut self, ev: &Event<'_>) -> Result<(), RunError> {
        self.origin_us.get_or_insert(ev.t_us);
        if ev.node != self.node {
            return Ok(());
        }
        match (ev.kind, ev.code) {
            (EV_ABS, ABS_X) => self.state.x = ev.value,
            (EV_ABS, ABS_Y) => self.state.y = ev.value,
            (EV_ABS, ABS_PRESSURE) => self.state.pressure = ev.value,
            (EV_ABS, ABS_TILT_X) => self.state.tilt_x = ev.value,
            (EV_ABS, ABS_TILT_Y) => self.state.tilt_y = ev.value,
            (EV_KEY, BTN_TOUCH) => self.touch = ev.value != 0,
            (EV_SYN, SYN_REPORT) if self.touch => self.push_frame(ev.t_us)?,
            (EV_SYN, SYN_REPORT) => self.close(),
            _ => {}
        }
        Ok(())
    }

    /// Closes a stroke still open at the end of the capture and returns every contact run in order.
    #[must_use]
    pub fn finish(mut self) -> Vec<RecordedStroke> {
        self.close();
        self.strokes
    }

    fn push_frame(&mut self, t_us: u64) -> Result<(), RunError> {
        let (start_us, frames) = self.open.get_or_insert_with(|| (t_us, Vec::new()));
        let start_us = *start_us;
        let dt = t_us
            .checked_sub(start_us)
            .ok_or(RunError::TimeWentBack { t_us, start_us })?;
        let dt = i32::try_from(dt).map_err(|_| RunError::StrokeTooLong { start_us })?;
        frames.push(RawFrame {
            t_us: dt,
            distance: 0,
            ..self.state
        });
        Ok(())
    }

    fn close(&mut self) {
        if let Some((start_us, frames)) = self.open.take() {
            self.strokes.push(RecordedStroke {
                kind: RunKind::Contact,
                t0_us: start_us.saturating_sub(self.origin_us.unwrap_or(start_us)),
                frames,
            });
        }
    }
}

#[cfg(test)]
mod tests;
