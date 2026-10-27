//! Snapshot of a [`super::Smoother`] between two samples, so a trimmed stroke piece resumes smoothing exactly
//! where its origin stood at the cut instead of re-warming from stored context samples.

use super::MIN_DT_S;
use super::speed::SPEED_HISTORY;
use crate::sample::POSITION_LIMIT_PX;

/// Largest speed-like magnitude (px/s) a stored state may hold: far above any pen, far below `f32` overflow of
/// its squares.
const SPEED_LIMIT: f32 = 1_073_741_824.0; // 2³⁰

/// Longest Δt a stored state may hold, seconds (a `u32` µs timestamp spans 4294.97 s).
const DT_LIMIT_S: f32 = 4295.0;

/// One Euro position filter memory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionState {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub t_us: u32,
    pub dt_s: f32,
}

/// One Euro pressure filter memory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressureState {
    pub value: f32,
    pub derivative: f32,
    pub t_us: u32,
    pub dt_s: f32,
}

/// One entry of the speed window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistoryPoint {
    pub x: f32,
    pub y: f32,
    pub t_us: u32,
}

/// Complete smoother memory after at least one sample. `history` is oldest first, `1 ..= HISTORY_LEN` entries
/// with non-decreasing timestamps. Positions live in the frame the filters ran in (page px of the item's
/// transform when taken from a stroke; see `StrokeItem::warm_state`).
#[derive(Debug, Clone, PartialEq)]
pub struct SmootherState {
    pub position: PositionState,
    pub pressure: PressureState,
    pub history: Vec<HistoryPoint>,
}

/// Most entries of [`SmootherState::history`].
pub const HISTORY_LEN: usize = SPEED_HISTORY;

impl SmootherState {
    /// True when every value is finite and in range and the history is well formed, so restoring it can never
    /// produce non-finite geometry or underflow a timestamp difference.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let p = &self.position;
        let q = &self.pressure;
        let dt_ok = |dt: f32| dt.is_finite() && (MIN_DT_S..=DT_LIMIT_S).contains(&dt);
        let pos_ok = |v: f32| v.is_finite() && v.abs() <= POSITION_LIMIT_PX * POSITION_LIMIT_PX;
        let speed_ok = |v: f32| v.is_finite() && v.abs() <= SPEED_LIMIT;
        pos_ok(p.x)
            && pos_ok(p.y)
            && speed_ok(p.dx)
            && speed_ok(p.dy)
            && dt_ok(p.dt_s)
            && pos_ok(q.value)
            && speed_ok(q.derivative)
            && dt_ok(q.dt_s)
            && (1..=HISTORY_LEN).contains(&self.history.len())
            && self.history.iter().all(|h| pos_ok(h.x) && pos_ok(h.y))
            && self.history.windows(2).all(|w| w[0].t_us <= w[1].t_us)
    }

    /// The state with positions mapped by `point` and velocities by `vector` (the linear part of the same map).
    /// Pressure memory is unchanged.
    #[must_use]
    pub fn mapped(
        &self,
        point: impl Fn([f32; 2]) -> [f32; 2],
        vector: impl Fn([f32; 2]) -> [f32; 2],
    ) -> Self {
        let [x, y] = point([self.position.x, self.position.y]);
        let [dx, dy] = vector([self.position.dx, self.position.dy]);
        Self {
            position: PositionState {
                x,
                y,
                dx,
                dy,
                ..self.position
            },
            pressure: self.pressure,
            history: self
                .history
                .iter()
                .map(|h| {
                    let [x, y] = point([h.x, h.y]);
                    HistoryPoint { x, y, ..*h }
                })
                .collect(),
        }
    }
}
