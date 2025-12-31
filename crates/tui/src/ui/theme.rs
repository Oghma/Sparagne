use ratatui::style::Color;

/// Modern theme for Sparagne TUI combining dashboard aesthetics with power-user
/// density.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    // Base colors
    pub background: Color,
    pub surface: Color,
    pub surface_bright: Color,

    // Text hierarchy
    pub text: Color,
    pub text_muted: Color,

    // Legacy aliases (kept for backward compatibility)
    pub dim: Color,
    pub error: Color,

    // Semantic colors
    pub accent: Color,
    pub positive: Color,
    pub negative: Color,
    pub warning: Color,

    // Border colors
    pub border: Color,
    pub border_focused: Color,
}

impl Default for Theme {
    fn default() -> Self {
        let text_dimmed = Color::Rgb(100, 100, 100);
        let negative = Color::Rgb(220, 80, 80);

        Self {
            // Base surfaces default to terminal background.
            background: Color::Reset,
            surface: Color::Reset,
            surface_bright: Color::Reset,

            // Text hierarchy
            text: Color::Rgb(220, 220, 220),
            text_muted: Color::Rgb(160, 160, 160),

            // Legacy aliases
            dim: text_dimmed,
            error: negative,

            // Semantic - teal accent, green income, red expenses
            accent: Color::Rgb(80, 180, 180),
            positive: Color::Rgb(80, 200, 120),
            negative,
            warning: Color::Rgb(220, 180, 60),

            // Borders
            border: Color::Rgb(60, 70, 80),
            border_focused: Color::Rgb(80, 180, 180),
        }
    }
}
