mod app;
mod fs;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(stdout(), LeaveAlternateScreen);
    }
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let _guard = TerminalGuard;

    crossterm::execute!(stdout(), EnterAlternateScreen)?;
    crossterm::execute!(stdout(), EnableBracketedPaste)?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = app::App::new();

    run(&mut terminal, &mut app)?;

    crossterm::execute!(stdout(), DisableBracketedPaste)?;

    Ok(())
}

fn run<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut app::App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::render(app, frame))?;

        if !event::poll(std::time::Duration::from_millis(16))? {
            continue;
        }

        if let event::Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q')
                    if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                {
                    app.running = false;
                    break;
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    app.running = false;
                    break;
                }
                KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                KeyCode::Enter => app.enter(),
                KeyCode::Backspace => app.go_up(),
                KeyCode::PageUp | KeyCode::Char('u') => app.scroll_up(),
                KeyCode::PageDown | KeyCode::Char('d') => {
                    let height = terminal.size()?.height;
                    app.scroll_down(height);
                }
                _ => {}
            }
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}
