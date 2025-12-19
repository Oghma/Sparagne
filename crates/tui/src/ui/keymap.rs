use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    TogglePalette,
    Quit,
    Cancel,
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
        if let KeyCode::Char('p') = key.code {
            return AppAction::TogglePalette;
        }
    }

    match key.code {
        KeyCode::Char('q') => AppAction::Quit,
        KeyCode::Esc => AppAction::Cancel,
        KeyCode::Tab => AppAction::NextField,
        KeyCode::Enter => AppAction::Submit,
        KeyCode::Backspace => AppAction::Backspace,
        KeyCode::Up => AppAction::Up,
        KeyCode::Down => AppAction::Down,
        KeyCode::Char(ch) => AppAction::Input(ch),
        _ => AppAction::None,
    }
}
