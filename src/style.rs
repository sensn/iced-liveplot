//! Style and theming configuration.
//!
//! Themes describe plot-level colors (background, grid, axes, overlays).

use iced::Color;

/// Visual theme for plot-level elements such as axes, grid, and overlays.
///
/// Themes are applied at the plot level and affect all series and overlays.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Plot background color.
    pub background: Color,
    /// Axis, tick, and label color.
    pub axis: Color,
    /// Major grid line color.
    pub grid_major: Color,
    /// Minor grid line color.
    pub grid_minor: Color,
    /// Hover tooltip background color.
    pub hover_bg: Color,
    /// Hover tooltip border color.
    pub hover_border: Color,
    /// Pin tooltip background color.
    pub pin_bg: Color,
    /// Pin tooltip border color.
    pub pin_border: Color,
    /// Selection rectangle fill color.
    pub selection_fill: Color,
    /// Selection rectangle border color.
    pub selection_border: Color,
    /// Legend background color.
    pub legend_bg: Color,
    /// Legend border color.
    pub legend_border: Color,
}

impl Theme {
    /// Create the default theme (alias of [`Theme::dark`]).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a light theme palette.
    pub fn light() -> Self {
        Self {
            background: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            axis: Color {
                r: 0.2,
                g: 0.2,
                b: 0.2,
                a: 1.0,
            },
            grid_major: Color {
                r: 0.86,
                g: 0.86,
                b: 0.86,
                a: 1.0,
            },
            grid_minor: Color {
                r: 0.93,
                g: 0.93,
                b: 0.93,
                a: 1.0,
            },
            hover_bg: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.9,
            },
            hover_border: Color {
                r: 0.2,
                g: 0.2,
                b: 0.2,
                a: 0.8,
            },
            pin_bg: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.92,
            },
            pin_border: Color {
                r: 0.2,
                g: 0.2,
                b: 0.2,
                a: 0.8,
            },
            selection_fill: Color {
                r: 0.1,
                g: 0.4,
                b: 0.9,
                a: 0.15,
            },
            selection_border: Color {
                r: 0.1,
                g: 0.4,
                b: 0.9,
                a: 0.9,
            },
            legend_bg: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.85,
            },
            legend_border: Color {
                r: 0.2,
                g: 0.2,
                b: 0.2,
                a: 0.6,
            },
        }
    }

    /// Create a dark theme palette.
    pub fn dark() -> Self {
        Self {
            background: Color {
                r: 0.08,
                g: 0.08,
                b: 0.09,
                a: 1.0,
            },
            axis: Color {
                r: 0.85,
                g: 0.85,
                b: 0.85,
                a: 1.0,
            },
            grid_major: Color {
                r: 0.25,
                g: 0.25,
                b: 0.28,
                a: 1.0,
            },
            grid_minor: Color {
                r: 0.18,
                g: 0.18,
                b: 0.2,
                a: 1.0,
            },
            hover_bg: Color {
                r: 0.12,
                g: 0.12,
                b: 0.13,
                a: 0.92,
            },
            hover_border: Color {
                r: 0.6,
                g: 0.6,
                b: 0.6,
                a: 0.8,
            },
            pin_bg: Color {
                r: 0.12,
                g: 0.12,
                b: 0.13,
                a: 0.92,
            },
            pin_border: Color {
                r: 0.6,
                g: 0.6,
                b: 0.6,
                a: 0.85,
            },
            selection_fill: Color {
                r: 0.2,
                g: 0.5,
                b: 0.95,
                a: 0.18,
            },
            selection_border: Color {
                r: 0.3,
                g: 0.6,
                b: 1.0,
                a: 0.9,
            },
            legend_bg: Color {
                r: 0.12,
                g: 0.12,
                b: 0.13,
                a: 0.9,
            },
            legend_border: Color {
                r: 0.5,
                g: 0.5,
                b: 0.5,
                a: 0.7,
            },
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
