//! Text measuring for the iced backend.
//!
//! The GPUI backend measured labels with the window's font system. iced's
//! [`Paragraph`] API requires a renderer instance, which is not available while
//! laying out geometry, so we approximate widths from an average glyph factor
//! (like many canvas-based charts do). Monospace digits keep this stable.

use crate::axis::TextMeasurer;

/// Average character width factor used to estimate label widths.
const CHAR_WIDTH_FACTOR: f32 = 0.55;

/// A [`TextMeasurer`] that estimates text extents without a font backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct EstimatedTextMeasurer;

impl TextMeasurer for EstimatedTextMeasurer {
    fn measure(&self, text: &str, size: f32) -> (f32, f32) {
        let width = text.chars().count() as f32 * size * CHAR_WIDTH_FACTOR;
        (width, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_grows_with_text_and_size() {
        let m = EstimatedTextMeasurer;
        assert!(m.measure("12345", 11.0).0 > m.measure("123", 11.0).0);
        assert!(m.measure("123", 16.0).0 > m.measure("123", 11.0).0);
        assert_eq!(m.measure("", 11.0).0, 0.0);
    }
}
