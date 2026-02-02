use ratatui::style::Color;

use crate::config::Density;

/// Spacing values based on UI density.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Spacing {
    /// Padding inside cards and blocks.
    pub padding: u16,
    /// Vertical spacing between sections.
    pub section_gap: u16,
    /// Height of list items.
    pub list_item_height: u16,
    /// Margin around main content areas.
    pub margin: u16,
}

#[allow(dead_code)]
impl Spacing {
    /// Creates spacing values for the given density.
    #[must_use]
    pub fn from_density(density: Density) -> Self {
        match density {
            Density::Compact => Self {
                padding: 0,
                section_gap: 0,
                list_item_height: 1,
                margin: 0,
            },
            Density::Normal => Self {
                padding: 1,
                section_gap: 1,
                list_item_height: 2,
                margin: 1,
            },
            Density::Comfortable => Self {
                padding: 2,
                section_gap: 2,
                list_item_height: 3,
                margin: 2,
            },
        }
    }
}

/// Gruvbox (Dark) theme for Sparagne TUI.
///
/// A retro groove color scheme for the terminal.
/// <https://github.com/morhetz/gruvbox>
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Theme {
    // Base colors
    pub background: Color,     // #282828 (bg)
    pub surface: Color,        // #3c3836 (bg1)
    pub surface_bright: Color, // #504945 (bg2)

    // Text hierarchy
    pub text: Color,       // #ebdbb2 (fg)
    pub text_muted: Color, // #a89984 (gray)

    // Legacy aliases (kept for backward compatibility)
    pub dim: Color,   // same as text_muted
    pub error: Color, // same as negative

    // Semantic colors
    pub accent: Color,   // #fe8019 (orange) - primary UI accent
    pub positive: Color, // #b8bb26 (green) - income, success
    pub negative: Color, // #fb4934 (red) - expenses, errors
    pub warning: Color,  // #fabd2f (yellow) - warnings, alerts
    pub info: Color,     // #83a598 (blue) - informational

    // Transaction type colors
    pub income: Color,   // #b8bb26 (green)
    pub expense: Color,  // #fb4934 (red)
    pub transfer: Color, // #83a598 (blue)
    pub refund: Color,   // #d3869b (purple)

    // Border colors
    pub border: Color,         // #504945 (bg2)
    pub border_focused: Color, // #fe8019 (orange)
}

impl Default for Theme {
    fn default() -> Self {
        // Gruvbox Dark Palette
        let background = Color::Rgb(40, 40, 40); // #282828
        let surface = Color::Rgb(60, 56, 54); // #3c3836 (bg1)
        let surface_bright = Color::Rgb(80, 73, 69); // #504945 (bg2)

        let text = Color::Rgb(235, 219, 178); // #ebdbb2 (fg)
        let text_muted = Color::Rgb(168, 153, 132); // #a89984 (gray)

        let orange = Color::Rgb(254, 128, 25); // #fe8019
        let green = Color::Rgb(184, 187, 38); // #b8bb26
        let red = Color::Rgb(251, 73, 52); // #fb4934
        let yellow = Color::Rgb(250, 189, 47); // #fabd2f
        let blue = Color::Rgb(131, 165, 152); // #83a598
        let purple = Color::Rgb(211, 134, 155); // #d3869b

        Self {
            // Base surfaces
            background,
            surface,
            surface_bright,

            // Text hierarchy
            text,
            text_muted,

            // Legacy aliases
            dim: text_muted,
            error: red,

            // Semantic colors
            accent: orange,
            positive: green,
            negative: red,
            warning: yellow,
            info: blue,

            // Transaction type colors
            income: green,
            expense: red,
            transfer: blue,
            refund: purple,

            // Borders
            border: surface_bright,
            border_focused: orange,
        }
    }
}
