//! Text items (recognition output or typed text).
//!
//! Owner: slice a.

use super::id::ItemId;
use super::paint::Paint;

/// Text in a box. Local frame: box `[0, size[0]] × [0, size[1]]` with its top-left at the local origin, so
/// the item transform's translation is the box's page position. Layout/shaping is the renderer's job; core
/// treats the box as the item's hit and lasso shape.
#[derive(Debug, Clone, PartialEq)]
pub struct TextItem {
    /// UTF-8 content (mutable: [`crate::edit::Field::Text`]).
    pub content: String,
    /// Box width, height in local px (mutable: [`crate::edit::Field::TextBox`]).
    pub size: [f32; 2],
    /// Font size in local px (mutable: [`crate::edit::Field::FontSize`]).
    pub font_size: f32,
    /// Text color (mutable: [`crate::edit::Field::Paint`]).
    pub paint: Paint,
    /// Stroke ids this text was recognized from (PLAN "Recognition service"), ascending; immutable.
    pub sources: Vec<ItemId>,
}
