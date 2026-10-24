//! Page layers (PLAN decision "Layers") and layer filters for queries.
//!
//! Owner: slice a.

/// Which layer an item belongs to. Layers are a visibility/ownership filter only: paint and hit order is the
/// global `(band, z, id)` order ([`crate::item::RenderKey`]), not the layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    /// The user's ink.
    Ink,
    /// Agent annotations (show/hide, erase, or promote to ink with an `Update { layer }` edit).
    Agent,
    /// Recognition output.
    Text,
}

impl Layer {
    /// Frozen byte tag: `Ink = 0, Agent = 1, Text = 2`.
    #[must_use]
    pub fn code(self) -> u8 {
        todo!()
    }

    /// Inverse of [`Layer::code`]; `None` for unknown bytes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        todo!()
    }
}

/// Set of layers a query considers (e.g. skip a hidden agent layer). Bit `1 << layer.code()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerSet {
    pub bits: u8,
}

impl LayerSet {
    /// No layer.
    pub const NONE: Self = Self { bits: 0 };
    /// Ink, agent and text.
    pub const ALL: Self = Self { bits: 0b111 };
    /// Ink only (fill walls, default tool targets).
    pub const INK: Self = Self { bits: 0b001 };

    /// `self` plus `layer`.
    #[must_use]
    pub fn with(self, layer: Layer) -> Self {
        todo!()
    }

    /// `self` minus `layer`.
    #[must_use]
    pub fn without(self, layer: Layer) -> Self {
        todo!()
    }

    /// True when `layer` is in the set.
    #[must_use]
    pub fn contains(self, layer: Layer) -> bool {
        todo!()
    }
}
