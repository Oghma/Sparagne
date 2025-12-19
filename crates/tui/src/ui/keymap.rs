use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    Quit,
    NextField,
    Submit,
    Backspace,
    Up,
    Down,
    Input(char),
    None,
}

pub fn map_key(key: KeyEvent) -> AppAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char('c') = key.code {
            return AppAction::Quit;
        }
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => AppAction::Quit,
        KeyCode::Tab => AppAction::NextField,
        KeyCode::Enter => AppAction::Submit,
        KeyCode::Backspace => AppAction::Backspace,
        KeyCode::Up => AppAction::Up,
        KeyCode::Down => AppAction::Down,
        KeyCode::Char(ch) => AppAction::Input(ch),
        _ => AppAction::None,
    }
}
