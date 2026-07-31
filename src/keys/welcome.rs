//! The onboarding screen's key map — fully separate from `handle_key`'s main
//! dispatch since Welcome owns its own two-phase (intro/setup) navigation
//! and text-entry form.

use crossterm::event::{KeyCode, KeyEvent};

use jira_tui::app::{self, App};

pub(super) fn handle_welcome_key(app: &mut App, key: KeyEvent) {
    use app::WelcomePhase;
    match app.onboarding.welcome_phase {
        WelcomePhase::Intro => match key.code {
            KeyCode::Char('s') => {
                app.onboarding.welcome_phase = WelcomePhase::Setup;
                app.onboarding.setup_msg.clear();
            }
            KeyCode::Char('d') | KeyCode::Enter => app.finish_onboarding(),
            KeyCode::Char('w') => app.write_config_from_welcome(),
            KeyCode::Char('?') => app.show_help = true,
            KeyCode::Char('q') | KeyCode::Esc => app.finish_onboarding(),
            _ => {}
        },
        WelcomePhase::Setup => match key.code {
            KeyCode::Esc => {
                app.onboarding.welcome_phase = WelcomePhase::Intro;
                app.onboarding.setup_msg.clear();
            }
            KeyCode::Enter => app.submit_credentials(),
            KeyCode::Tab | KeyCode::Down => app.focus_next(),
            KeyCode::BackTab | KeyCode::Up => app.focus_prev(),
            KeyCode::Backspace => app.input_backspace(),
            KeyCode::Char(c) => app.input_char(c),
            _ => {}
        },
    }
}
