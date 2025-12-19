use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub background: Color,
    pub panel: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::Rgb(8, 12, 16),
            panel: Color::Rgb(20, 26, 32),
            text: Color::Rgb(220, 220, 220),
            dim: Color::Rgb(140, 140, 140),
            accent: Color::Rgb(80, 160, 160),
            error: Color::Rgb(200, 80, 80),
        }
    }
}
