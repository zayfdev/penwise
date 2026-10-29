//! The binding stored on a stroke: which frame it belongs to and how it is anchored on that frame's page.

use crate::edit::EditError;
use crate::item::{FramePreset, ItemId};

/// Longest element key, in bytes.
pub const MAX_KEY_LEN: usize = 512;
/// Longest state recipe, in bytes.
pub const MAX_STATE_LEN: usize = 1024;

/// How a binding-local stroke maps onto the frame's page. Codes are frozen (item codec; append only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BindingMode {
    /// Relative to the element's top-left corner: the stroke follows the element.
    Bound,
    /// Document coordinates of the framed page: the stroke scrolls with the page.
    Area,
}

impl BindingMode {
    /// Frozen byte tag: `Bound = 0, Area = 1`.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Bound => 0,
            Self::Area => 1,
        }
    }

    /// Inverse of [`BindingMode::code`]; `None` for unknown bytes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Bound),
            1 => Some(Self::Area),
            _ => None,
        }
    }

    /// The lowercase name the clients and JSON use (`"bound"`, `"area"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bound => "bound",
            Self::Area => "area",
        }
    }

    /// Inverse of [`BindingMode::as_str`]; `None` for any other text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "bound" => Some(Self::Bound),
            "area" => Some(Self::Area),
            _ => None,
        }
    }
}

/// Where a stroke lives on a frame (K1): the frame, the binding mode, the size preset it was drawn at, the
/// element key (`bound` only), the page state recipe active at draw time and the page position the binding
/// origin had then (the fallback placement when no layout is known).
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    /// The frame item this stroke is drawn on.
    pub frame: ItemId,
    pub mode: BindingMode,
    /// The preset the stroke was drawn at; marks show only at their preset.
    pub preset: FramePreset,
    /// Element key (`v1|…` from the bridge); required for [`BindingMode::Bound`], `None` for area strokes.
    pub key: Option<String>,
    /// The frame's page state when drawn (empty = the default state).
    pub state: String,
    /// Page position of the binding origin (element top-left, or viewport top-left for area) at draw time.
    pub anchor: [f32; 2],
}

impl Binding {
    /// A binding, checked by [`Binding::check`].
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] naming `"binding"` for a bad key, state or anchor.
    pub fn new(
        frame: ItemId,
        mode: BindingMode,
        preset: FramePreset,
        key: Option<String>,
        state: String,
        anchor: [f32; 2],
    ) -> Result<Self, EditError> {
        let binding = Self {
            frame,
            mode,
            preset,
            key,
            state,
            anchor,
        };
        binding.check()?;
        Ok(binding)
    }

    /// - `bound`: a non-empty key of at most [`MAX_KEY_LEN`] bytes;
    /// - `area`: no key;
    /// - state at most [`MAX_STATE_LEN`] bytes; anchor finite.
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] naming `"binding"`.
    pub fn check(&self) -> Result<(), EditError> {
        let key_ok = match (self.mode, &self.key) {
            (BindingMode::Bound, Some(key)) => !key.is_empty() && key.len() <= MAX_KEY_LEN,
            (BindingMode::Area, None) => true,
            _ => false,
        };
        let state_ok = self.state.len() <= MAX_STATE_LEN;
        let anchor_ok = self.anchor[0].is_finite() && self.anchor[1].is_finite();
        if key_ok && state_ok && anchor_ok {
            Ok(())
        } else {
            Err(EditError::InvalidValue { field: "binding" })
        }
    }
}
