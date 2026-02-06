use ratatui::style::Color;

/// Gruvbox (Dark) theme for Sparagne TUI.
///
/// A retro groove color scheme for the terminal.
/// <https://github.com/morhetz/gruvbox>
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // Base colors
    pub background: Color, // #282828 (bg)

    // Text hierarchy
    pub text: Color,       // #ebdbb2 (fg)
    pub text_muted: Color, // #a89984 (gray)

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

            // Text hierarchy
            text,
            text_muted,

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
