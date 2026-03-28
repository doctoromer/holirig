use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;

pub enum InputAction {
    Submit(String),
    Quit,
    None,
}

pub fn handle_key_event(key: KeyEvent, app: &mut App) -> InputAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return InputAction::Quit;
    }

    match key.code {
        KeyCode::Enter => {
            let input = app.input.trim().to_string();
            if input.is_empty() {
                return InputAction::None;
            }
            app.push_command(&input);
            app.input.clear();
            app.cursor_pos = 0;
            InputAction::Submit(input)
        }
        KeyCode::Char('q') if app.input.is_empty() => InputAction::Quit,
        KeyCode::Char(c) => {
            app.input.insert(app.cursor_pos, c);
            app.cursor_pos += 1;
            InputAction::None
        }
        KeyCode::Backspace => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
                app.input.remove(app.cursor_pos);
            }
            InputAction::None
        }
        KeyCode::Delete => {
            if app.cursor_pos < app.input.len() {
                app.input.remove(app.cursor_pos);
            }
            InputAction::None
        }
        KeyCode::Left => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
            }
            InputAction::None
        }
        KeyCode::Right => {
            if app.cursor_pos < app.input.len() {
                app.cursor_pos += 1;
            }
            InputAction::None
        }
        KeyCode::Up => {
            app.history_up();
            InputAction::None
        }
        KeyCode::Down => {
            app.history_down();
            InputAction::None
        }
        KeyCode::Home => {
            app.cursor_pos = 0;
            InputAction::None
        }
        KeyCode::End => {
            app.cursor_pos = app.input.len();
            InputAction::None
        }
        _ => InputAction::None,
    }
}
