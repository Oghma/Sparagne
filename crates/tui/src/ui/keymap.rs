use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    TogglePalette,
    Search,
    CycleAmbiguous,
    Quit,
    Cancel,
    NextField,
    Submit,
    Backspace,
    Up,
    Down,
    Left,
    Right,
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
        if let KeyCode::Char('f') = key.code {
            return AppAction::Search;
        }
        if let KeyCode::Char('r') = key.code {
            return AppAction::CycleAmbiguous;
        }
    }

    match key.code {
        KeyCode::Esc => AppAction::Cancel,
        KeyCode::Tab => AppAction::NextField,
        KeyCode::Enter => AppAction::Submit,
        KeyCode::Backspace => AppAction::Backspace,
        KeyCode::Up => AppAction::Up,
        KeyCode::Down => AppAction::Down,
        KeyCode::Left => AppAction::Left,
        KeyCode::Right => AppAction::Right,
        KeyCode::Char(ch) => AppAction::Input(ch),
        _ => AppAction::None,
    }
}
