mod app;
mod clipboard;
mod config;
mod finder;
mod fs;
mod git;
mod icons;
mod markdown;
mod palette;
mod search;
mod symbols;
mod syntax;
mod text;
mod theme;
mod tree;
mod ui;
mod viewer;
mod watcher;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyEventKind,
    },
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use std::time::Duration;

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(
            stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
    }
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let _guard = TerminalGuard;

    crossterm::execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = app::App::new();

    let result = run(&mut terminal, &mut app);
    app.save_state();
    result
}

fn run<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut app::App) -> Result<()> {
    let mut redraw = true;
    while app.running {
        if redraw {
            terminal.draw(|frame| ui::render(app, frame))?;
            redraw = false;
        }

        // Wake up periodically so background results (file index, search) get shown.
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                event::Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key);
                    redraw = true;
                }
                event::Event::Mouse(mouse) => redraw |= app.handle_mouse(mouse),
                event::Event::Paste(text) => {
                    app.handle_paste(&text);
                    redraw = true;
                }
                event::Event::Resize(_, _) => redraw = true,
                _ => {}
            }
        }

        if let Some(text) = app.clipboard.take() {
            clipboard::copy(&text);
        }
        redraw |= app.tick();
    }

    Ok(())
}
