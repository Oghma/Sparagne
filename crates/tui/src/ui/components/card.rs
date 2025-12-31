use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Span,
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};

use crate::ui::theme::Theme;

/// A modern card widget with rounded borders and consistent styling.
///
/// Cards are the primary container for dashboard panels and content sections.
pub struct Card<'a> {
    title: &'a str,
    theme: &'a Theme,
    focused: bool,
}

impl<'a> Card<'a> {
    pub fn new(title: &'a str, theme: &'a Theme) -> Self {
        Self {
            title,
            theme,
            focused: false,
        }
    }

    /// Mark this card as focused (uses accent border color).
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Creates the Block widget for this card.
    pub fn block(&self) -> Block<'a> {
        let border_color = if self.focused {
            self.theme.border_focused
        } else {
            self.theme.border
        };

        Block::default()
            .title(Span::styled(
                format!(" {} ", self.title),
                Style::default().fg(self.theme.accent),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(self.theme.surface_bright))
    }

    /// Returns the inner area after accounting for borders.
    pub fn inner(&self, area: Rect) -> Rect {
        self.block().inner(area)
    }

    /// Renders the card border/frame without content.
    pub fn render_frame(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(self.block(), area);
    }

    /// Renders the card with the given widget as content.
    pub fn render_with<W: Widget>(&self, frame: &mut Frame<'_>, area: Rect, content: W) {
        let inner = self.inner(area);
        frame.render_widget(self.block(), area);
        frame.render_widget(content, inner);
    }
}

/// A simple stat card showing a label and value.
pub struct StatCard<'a> {
    title: &'a str,
    value: String,
    subtitle: Option<String>,
    theme: &'a Theme,
}

impl<'a> StatCard<'a> {
    pub fn new(title: &'a str, value: impl Into<String>, theme: &'a Theme) -> Self {
        Self {
            title,
            value: value.into(),
            subtitle: None,
            theme,
        }
    }

    /// Add a subtitle below the main value.
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Render the stat card.
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let card = Card::new(self.title, self.theme);
        let inner = card.inner(area);
        card.render_frame(frame, area);

        let mut lines = vec![ratatui::text::Line::from(Span::styled(
            self.value.clone(),
            Style::default()
                .fg(self.theme.text)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ))];

        if let Some(sub) = &self.subtitle {
            lines.push(ratatui::text::Line::from(Span::styled(
                sub.clone(),
                Style::default().fg(self.theme.dim),
            )));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }
}
